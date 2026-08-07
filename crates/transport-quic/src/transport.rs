//! QUIC (quinn) + TCP framed transport with control-plane priority.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use parking_lot::Mutex;
use quinn::{ClientConfig, Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::protocol::{decode_msg, encode_msg, frame, NodeId, WireMsg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamClass {
    /// Heartbeats, reclaim, ownership â€” never starved by bulk I/O.
    Control,
    /// Task assign / progress / receipts.
    Compute,
    /// ChimeraFS blocks / ChimeraMEM pages.
    Bulk,
}

#[derive(Clone)]
pub struct MeshTransport {
    #[allow(dead_code)]
    local_id: NodeId,
    quic_bind: SocketAddr,
    tcp_bind: SocketAddr,
    inbound: mpsc::UnboundedSender<(NodeId, WireMsg, StreamClass)>,
    peers_tcp: Arc<Mutex<HashMap<NodeId, SocketAddr>>>,
    peers_quic: Arc<Mutex<HashMap<NodeId, SocketAddr>>>,
    endpoint: Arc<Mutex<Option<Endpoint>>>,
}

impl MeshTransport {
    pub fn new(
        local_id: NodeId,
        quic_bind: SocketAddr,
        tcp_bind: SocketAddr,
    ) -> (Self, mpsc::UnboundedReceiver<(NodeId, WireMsg, StreamClass)>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                local_id,
                quic_bind,
                tcp_bind,
                inbound: tx,
                peers_tcp: Arc::new(Mutex::new(HashMap::new())),
                peers_quic: Arc::new(Mutex::new(HashMap::new())),
                endpoint: Arc::new(Mutex::new(None)),
            },
            rx,
        )
    }

    pub fn remember_peer(&self, id: NodeId, tcp: SocketAddr, quic: SocketAddr) {
        self.peers_tcp.lock().insert(id, tcp);
        self.peers_quic.lock().insert(id, quic);
    }

    pub async fn serve(self: Arc<Self>) -> anyhow::Result<()> {
        let endpoint = make_server_endpoint(self.quic_bind)?;
        *self.endpoint.lock() = Some(endpoint.clone());
        info!("QUIC listening on {}", self.quic_bind);

        let tcp_self = self.clone();
        tokio::spawn(async move {
            if let Err(e) = tcp_self.serve_tcp().await {
                warn!("tcp serve: {e}");
            }
        });

        loop {
            let connecting = endpoint.accept().await;
            let Some(connecting) = connecting else { break };
            let this = self.clone();
            tokio::spawn(async move {
                match connecting.await {
                    Ok(conn) => {
                        let peer = NodeId::new(); // identity resolved via first heartbeat
                        while let Ok(mut recv) = conn.accept_uni().await {
                            let this = this.clone();
                            tokio::spawn(async move {
                                if let Ok(bytes) = read_framed_quic(&mut recv).await {
                                    if let Ok(msg) = decode_msg(&bytes) {
                                        let class = classify(&msg);
                                        let from = extract_from(&msg).unwrap_or(peer);
                                        let _ = this.inbound.send((from, msg, class));
                                    }
                                }
                            });
                        }
                    }
                    Err(e) => debug!("quic accept: {e}"),
                }
            });
        }
        Ok(())
    }

    async fn serve_tcp(self: Arc<Self>) -> anyhow::Result<()> {
        let listener = TcpListener::bind(self.tcp_bind).await?;
        info!("TCP listening on {}", self.tcp_bind);
        loop {
            let (stream, addr) = listener.accept().await?;
            let this = self.clone();
            tokio::spawn(async move {
                if let Err(e) = this.handle_tcp(stream, addr).await {
                    debug!("tcp session {addr}: {e}");
                }
            });
        }
    }

    async fn handle_tcp(self: Arc<Self>, mut stream: TcpStream, _addr: SocketAddr) -> anyhow::Result<()> {
        loop {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).await?;
            let len = u32::from_le_bytes(len_buf) as usize;
            if len > 16 * 1024 * 1024 {
                anyhow::bail!("frame too large");
            }
            let mut buf = vec![0u8; len];
            stream.read_exact(&mut buf).await?;
            let msg = decode_msg(&buf)?;
            let class = classify(&msg);
            let from = extract_from(&msg).unwrap_or_else(NodeId::new);
            let _ = self.inbound.send((from, msg, class));
        }
    }

    pub async fn send(&self, to: NodeId, msg: WireMsg, class: StreamClass) -> anyhow::Result<()> {
        let bytes = encode_msg(&msg)?;
        // Prefer QUIC for control; TCP as reliable fallback.
        if class == StreamClass::Control || class == StreamClass::Compute {
            if self.send_quic(to, &bytes).await.is_ok() {
                return Ok(());
            }
        }
        self.send_tcp(to, &bytes).await
    }

    pub async fn broadcast_control(&self, msg: WireMsg) {
        let peers: Vec<NodeId> = self.peers_tcp.lock().keys().copied().collect();
        for id in peers {
            let _ = self.send(id, msg.clone(), StreamClass::Control).await;
        }
    }

    async fn send_tcp(&self, to: NodeId, payload: &[u8]) -> anyhow::Result<()> {
        let addr = self
            .peers_tcp
            .lock()
            .get(&to)
            .copied()
            .context("unknown tcp peer")?;
        let mut stream = TcpStream::connect(addr).await?;
        let framed = frame(payload);
        stream.write_all(&framed).await?;
        Ok(())
    }

    async fn send_quic(&self, to: NodeId, payload: &[u8]) -> anyhow::Result<()> {
        let addr = self
            .peers_quic
            .lock()
            .get(&to)
            .copied()
            .context("unknown quic peer")?;
        let endpoint = {
            let guard = self.endpoint.lock();
            guard.clone().context("quic endpoint not ready")?
        };
        let client = make_client_endpoint(endpoint.local_addr()?)?;
        let conn = client
            .connect(addr, "chimera.local")?
            .await
            .context("quic connect")?;
        let mut send = conn.open_uni().await?;
        let framed = frame(payload);
        send.write_all(&framed).await?;
        send.finish()?;
        Ok(())
    }
}

