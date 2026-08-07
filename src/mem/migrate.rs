//! Live Wasm state migration over QUIC (linear memory authoritative).

use std::collections::HashMap;

use crate::protocol::{NodeId, TaskId, WasmSnapshotMeta};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationState {
    Idle,
    Offering,
    Transferring { chunks: u32 },
    Completing,
    Done,
    Failed(String),
}

pub struct MigrationSession {
    pub task_id: TaskId,
    pub target: NodeId,
    pub meta: WasmSnapshotMeta,
    pub blob: Vec<u8>,
    pub state: MigrationState,
    pub seq_sent: u32,
}

pub struct MigrationManager {
    local: NodeId,
    sessions: HashMap<TaskId, MigrationSession>,
    completed: u64,
}

impl MigrationManager {
    pub fn new(local: NodeId) -> Self {
        Self {
            local,
            sessions: HashMap::new(),
            completed: 0,
        }
    }

    pub fn begin(
        &mut self,
        task_id: TaskId,
        target: NodeId,
        meta: WasmSnapshotMeta,
        blob: Vec<u8>,
    ) {
        self.sessions.insert(
            task_id,
            MigrationSession {
                task_id,
                target,
                meta,
                blob,
                state: MigrationState::Offering,
                seq_sent: 0,
            },
        );
    }

    pub fn next_chunk(&mut self, task_id: TaskId, chunk_size: usize) -> Option<(u32, Vec<u8>)> {
        let s = self.sessions.get_mut(&task_id)?;
        s.state = MigrationState::Transferring {
            chunks: s.seq_sent + 1,
        };
        let start = (s.seq_sent as usize).saturating_mul(chunk_size);
        if start >= s.blob.len() {
            s.state = MigrationState::Completing;
            return None;
        }
        let end = (start + chunk_size).min(s.blob.len());
        let data = s.blob[start..end].to_vec();
        let seq = s.seq_sent;
        s.seq_sent += 1;
        Some((seq, data))
    }

    pub fn finish(&mut self, task_id: TaskId) {
        if let Some(mut s) = self.sessions.remove(&task_id) {
            s.state = MigrationState::Done;
            self.completed += 1;
        }
    }

    pub fn completed(&self) -> u64 {
        self.completed
    }

    pub fn local_id(&self) -> NodeId {
        self.local
    }

    pub fn compress_delta(base: &[u8], next: &[u8]) -> Vec<u8> {
        // Lightweight XOR delta for similar pages.
        let n = base.len().min(next.len());
        let mut out = Vec::with_capacity(n + 8);
        out.extend_from_slice(&(n as u32).to_le_bytes());
        out.extend_from_slice(&(next.len() as u32).to_le_bytes());
        for i in 0..n {
            out.push(base[i] ^ next[i]);
        }
        if next.len() > n {
            out.extend_from_slice(&next[n..]);
        }
        out
    }
}
