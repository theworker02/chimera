//! Lightweight mesh frame codec (postcard) for CNK / smoltcp payloads.

use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::CNK_VERSION;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FrameHeader {
    pub version: u16,
    pub msg_type: u16,
    pub flags: u16,
    pub seq: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MeshFrame {
    pub header: FrameHeader,
    pub body: Vec<u8>,
}

impl MeshFrame {
    pub fn new(msg_type: u16, seq: u32, body: Vec<u8>) -> Self {
        Self {
            header: FrameHeader {
                version: CNK_VERSION,
                msg_type,
                flags: 0,
                seq,
            },
            body,
        }
    }

    pub fn with_flags(mut self, flags: u16) -> Self {
        self.header.flags = flags;
        self
    }
}

pub fn encode_frame(frame: &MeshFrame) -> Result<Vec<u8>, ()> {
    postcard::to_allocvec(frame).map_err(|_| ())
}

pub fn decode_frame(bytes: &[u8]) -> Result<MeshFrame, ()> {
    postcard::from_bytes(bytes).map_err(|_| ())
}

/// Length-prefixed datagram for UDP/smoltcp.
pub fn encode_datagram(frame: &MeshFrame) -> Result<Vec<u8>, ()> {
    let payload = encode_frame(frame)?;
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

pub fn decode_datagram(bytes: &[u8]) -> Result<MeshFrame, ()> {
    if bytes.len() < 4 {
        return Err(());
    }
    let len = u32::from_le_bytes(bytes[0..4].try_into().map_err(|_| ())?) as usize;
    if bytes.len() < 4 + len {
        return Err(());
    }
    decode_frame(&bytes[4..4 + len])
}

pub mod msg {
    pub const HEARTBEAT: u16 = 1;
    pub const TX_LOG_SYNC: u16 = 2;
    pub const PQ_HANDSHAKE: u16 = 3;
    pub const TASK: u16 = 4;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let f = MeshFrame::new(msg::HEARTBEAT, 7, b"ping".to_vec());
        let enc = encode_datagram(&f).unwrap();
        let dec = decode_datagram(&enc).unwrap();
        assert_eq!(dec, f);
    }
}
