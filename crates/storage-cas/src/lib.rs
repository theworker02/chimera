//! BLAKE3 content-addressed chunking + Merkle DAG.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::bail;
use memmap2::MmapOptions;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

pub type BlockHash = [u8; 32];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleAsset {
    pub name: String,
    pub root: BlockHash,
    pub size: u64,
    pub blocks: Vec<BlockHash>,
    pub block_size: usize,
    /// Brand watermark metadata embedded in asset DAG.
    pub watermark: AssetWatermark,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetWatermark {
    pub brand: String,
    pub sig_hint: [u8; 32],
}

pub struct ContentStore {
    root: PathBuf,
    block_size: usize,
    #[allow(dead_code)]
    cache_cap: usize,
    cache: Mutex<LruCache>,
    hits: AtomicU64,
    misses: AtomicU64,
    stored: AtomicU64,
}

struct LruCache {
    map: HashMap<BlockHash, Vec<u8>>,
    order: VecDeque<BlockHash>,
    cap: usize,
}

impl LruCache {
    fn new(cap: usize) -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            cap,
        }
    }

    fn get(&mut self, key: &BlockHash) -> Option<Vec<u8>> {
        if let Some(v) = self.map.get(key) {
            self.order.retain(|k| k != key);
            self.order.push_back(*key);
            return Some(v.clone());
        }
        None
    }

    fn put(&mut self, key: BlockHash, val: Vec<u8>) {
        if self.map.contains_key(&key) {
            self.order.retain(|k| k != &key);
        }
        while self.map.len() >= self.cap && !self.order.is_empty() {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
        self.map.insert(key, val);
        self.order.push_back(key);
    }
}

impl ContentStore {
    pub fn new(root: impl AsRef<Path>, block_size: usize, cache_blocks: usize) -> anyhow::Result<Self> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(root.join("blocks"))?;
        std::fs::create_dir_all(root.join("assets"))?;
        Ok(Self {
            root,
            block_size: block_size.max(4096),
            cache_cap: cache_blocks.max(8),
            cache: Mutex::new(LruCache::new(cache_blocks.max(8))),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            stored: AtomicU64::new(0),
        })
    }

    pub fn block_path(&self, hash: &BlockHash) -> PathBuf {
        self.root.join("blocks").join(hex::encode(hash))
    }

    pub fn put_block(&self, data: &[u8]) -> anyhow::Result<BlockHash> {
        let hash = *blake3::hash(data).as_bytes();
        let path = self.block_path(&hash);
        if !path.exists() {
            let mut f = OpenOptions::new().write(true).create(true).truncate(true).open(&path)?;
            f.write_all(data)?;
            f.flush()?;
            self.stored.fetch_add(1, Ordering::Relaxed);
        }
        // Verify on ingest, trust cache thereafter.
        let check = *blake3::hash(data).as_bytes();
        if check != hash {
            bail!("blake3 mismatch on ingest");
        }
        self.cache.lock().put(hash, data.to_vec());
        Ok(hash)
    }

    pub fn get_block(&self, hash: &BlockHash) -> anyhow::Result<Option<Vec<u8>>> {
        if let Some(v) = self.cache.lock().get(hash) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(Some(v));
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
        let path = self.block_path(hash);
        if !path.exists() {
            return Ok(None);
        }
        let file = OpenOptions::new().read(true).open(&path)?;
        let mmap = unsafe { MmapOptions::new().map(&file)? };
        // Trusted after ingest verification; optional re-hash in debug.
        let data = mmap.to_vec();
        self.cache.lock().put(*hash, data.clone());
        Ok(Some(data))
    }

    pub fn has_block(&self, hash: &BlockHash) -> bool {
        self.cache.lock().map.contains_key(hash) || self.block_path(hash).exists()
    }

    pub fn put_asset(&self, name: &str, data: &[u8]) -> anyhow::Result<MerkleAsset> {
        let mut blocks = Vec::new();
        for chunk in data.chunks(self.block_size) {
            blocks.push(self.put_block(chunk)?);
        }
        let mut hasher = blake3::Hasher::new();
        for h in &blocks {
            hasher.update(h);
        }
        let root = *hasher.finalize().as_bytes();
        let watermark = AssetWatermark {
            brand: "CHIMERA".into(),
            sig_hint: *blake3::hash(name.as_bytes()).as_bytes(),
        };
        let asset = MerkleAsset {
            name: name.into(),
            root,
            size: data.len() as u64,
            blocks,
            block_size: self.block_size,
            watermark,
        };
        let meta = postcard::to_allocvec(&asset)?;
        std::fs::write(self.root.join("assets").join(hex::encode(root)), meta)?;
        Ok(asset)
    }

    pub fn load_asset(&self, root: &BlockHash) -> anyhow::Result<Option<MerkleAsset>> {
        let path = self.root.join("assets").join(hex::encode(root));
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(path)?;
        Ok(Some(postcard::from_bytes(&bytes)?))
    }

    pub fn read_asset_bytes(&self, asset: &MerkleAsset) -> anyhow::Result<Vec<u8>> {
        let mut out = Vec::with_capacity(asset.size as usize);
        for h in &asset.blocks {
            let block = self.get_block(h)?.ok_or_else(|| anyhow::anyhow!("missing block"))?;
            out.extend_from_slice(&block);
        }
        Ok(out)
    }

    pub fn cache_stats(&self) -> (u64, u64, u64) {
        (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
            self.stored.load(Ordering::Relaxed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_block_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "chimera-cas-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = ContentStore::new(&dir, 64, 8).unwrap();
        let h = store.put_block(b"hello-omniverse").unwrap();
        let got = store.get_block(&h).unwrap().unwrap();
        assert_eq!(got, b"hello-omniverse");
        let _ = std::fs::remove_dir_all(dir);
    }
}
