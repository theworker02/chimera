//! Work-aware predictive prefetch into mmap cache.

use std::sync::Arc;

use parking_lot::Mutex;
use tracing::debug;

use super::cas::{BlockHash, ContentStore};
use super::dht::BlockDht;

pub struct Prefetcher {
    store: Arc<ContentStore>,
    dht: Arc<Mutex<BlockDht>>,
}

impl Prefetcher {
    pub fn new(store: Arc<ContentStore>, dht: Arc<Mutex<BlockDht>>) -> Self {
        Self { store, dht }
    }

    /// Ensure blocks are warm in local cache before Wasm starts.
    /// Missing blocks are recorded as DHT lookups (actual fetch via transport in node loop).
    pub fn prefetch(&self, hashes: &[BlockHash]) -> anyhow::Result<()> {
        for h in hashes {
            if self.store.has_block(h) {
                let _ = self.store.get_block(h)?;
                continue;
            }
            let holders = self.dht.lock().nearest(h, 3);
            debug!(
                "prefetch miss {} holders={}",
                hex::encode(&h[..4]),
                holders.len()
            );
        }
        Ok(())
    }

    pub fn missing(&self, hashes: &[BlockHash]) -> Vec<BlockHash> {
        hashes
            .iter()
            .copied()
            .filter(|h| !self.store.has_block(h))
            .collect()
    }
}
