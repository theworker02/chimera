//! Portable user-space DSM (soft page table). Linux userfaultfd behind feature flag.

use std::collections::HashMap;

use transport_quic::protocol::NodeId;

pub type PageId = u64;

#[derive(Debug, Clone)]
pub struct Page {
    pub owner: NodeId,
    pub lease_seq: u64,
    pub data: Option<Vec<u8>>,
    pub hotness: u32,
}

pub struct DsmRegion {
    pub id: u64,
    pub page_size: usize,
    pub pages: HashMap<PageId, Page>,
}

pub struct MemoryFabric {
    local: NodeId,
    page_size: usize,
    regions: HashMap<u64, DsmRegion>,
    next_region: u64,
    faults: u64,
}

impl MemoryFabric {
    pub fn new(local: NodeId, page_size: usize) -> Self {
        Self {
            local,
            page_size: page_size.max(4096),
            regions: HashMap::new(),
            next_region: 1,
            faults: 0,
        }
    }

    pub fn create_region(&mut self, pages: u64) -> u64 {
        let id = self.next_region;
        self.next_region += 1;
        let mut region = DsmRegion {
            id,
            page_size: self.page_size,
            pages: HashMap::new(),
        };
        for p in 0..pages {
            region.pages.insert(
                p,
                Page {
                    owner: self.local,
                    lease_seq: 0,
                    data: Some(vec![0u8; self.page_size]),
                    hotness: 0,
                },
            );
        }
        self.regions.insert(id, region);
        id
    }

    /// Soft page-fault: if page not local, return miss for QUIC fetch.
    pub fn access(&mut self, region: u64, page: PageId) -> AccessResult {
        let Some(reg) = self.regions.get_mut(&region) else {
            return AccessResult::Invalid;
        };
        let Some(pg) = reg.pages.get_mut(&page) else {
            return AccessResult::Invalid;
        };
        pg.hotness = pg.hotness.saturating_add(1);
        if pg.owner == self.local {
            if let Some(data) = &pg.data {
                return AccessResult::Hit(data.clone());
            }
        }
        self.faults += 1;
        AccessResult::RemoteFault {
            owner: pg.owner,
            page_size: reg.page_size,
        }
    }

    pub fn fill_page(&mut self, region: u64, page: PageId, owner: NodeId, data: Vec<u8>, lease: u64) {
        if let Some(reg) = self.regions.get_mut(&region) {
            reg.pages.insert(
                page,
                Page {
                    owner,
                    lease_seq: lease,
                    data: Some(data),
                    hotness: 1,
                },
            );
        }
    }

    pub fn transfer_ownership(&mut self, region: u64, page: PageId, new_owner: NodeId, lease: u64) {
        if let Some(reg) = self.regions.get_mut(&region) {
            if let Some(pg) = reg.pages.get_mut(&page) {
                pg.owner = new_owner;
                pg.lease_seq = lease;
                if new_owner != self.local {
                    // Keep data as peer cache until tiering evicts.
                }
            }
        }
    }

    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    pub fn local_page_count(&self) -> usize {
        self.regions
            .values()
            .flat_map(|r| r.pages.values())
            .filter(|p| p.owner == self.local)
            .count()
    }

    pub fn fault_count(&self) -> u64 {
        self.faults
    }

    pub fn page_size(&self) -> usize {
        self.page_size
    }

    pub fn export_page(&self, region: u64, page: PageId) -> Option<(NodeId, Vec<u8>, u64)> {
        let reg = self.regions.get(&region)?;
        let pg = reg.pages.get(&page)?;
        Some((pg.owner, pg.data.clone().unwrap_or_default(), pg.lease_seq))
    }
}

#[derive(Debug)]
pub enum AccessResult {
    Hit(Vec<u8>),
    RemoteFault { owner: NodeId, page_size: usize },
    Invalid,
}

/// Linux userfaultfd hook (feature-gated).
#[cfg(all(target_os = "linux", feature = "userfaultfd"))]
pub mod uffd {
    pub fn available() -> bool {
        true
    }
}

#[cfg(not(all(target_os = "linux", feature = "userfaultfd")))]
pub mod uffd {
    pub fn available() -> bool {
        false
    }
}
