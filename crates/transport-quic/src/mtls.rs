//! Local CA + mutual TLS helpers for QUIC (quinn/rustls).

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use quinn::{ClientConfig, Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer, PrivateKeyDer};
use rustls::RootCertStore;

/// Material for one mesh identity under a shared CA.
pub struct MeshIdentity {
    pub ca_cert: CertificateDer<'static>,
    pub cert: CertificateDer<'static>,
    pub key: PrivateKeyDer<'static>,
}

/// Lab CA that can mint server/client leaves.
pub struct LocalCa {
    cert: rcgen::Certificate,
    key: rcgen::KeyPair,
    der: CertificateDer<'static>,
}

impl LocalCa {
    pub fn new() -> anyhow::Result<Self> {
        let mut params = rcgen::CertificateParams::new(vec!["Chimera Local CA".into()])?;
        params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        params.key_usages = vec![
            rcgen::KeyUsagePurpose::KeyCertSign,
            rcgen::KeyUsagePurpose::DigitalSignature,
            rcgen::KeyUsagePurpose::CrlSign,
        ];
        let key = rcgen::KeyPair::generate()?;
        let cert = params.self_signed(&key)?;
        let der = CertificateDer::from(cert.der().to_vec());
        Ok(Self { cert, key, der })
    }

    pub fn mint(&self, name: &str) -> anyhow::Result<MeshIdentity> {
        let mut leaf = rcgen::CertificateParams::new(vec![
            name.into(),
            "localhost".into(),
            "chimera.local".into(),
        ])?;
        leaf.is_ca = rcgen::IsCa::NoCa;
        leaf.extended_key_usages = vec![
            rcgen::ExtendedKeyUsagePurpose::ClientAuth,
            rcgen::ExtendedKeyUsagePurpose::ServerAuth,
        ];
        let leaf_key = rcgen::KeyPair::generate()?;
        let leaf_cert = leaf.signed_by(&leaf_key, &self.cert, &self.key)?;
        Ok(MeshIdentity {
            ca_cert: self.der.clone(),
            cert: CertificateDer::from(leaf_cert.der().to_vec()),
            key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der())),
        })
    }

    pub fn ca_der(&self) -> CertificateDer<'static> {
        self.der.clone()
    }
}

fn clone_key(key: &PrivateKeyDer<'static>) -> PrivateKeyDer<'static> {
    match key {
        PrivateKeyDer::Pkcs8(k) => {
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(k.secret_pkcs8_der().to_vec()))
        }
        PrivateKeyDer::Sec1(k) => PrivateKeyDer::Sec1(rustls::pki_types::PrivateSec1KeyDer::from(
            k.secret_sec1_der().to_vec(),
        )),
        PrivateKeyDer::Pkcs1(k) => {
            PrivateKeyDer::Pkcs1(rustls::pki_types::PrivatePkcs1KeyDer::from(
                k.secret_pkcs1_der().to_vec(),
            ))
        }
        _ => panic!("unsupported key type"),
    }
}

pub fn mtls_server_endpoint(
    bind: std::net::SocketAddr,
    identity: &MeshIdentity,
) -> anyhow::Result<Endpoint> {
    let mut roots = RootCertStore::empty();
    roots
        .add(identity.ca_cert.clone())
        .map_err(|e| anyhow::anyhow!("ca: {e}"))?;
    let verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(roots))
        .build()
        .context("client verifier")?;
    let mut server_crypto = rustls::ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(vec![identity.cert.clone()], clone_key(&identity.key))?;
    server_crypto.alpn_protocols = vec![b"chimera".to_vec()];
    let mut server_config = ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)?,
    ));
    server_config.transport = Arc::new({
        let mut t = quinn::TransportConfig::default();
        t.keep_alive_interval(Some(Duration::from_millis(200)));
        t.max_idle_timeout(Some(Duration::from_secs(5).try_into()?));
        t
    });
    Ok(Endpoint::server(server_config, bind)?)
}

pub fn mtls_client_config(identity: &MeshIdentity) -> anyhow::Result<ClientConfig> {
    let mut roots = RootCertStore::empty();
    roots
        .add(identity.ca_cert.clone())
        .map_err(|e| anyhow::anyhow!("ca: {e}"))?;
    let mut crypto = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_client_auth_cert(vec![identity.cert.clone()], clone_key(&identity.key))?;
    crypto.alpn_protocols = vec![b"chimera".to_vec()];
    Ok(ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(crypto)?,
    )))
}

pub fn mtls_client_endpoint(
    bind: std::net::SocketAddr,
    identity: &MeshIdentity,
) -> anyhow::Result<Endpoint> {
    let mut endpoint = Endpoint::client(bind)?;
    endpoint.set_default_client_config(mtls_client_config(identity)?);
    Ok(endpoint)
}

#[cfg(test)]
mod tests {
    use super::*;
#[allow(unused_imports)]
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn mtls_handshake_ok_and_rejects_unauthenticated() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let ca = LocalCa::new().unwrap();
        let server_id = ca.mint("server").unwrap();
        let client_id = ca.mint("client").unwrap();

        let server = mtls_server_endpoint("127.0.0.1:0".parse().unwrap(), &server_id).unwrap();
        let addr = server.local_addr().unwrap();
        let accept = tokio::spawn(async move {
            let connecting = server.accept().await.unwrap();
            let conn = connecting.await.unwrap();
            let mut recv = conn.accept_uni().await.unwrap();
            let mut buf = [0u8; 4];
            recv.read_exact(&mut buf).await.unwrap();
            buf
        });

        let client = mtls_client_endpoint("0.0.0.0:0".parse().unwrap(), &client_id).unwrap();
        let conn = client
            .connect(addr, "localhost")
            .unwrap()
            .await
            .expect("authenticated mTLS connect");
        let mut send = conn.open_uni().await.unwrap();
        send.write_all(b"ping").await.unwrap();
        send.finish().unwrap();
        assert_eq!(&accept.await.unwrap(), b"ping");

        // Reject peer without client certificate (server handshake must fail).
        let server2 = mtls_server_endpoint("127.0.0.1:0".parse().unwrap(), &server_id).unwrap();
        let addr2 = server2.local_addr().unwrap();
        let server_result = tokio::spawn(async move {
            let connecting = server2.accept().await.expect("accept");
            connecting.await
        });
        tokio::time::sleep(Duration::from_millis(40)).await;

        let mut roots = RootCertStore::empty();
        roots.add(ca.ca_der()).unwrap();
        let mut crypto = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        crypto.alpn_protocols = vec![b"chimera".to_vec()];
        let bad = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(crypto).unwrap(),
        ));
        let mut bad_ep = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        bad_ep.set_default_client_config(bad);
        let client_res = bad_ep.connect(addr2, "localhost").unwrap().await;
        let server_res = server_result.await.unwrap();
        assert!(
            client_res.is_err() || server_res.is_err(),
            "unauthenticated peer must fail mTLS (client={client_res:?} server={server_res:?})"
        );
    }
}
