//! GUID Partition Table writer / parser (UEFI-compatible).

use anyhow::{bail, Context, Result};
use uuid::Uuid;

use crate::block::BlockTarget;
use crate::crc32::crc32;
use crate::mbr::{MbrPartition, MbrTable, write_mbr};

pub const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";
pub const GPT_REVISION: u32 = 0x0001_0000;
pub const GPT_HEADER_SIZE: u32 = 92;
pub const GPT_ENTRY_SIZE: u32 = 128;
pub const GPT_ENTRY_COUNT: u32 = 128;

/// EFI System Partition type GUID.
pub fn esp_type_guid() -> Uuid {
    Uuid::parse_str("C12A7328-F81F-11D2-BA4B-00A0C93EC93B").unwrap()
}

/// Microsoft Basic Data type GUID (for FAT32 data partitions).
pub fn basic_data_type_guid() -> Uuid {
    Uuid::parse_str("EBD0A0A2-B9E5-4433-87C0-68B6B72699C7").unwrap()
}

#[derive(Debug, Clone)]
pub struct GptPartition {
    pub type_guid: Uuid,
    pub unique_guid: Uuid,
    pub first_lba: u64,
    pub last_lba: u64,
    pub attrs: u64,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct GptDisk {
    pub disk_guid: Uuid,
    pub partitions: Vec<GptPartition>,
}

fn uuid_to_gpt_bytes(u: &Uuid) -> [u8; 16] {
    // GPT uses mixed-endian UUID layout (UEFI).
    let b = u.as_bytes();
    let mut out = [0u8; 16];
    // time_low LE
    out[0] = b[3];
    out[1] = b[2];
    out[2] = b[1];
    out[3] = b[0];
    // time_mid LE
    out[4] = b[5];
    out[5] = b[4];
    // time_hi LE
    out[6] = b[7];
    out[7] = b[6];
    // rest as-is
    out[8..16].copy_from_slice(&b[8..16]);
    out
}

fn gpt_bytes_to_uuid(b: &[u8]) -> Uuid {
    let mut raw = [0u8; 16];
    raw[0] = b[3];
    raw[1] = b[2];
    raw[2] = b[1];
    raw[3] = b[0];
    raw[4] = b[5];
    raw[5] = b[4];
    raw[6] = b[7];
    raw[7] = b[6];
    raw[8..16].copy_from_slice(&b[8..16]);
    Uuid::from_bytes(raw)
}

fn encode_entry(p: &GptPartition) -> [u8; 128] {
    let mut e = [0u8; 128];
    e[0..16].copy_from_slice(&uuid_to_gpt_bytes(&p.type_guid));
    e[16..32].copy_from_slice(&uuid_to_gpt_bytes(&p.unique_guid));
    e[32..40].copy_from_slice(&p.first_lba.to_le_bytes());
    e[40..48].copy_from_slice(&p.last_lba.to_le_bytes());
    e[48..56].copy_from_slice(&p.attrs.to_le_bytes());
    let mut name16: Vec<u16> = p.name.encode_utf16().collect();
    name16.truncate(36);
    for (i, c) in name16.iter().enumerate() {
        e[56 + i * 2..56 + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
    }
    e
}

fn decode_entry(e: &[u8]) -> Option<GptPartition> {
    if e.len() < 128 {
        return None;
    }
    if e[0..16].iter().all(|&b| b == 0) {
        return None;
    }
    let type_guid = gpt_bytes_to_uuid(&e[0..16]);
    let unique_guid = gpt_bytes_to_uuid(&e[16..32]);
    let first_lba = u64::from_le_bytes(e[32..40].try_into().ok()?);
    let last_lba = u64::from_le_bytes(e[40..48].try_into().ok()?);
    let attrs = u64::from_le_bytes(e[48..56].try_into().ok()?);
    let mut utf16 = Vec::new();
    for i in 0..36 {
        let c = u16::from_le_bytes(e[56 + i * 2..56 + i * 2 + 2].try_into().ok()?);
        if c == 0 {
            break;
        }
        utf16.push(c);
    }
    let name = String::from_utf16_lossy(&utf16);
    Some(GptPartition {
        type_guid,
        unique_guid,
        first_lba,
        last_lba,
        attrs,
        name,
    })
}

fn write_header(
    sector: &mut [u8],
    disk_guid: Uuid,
    current_lba: u64,
    backup_lba: u64,
    first_usable: u64,
    last_usable: u64,
    entries_lba: u64,
    entries_crc: u32,
) {
    sector.fill(0);
    sector[0..8].copy_from_slice(GPT_SIGNATURE);
    sector[8..12].copy_from_slice(&GPT_REVISION.to_le_bytes());
    sector[12..16].copy_from_slice(&GPT_HEADER_SIZE.to_le_bytes());
    // CRC32 at 16..20 — zero while computing
    sector[16..20].copy_from_slice(&0u32.to_le_bytes());
    sector[24..32].copy_from_slice(&current_lba.to_le_bytes());
    sector[32..40].copy_from_slice(&backup_lba.to_le_bytes());
    sector[40..48].copy_from_slice(&first_usable.to_le_bytes());
    sector[48..56].copy_from_slice(&last_usable.to_le_bytes());
    sector[56..72].copy_from_slice(&uuid_to_gpt_bytes(&disk_guid));
    sector[72..80].copy_from_slice(&entries_lba.to_le_bytes());
    sector[80..84].copy_from_slice(&GPT_ENTRY_COUNT.to_le_bytes());
    sector[84..88].copy_from_slice(&GPT_ENTRY_SIZE.to_le_bytes());
    sector[88..92].copy_from_slice(&entries_crc.to_le_bytes());
    let crc = crc32(&sector[..GPT_HEADER_SIZE as usize]);
    sector[16..20].copy_from_slice(&crc.to_le_bytes());
}

/// Layout a single FAT/ESP-style partition covering most of the disk (after GPT metadata).
pub fn layout_single_data_partition(total_sectors: u64, name: &str) -> GptDisk {
    // Primary entries occupy LBA 2..(2 + entries_sectors - 1); entries = 128*128 = 16384 bytes = 32 sectors @512
    let entries_sectors = ((GPT_ENTRY_COUNT * GPT_ENTRY_SIZE) as u64).div_ceil(512);
    let first_usable = 2 + entries_sectors;
    let last_usable = total_sectors.saturating_sub(1 + 1 + entries_sectors); // backup header + entries
    let part = GptPartition {
        type_guid: basic_data_type_guid(),
        unique_guid: Uuid::new_v4(),
        first_lba: first_usable,
        last_lba: last_usable,
        attrs: 0,
        name: name.to_string(),
    };
    GptDisk {
        disk_guid: Uuid::new_v4(),
        partitions: vec![part],
    }
}

/// Write protective MBR + primary/backup GPT to `target`.
pub fn write_gpt(target: &mut dyn BlockTarget, disk: &GptDisk) -> Result<()> {
    let ss = target.sector_size() as usize;
    if ss != 512 {
        // Structures below assume 512; larger sector sizes need padding — refuse for honesty.
        bail!("GPT writer currently requires 512-byte sectors (got {ss})");
    }
    let total = target.size_bytes() / ss as u64;
    if total < 64 {
        bail!("disk too small for GPT");
    }

    let entries_bytes = (GPT_ENTRY_COUNT * GPT_ENTRY_SIZE) as usize;
    let entries_sectors = entries_bytes.div_ceil(ss) as u64;
    let mut entries = vec![0u8; entries_bytes];
    for (i, p) in disk.partitions.iter().enumerate() {
        if i >= GPT_ENTRY_COUNT as usize {
            bail!("too many partitions");
        }
        let e = encode_entry(p);
        entries[i * 128..(i + 1) * 128].copy_from_slice(&e);
    }
    let entries_crc = crc32(&entries);

    let first_usable = 2 + entries_sectors;
    let last_usable = total - 1 - 1 - entries_sectors;
    let backup_header_lba = total - 1;
    let backup_entries_lba = total - 1 - entries_sectors;

    // Protective MBR
    let mut mbr = MbrTable::default();
    mbr.partitions[0] = MbrPartition {
        bootable: false,
        type_id: 0xEE,
        start_lba: 1,
        sectors: if total > u32::MAX as u64 {
            0xFFFF_FFFF
        } else {
            (total - 1) as u32
        },
    };
    write_mbr(target, &mbr)?;

    // Primary entries at LBA 2
    target.write_sectors(2, &entries)?;

    // Primary header at LBA 1
    let mut hdr = vec![0u8; ss];
    write_header(
        &mut hdr,
        disk.disk_guid,
        1,
        backup_header_lba,
        first_usable,
        last_usable,
        2,
        entries_crc,
    );
    target.write_sectors(1, &hdr)?;

    // Backup entries
    target.write_sectors(backup_entries_lba, &entries)?;

    // Backup header
    let mut bh = vec![0u8; ss];
    write_header(
        &mut bh,
        disk.disk_guid,
        backup_header_lba,
        1,
        first_usable,
        last_usable,
        backup_entries_lba,
        entries_crc,
    );
    target.write_sectors(backup_header_lba, &bh)?;
    target.sync()?;
    Ok(())
}

/// Parse primary GPT header + entries.
pub fn read_gpt(target: &mut dyn BlockTarget) -> Result<GptDisk> {
    let ss = target.sector_size() as usize;
    let mut hdr = vec![0u8; ss];
    target.read_sectors(1, &mut hdr)?;
    if &hdr[0..8] != GPT_SIGNATURE {
        bail!("missing GPT signature");
    }
    let header_size = u32::from_le_bytes(hdr[12..16].try_into().unwrap()) as usize;
    let stored_crc = u32::from_le_bytes(hdr[16..20].try_into().unwrap());
    hdr[16..20].copy_from_slice(&0u32.to_le_bytes());
    let calc = crc32(&hdr[..header_size]);
    if calc != stored_crc {
        bail!("GPT header CRC mismatch (stored {stored_crc:#x} calc {calc:#x})");
    }
    // restore for further reads
    hdr[16..20].copy_from_slice(&stored_crc.to_le_bytes());

    let disk_guid = gpt_bytes_to_uuid(&hdr[56..72]);
    let entries_lba = u64::from_le_bytes(hdr[72..80].try_into().unwrap());
    let entry_count = u32::from_le_bytes(hdr[80..84].try_into().unwrap());
    let entry_size = u32::from_le_bytes(hdr[84..88].try_into().unwrap());
    let entries_crc = u32::from_le_bytes(hdr[88..92].try_into().unwrap());

    let entries_bytes = (entry_count * entry_size) as usize;
    let mut entries = vec![0u8; entries_bytes.div_ceil(ss) * ss];
    target.read_sectors(entries_lba, &mut entries)?;
    let entries = &entries[..entries_bytes];
    if crc32(entries) != entries_crc {
        bail!("GPT entries CRC mismatch");
    }

    let mut partitions = Vec::new();
    for i in 0..entry_count as usize {
        let off = i * entry_size as usize;
        if let Some(p) = decode_entry(&entries[off..off + entry_size as usize]) {
            partitions.push(p);
        }
    }
    Ok(GptDisk {
        disk_guid,
        partitions,
    })
}

/// Convenience: GPT with one basic-data partition sized to the image.
pub fn init_gpt_single(target: &mut dyn BlockTarget, label: &str) -> Result<GptDisk> {
    let total = target.size_bytes() / target.sector_size() as u64;
    let disk = layout_single_data_partition(total, label);
    write_gpt(target, &disk).context("write_gpt")?;
    Ok(disk)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::FileImageTarget;
    use tempfile::tempdir;

    #[test]
    fn gpt_roundtrip_file_image() {
        let dir = tempdir().unwrap();
        // 64 MiB image
        let mut img =
            FileImageTarget::create(dir.path().join("gpt.img"), 64 * 1024 * 1024, 512).unwrap();
        let written = init_gpt_single(&mut img, "CHIMERA").unwrap();
        let read = read_gpt(&mut img).unwrap();
        assert_eq!(read.disk_guid, written.disk_guid);
        assert_eq!(read.partitions.len(), 1);
        assert_eq!(read.partitions[0].name, "CHIMERA");
        assert!(read.partitions[0].last_lba > read.partitions[0].first_lba);

        // Protective MBR type 0xEE
        let mbr = crate::mbr::read_mbr(&mut img).unwrap();
        assert_eq!(mbr.partitions[0].type_id, 0xEE);
    }
}
