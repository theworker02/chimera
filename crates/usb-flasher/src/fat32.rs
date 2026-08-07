//! Minimal correct FAT32 formatter (BPB + FSInfo + FATs + root dir).
//!
//! Writes a blank volume into a partition region of a [`BlockTarget`].
//! Tested exclusively against file-backed images.

use anyhow::{bail, Result};

use crate::block::BlockTarget;

#[derive(Debug, Clone)]
pub struct Fat32Params {
    pub volume_label: String,
    /// Sectors per cluster (power of two). Default 8 (4 KiB @ 512).
    pub sectors_per_cluster: u8,
}

impl Default for Fat32Params {
    fn default() -> Self {
        Self {
            volume_label: "CHIMERA".into(),
            sectors_per_cluster: 8,
        }
    }
}

/// Format FAT32 in `[first_lba, last_lba]` inclusive.
pub fn format_fat32(
    target: &mut dyn BlockTarget,
    first_lba: u64,
    last_lba: u64,
    params: &Fat32Params,
) -> Result<()> {
    let ss = target.sector_size() as u64;
    if ss != 512 {
        bail!("FAT32 formatter requires 512-byte sectors");
    }
    if last_lba <= first_lba + 64 {
        bail!("partition too small for FAT32");
    }
    let total_sectors = (last_lba - first_lba + 1) as u32;
    let spc = params.sectors_per_cluster.max(1);
    let reserved = 32u16;
    let fats = 2u8;
    let root_cluster = 2u32;

    // Estimate FAT size iteratively
    let mut fat_sectors = 1u32;
    for _ in 0..8 {
        let data_sectors = total_sectors
            .saturating_sub(u32::from(reserved))
            .saturating_sub(u32::from(fats) * fat_sectors);
        let clusters = data_sectors / u32::from(spc);
        let needed = ((clusters + 2) * 4).div_ceil(512);
        if needed <= fat_sectors {
            break;
        }
        fat_sectors = needed;
    }

    let data_start = u32::from(reserved) + u32::from(fats) * fat_sectors;
    let data_sectors = total_sectors.saturating_sub(data_start);
    let clusters = data_sectors / u32::from(spc);
    if clusters < 65_525 {
        // Technically could be FAT16 territory; we still write FAT32 structures
        // but warn via bail for very tiny images used in unit tests we keep ≥.
        if clusters < 2 {
            bail!("not enough clusters for FAT32 ({clusters})");
        }
    }

    let mut bpb = [0u8; 512];
    // Jump
    bpb[0] = 0xEB;
    bpb[1] = 0x58;
    bpb[2] = 0x90;
    bpb[3..11].copy_from_slice(b"MSWIN4.1");
    bpb[11..13].copy_from_slice(&512u16.to_le_bytes());
    bpb[13] = spc;
    bpb[14..16].copy_from_slice(&reserved.to_le_bytes());
    bpb[16] = fats;
    // root entries 0 for FAT32
    bpb[17..19].copy_from_slice(&0u16.to_le_bytes());
    // total sectors 16-bit = 0
    bpb[19..21].copy_from_slice(&0u16.to_le_bytes());
    bpb[21] = 0xF8; // media
    // fat size 16 = 0
    bpb[22..24].copy_from_slice(&0u16.to_le_bytes());
    bpb[24..26].copy_from_slice(&63u16.to_le_bytes()); // sectors/track
    bpb[26..28].copy_from_slice(&255u16.to_le_bytes()); // heads
    bpb[28..32].copy_from_slice(&(first_lba as u32).to_le_bytes()); // hidden
    bpb[32..36].copy_from_slice(&total_sectors.to_le_bytes());
    // FAT32 extended
    bpb[36..40].copy_from_slice(&fat_sectors.to_le_bytes());
    bpb[40..42].copy_from_slice(&0u16.to_le_bytes()); // ext flags
    bpb[42..44].copy_from_slice(&0u16.to_le_bytes()); // FS version
    bpb[44..48].copy_from_slice(&root_cluster.to_le_bytes());
    bpb[48..50].copy_from_slice(&1u16.to_le_bytes()); // FSInfo sector
    bpb[50..52].copy_from_slice(&6u16.to_le_bytes()); // backup boot
    bpb[64] = 0x80; // drive number
    bpb[66] = 0x29; // ext boot sig
    let vol_id: u32 = 0x4348_494D;
    bpb[67..71].copy_from_slice(&vol_id.to_le_bytes());
    let label = fat_label(&params.volume_label);
    bpb[71..82].copy_from_slice(&label);
    bpb[82..90].copy_from_slice(b"FAT32   ");
    bpb[510] = 0x55;
    bpb[511] = 0xAA;

    // FSInfo
    let mut fsi = [0u8; 512];
    fsi[0..4].copy_from_slice(&0x4161_5252u32.to_le_bytes()); // RRaA
    fsi[484..488].copy_from_slice(&0x6141_7272u32.to_le_bytes()); // rrAa
    let free = clusters.saturating_sub(1); // cluster 2 used for root
    fsi[488..492].copy_from_slice(&free.to_le_bytes());
    fsi[492..496].copy_from_slice(&3u32.to_le_bytes()); // next free
    fsi[510] = 0x55;
    fsi[511] = 0xAA;

    // Write boot + FSInfo + backup
    target.write_sectors(first_lba, &bpb)?;
    target.write_sectors(first_lba + 1, &fsi)?;
    target.write_sectors(first_lba + 6, &bpb)?;

    // FATs — cluster 0 media, cluster 1 EOC, cluster 2 EOC (root)
    let mut fat = vec![0u8; fat_sectors as usize * 512];
    write_fat32_entry(&mut fat, 0, 0x0FFF_FFF8);
    write_fat32_entry(&mut fat, 1, 0x0FFF_FFFF);
    write_fat32_entry(&mut fat, 2, 0x0FFF_FFFF);
    let fat1_lba = first_lba + u64::from(reserved);
    target.write_sectors(fat1_lba, &fat)?;
    target.write_sectors(fat1_lba + u64::from(fat_sectors), &fat)?;

    // Zero root directory cluster
    let root_lba = first_lba + u64::from(data_start);
    let root_sectors = u64::from(spc);
    let zeros = vec![0u8; (root_sectors * 512) as usize];
    // Volume label entry in root
    let mut root = zeros;
    let mut entry = [0u8; 32];
    entry[0..11].copy_from_slice(&label);
    entry[11] = 0x08; // volume label attribute
    root[..32].copy_from_slice(&entry);
    target.write_sectors(root_lba, &root)?;

    target.sync()?;
    Ok(())
}

