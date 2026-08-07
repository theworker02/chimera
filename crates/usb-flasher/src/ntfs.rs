//! NTFS: NOT implemented in-process.
//!
//! Building a correct NTFS formatter from scratch is not feasible for this phase.
//! On Windows, callers may invoke the OS `format` utility **after** partitions exist.
//! On Linux, `mkfs.ntfs` (ntfs-3g) is the documented path.
//!
//! This module only documents that policy and returns a clear error if selected.

use anyhow::{bail, Result};

use crate::block::BlockTarget;

#[derive(Debug, Clone)]
pub struct NtfsParams {
    pub volume_label: String,
}

/// Always errors — NTFS formatting is delegated to the OS (roadmap / external).
pub fn format_ntfs(
    _target: &mut dyn BlockTarget,
    _first_lba: u64,
    _last_lba: u64,
    _params: &NtfsParams,
) -> Result<()> {
    bail!(
        "NTFS formatting is not implemented in chimera-boot. \
         Create the partition table with this crate, then run the OS formatter \
         (`format X: /fs:ntfs` on Windows, or `mkfs.ntfs` on Linux). \
         See ADR-0023."
    )
}
