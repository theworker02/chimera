//! ChimeraFS — content-addressed VFS over the QUIC mesh.

pub mod cas;
pub mod dht;
pub mod mount;
pub mod prefetch;

pub use cas::{BlockHash, ContentStore, MerkleAsset};
pub use dht::BlockDht;
pub use mount::VirtualMount;
pub use prefetch::Prefetcher;

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::protocol::NodeId;

#[derive(Clone)]
pub struct ChimeraFs {
    pub store: Arc<ContentStore>,
    pub dht: Arc<Mutex<BlockDht>>,
    pub mount: Arc<VirtualMount>,
    pub prefetcher: Arc<Prefetcher>,
}

impl ChimeraFs {
    pub fn open(
        root: impl AsRef<Path>,
        local_id: NodeId,
        block_size: usize,
        cache_blocks: usize,
    ) -> anyhow::Result<Self> {
        let root = root.as_ref().join("chimerafs");
        std::fs::create_dir_all(&root)?;
        let store = Arc::new(ContentStore::new(&root, block_size, cache_blocks)?);
        let dht = Arc::new(Mutex::new(BlockDht::new(local_id)));
        let mount = Arc::new(VirtualMount::new(store.clone()));
        let prefetcher = Arc::new(Prefetcher::new(store.clone(), dht.clone()));
        Ok(Self {
            store,
            dht,
            mount,
            prefetcher,
        })
    }

    pub fn ingest_bytes(&self, name: &str, data: &[u8]) -> anyhow::Result<MerkleAsset> {
        let asset = self.store.put_asset(name, data)?;
        {
            let mut dht = self.dht.lock();
            for h in &asset.blocks {
                dht.announce(*h);
            }
        }
        self.mount.register(&asset);
        Ok(asset)
    }

    pub fn list_assets(&self) -> Vec<MerkleAsset> {
        self.mount.list_assets()
    }

    pub fn read_asset_by_root(&self, root_hex: &str) -> anyhow::Result<Option<(MerkleAsset, Vec<u8>)>> {
        let bytes = hex::decode(root_hex)?;
        if bytes.len() != 32 {
            anyhow::bail!("root hash must be 32 bytes");
        }
        let mut root = [0u8; 32];
        root.copy_from_slice(&bytes);
        if let Some(asset) = self.mount.find_by_root(&root) {
            let data = self.store.read_asset_bytes(&asset)?;
            return Ok(Some((asset, data)));
        }
        if let Some(asset) = self.store.load_asset(&root)? {
            let data = self.store.read_asset_bytes(&asset)?;
            return Ok(Some((asset, data)));
        }
        Ok(None)
    }

    pub fn prefetch(&self, hashes: &[[u8; 32]]) -> anyhow::Result<()> {
        self.prefetcher.prefetch(hashes)
    }

    pub fn stats(&self) -> FsStats {
        let (hits, misses, blocks) = self.store.cache_stats();
        FsStats {
            cache_hits: hits,
            cache_misses: misses,
            blocks_stored: blocks,
            dht_entries: self.dht.lock().len(),
            mounted_assets: self.mount.asset_count(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FsStats {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub blocks_stored: u64,
    pub dht_entries: usize,
    pub mounted_assets: usize,
}
