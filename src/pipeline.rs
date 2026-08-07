//! Zero-copy mmap data pipeline with postcard serialization.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Context;
use memmap2::{Mmap, MmapMut, MmapOptions};
use serde::{Deserialize, Serialize};

use crate::protocol::TaskId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkHeader {
    pub task_id: TaskId,
    pub index: u32,
    pub byte_len: u64,
    pub content_hash: [u8; 32],
}

pub struct DataPipeline {
    root: PathBuf,
    bytes_written: AtomicU64,
    bytes_read: AtomicU64,
}

impl DataPipeline {
    pub fn new(root: impl AsRef<Path>) -> anyhow::Result<Self> {
        let root = root.as_ref().join("pipeline");
        std::fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            bytes_written: AtomicU64::new(0),
            bytes_read: AtomicU64::new(0),
        })
    }

    pub fn chunk_path(&self, hash: &[u8; 32]) -> PathBuf {
        self.root.join(hex::encode(hash))
    }

    /// Write payload via postcard header + raw bytes, mmap-friendly layout.
    pub fn write_chunk(&self, header: &ChunkHeader, data: &[u8]) -> anyhow::Result<[u8; 32]> {
        let hash = *blake3::hash(data).as_bytes();
        let path = self.chunk_path(&hash);
        if path.exists() {
            return Ok(hash);
        }
        let mut hdr = header.clone();
        hdr.content_hash = hash;
        hdr.byte_len = data.len() as u64;
        let meta = postcard::to_allocvec(&hdr)?;
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;
        file.write_all(&(meta.len() as u32).to_le_bytes())?;
        file.write_all(&meta)?;
        file.write_all(data)?;
        file.flush()?;
        self.bytes_written
            .fetch_add(data.len() as u64, Ordering::Relaxed);
        Ok(hash)
    }

    /// Memory-map chunk payload (zero-copy read).
    pub fn map_chunk(&self, hash: &[u8; 32]) -> anyhow::Result<(ChunkHeader, Mmap)> {
        let path = self.chunk_path(hash);
        let mut file = OpenOptions::new().read(true).open(&path)?;
        let mut len_buf = [0u8; 4];
        file.read_exact(&mut len_buf)?;
        let meta_len = u32::from_le_bytes(len_buf) as usize;
        let mut meta = vec![0u8; meta_len];
        file.read_exact(&mut meta)?;
        let header: ChunkHeader = postcard::from_bytes(&meta)?;
        let data_off = 4 + meta_len;
        let mmap = unsafe { MmapOptions::new().offset(data_off as u64).map(&file)? };
        self.bytes_read
            .fetch_add(mmap.len() as u64, Ordering::Relaxed);
        Ok((header, mmap))
    }

    pub fn create_scratch(&self, name: &str, len: usize) -> anyhow::Result<MmapMut> {
        let path = self.root.join(name);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;
        file.set_len(len as u64)?;
        Ok(unsafe { MmapOptions::new().map_mut(&file)? })
    }

    pub fn stats(&self) -> (u64, u64) {
        (
            self.bytes_read.load(Ordering::Relaxed),
            self.bytes_written.load(Ordering::Relaxed),
        )
    }
}

pub fn serialize_zero_copy<T: Serialize>(value: &T) -> anyhow::Result<Vec<u8>> {
    Ok(postcard::to_allocvec(value).context("postcard encode")?)
}

pub fn deserialize_zero_copy<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> anyhow::Result<T> {
    Ok(postcard::from_bytes(bytes).context("postcard decode")?)
}
