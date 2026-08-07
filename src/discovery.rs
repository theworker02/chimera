//! UDP multicast gossip peer discovery.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use parking_lot::RwLock;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::protocol::{decode_msg, encode_msg, now_ms, Capabilities, NodeId, PeerInfo, WireMsg};

const MAX_DATAGRAM: usize = 8 * 1024;

#[derive(Clone)]
pub struct PeerTable {
    inner: Arc<RwLock<HashMap<NodeId, PeerInfo>>>,
    local_id: NodeId,
}

impl PeerTable {
    pub fn new(local_id: NodeId) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            local_id,
        }
    }

    pub fn upsert(&self, peer: PeerInfo) {
        if peer.id == self.local_id {
            return;
        }
        self.inner.write().insert(peer.id, peer);
    }

    pub fn get(&self, id: &NodeId) -> Option<PeerInfo> {
        self.inner.read().get(id).cloned()
    }

    pub fn all(&self) -> Vec<PeerInfo> {
        self.inner.read().values().cloned().collect()
    }

    pub fn alive(&self, timeout: Duration) -> Vec<PeerInfo> {
        let now = now_ms();
        self.inner
            .read()
            .values()
            .filter(|p| !p.is_stale(timeout, now))
            .cloned()
            .collect()
    }

    pub fn prune(&self, timeout: Duration) -> Vec<NodeId> {
        let now = now_ms();
        let mut guard = self.inner.write();
        let dead: Vec<NodeId> = guard
            .iter()
            .filter(|(_, p)| p.is_stale(timeout, now))
            .map(|(id, _)| *id)
            .collect();
        for id in &dead {
            guard.remove(id);
        }
        dead
    }

    pub fn underutilized(&self, timeout: Duration) -> Option<PeerInfo> {
        let mut peers = self.alive(timeout);
        peers.sort_by(|a, b| {
            a.caps
                .load_score
                .partial_cmp(&b.caps.load_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        peers.into_iter().next()
    }

    pub fn len(&self) -> usize {
        self.inner.read().len()
    }
}

pub struct GossipService {
    peers: PeerTable,
    local: PeerInfo,
    group: Ipv4Addr,
    port: u16,
    interval: Duration,
    timeout: Duration,
    tx_events: mpsc::UnboundedSender<GossipEvent>,
}

#[derive(Debug, Clone)]
pub enum GossipEvent {
    PeerJoined(PeerInfo),
    PeerUpdated(PeerInfo),
    PeerLost(NodeId),
}

impl GossipService {
    pub fn new(
        local: PeerInfo,
        peers: PeerTable,
        group: Ipv4Addr,
        port: u16,
        interval: Duration,
        timeout: Duration,
    ) -> (Self, mpsc::UnboundedReceiver<GossipEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                peers,
                local,
                group,
                port,
                interval,
                timeout,
                tx_events: tx,
            },
            rx,
        )
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        let sock = bind_multicast(self.group, self.port)?;
        let sock = Arc::new(sock);
        let dest = SocketAddr::V4(SocketAddrV4::new(self.group, self.port));

        let recv_sock = sock.clone();
        let peers = self.peers.clone();
        let local_id = self.local.id;
        let tx = self.tx_events.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; MAX_DATAGRAM];
            loop {
                match recv_sock.recv_from(&mut buf).await {
                    Ok((n, _from)) => {
                        if let Ok(WireMsg::GossipAnnounce { peer, .. }) = decode_msg(&buf[..n]) {
                            if peer.id == local_id {
                                continue;
                            }
                            let existed = peers.get(&peer.id).is_some();
                            peers.upsert(peer.clone());
                            let ev = if existed {
                                GossipEvent::PeerUpdated(peer)
                            } else {
                                GossipEvent::PeerJoined(peer)
                            };
                            let _ = tx.send(ev);
                        }
                    }
                    Err(e) => {
                        warn!("gossip recv error: {e}");
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                }
            }
        });

        let mut tick = tokio::time::interval(self.interval);
        loop {
            tick.tick().await;
            self.local.last_seen_ms = now_ms();
            let known: Vec<NodeId> = self.peers.alive(self.timeout).into_iter().map(|p| p.id).collect();
            let msg = WireMsg::GossipAnnounce {
                peer: self.local.clone(),
                known_peers: known,
            };
            if let Ok(bytes) = encode_msg(&msg) {
                if let Err(e) = sock.send_to(&bytes, dest).await {
                    debug!("gossip send: {e}");
                }
            }
            for dead in self.peers.prune(self.timeout) {
                let _ = self.tx_events.send(GossipEvent::PeerLost(dead));
            }
        }
    }

    pub fn update_local_caps(&mut self, caps: Capabilities) {
        self.local.caps = caps;
        self.local.last_seen_ms = now_ms();
    }
}

fn bind_multicast(group: Ipv4Addr, port: u16) -> anyhow::Result<UdpSocket> {
    let addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port);
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.bind(&socket2::SockAddr::from(addr))?;
    socket.join_multicast_v4(&group, &Ipv4Addr::UNSPECIFIED)?;
    socket.set_multicast_loop_v4(true)?;
    socket.set_nonblocking(true)?;
    let std_sock: std::net::UdpSocket = socket.into();
    UdpSocket::from_std(std_sock).context("tokio udp from_std")
}
