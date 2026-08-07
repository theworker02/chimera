//! BLAKE3-chained append-only transaction log for deterministic replay.

use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    HashMismatch,
    Empty,
    Decode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TxEntry {
    pub seq: u64,
    pub kind: u16,
    pub payload: Vec<u8>,
    /// BLAKE3(prev_hash || seq || kind || payload)
    pub hash: [u8; 32],
}

#[derive(Debug, Clone, Default)]
pub struct TxLog {
    entries: Vec<TxEntry>,
    tip: [u8; 32],
}

impl TxLog {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            tip: [0u8; 32],
        }
    }

    pub fn tip(&self) -> [u8; 32] {
        self.tip
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[TxEntry] {
        &self.entries
    }

    pub fn append(&mut self, kind: u16, payload: &[u8]) -> &TxEntry {
        let seq = self.entries.len() as u64;
        let hash = chain_hash(&self.tip, seq, kind, payload);
        let entry = TxEntry {
            seq,
            kind,
            payload: payload.to_vec(),
            hash,
        };
        self.tip = hash;
        self.entries.push(entry);
        self.entries.last().unwrap()
    }

    /// Verify the entire chain; returns Ok(tip) or first mismatch.
    pub fn verify(&self) -> Result<[u8; 32], ReplayError> {
        let mut tip = [0u8; 32];
        for e in &self.entries {
            let h = chain_hash(&tip, e.seq, e.kind, &e.payload);
            if h != e.hash {
                return Err(ReplayError::HashMismatch);
            }
            tip = h;
        }
        if tip != self.tip {
            return Err(ReplayError::HashMismatch);
        }
        Ok(tip)
    }

    /// Rebuild state by replaying entries through `apply` in order.
    pub fn replay<F, S>(&self, mut state: S, mut apply: F) -> Result<S, ReplayError>
    where
        F: FnMut(&mut S, &TxEntry),
    {
        self.verify()?;
        for e in &self.entries {
            apply(&mut state, e);
        }
        Ok(state)
    }

    /// Merge a neighbor replica: accept suffix entries that extend our tip.
    pub fn merge_replica(&mut self, other: &[TxEntry]) -> Result<usize, ReplayError> {
        let mut added = 0;
        for e in other {
            if e.seq < self.entries.len() as u64 {
                // Already have — check hash agreement.
                if let Some(mine) = self.entries.get(e.seq as usize) {
                    if mine.hash != e.hash {
                        return Err(ReplayError::HashMismatch);
                    }
                }
                continue;
            }
            if e.seq != self.entries.len() as u64 {
                // Gap — refuse (scaffolding: no complex fork choice yet).
                return Err(ReplayError::HashMismatch);
            }
            let expected = chain_hash(&self.tip, e.seq, e.kind, &e.payload);
            if expected != e.hash {
                return Err(ReplayError::HashMismatch);
            }
            self.tip = e.hash;
            self.entries.push(e.clone());
            added += 1;
        }
        Ok(added)
    }

    pub fn encode(&self) -> Result<Vec<u8>, ReplayError> {
        postcard::to_allocvec(&self.entries).map_err(|_| ReplayError::Decode)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ReplayError> {
        let entries: Vec<TxEntry> = postcard::from_bytes(bytes).map_err(|_| ReplayError::Decode)?;
        let mut log = Self::new();
        for e in entries {
            let expected = chain_hash(&log.tip, e.seq, e.kind, &e.payload);
            if expected != e.hash {
                return Err(ReplayError::HashMismatch);
            }
            log.tip = e.hash;
            log.entries.push(e);
        }
        Ok(log)
    }
}

fn chain_hash(prev: &[u8; 32], seq: u64, kind: u16, payload: &[u8]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(prev);
    h.update(&seq.to_le_bytes());
    h.update(&kind.to_le_bytes());
    h.update(payload);
    *h.finalize().as_bytes()
}

/// Transaction kinds used by CNK demos / integration.
pub mod kinds {
    pub const TASK_SPAWN: u16 = 1;
    pub const TASK_COMPLETE: u16 = 2;
    pub const MEM_WRITE: u16 = 3;
    pub const CHECKPOINT: u16 = 4;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_recovers_counter() {
        let mut log = TxLog::new();
        log.append(kinds::TASK_SPAWN, &[1]);
        log.append(kinds::TASK_COMPLETE, &[1, 2]);
        log.append(kinds::MEM_WRITE, b"hello");
        let tip = log.verify().unwrap();
        assert_eq!(tip, log.tip());

        let state = log
            .replay(0u64, |s, e| {
                *s += e.payload.len() as u64;
            })
            .unwrap();
        assert_eq!(state, 1 + 2 + 5);

        let bytes = log.encode().unwrap();
        let restored = TxLog::decode(&bytes).unwrap();
        assert_eq!(restored.tip(), log.tip());
    }

    #[test]
    fn neighbor_merge() {
        let mut a = TxLog::new();
        a.append(1, b"a");
        let mut b = a.clone();
        b.append(2, b"b");
        let added = a.merge_replica(b.entries()).unwrap();
        assert_eq!(added, 1);
        assert_eq!(a.len(), 2);
    }
}
