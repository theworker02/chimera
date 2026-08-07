//! Legacy mesh bridging — plain TCP tunnel (working); serial/BT stubs (roadmap).

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// Mesh frame envelope shared with CNK wire framing story (length-prefixed bytes).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeFrame {
    pub src: String,
    pub dst: String,
    pub kind: u16,
    pub payload: Vec<u8>,
}

impl BridgeFrame {
    pub fn encode(&self) -> Vec<u8> {
        let body = serde_json::to_vec(self).unwrap_or_default();
        let mut out = Vec::with_capacity(4 + body.len());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    pub fn decode(buf: &[u8]) -> anyhow::Result<(Self, usize)> {
        if buf.len() < 4 {
            anyhow::bail!("short header");
        }
        let n = u32::from_le_bytes(buf[..4].try_into()?) as usize;
        if buf.len() < 4 + n {
            anyhow::bail!("short body");
        }
        let frame: BridgeFrame = serde_json::from_slice(&buf[4..4 + n])?;
        Ok((frame, 4 + n))
    }
}

pub trait TransportAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn send(&self, frame: &BridgeFrame) -> anyhow::Result<()>;
    fn try_recv(&self) -> anyhow::Result<Option<BridgeFrame>>;
}

/// In-process + TCP legacy adapter: each endpoint has a mailbox; TCP pipes length-prefixed frames.
pub struct TcpBridgeEndpoint {
    pub id: String,
    inbox: Arc<Mutex<VecDeque<BridgeFrame>>>,
    peer: Arc<Mutex<Option<TcpStream>>>,
}

impl TcpBridgeEndpoint {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            inbox: Arc::new(Mutex::new(VecDeque::new())),
            peer: Arc::new(Mutex::new(None)),
        }
    }

    /// Listen and accept one peer connection (blocking).
    pub fn listen_once(&self, addr: &str) -> anyhow::Result<()> {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(false)?;
        let (stream, _) = listener.accept()?;
        stream.set_nodelay(true)?;
        *self.peer.lock() = Some(stream.try_clone()?);
        let inbox = self.inbox.clone();
        thread::spawn(move || {
            let mut stream = stream;
            let mut buf = Vec::new();
            let mut tmp = [0u8; 4096];
            loop {
                match stream.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&tmp[..n]);
                        while let Ok((frame, used)) = BridgeFrame::decode(&buf) {
                            inbox.lock().push_back(frame);
                            buf.drain(..used);
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(())
    }

    pub fn connect(&self, addr: &str) -> anyhow::Result<()> {
        let stream = TcpStream::connect(addr)?;
        stream.set_nodelay(true)?;
        *self.peer.lock() = Some(stream.try_clone()?);
        let inbox = self.inbox.clone();
        thread::spawn(move || {
            let mut stream = stream;
            let mut buf = Vec::new();
            let mut tmp = [0u8; 4096];
            loop {
                match stream.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&tmp[..n]);
                        while let Ok((frame, used)) = BridgeFrame::decode(&buf) {
                            inbox.lock().push_back(frame);
                            buf.drain(..used);
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(())
    }
}

impl TransportAdapter for TcpBridgeEndpoint {
    fn name(&self) -> &str {
        "tcp-legacy"
    }

    fn send(&self, frame: &BridgeFrame) -> anyhow::Result<()> {
        let mut g = self.peer.lock();
        let stream = g.as_mut().ok_or_else(|| anyhow::anyhow!("not connected"))?;
        stream.write_all(&frame.encode())?;
        stream.flush()?;
        Ok(())
    }

    fn try_recv(&self) -> anyhow::Result<Option<BridgeFrame>> {
        Ok(self.inbox.lock().pop_front())
    }
}

/// Serial adapter stub — no hardware in CI.
#[cfg(feature = "bridge-serial")]
pub struct SerialBridgeStub;

#[cfg(feature = "bridge-serial")]
impl TransportAdapter for SerialBridgeStub {
    fn name(&self) -> &str {
        "serial-stub"
    }
    fn send(&self, _frame: &BridgeFrame) -> anyhow::Result<()> {
        anyhow::bail!("serial bridge is a roadmap stub")
    }
    fn try_recv(&self) -> anyhow::Result<Option<BridgeFrame>> {
        Ok(None)
    }
}

/// Bluetooth adapter stub — no hardware in CI.
#[cfg(feature = "bridge-bluetooth")]
pub struct BluetoothBridgeStub;

#[cfg(feature = "bridge-bluetooth")]
impl TransportAdapter for BluetoothBridgeStub {
    fn name(&self) -> &str {
        "bluetooth-stub"
    }
    fn send(&self, _frame: &BridgeFrame) -> anyhow::Result<()> {
        anyhow::bail!("bluetooth bridge is a roadmap stub")
    }
    fn try_recv(&self) -> anyhow::Result<Option<BridgeFrame>> {
        Ok(None)
    }
}

/// Exchange a task request/response between two in-process TCP-bridged endpoints.
pub fn exchange_task_over_tcp(
    listen_addr: &str,
    connect_addr: &str,
    payload: &[u8],
) -> anyhow::Result<Vec<u8>> {
    let server = Arc::new(TcpBridgeEndpoint::new("server"));
    let client = Arc::new(TcpBridgeEndpoint::new("client"));
    let s = server.clone();
    let addr = listen_addr.to_string();
    let handle = thread::spawn(move || s.listen_once(&addr));
    thread::sleep(Duration::from_millis(50));
    client.connect(connect_addr)?;
    handle.join().map_err(|_| anyhow::anyhow!("listen join"))??;

    let req = BridgeFrame {
        src: "client".into(),
        dst: "server".into(),
        kind: 1, // task request
        payload: payload.to_vec(),
    };
    client.send(&req)?;

    // Server receives, replies with doubled payload as "result"
    let mut got = None;
    for _ in 0..50 {
        if let Some(f) = server.try_recv()? {
            got = Some(f);
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let frame = got.ok_or_else(|| anyhow::anyhow!("no request received"))?;
    let mut reply_payload = frame.payload.clone();
    reply_payload.extend_from_slice(b":done");
    let reply = BridgeFrame {
        src: "server".into(),
        dst: "client".into(),
        kind: 2,
        payload: reply_payload.clone(),
    };
    server.send(&reply)?;

    let mut out = None;
    for _ in 0..50 {
        if let Some(f) = client.try_recv()? {
            out = Some(f.payload);
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    out.ok_or_else(|| anyhow::anyhow!("no reply"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip() {
        let f = BridgeFrame {
            src: "a".into(),
            dst: "b".into(),
            kind: 7,
            payload: b"hi".to_vec(),
        };
        let enc = f.encode();
        let (dec, n) = BridgeFrame::decode(&enc).unwrap();
        assert_eq!(n, enc.len());
        assert_eq!(dec, f);
    }

    #[test]
    fn two_nodes_tcp_exchange_task() {
        // Ephemeral port
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let addr = format!("127.0.0.1:{port}");
        let out = exchange_task_over_tcp(&addr, &addr, b"slice-1").unwrap();
        assert_eq!(out, b"slice-1:done");
    }
}
