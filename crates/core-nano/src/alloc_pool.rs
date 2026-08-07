//! Deterministic fragmentation-resistant block-pool allocator (TLSF-inspired).
//!
//! Fixed-size block classes avoid external fragmentation on kilobyte-RAM devices.
//! This is a **pool allocator**, not a general malloc — suitable for CNK task
//! arenas and frame buffers.

use core::mem::MaybeUninit;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolError {
    Oom,
    BadPtr,
    DoubleFree,
}

/// Single size-class free list over a static (or caller-owned) slab.
pub struct BlockPool<'a> {
    block_size: usize,
    slab: &'a mut [MaybeUninit<u8>],
    free_head: Option<usize>,
    /// Bitset-ish occupancy via parallel free links stored in-block.
    free_count: usize,
    total_blocks: usize,
}

impl<'a> BlockPool<'a> {
    /// `block_size` must be ≥ `size_of::<usize>()` (free-list next pointer).
    pub fn new(slab: &'a mut [MaybeUninit<u8>], block_size: usize) -> Result<Self, PoolError> {
        if block_size < core::mem::size_of::<usize>() || slab.len() < block_size {
            return Err(PoolError::Oom);
        }
        let total = slab.len() / block_size;
        let mut pool = Self {
            block_size,
            slab,
            free_head: None,
            free_count: 0,
            total_blocks: total,
        };
        // Push all blocks onto free list (high → low for determinism).
        for i in (0..total).rev() {
            pool.push_free(i);
        }
        Ok(pool)
    }

    fn push_free(&mut self, idx: usize) {
        let next = self.free_head;
        let offset = idx * self.block_size;
        unsafe {
            let ptr = self.slab.as_mut_ptr().add(offset) as *mut usize;
            core::ptr::write(ptr, next.unwrap_or(usize::MAX));
        }
        self.free_head = Some(idx);
        self.free_count += 1;
    }

    pub fn alloc(&mut self) -> Result<*mut u8, PoolError> {
        let idx = self.free_head.ok_or(PoolError::Oom)?;
        let offset = idx * self.block_size;
        let next = unsafe {
            let ptr = self.slab.as_ptr().add(offset) as *const usize;
            let n = core::ptr::read(ptr);
            if n == usize::MAX {
                None
            } else {
                Some(n)
            }
        };
        self.free_head = next;
        self.free_count = self.free_count.saturating_sub(1);
        Ok(unsafe { self.slab.as_mut_ptr().add(offset) as *mut u8 })
    }

    /// Free a pointer previously returned by `alloc`.
    pub fn free(&mut self, ptr: *mut u8) -> Result<(), PoolError> {
        let base = self.slab.as_mut_ptr() as usize;
        let p = ptr as usize;
        if p < base {
            return Err(PoolError::BadPtr);
        }
        let offset = p - base;
        if offset % self.block_size != 0 {
            return Err(PoolError::BadPtr);
        }
        let idx = offset / self.block_size;
        if idx >= self.total_blocks {
            return Err(PoolError::BadPtr);
        }
        // Detect double-free by walking free list (O(n) — fine for tiny pools).
        let mut cur = self.free_head;
        while let Some(i) = cur {
            if i == idx {
                return Err(PoolError::DoubleFree);
            }
            let off = i * self.block_size;
            let n = unsafe { core::ptr::read(self.slab.as_ptr().add(off) as *const usize) };
            cur = if n == usize::MAX { None } else { Some(n) };
        }
        self.push_free(idx);
        Ok(())
    }

    pub fn free_count(&self) -> usize {
        self.free_count
    }

    pub fn total_blocks(&self) -> usize {
        self.total_blocks
    }

    pub fn block_size(&self) -> usize {
        self.block_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn pool_alloc_free_cycle() {
        let mut raw = Vec::<MaybeUninit<u8>>::with_capacity(64 * 16);
        raw.resize(64 * 16, MaybeUninit::uninit());
        let mut pool = BlockPool::new(&mut raw, 64).unwrap();
        assert_eq!(pool.free_count(), 16);
        let a = pool.alloc().unwrap();
        let b = pool.alloc().unwrap();
        assert_ne!(a, b);
        pool.free(a).unwrap();
        pool.free(b).unwrap();
        assert_eq!(pool.free_count(), 16);
        assert!(matches!(pool.free(a), Err(PoolError::DoubleFree)));
    }
}
