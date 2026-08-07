//! TEE attestation abstraction — simulated backend working; hardware stubs roadmap.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TeeBackendKind {
    /// Software-simulated measurement + ed25519 quote. **Status: working**
    Simulated,
    /// Intel TDX. **Status: roadmap / unimplemented on this host**
    IntelTdx,
    /// AMD SEV-SNP. **Status: roadmap / unimplemented on this host**
    AmdSevSnp,
    /// ARM TrustZone. **Status: roadmap / unimplemented on this host**
    ArmTrustZone,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeeAttestation {
    pub backend: TeeBackendKind,
    pub measurement: [u8; 32],
    pub nonce: [u8; 16],
    pub report_data: Vec<u8>,
    pub quote: Vec<u8>,
    pub public_key: [u8; 32],
}

pub trait TeeProvider: Send + Sync {
    fn kind(&self) -> TeeBackendKind;
    fn attest(&self, nonce: &[u8; 16], report_data: &[u8]) -> anyhow::Result<TeeAttestation>;
    fn verify(&self, att: &TeeAttestation, expected_nonce: Option<&[u8; 16]>) -> bool;
}

/// Working software TEE — BLAKE3 measurement of a sealed blob + ed25519 quote.
pub struct SimulatedTee {
    signing: SigningKey,
    verifying: VerifyingKey,
    /// Sealed “enclave” image bytes (lab stand-in for firmware measurement).
    image: Vec<u8>,
}

impl SimulatedTee {
    pub fn new(image: impl AsRef<[u8]>) -> Self {
        let signing = SigningKey::generate(&mut OsRng);
        let verifying = signing.verifying_key();
        Self {
            signing,
            verifying,
            image: image.as_ref().to_vec(),
        }
    }

    fn measure(&self, nonce: &[u8; 16], report_data: &[u8]) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(&self.image);
        h.update(nonce);
        h.update(report_data);
        *h.finalize().as_bytes()
    }
}

impl TeeProvider for SimulatedTee {
    fn kind(&self) -> TeeBackendKind {
        TeeBackendKind::Simulated
    }

    fn attest(&self, nonce: &[u8; 16], report_data: &[u8]) -> anyhow::Result<TeeAttestation> {
        let measurement = self.measure(nonce, report_data);
        let mut pre = Vec::new();
        pre.extend_from_slice(&measurement);
        pre.extend_from_slice(nonce);
        pre.extend_from_slice(report_data);
        let sig = self.signing.sign(&pre);
        Ok(TeeAttestation {
            backend: TeeBackendKind::Simulated,
            measurement,
            nonce: *nonce,
            report_data: report_data.to_vec(),
            quote: sig.to_bytes().to_vec(),
            public_key: self.verifying.to_bytes(),
        })
    }

    fn verify(&self, att: &TeeAttestation, expected_nonce: Option<&[u8; 16]>) -> bool {
        if att.backend != TeeBackendKind::Simulated {
            return false;
        }
        if let Some(n) = expected_nonce {
            if &att.nonce != n {
                return false;
            }
        }
        let expected = self.measure(&att.nonce, &att.report_data);
        if expected != att.measurement {
            return false;
        }
        let Ok(vk) = VerifyingKey::from_bytes(&att.public_key) else {
            return false;
        };
        let mut pre = Vec::new();
        pre.extend_from_slice(&att.measurement);
        pre.extend_from_slice(&att.nonce);
        pre.extend_from_slice(&att.report_data);
        let Ok(sig) = Signature::from_slice(&att.quote) else {
            return false;
        };
        vk.verify(&pre, &sig).is_ok()
    }
}

/// Hardware backends — explicit unimplemented stubs.
pub struct HardwareTeeStub {
    pub kind: TeeBackendKind,
}

impl TeeProvider for HardwareTeeStub {
    fn kind(&self) -> TeeBackendKind {
        self.kind
    }

    fn attest(&self, _nonce: &[u8; 16], _report_data: &[u8]) -> anyhow::Result<TeeAttestation> {
        anyhow::bail!(
            "{:?} attestation unimplemented on this host (roadmap)",
            self.kind
        )
    }

    fn verify(&self, _att: &TeeAttestation, _expected_nonce: Option<&[u8; 16]>) -> bool {
        false
    }
}

/// Envelope combining TEE quote with optional PQ handshake material (Phase 6 story).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestedHandshake {
    pub attestation: TeeAttestation,
    pub pq_kem_ciphertext: Vec<u8>,
    pub pq_sig: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulated_attestation_pass_fail() {
        let tee = SimulatedTee::new(b"enclave-image-v1");
        let nonce = [7u8; 16];
        let att = tee.attest(&nonce, b"chimera-node").unwrap();
        assert!(tee.verify(&att, Some(&nonce)));
        let mut bad = att.clone();
        bad.measurement[0] ^= 0xff;
        assert!(!tee.verify(&bad, Some(&nonce)));
        let wrong_nonce = [9u8; 16];
        assert!(!tee.verify(&att, Some(&wrong_nonce)));
    }

    #[test]
    fn hardware_stub_errors() {
        let stub = HardwareTeeStub {
            kind: TeeBackendKind::IntelTdx,
        };
        assert!(stub.attest(&[0u8; 16], b"x").is_err());
    }
}
