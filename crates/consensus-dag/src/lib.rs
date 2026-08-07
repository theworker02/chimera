//! Compact Raft-replicated KV store over in-process / mesh log replication.
//!
//! Strongly consistent single-leader KV. Network transport hooks are pluggable;
//! unit tests run a multi-node in-memory cluster.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RaftRole {
    Follower,
    Candidate,
    Leader,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogEntry {
    pub term: u64,
    pub index: u64,
    pub cmd: KvCmd,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum KvCmd {
    Set { key: String, value: Vec<u8> },
    Delete { key: String },
    Noop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppendEntries {
    pub term: u64,
    pub leader_id: u64,
    pub prev_log_index: u64,
    pub prev_log_term: u64,
    pub entries: Vec<LogEntry>,
    pub leader_commit: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppendEntriesResp {
    pub term: u64,
    pub success: bool,
    pub match_index: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestVote {
    pub term: u64,
    pub candidate_id: u64,
    pub last_log_index: u64,
    pub last_log_term: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestVoteResp {
    pub term: u64,
    pub vote_granted: bool,
}

pub struct RaftNode {
    pub id: u64,
    pub role: RaftRole,
    pub current_term: u64,
    pub voted_for: Option<u64>,
    pub log: Vec<LogEntry>,
    pub commit_index: u64,
    pub last_applied: u64,
    pub kv: HashMap<String, Vec<u8>>,
    pub peers: Vec<u64>,
    /// next_index per peer
    pub next_index: HashMap<u64, u64>,
    pub match_index: HashMap<u64, u64>,
}

impl RaftNode {
    pub fn new(id: u64, peers: Vec<u64>) -> Self {
        let mut next_index = HashMap::new();
        let mut match_index = HashMap::new();
        for p in &peers {
            next_index.insert(*p, 1);
            match_index.insert(*p, 0);
        }
        Self {
            id,
            role: RaftRole::Follower,
            current_term: 0,
            voted_for: None,
            log: vec![LogEntry {
                term: 0,
                index: 0,
                cmd: KvCmd::Noop,
            }],
            commit_index: 0,
            last_applied: 0,
            kv: HashMap::new(),
            peers,
            next_index,
            match_index,
        }
    }

    pub fn last_log_index(&self) -> u64 {
        self.log.last().map(|e| e.index).unwrap_or(0)
    }

    pub fn last_log_term(&self) -> u64 {
        self.log.last().map(|e| e.term).unwrap_or(0)
    }

    pub fn become_leader(&mut self) {
        self.role = RaftRole::Leader;
        let next = self.last_log_index() + 1;
        for p in &self.peers {
            self.next_index.insert(*p, next);
            self.match_index.insert(*p, 0);
        }
    }

    pub fn start_election(&mut self) -> RequestVote {
        self.role = RaftRole::Candidate;
        self.current_term += 1;
        self.voted_for = Some(self.id);
        RequestVote {
            term: self.current_term,
            candidate_id: self.id,
            last_log_index: self.last_log_index(),
            last_log_term: self.last_log_term(),
        }
    }

    pub fn handle_request_vote(&mut self, req: &RequestVote) -> RequestVoteResp {
        if req.term > self.current_term {
            self.current_term = req.term;
            self.role = RaftRole::Follower;
            self.voted_for = None;
        }
        let up_to_date = req.last_log_term > self.last_log_term()
            || (req.last_log_term == self.last_log_term()
                && req.last_log_index >= self.last_log_index());
        let grant = req.term == self.current_term
            && up_to_date
            && (self.voted_for.is_none() || self.voted_for == Some(req.candidate_id));
        if grant {
            self.voted_for = Some(req.candidate_id);
            self.role = RaftRole::Follower;
        }
        RequestVoteResp {
            term: self.current_term,
            vote_granted: grant,
        }
    }

    pub fn propose(&mut self, cmd: KvCmd) -> Option<LogEntry> {
        if self.role != RaftRole::Leader {
            return None;
        }
        let entry = LogEntry {
            term: self.current_term,
            index: self.last_log_index() + 1,
            cmd,
        };
        self.log.push(entry.clone());
        Some(entry)
    }

    pub fn append_entries_for(&self, peer: u64) -> AppendEntries {
        let next = *self.next_index.get(&peer).unwrap_or(&1);
        let prev_index = next.saturating_sub(1);
        let prev_term = self
            .log
            .iter()
            .find(|e| e.index == prev_index)
            .map(|e| e.term)
            .unwrap_or(0);
        let entries: Vec<_> = self
            .log
            .iter()
            .filter(|e| e.index >= next)
            .cloned()
            .collect();
        AppendEntries {
            term: self.current_term,
            leader_id: self.id,
            prev_log_index: prev_index,
            prev_log_term: prev_term,
            entries,
            leader_commit: self.commit_index,
        }
    }

    pub fn handle_append_entries(&mut self, req: &AppendEntries) -> AppendEntriesResp {
        if req.term < self.current_term {
            return AppendEntriesResp {
                term: self.current_term,
                success: false,
                match_index: self.last_log_index(),
            };
        }
        if req.term > self.current_term {
            self.current_term = req.term;
            self.voted_for = None;
        }
        self.role = RaftRole::Follower;

        let prev_ok = self
            .log
            .iter()
            .any(|e| e.index == req.prev_log_index && e.term == req.prev_log_term)
            || (req.prev_log_index == 0 && req.prev_log_term == 0);
        if !prev_ok {
            return AppendEntriesResp {
                term: self.current_term,
                success: false,
                match_index: self.last_log_index(),
            };
        }

        // Truncate conflicting suffix
        self.log.retain(|e| e.index <= req.prev_log_index);
        for e in &req.entries {
            if !self.log.iter().any(|x| x.index == e.index) {
                self.log.push(e.clone());
            }
        }
        self.log.sort_by_key(|e| e.index);

        if req.leader_commit > self.commit_index {
            self.commit_index = req.leader_commit.min(self.last_log_index());
        }
        self.apply_committed();
        AppendEntriesResp {
            term: self.current_term,
            success: true,
            match_index: self.last_log_index(),
        }
    }

    pub fn on_append_resp(&mut self, peer: u64, resp: &AppendEntriesResp) {
        if resp.term > self.current_term {
            self.current_term = resp.term;
            self.role = RaftRole::Follower;
            self.voted_for = None;
            return;
        }
        if self.role != RaftRole::Leader {
            return;
        }
        if resp.success {
            self.match_index.insert(peer, resp.match_index);
            self.next_index.insert(peer, resp.match_index + 1);
            self.advance_commit();
        } else {
            let ni = self.next_index.entry(peer).or_insert(1);
            *ni = (*ni).saturating_sub(1).max(1);
        }
    }

    fn advance_commit(&mut self) {
        let last = self.last_log_index();
        for n in (self.commit_index + 1)..=last {
            let replicas = 1 + self
                .match_index
                .values()
                .filter(|&&m| m >= n)
                .count();
            let majority = (self.peers.len() + 1) / 2 + 1;
            let term_ok = self
                .log
                .iter()
                .find(|e| e.index == n)
                .map(|e| e.term == self.current_term)
                .unwrap_or(false);
            if replicas >= majority && term_ok {
                self.commit_index = n;
            }
        }
        self.apply_committed();
    }

    pub fn apply_committed(&mut self) {
        while self.last_applied < self.commit_index {
            self.last_applied += 1;
            if let Some(e) = self.log.iter().find(|e| e.index == self.last_applied).cloned() {
                match e.cmd {
                    KvCmd::Set { key, value } => {
                        self.kv.insert(key, value);
                    }
                    KvCmd::Delete { key } => {
                        self.kv.remove(&key);
                    }
                    KvCmd::Noop => {}
                }
            }
        }
    }

    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.kv.get(key).cloned()
    }
}

/// Shared handle for gateway / mgmt.
#[derive(Clone)]
pub struct KvStore {
    inner: std::sync::Arc<RwLock<RaftNode>>,
    applies: std::sync::Arc<AtomicU64>,
}

impl KvStore {
    pub fn leader(id: u64, peers: Vec<u64>) -> Self {
        let mut n = RaftNode::new(id, peers);
        n.become_leader();
        n.current_term = 1;
        Self {
            inner: std::sync::Arc::new(RwLock::new(n)),
            applies: std::sync::Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn follower(id: u64, peers: Vec<u64>, term: u64) -> Self {
        let mut n = RaftNode::new(id, peers);
        n.current_term = term;
        Self {
            inner: std::sync::Arc::new(RwLock::new(n)),
            applies: std::sync::Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn set(&self, key: impl Into<String>, value: Vec<u8>) -> anyhow::Result<()> {
        let mut g = self.inner.write();
        let entry = g
            .propose(KvCmd::Set {
                key: key.into(),
                value,
            })
            .ok_or_else(|| anyhow::anyhow!("not leader"))?;
        // Single-node / lab: commit immediately (multi-node uses replicate()).
        if g.peers.is_empty() {
            g.commit_index = entry.index;
            g.apply_committed();
            self.applies.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }

    /// Multi-peer lab: propose without immediate commit (call `replicate_to` then `apply`).
    pub fn propose_set(&self, key: impl Into<String>, value: Vec<u8>) -> anyhow::Result<()> {
        let mut g = self.inner.write();
        g.propose(KvCmd::Set {
            key: key.into(),
            value,
        })
        .ok_or_else(|| anyhow::anyhow!("not leader"))?;
        Ok(())
    }

    pub fn apply(&self) {
        self.inner.write().apply_committed();
    }

    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.inner.read().get(key)
    }

    pub fn replicate_to(&self, follower: &KvStore) {
        let (peer_id, msg) = {
            let g = self.inner.read();
            let peer = follower.inner.read().id;
            (peer, g.append_entries_for(peer))
        };
        let resp = follower.inner.write().handle_append_entries(&msg);
        self.inner.write().on_append_resp(peer_id, &resp);
        self.applies.fetch_add(1, Ordering::Relaxed);
    }

    pub fn is_leader(&self) -> bool {
        self.inner.read().role == RaftRole::Leader
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_node_kv() {
        let kv = KvStore::leader(1, vec![]);
        kv.set("a", b"1".to_vec()).unwrap();
        assert_eq!(kv.get("a").unwrap(), b"1");
    }

    #[test]
    fn three_node_replication() {
        let leader = KvStore::leader(1, vec![2, 3]);
        let f2 = {
            let mut n = RaftNode::new(2, vec![1, 3]);
            n.current_term = 1;
            KvStore {
                inner: std::sync::Arc::new(RwLock::new(n)),
                applies: std::sync::Arc::new(AtomicU64::new(0)),
            }
        };
        let f3 = {
            let mut n = RaftNode::new(3, vec![1, 2]);
            n.current_term = 1;
            KvStore {
                inner: std::sync::Arc::new(RwLock::new(n)),
                applies: std::sync::Arc::new(AtomicU64::new(0)),
            }
        };
        {
            let mut g = leader.inner.write();
            g.propose(KvCmd::Set {
                key: "k".into(),
                value: b"v".to_vec(),
            })
            .unwrap();
        }
        leader.replicate_to(&f2);
        leader.replicate_to(&f3);
        // Commit advances after majority; ship empty AppendEntries so followers learn leader_commit.
        leader.replicate_to(&f2);
        leader.replicate_to(&f3);
        assert_eq!(leader.inner.read().commit_index, 1);
        leader.inner.write().apply_committed();
        f2.inner.write().apply_committed();
        f3.inner.write().apply_committed();
        assert_eq!(leader.get("k").unwrap(), b"v");
        assert_eq!(f2.get("k").unwrap(), b"v");
        assert_eq!(f3.get("k").unwrap(), b"v");
    }

    #[test]
    fn election_grants_vote() {
        let mut a = RaftNode::new(1, vec![2]);
        let mut b = RaftNode::new(2, vec![1]);
        let req = a.start_election();
        let resp = b.handle_request_vote(&req);
        assert!(resp.vote_granted);
    }
}
