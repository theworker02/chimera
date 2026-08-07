//! Disk enumeration (READ-ONLY). Never opens handles for write.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartitionHealth {
    Healthy,
    Degraded,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    pub id: String,
    /// OS path, e.g. `\\.\PhysicalDrive1` or `/dev/sdb`.
    pub path: String,
    pub serial: String,
    pub model: String,
    pub size_bytes: u64,
    pub sector_size: u32,
    pub removable: bool,
    pub is_system: bool,
    pub contains_system_volume: bool,
    pub partition_health: PartitionHealth,
    pub bus: String,
}

/// Enumerate physical disks (read-only metadata). Safe to call on the host.
pub fn list_disks() -> anyhow::Result<Vec<DiskInfo>> {
    #[cfg(windows)]
    {
        windows_list_disks()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        linux_list_disks()
    }
    #[cfg(target_os = "macos")]
    {
        Ok(Vec::new())
    }
}

#[cfg(windows)]
fn windows_list_disks() -> anyhow::Result<Vec<DiskInfo>> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE, GENERIC_READ};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Ioctl::{
        DISK_GEOMETRY_EX, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, IOCTL_STORAGE_QUERY_PROPERTY,
        STORAGE_DEVICE_DESCRIPTOR, STORAGE_PROPERTY_QUERY, StorageDeviceProperty,
        PropertyStandardQuery,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;

    let mut out = Vec::new();
    // Probe PhysicalDrive0..31 — READ-ONLY open for geometry / descriptor.
    for idx in 0..32u32 {
        let path = format!(r"\\.\PhysicalDrive{idx}");
        let wide: Vec<u16> = std::ffi::OsStr::new(&path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            continue;
        }

        let mut geom = unsafe { std::mem::zeroed::<DISK_GEOMETRY_EX>() };
        let mut ret = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_DISK_GET_DRIVE_GEOMETRY_EX,
                std::ptr::null(),
                0,
                &mut geom as *mut _ as *mut _,
                std::mem::size_of_val(&geom) as u32,
                &mut ret,
                std::ptr::null_mut(),
            )
        };
        let (size_bytes, sector_size) = if ok != 0 {
            (
                geom.DiskSize as u64,
                geom.Geometry.BytesPerSector,
            )
        } else {
            (0u64, 512u32)
        };

        // Removable media heuristic via STORAGE_DEVICE_DESCRIPTOR
        let mut query = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageDeviceProperty,
            QueryType: PropertyStandardQuery,
            AdditionalParameters: [0],
        };
        let mut buf = vec![0u8; 1024];
        let ok2 = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_STORAGE_QUERY_PROPERTY,
                &mut query as *mut _ as *mut _,
                std::mem::size_of_val(&query) as u32,
                buf.as_mut_ptr() as *mut _,
                buf.len() as u32,
                &mut ret,
                std::ptr::null_mut(),
            )
        };
        let mut removable = false;
        let mut serial = format!("PD{idx}");
        let mut model = format!("PhysicalDrive{idx}");
        let mut bus = "unknown".to_string();
        if ok2 != 0 && ret as usize >= std::mem::size_of::<STORAGE_DEVICE_DESCRIPTOR>() {
            let desc = unsafe { &*(buf.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR) };
            removable = desc.RemovableMedia != 0;
            if desc.SerialNumberOffset != 0
                && (desc.SerialNumberOffset as usize) < buf.len()
            {
                let s = read_c_str(&buf[desc.SerialNumberOffset as usize..]);
                if !s.is_empty() {
                    serial = s;
                }
            }
            if desc.ProductIdOffset != 0 && (desc.ProductIdOffset as usize) < buf.len() {
                let s = read_c_str(&buf[desc.ProductIdOffset as usize..]);
                if !s.is_empty() {
                    model = s;
                }
            }
            bus = format!("{}", desc.BusType as i32);
        }

        // Drive 0 is almost always the system disk — mark conservatively.
        let is_system = idx == 0;
        let contains_system_volume = idx == 0;

        unsafe { CloseHandle(handle) };

        out.push(DiskInfo {
            id: format!("pd{idx}"),
            path,
            serial: serial.trim().to_string(),
            model: model.trim().to_string(),
            size_bytes,
            sector_size: if sector_size == 0 { 512 } else { sector_size },
            removable,
            is_system,
            contains_system_volume,
            partition_health: PartitionHealth::Unknown,
            bus,
        });
    }
    Ok(out)
}

#[cfg(windows)]
fn read_c_str(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn linux_list_disks() -> anyhow::Result<Vec<DiskInfo>> {
    use std::fs;
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/block") else {
        return Ok(out);
    };
    for ent in entries.flatten() {
        let name = ent.file_name().to_string_lossy().to_string();
        if !(name.starts_with("sd")
            || name.starts_with("vd")
            || name.starts_with("nvme")
            || name.starts_with("mmcblk"))
        {
            continue;
        }
        // Skip partitions (e.g. sda1)
        if name.chars().last().is_some_and(|c| c.is_ascii_digit()) && !name.starts_with("nvme")
        {
            if !name.contains('p') {
                // sda1 style
                continue;
            }
        }
        if name.starts_with("sd") && name.len() > 3 {
            continue; // sda1
        }
        let path = format!("/dev/{name}");
        let size_sectors: u64 = fs::read_to_string(format!("/sys/block/{name}/size"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let sector_size: u32 = fs::read_to_string(format!(
            "/sys/block/{name}/queue/logical_block_size"
        ))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(512);
        let removable = fs::read_to_string(format!("/sys/block/{name}/removable"))
            .ok()
            .map(|s| s.trim() == "1")
            .unwrap_or(false);
        let serial = fs::read_to_string(format!("/sys/block/{name}/device/serial"))
            .unwrap_or_else(|_| name.clone());
        let model = fs::read_to_string(format!("/sys/block/{name}/device/model"))
            .unwrap_or_else(|_| name.clone());
        let is_system = name == "sda" || name.starts_with("nvme0n1");
        out.push(DiskInfo {
            id: name.clone(),
            path,
            serial: serial.trim().to_string(),
            model: model.trim().to_string(),
            size_bytes: size_sectors * u64::from(sector_size),
            sector_size,
            removable,
            is_system,
            contains_system_volume: is_system,
            partition_health: PartitionHealth::Unknown,
            bus: "linux-block".into(),
        });
    }
    Ok(out)
}
