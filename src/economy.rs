//! Verifiable compute receipts (ed25519 + BLAKE3). Optional ZK feature stub.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;

use crate::protocol::{now_ms, ComputeReceipt, NodeId, TaskId};

#[derive(Clone)]
pub struct ReceiptSigner {
    signing: SigningKey,
    verifying: VerifyingKey,
}

impl ReceiptSigner {
    pub fn generate() -> Self {
        let signing = SigningKey::generate(&mut OsRng);
        let verifying = signing.verifying_key();
        Self { signing, verifying }
    }

    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.verifying.to_bytes()
    }

    pub fn sign_receipt(
        &self,
        task_id: TaskId,
        executor: NodeId,
        transcript_hash: [u8; 32],
        fuel_consumed: u64,
        io_merkle_root: [u8; 32],
    ) -> ComputeReceipt {
        let mut preimage = Vec::with_capacity(128);
        preimage.extend_from_slice(task_id.0.as_bytes());
        preimage.extend_from_slice(executor.0.as_bytes());
        preimage.extend_from_slice(&transcript_hash);
        preimage.extend_from_slice(&fuel_consumed.to_le_bytes());
        preimage.extend_from_slice(&io_merkle_root);
        let sig = self.signing.sign(&preimage);
        ComputeReceipt {
            task_id,
            executor,
            transcript_hash,
            fuel_consumed,
            io_merkle_root,
            public_key: self.verifying.to_bytes(),
            signature: sig.to_bytes().to_vec(),
            timestamp_ms: now_ms(),
        }
    }
}

pub fn verify_receipt(receipt: &ComputeReceipt) -> bool {
    let Ok(vk) = VerifyingKey::from_bytes(&receipt.public_key) else {
        return false;
    };
    let mut preimage = Vec::with_capacity(128);
    preimage.extend_from_slice(receipt.task_id.0.as_bytes());
    preimage.extend_from_slice(receipt.executor.0.as_bytes());
    preimage.extend_from_slice(&receipt.transcript_hash);
    preimage.extend_from_slice(&receipt.fuel_consumed.to_le_bytes());
    preimage.extend_from_slice(&receipt.io_merkle_root);
    let Ok(sig) = Signature::from_slice(&receipt.signature) else {
        return false;
    };
    vk.verify(&preimage, &sig).is_ok()
}

#[cfg(feature = "zk-receipts")]
pub mod zk {
    //! Optional zk-SNARK path stub — enable with `--features zk-receipts`.
    pub fn prove_stub(_transcript: &[u8]) -> Vec<u8> {
        b"zk-stub-proof".to_vec()
    }
    pub fn verify_stub(proof: &[u8]) -> bool {
        proof == b"zk-stub-proof"
    }
}
