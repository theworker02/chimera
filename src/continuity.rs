//! Continuity plane — replicate Wasm frames / memory segments; recover after partition.
//!
//! Honesty: demonstrates **zero data loss via deterministic replay of replicated logs**,
//! not literal zero packet loss on the wire.

use std::collections::HashMap;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::raft_kv::KvStore;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FrameReplica {
    pub task_id: String,
    pub seq: u64,
    pub wasm_frame: Vec<u8>,
    pub memory_segment: Vec<u8>,
    pub transcript_hash: [u8; 32],
}

#[derive(Clone)]
pub struct ContinuityPlane {
    peers: std::sync::Arc<RwLock<HashMap<u64, Vec<FrameReplica>>>>,
    replication_factor: usize,
}

impl ContinuityPlane {
    pub fn new(peer_ids: &[u64], replication_factor: usize) -> Self {
        let mut peers = HashMap::new();
        for id in peer_ids {
            peers.insert(*id, Vec::new());
        }
        Self {
            peers: std::sync::Arc::new(RwLock::new(peers)),
            replication_factor: replication_factor.max(1),
        }
    }

    pub fn replicate(&self, primary: u64, frame: FrameReplica) {
        let mut g = self.peers.write();
        let mut targets: Vec<u64> = g.keys().copied().filter(|id| *id != primary).collect();
        targets.sort_unstable();
        let n = self
            .replication_factor
            .saturating_sub(1)
            .max(1)
            .min(targets.len());
        targets.truncate(n);
        if let Some(log) = g.get_mut(&primary) {
            log.push(frame.clone());
        }
        for t in targets {
            if let Some(log) = g.get_mut(&t) {
                log.push(frame.clone());
            }
        }
    }

    pub fn recover_after_kill(&self, killed: &[u64], task_id: &str) -> Option<FrameReplica> {
        let g = self.peers.read();
        let mut best: Option<FrameReplica> = None;
        for (id, log) in g.iter() {
            if killed.contains(id) {
                continue;
            }
            for f in log.iter().filter(|f| f.task_id == task_id) {
                if best.as_ref().map(|b| f.seq > b.seq).unwrap_or(true) {
                    best = Some(f.clone());
                }
            }
        }
        best
    }

    pub fn replay_matches(original: &FrameReplica, recovered: &FrameReplica) -> bool {
        original.transcript_hash == recovered.transcript_hash
            && original.wasm_frame == recovered.wasm_frame
            && original.memory_segment == recovered.memory_segment
    }
}

pub fn hash_frame(wasm: &[u8], mem: &[u8]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(wasm);
    h.update(mem);
    *h.finalize().as_bytes()
}

pub fn raft_replicate_all(leader: &KvStore, followers: &[KvStore]) {
    for f in followers {
        leader.replicate_to(f);
    }
    for f in followers {
        leader.replicate_to(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partition_recovery_preserves_state() {
        let plane = ContinuityPlane::new(&[1, 2, 3], 3);
        let mem = b"linear-memory-segment".to_vec();
        let frame = b"wasm-frame-bytes".to_vec();
        let hash = hash_frame(&frame, &mem);
        let rep = FrameReplica {
            task_id: "t1".into(),
            seq: 7,
            wasm_frame: frame,
            memory_segment: mem,
            transcript_hash: hash,
        };
        plane.replicate(1, rep.clone());
        let recovered = plane.recover_after_kill(&[1, 2], "t1").unwrap();
        assert!(ContinuityPlane::replay_matches(&rep, &recovered));
        assert_eq!(recovered.seq, 7);
    }

    #[test]
    fn raft_kv_survives_follower_loss() {
        let leader = KvStore::leader(1, vec![2, 3]);
        let f2 = KvStore::follower(2, vec![1, 3], 1);
        let f3 = KvStore::follower(3, vec![1, 2], 1);
        leader
            .propose_set("continuity", b"alive".to_vec())
            .unwrap();
        raft_replicate_all(&leader, &[f2.clone(), f3.clone()]);
        leader.apply();
        f2.apply();
        assert_eq!(leader.get("continuity").unwrap(), b"alive");
        assert_eq!(f2.get("continuity").unwrap(), b"alive");
        let _ = f3;
    }
}
