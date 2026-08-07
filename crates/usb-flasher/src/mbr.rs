//! Master Boot Record (DOS-compatible) writer / parser.

use anyhow::{bail, Result};

use crate::block::BlockTarget;

pub const MBR_BOOTSTRAP_LEN: usize = 440;
pub const MBR_SIGNATURE: u16 = 0xAA55;

#[derive(Debug, Clone, Copy)]
pub struct MbrPartition {
    pub bootable: bool,
    pub type_id: u8,
    pub start_lba: u32,
    pub sectors: u32,
}

#[derive(Debug, Clone)]
pub struct MbrTable {
    /// First 440 bytes — user-supplied bootstrap or zeros (documented no-op default).
    pub bootstrap: [u8; MBR_BOOTSTRAP_LEN],
    pub disk_signature: u32,
    pub partitions: [MbrPartition; 4],
}

impl Default for MbrTable {
    fn default() -> Self {
        Self {
            bootstrap: [0u8; MBR_BOOTSTRAP_LEN],
            disk_signature: 0x4348_494D, // "CHIM"
            partitions: [MbrPartition {
                bootable: false,
                type_id: 0,
                start_lba: 0,
                sectors: 0,
            }; 4],
        }
    }
}

impl MbrTable {
    pub fn encode(&self) -> [u8; 512] {
        let mut sec = [0u8; 512];
        sec[..MBR_BOOTSTRAP_LEN].copy_from_slice(&self.bootstrap);
        sec[440..444].copy_from_slice(&self.disk_signature.to_le_bytes());
        // 444..446 = 0 (reserved)
        for (i, p) in self.partitions.iter().enumerate() {
            let off = 446 + i * 16;
            sec[off] = if p.bootable { 0x80 } else { 0x00 };
            // CHS fields left zero (LBA-only)
            sec[off + 4] = p.type_id;
            sec[off + 8..off + 12].copy_from_slice(&p.start_lba.to_le_bytes());
            sec[off + 12..off + 16].copy_from_slice(&p.sectors.to_le_bytes());
        }
        sec[510..512].copy_from_slice(&MBR_SIGNATURE.to_le_bytes());
        sec
    }

    pub fn parse(sec: &[u8]) -> Result<Self> {
        if sec.len() < 512 {
            bail!("MBR sector too short");
        }
        let sig = u16::from_le_bytes([sec[510], sec[511]]);
        if sig != MBR_SIGNATURE {
            bail!("invalid MBR signature {sig:#x}");
        }
        let mut bootstrap = [0u8; MBR_BOOTSTRAP_LEN];
        bootstrap.copy_from_slice(&sec[..MBR_BOOTSTRAP_LEN]);
        let disk_signature = u32::from_le_bytes(sec[440..444].try_into().unwrap());
        let mut partitions = [MbrPartition {
            bootable: false,
            type_id: 0,
            start_lba: 0,
            sectors: 0,
        }; 4];
        for i in 0..4 {
            let off = 446 + i * 16;
            partitions[i] = MbrPartition {
                bootable: sec[off] == 0x80,
                type_id: sec[off + 4],
                start_lba: u32::from_le_bytes(sec[off + 8..off + 12].try_into().unwrap()),
                sectors: u32::from_le_bytes(sec[off + 12..off + 16].try_into().unwrap()),
            };
        }
        Ok(Self {
            bootstrap,
            disk_signature,
            partitions,
        })
    }
}

/// Write MBR to LBA 0.
pub fn write_mbr(target: &mut dyn BlockTarget, table: &MbrTable) -> Result<()> {
    let sec = table.encode();
    target.write_sectors(0, &sec)?;
    Ok(())
}

/// Read MBR from LBA 0.
pub fn read_mbr(target: &mut dyn BlockTarget) -> Result<MbrTable> {
    let mut sec = vec![0u8; target.sector_size() as usize];
    if sec.len() < 512 {
        bail!("sector size < 512");
    }
    target.read_sectors(0, &mut sec)?;
    MbrTable::parse(&sec[..512])
}

/// Inject a 440-byte bootstrap into an existing MBR (preserves partition table).
pub fn inject_bootstrap(target: &mut dyn BlockTarget, bootstrap: &[u8]) -> Result<()> {
    if bootstrap.len() != MBR_BOOTSTRAP_LEN {
        bail!(
            "bootstrap must be exactly {MBR_BOOTSTRAP_LEN} bytes (got {})",
            bootstrap.len()
        );
    }
    let mut table = read_mbr(target)?;
    table.bootstrap.copy_from_slice(bootstrap);
    write_mbr(target, &table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::FileImageTarget;
    use tempfile::tempdir;

    #[test]
    fn mbr_roundtrip() {
        let dir = tempdir().unwrap();
        let mut img = FileImageTarget::create(dir.path().join("mbr.img"), 1024 * 1024, 512).unwrap();
        let mut table = MbrTable::default();
        table.partitions[0] = MbrPartition {
            bootable: true,
            type_id: 0x0C, // FAT32 LBA
            start_lba: 2048,
            sectors: 2048,
        };
        write_mbr(&mut img, &table).unwrap();
        let back = read_mbr(&mut img).unwrap();
        assert_eq!(back.partitions[0].type_id, 0x0C);
        assert_eq!(back.partitions[0].start_lba, 2048);
        assert!(back.partitions[0].bootable);
        assert_eq!(back.disk_signature, 0x4348_494D);
    }
}
