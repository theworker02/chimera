//! Hardware-aware memory tiering hints.

use std::collections::{HashMap, HashSet};

use super::dsm::PageId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryTier {
    HotRam,
    PeerCache,
    ColdFs,
    GpuHint,
}

#[derive(Debug, Clone)]
pub struct TieringPolicy {
    hot: HashSet<(u64, PageId)>,
    tiers: HashMap<(u64, PageId), MemoryTier>,
    hot_threshold: u32,
}

impl Default for TieringPolicy {
    fn default() -> Self {
        Self {
            hot: HashSet::new(),
            tiers: HashMap::new(),
            hot_threshold: 8,
        }
    }
}

impl TieringPolicy {
    pub fn observe(&mut self, region: u64, page: PageId, hotness: u32) {
        let key = (region, page);
        let tier = if hotness >= self.hot_threshold {
            self.hot.insert(key);
            MemoryTier::HotRam
        } else if hotness >= 2 {
            self.hot.remove(&key);
            MemoryTier::PeerCache
        } else {
            self.hot.remove(&key);
            MemoryTier::ColdFs
        };
        self.tiers.insert(key, tier);
    }

    pub fn tier_of(&self, region: u64, page: PageId) -> MemoryTier {
        self.tiers
            .get(&(region, page))
            .copied()
            .unwrap_or(MemoryTier::ColdFs)
    }

    pub fn hot_count(&self) -> usize {
        self.hot.len()
    }

    pub fn mark_gpu_hint(&mut self, region: u64, page: PageId) {
        self.tiers.insert((region, page), MemoryTier::GpuHint);
    }
}
