//! Tamper-evident audit trail: BLAKE3-chained + ed25519-signed JSONL.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

fn now_ms() -> u64 { use std::time::{SystemTime, UNIX_EPOCH}; SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub seq: u64,
    pub ts_ms: u64,
    pub actor: String,
    pub action: String,
    pub resource: String,
    pub detail: String,
    pub prev_hash: [u8; 32],
    pub hash: [u8; 32],
    pub signature: Vec<u8>,
    pub public_key: [u8; 32],
}

#[derive(Clone)]
pub struct AuditLog {
    path: PathBuf,
    signing: SigningKey,
    verifying: VerifyingKey,
    tip: [u8; 32],
    seq: u64,
}

impl AuditLog {
    pub fn open(dir: impl AsRef<Path>) -> anyhow::Result<Self> {
        let dir = dir.as_ref().join("audit");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("trail.jsonl");
        let key_path = dir.join("audit.key");
        let signing = if key_path.exists() {
            let bytes = std::fs::read(&key_path)?;
            let arr: [u8; 32] = bytes
                .as_slice()
                .try_into()
                .map_err(|_| anyhow::anyhow!("bad audit key"))?;
            SigningKey::from_bytes(&arr)
        } else {
            let sk = SigningKey::generate(&mut OsRng);
            std::fs::write(&key_path, sk.to_bytes())?;
            sk
        };
        let verifying = signing.verifying_key();
        let mut log = Self {
            path,
            signing,
            verifying,
            tip: [0u8; 32],
            seq: 0,
        };
        log.replay_tip()?;
        Ok(log)
    }

    fn replay_tip(&mut self) -> anyhow::Result<()> {
        if !self.path.exists() {
            return Ok(());
        }
        let f = std::fs::File::open(&self.path)?;
        for line in BufReader::new(f).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let ev: AuditEvent = serde_json::from_str(&line)?;
            self.tip = ev.hash;
            self.seq = ev.seq + 1;
        }
        Ok(())
    }

    pub fn append(
        &mut self,
        actor: &str,
        action: &str,
        resource: &str,
        detail: &str,
    ) -> anyhow::Result<AuditEvent> {
        let mut preimage = Vec::new();
        preimage.extend_from_slice(&self.tip);
        preimage.extend_from_slice(&self.seq.to_le_bytes());
        preimage.extend_from_slice(actor.as_bytes());
        preimage.extend_from_slice(action.as_bytes());
        preimage.extend_from_slice(resource.as_bytes());
        preimage.extend_from_slice(detail.as_bytes());
        let hash = *blake3::hash(&preimage).as_bytes();
        let sig = self.signing.sign(&hash);
        let ev = AuditEvent {
            seq: self.seq,
            ts_ms: now_ms(),
            actor: actor.into(),
            action: action.into(),
            resource: resource.into(),
            detail: detail.into(),
            prev_hash: self.tip,
            hash,
            signature: sig.to_bytes().to_vec(),
            public_key: self.verifying.to_bytes(),
        };
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(f, "{}", serde_json::to_string(&ev)?)?;
        self.tip = hash;
        self.seq += 1;
        Ok(ev)
    }

    pub fn verify_file(path: impl AsRef<Path>) -> anyhow::Result<u64> {
        let f = std::fs::File::open(path.as_ref())?;
        let mut tip = [0u8; 32];
        let mut count = 0u64;
        for line in BufReader::new(f).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let ev: AuditEvent = serde_json::from_str(&line)?;
            if ev.prev_hash != tip {
                anyhow::bail!("hash chain break at seq {}", ev.seq);
            }
            let mut preimage = Vec::new();
            preimage.extend_from_slice(&ev.prev_hash);
            preimage.extend_from_slice(&ev.seq.to_le_bytes());
            preimage.extend_from_slice(ev.actor.as_bytes());
            preimage.extend_from_slice(ev.action.as_bytes());
            preimage.extend_from_slice(ev.resource.as_bytes());
            preimage.extend_from_slice(ev.detail.as_bytes());
            let expect = *blake3::hash(&preimage).as_bytes();
            if expect != ev.hash {
                anyhow::bail!("content hash mismatch at seq {}", ev.seq);
            }
            let vk = VerifyingKey::from_bytes(&ev.public_key)?;
            let sig = Signature::from_slice(&ev.signature)?;
            vk.verify(&ev.hash, &sig)?;
            tip = ev.hash;
            count += 1;
        }
        Ok(count)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn len(&self) -> u64 {
        self.seq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_verify() {
        let dir = std::env::temp_dir().join(format!("chimera-audit-{}", uuid::Uuid::new_v4()));
        let mut log = AuditLog::open(&dir).unwrap();
        log.append("admin", "job.submit", "job/1", "ok").unwrap();
        log.append("admin", "asset.read", "cas/abc", "ok").unwrap();
        let n = AuditLog::verify_file(log.path()).unwrap();
        assert_eq!(n, 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
