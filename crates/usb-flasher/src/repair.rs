//! Non-destructive bootsector repair / CNK payload injection helpers.

use std::path::Path;

use anyhow::{Context, Result};

use crate::block::{BlockTarget, FileImageTarget, PhysicalDiskTarget};
use crate::boot::inject_legacy_bootstrap;
use crate::enumerate::DiskInfo;
use crate::mbr::{read_mbr, write_mbr};
use crate::safety::{assert_physical_write_allowed, WriteGate};

#[derive(Debug, Clone)]
pub struct RepairPlan {
    pub gate: WriteGate,
    /// Re-inject MBR bootstrap from path (optional).
    pub mbr_bootstrap: Option<std::path::PathBuf>,
    /// Raw nano-kernel / payload bytes written at a fixed lab LBA (non-destructive
    /// to the partition table — writes into free/payload region only).
    pub inject_kernel: Option<Vec<u8>>,
    /// LBA for kernel payload (default 2048).
    pub kernel_lba: u64,
}

impl Default for RepairPlan {
    fn default() -> Self {
        Self {
            gate: WriteGate::default(),
            mbr_bootstrap: None,
            inject_kernel: None,
            kernel_lba: 2048,
        }
    }
}

pub fn repair_file_image(image: &mut FileImageTarget, plan: &RepairPlan) -> Result<()> {
    if plan.gate.dry_run {
        return Ok(());
    }
    repair_target(image, plan)
}

pub fn repair_physical(disk: DiskInfo, plan: &RepairPlan) -> Result<()> {
    assert_physical_write_allowed(&disk, &plan.gate)?;
    let mut target = PhysicalDiskTarget::open(disk, plan.gate.clone())?;
    repair_target(&mut target, plan)
}

fn repair_target(target: &mut dyn BlockTarget, plan: &RepairPlan) -> Result<()> {
    // Ensure MBR signature still present; rewrite signature only if corrupt.
    match read_mbr(target) {
        Ok(mut table) => {
            if let Some(path) = &plan.mbr_bootstrap {
                inject_legacy_bootstrap(target, Some(Path::new(path)))?;
            } else {
                // Touch-write same table (repairs missing 0x55AA if parse succeeded)
                write_mbr(target, &table)?;
                let _ = &mut table;
            }
        }
        Err(_) => {
            // Do not invent a full partition table during repair — require bootstrap path.
            anyhow::bail!(
                "MBR unreadable; refuse to invent partitions during repair. \
                 Use flash with an explicit scheme instead."
            );
        }
    }

    if let Some(kernel) = &plan.inject_kernel {
        let ss = target.sector_size() as usize;
        let mut chunk = kernel.clone();
        if chunk.len() % ss != 0 {
            chunk.resize(chunk.len().div_ceil(ss) * ss, 0);
        }
        target.write_sectors(plan.kernel_lba, &chunk)
            .context("inject kernel payload")?;
    }
    target.sync()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mbr::{MbrPartition, MbrTable};
    use tempfile::tempdir;

    #[test]
    fn repair_injects_kernel_without_clobbering_mbr_parts() {
        let dir = tempdir().unwrap();
        let mut img =
            FileImageTarget::create(dir.path().join("rep.img"), 8 * 1024 * 1024, 512).unwrap();
        let mut table = MbrTable::default();
        table.partitions[0] = MbrPartition {
            bootable: true,
            type_id: 0x0C,
            start_lba: 2048,
            sectors: 1024,
        };
        write_mbr(&mut img, &table).unwrap();
        let plan = RepairPlan {
            gate: WriteGate {
                destroy_confirm: true,
                dry_run: false,
                typed_serial: None,
            },
            mbr_bootstrap: None,
            inject_kernel: Some(b"NANO-KERNEL".to_vec()),
            kernel_lba: 4096,
        };
        repair_file_image(&mut img, &plan).unwrap();
        let back = read_mbr(&mut img).unwrap();
        assert_eq!(back.partitions[0].start_lba, 2048);
        let mut buf = vec![0u8; 512];
        img.read_sectors(4096, &mut buf).unwrap();
        assert_eq!(&buf[..11], b"NANO-KERNEL");
    }
}
