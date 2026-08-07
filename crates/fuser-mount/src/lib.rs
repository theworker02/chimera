//! Virtual mount / VFS API (Windows-first; FUSE optional on Unix via feature).

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::RwLock;

use storage_cas::{BlockHash, ContentStore, MerkleAsset};

/// User-space virtual mount. Compute tasks use this instead of OS FUSE on Windows.
pub struct VirtualMount {
    store: Arc<ContentStore>,
    assets: RwLock<HashMap<PathBuf, MerkleAsset>>,
}

impl VirtualMount {
    pub fn new(store: Arc<ContentStore>) -> Self {
        Self {
            store,
            assets: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, asset: &MerkleAsset) {
        let path = PathBuf::from("/chimera").join(&asset.name);
        self.assets.write().insert(path, asset.clone());
    }

    pub fn asset_count(&self) -> usize {
        self.assets.read().len()
    }

    pub fn list(&self) -> Vec<PathBuf> {
        self.assets.read().keys().cloned().collect()
    }

    pub fn list_assets(&self) -> Vec<MerkleAsset> {
        self.assets.read().values().cloned().collect()
    }

    pub fn find_by_root(&self, root: &BlockHash) -> Option<MerkleAsset> {
        self.assets
            .read()
            .values()
            .find(|a| &a.root == root)
            .cloned()
    }

    pub fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let assets = self.assets.read();
        let asset = assets
            .get(path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "not mounted"))?;
        self.store
            .read_asset_bytes(asset)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }

    pub fn open_reader(&self, path: &Path) -> io::Result<VfsFile> {
        let data = self.read(path)?;
        Ok(VfsFile { data, pos: 0 })
    }
}

pub struct VfsFile {
    data: Vec<u8>,
    pos: usize,
}

impl VfsFile {
    pub fn read_at(&mut self, buf: &mut [u8]) -> usize {
        let n = buf.len().min(self.data.len().saturating_sub(self.pos));
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        n
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }
}

/// Optional FUSE mount stub — enabled with `--features fuse` on Unix.
#[cfg(all(unix, feature = "fuse"))]
pub mod fuse_mount {
    //! Placeholder: integrate `fuser` here for real /chimera FUSE mounts.
    pub fn mount_hint() -> &'static str {
        "FUSE feature enabled — wire fuser::mount2 to VirtualMount"
    }
}

#[cfg(not(all(unix, feature = "fuse")))]
pub mod fuse_mount {
    pub fn mount_hint() -> &'static str {
        "Using VirtualMount VFS (Windows-compatible); enable feature `fuse` on Unix for kernel mounts"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use storage_cas::ContentStore;

    #[test]
    fn virtual_mount_registers_asset() {
        let dir = std::env::temp_dir().join(format!(
            "chimera-vfs-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = Arc::new(ContentStore::new(&dir, 64, 4).unwrap());
        let asset = store.put_asset("demo.bin", b"vfs-bytes").unwrap();
        let mount = VirtualMount::new(store);
        mount.register(&asset);
        assert_eq!(mount.asset_count(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn mount_hint_is_nonempty() {
        assert!(!fuse_mount::mount_hint().is_empty());
    }
}