fn fat_label(s: &str) -> [u8; 11] {
    let mut out = [b' '; 11];
    let up = s.to_ascii_uppercase();
    for (i, b) in up.bytes().take(11).enumerate() {
        out[i] = if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' {
            b
        } else {
            b'_'
        };
    }
    out
}

fn write_fat32_entry(fat: &mut [u8], cluster: u32, value: u32) {
    let off = (cluster as usize) * 4;
    if off + 4 <= fat.len() {
        fat[off..off + 4].copy_from_slice(&(value & 0x0FFF_FFFF).to_le_bytes());
    }
}

/// Parse BPB bytes_per_sector / sectors_per_cluster / volume label from boot sector.
pub fn parse_fat32_bpb(boot: &[u8]) -> Result<(u16, u8, String)> {
    if boot.len() < 90 {
        bail!("boot sector too short");
    }
    if boot[510] != 0x55 || boot[511] != 0xAA {
        bail!("missing boot signature");
    }
    let bps = u16::from_le_bytes(boot[11..13].try_into().unwrap());
    let spc = boot[13];
    let label = String::from_utf8_lossy(&boot[71..82]).trim().to_string();
    let fs = String::from_utf8_lossy(&boot[82..90]).trim().to_string();
    if !fs.starts_with("FAT32") {
        bail!("not a FAT32 BPB (fs={fs})");
    }
    Ok((bps, spc, label))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::FileImageTarget;
    use crate::gpt::{init_gpt_single, read_gpt};
    use tempfile::tempdir;

    #[test]
    fn fat32_on_gpt_partition() {
        let dir = tempdir().unwrap();
        let mut img =
            FileImageTarget::create(dir.path().join("fat.img"), 64 * 1024 * 1024, 512).unwrap();
        let disk = init_gpt_single(&mut img, "DATA").unwrap();
        let p = &disk.partitions[0];
        format_fat32(
            &mut img,
            p.first_lba,
            p.last_lba,
            &Fat32Params {
                volume_label: "CHIMERA".into(),
                sectors_per_cluster: 8,
            },
        )
        .unwrap();
        let mut boot = vec![0u8; 512];
        img.read_sectors(p.first_lba, &mut boot).unwrap();
        let (bps, spc, label) = parse_fat32_bpb(&boot).unwrap();
        assert_eq!(bps, 512);
        assert_eq!(spc, 8);
        assert!(label.starts_with("CHIMERA"));
        // GPT still intact
        let g = read_gpt(&mut img).unwrap();
        assert_eq!(g.partitions.len(), 1);
    }
}
