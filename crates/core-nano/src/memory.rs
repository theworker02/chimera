//! Immutable memory region abstraction for fault-isolated sandboxes.

use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionError {
    OutOfBounds,
    Immutable,
    Align,
}

/// Memory region that can be sealed immutable after init (CNK fault isolation).
pub struct ImmutableRegion {
    data: Vec<u8>,
    sealed: bool,
    page_size: usize,
}

impl ImmutableRegion {
    pub fn new(len: usize, page_size: usize) -> Self {
        let page_size = page_size.max(64);
        Self {
            data: alloc::vec![0u8; len],
            sealed: false,
            page_size,
        }
    }

    pub fn from_bytes(bytes: &[u8], page_size: usize) -> Self {
        let mut r = Self::new(bytes.len(), page_size);
        r.data.copy_from_slice(bytes);
        r
    }

    pub fn seal(&mut self) {
        self.sealed = true;
    }

    pub fn is_sealed(&self) -> bool {
        self.sealed
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    pub fn write(&mut self, offset: usize, src: &[u8]) -> Result<(), RegionError> {
        if self.sealed {
            return Err(RegionError::Immutable);
        }
        let end = offset.checked_add(src.len()).ok_or(RegionError::OutOfBounds)?;
        if end > self.data.len() {
            return Err(RegionError::OutOfBounds);
        }
        self.data[offset..end].copy_from_slice(src);
        Ok(())
    }

    pub fn read(&self, offset: usize, dst: &mut [u8]) -> Result<(), RegionError> {
        let end = offset.checked_add(dst.len()).ok_or(RegionError::OutOfBounds)?;
        if end > self.data.len() {
            return Err(RegionError::OutOfBounds);
        }
        dst.copy_from_slice(&self.data[offset..end]);
        Ok(())
    }

    pub fn page_count(&self) -> usize {
        self.data.len().div_ceil(self.page_size)
    }

    pub fn page_hash(&self, page: usize) -> Result<[u8; 32], RegionError> {
        let start = page.checked_mul(self.page_size).ok_or(RegionError::OutOfBounds)?;
        if start >= self.data.len() {
            return Err(RegionError::OutOfBounds);
        }
        let end = (start + self.page_size).min(self.data.len());
        Ok(*blake3::hash(&self.data[start..end]).as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_blocks_writes() {
        let mut r = ImmutableRegion::new(32, 16);
        r.write(0, b"hi").unwrap();
        r.seal();
        assert!(matches!(r.write(0, b"x"), Err(RegionError::Immutable)));
        let mut buf = [0u8; 2];
        r.read(0, &mut buf).unwrap();
        assert_eq!(&buf, b"hi");
    }
}
