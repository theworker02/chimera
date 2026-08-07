//! ChimeraMEM — decentralized memory fabric & live Wasm migration.

pub mod crdt;
pub mod dsm;
pub mod migrate;
pub mod tiering;

pub use crdt::{GCounter, LwwRegister, OrSet, VectorClock};
pub use dsm::{DsmRegion, MemoryFabric, PageId};
pub use migrate::{MigrationManager, MigrationState};
pub use tiering::{MemoryTier, TieringPolicy};

use std::sync::Arc;

use parking_lot::Mutex;

use transport_quic::protocol::NodeId;

#[derive(Clone)]
pub struct ChimeraMem {
    pub fabric: Arc<Mutex<MemoryFabric>>,
    pub migration: Arc<Mutex<MigrationManager>>,
    pub tiers: Arc<Mutex<TieringPolicy>>,
}

impl ChimeraMem {
    pub fn new(local: NodeId, page_size: usize) -> Self {
        Self {
            fabric: Arc::new(Mutex::new(MemoryFabric::new(local, page_size))),
            migration: Arc::new(Mutex::new(MigrationManager::new(local))),
            tiers: Arc::new(Mutex::new(TieringPolicy::default())),
        }
    }

    pub fn stats(&self) -> MemStats {
        let fabric = self.fabric.lock();
        MemStats {
            regions: fabric.region_count(),
            local_pages: fabric.local_page_count(),
            remote_faults: fabric.fault_count(),
            migrations: self.migration.lock().completed(),
            hot_pages: self.tiers.lock().hot_count(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MemStats {
    pub regions: usize,
    pub local_pages: usize,
    pub remote_faults: u64,
    pub migrations: u64,
    pub hot_pages: usize,
}
