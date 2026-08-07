//! Lightweight gossip-indexed DHT (Kademlia-inspired for LAN/mesh).

use std::collections::{HashMap, HashSet};

use transport_quic::protocol::NodeId;

pub type BlockHash = [u8; 32];

pub struct BlockDht {
    local: NodeId,
    /// block hash → peers believed to hold it
    table: HashMap<BlockHash, HashSet<NodeId>>,
    /// XOR-distance buckets by first byte distance niche
    buckets: Vec<Vec<NodeId>>,
}

impl BlockDht {
    pub fn new(local: NodeId) -> Self {
        Self {
            local,
            table: HashMap::new(),
            buckets: vec![Vec::new(); 16],
        }
    }

    pub fn announce(&mut self, hash: BlockHash) {
        self.table.entry(hash).or_default().insert(self.local);
    }

    pub fn put_provider(&mut self, hash: BlockHash, peer: NodeId) {
        self.table.entry(hash).or_default().insert(peer);
        self.touch_peer(peer);
    }

    pub fn find(&self, hash: &BlockHash) -> Vec<NodeId> {
        self.table
            .get(hash)
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default()
    }

    pub fn nearest(&self, hash: &BlockHash, k: usize) -> Vec<NodeId> {
        let mut holders = self.find(hash);
        if holders.len() >= k {
            holders.truncate(k);
            return holders;
        }
        // Fall back to XOR-close peers from buckets.
        let mut scored: Vec<(u32, NodeId)> = self
            .buckets
            .iter()
            .flatten()
            .copied()
            .map(|id| (xor_dist_prefix(hash, &id), id))
            .collect();
        scored.sort_by_key(|(d, _)| *d);
        for (_, id) in scored {
            if !holders.contains(&id) {
                holders.push(id);
            }
            if holders.len() >= k {
                break;
            }
        }
        holders
    }

    pub fn touch_peer(&mut self, peer: NodeId) {
        let bucket = (peer.0.as_bytes()[0] >> 4) as usize;
        let b = &mut self.buckets[bucket.min(15)];
        if !b.contains(&peer) {
            b.push(peer);
            if b.len() > 32 {
                b.remove(0);
            }
        }
    }

    pub fn remove_peer(&mut self, peer: NodeId) {
        for set in self.table.values_mut() {
            set.remove(&peer);
        }
        for b in &mut self.buckets {
            b.retain(|p| p != &peer);
        }
    }

    pub fn len(&self) -> usize {
        self.table.len()
    }

    pub fn local_holds(&self) -> Vec<BlockHash> {
        self.table
            .iter()
            .filter(|(_, v)| v.contains(&self.local))
            .map(|(k, _)| *k)
            .collect()
    }
}

fn xor_dist_prefix(hash: &BlockHash, id: &NodeId) -> u32 {
    let ib = id.0.as_bytes();
    let mut d = 0u32;
    for i in 0..4 {
        d = (d << 8) | u32::from(hash[i] ^ ib[i]);
    }
    d
}