fn classify(msg: &WireMsg) -> StreamClass {
    match msg {
        WireMsg::Heartbeat { .. }
        | WireMsg::ProtocolHello { .. }
        | WireMsg::Reclaim { .. }
        | WireMsg::PageOwn { .. }
        | WireMsg::AgentVote { .. } => StreamClass::Control,
        WireMsg::BlockGet { .. }
        | WireMsg::BlockPut { .. }
        | WireMsg::PageFetch { .. }
        | WireMsg::PageData { .. }
        | WireMsg::MigrateChunk { .. } => StreamClass::Bulk,
        _ => StreamClass::Compute,
    }
}

fn extract_from(msg: &WireMsg) -> Option<NodeId> {
    match msg {
        WireMsg::Heartbeat { from, .. } => Some(*from),
        WireMsg::StealRequest { from, .. } => Some(*from),
        WireMsg::GossipAnnounce { peer, .. } => Some(peer.id),
        WireMsg::TaskComplete { receipt, .. } => Some(receipt.executor),
        _ => None,
    }
}

async fn read_framed_quic(recv: &mut quinn::RecvStream) -> anyhow::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    recv.read_exact(&mut buf).await?;
    Ok(buf)
}

fn make_server_endpoint(bind: SocketAddr) -> anyhow::Result<Endpoint> {
    let (cert, key) = gen_self_signed()?;
    let mut server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)?;
    server_crypto.alpn_protocols = vec![b"chimera".to_vec()];
    let mut server_config = ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)?,
    ));
    server_config.transport = Arc::new({
        let mut t = quinn::TransportConfig::default();
        t.keep_alive_interval(Some(Duration::from_secs(2)));
        t.max_idle_timeout(Some(Duration::from_secs(30).try_into()?));
        t
    });
    Ok(Endpoint::server(server_config, bind)?)
}

fn make_client_endpoint(bind: SocketAddr) -> anyhow::Result<Endpoint> {
    let mut endpoint = Endpoint::client(bind)?;
    endpoint.set_default_client_config(insecure_client_config()?);
    Ok(endpoint)
}

fn insecure_client_config() -> anyhow::Result<ClientConfig> {
    let provider = rustls::crypto::ring::default_provider();
    let _ = provider.install_default();
    #[derive(Debug)]
    struct Skip;
    impl rustls::client::danger::ServerCertVerifier for Skip {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &rustls::pki_types::ServerName<'_>,
            _ocsp_response: &[u8],
            _now: rustls::pki_types::UnixTime,
        ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        }
        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }
        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }
        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            rustls::crypto::ring::default_provider()
                .signature_verification_algorithms
                .supported_schemes()
        }
    }
    let mut crypto = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Skip))
        .with_no_client_auth();
    crypto.alpn_protocols = vec![b"chimera".to_vec()];
    Ok(ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(crypto)?,
    )))
}

fn gen_self_signed() -> anyhow::Result<(CertificateDer<'static>, rustls::pki_types::PrivateKeyDer<'static>)> {
    let cert = rcgen::generate_simple_self_signed(vec!["chimera.local".into()])?;
    let key = PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());
    Ok((
        CertificateDer::from(cert.cert),
        rustls::pki_types::PrivateKeyDer::Pkcs8(key),
    ))
}
