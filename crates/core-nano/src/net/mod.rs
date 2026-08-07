//! smoltcp-based framing with a host-simulated device.
//!
//! # Honesty
//! QUIC-over-smoltcp is **not** implemented. Quinn needs OS UDP sockets.
//! On bare metal, CNK carries postcard mesh frames inside UDP/Ethernet via
//! smoltcp. Hosts continue to use Quinn/TCP from Phase 1.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::time::Instant;

use crate::frame::{decode_datagram, encode_datagram, MeshFrame};

/// Loopback-ish simulated Ethernet device for host tests.
pub struct SimDevice {
    rx: VecDeque<Vec<u8>>,
    tx: VecDeque<Vec<u8>>,
}

impl SimDevice {
    pub fn new() -> Self {
        Self {
            rx: VecDeque::new(),
            tx: VecDeque::new(),
        }
    }

    pub fn push_rx(&mut self, frame: Vec<u8>) {
        self.rx.push_back(frame);
    }

    pub fn pop_tx(&mut self) -> Option<Vec<u8>> {
        self.tx.pop_front()
    }
}

impl Default for SimDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl Device for SimDevice {
    type RxToken<'a> = SimRxToken
    where
        Self: 'a;
    type TxToken<'a> = SimTxToken<'a>
    where
        Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let rx = self.rx.pop_front()?;
        Some((SimRxToken(rx), SimTxToken(&mut self.tx)))
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Some(SimTxToken(&mut self.tx))
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = 1500;
        caps.medium = Medium::Ethernet;
        caps
    }
}

pub struct SimRxToken(Vec<u8>);

impl RxToken for SimRxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.0)
    }
}

pub struct SimTxToken<'a>(&'a mut VecDeque<Vec<u8>>);

impl<'a> TxToken for SimTxToken<'a> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buf = alloc::vec![0u8; len];
        let result = f(&mut buf);
        self.0.push_back(buf);
        result
    }
}

pub fn encapsulate(frame: &MeshFrame) -> Result<Vec<u8>, ()> {
    encode_datagram(frame)
}

pub fn decapsulate(bytes: &[u8]) -> Result<MeshFrame, ()> {
    decode_datagram(bytes)
}

/// Smoke: device round-trip of a mesh datagram (no full IP stack required).
pub fn sim_loopback_frame(frame: &MeshFrame) -> Result<MeshFrame, ()> {
    let mut dev = SimDevice::new();
    let bytes = encapsulate(frame)?;
    if let Some(token) = Device::transmit(&mut dev, Instant::from_millis(0)) {
        token.consume(bytes.len(), |buf| {
            buf.copy_from_slice(&bytes);
        });
    }
    let tx = dev.pop_tx().ok_or(())?;
    dev.push_rx(tx);
    if let Some((rx, _tx)) = Device::receive(&mut dev, Instant::from_millis(1)) {
        return rx.consume(|b| decapsulate(b));
    }
    Err(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{msg, MeshFrame};

    #[test]
    fn sim_loopback() {
        let f = MeshFrame::new(msg::TX_LOG_SYNC, 1, b"log".to_vec());
        let out = sim_loopback_frame(&f).unwrap();
        assert_eq!(out, f);
    }
}
