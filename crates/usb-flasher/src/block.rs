//! Block device abstraction — file images (tests) vs physical disks (gated).

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::enumerate::DiskInfo;
use crate::safety::{assert_file_write_allowed, assert_physical_write_allowed, WriteGate};

/// Sector-oriented target. Implementations MUST honor `WriteGate` on physical writes.
pub trait BlockTarget {
    fn sector_size(&self) -> u32;
    fn size_bytes(&self) -> u64;
    fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> Result<()>;
    fn write_sectors(&mut self, lba: u64, data: &[u8]) -> Result<()>;
    fn sync(&mut self) -> Result<()>;
    fn info(&self) -> DiskInfo;
}

/// Sparse / ordinary file acting as a virtual disk. Used by **all** tests.
pub struct FileImageTarget {
    file: File,
    path: PathBuf,
    size: u64,
    sector_size: u32,
    serial: String,
    gate: WriteGate,
}

impl FileImageTarget {
    /// Create or open a file image of at least `size_bytes` (zero-extends if needed).
    pub fn create(path: impl AsRef<Path>, size_bytes: u64, sector_size: u32) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .with_context(|| format!("create image {}", path.display()))?;
        file.set_len(size_bytes)?;
        Ok(Self {
            file,
            path,
            size: size_bytes,
            sector_size: if sector_size == 0 { 512 } else { sector_size },
            serial: format!("FILE-{}", uuid::Uuid::new_v4()),
            gate: WriteGate {
                destroy_confirm: true,
                dry_run: false,
                typed_serial: None,
            },
        })
    }

    pub fn open(path: impl AsRef<Path>, sector_size: u32) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .with_context(|| format!("open image {}", path.display()))?;
        let size = file.metadata()?.len();
        let serial = format!("FILE-{}", path.display());
        Ok(Self {
            file,
            path,
            size,
            sector_size: if sector_size == 0 { 512 } else { sector_size },
            serial,
            gate: WriteGate {
                destroy_confirm: true,
                dry_run: false,
                typed_serial: None,
            },
        })
    }

    pub fn with_gate(mut self, gate: WriteGate) -> Self {
        self.gate = gate;
        self
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl BlockTarget for FileImageTarget {
    fn sector_size(&self) -> u32 {
        self.sector_size
    }

    fn size_bytes(&self) -> u64 {
        self.size
    }

    fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> Result<()> {
        let ss = self.sector_size as u64;
        if buf.len() as u64 % ss != 0 {
            bail!("buffer length must be a multiple of sector size");
        }
        let off = lba.checked_mul(ss).context("lba overflow")?;
        if off + buf.len() as u64 > self.size {
            bail!("read past end of image");
        }
        self.file.seek(SeekFrom::Start(off))?;
        self.file.read_exact(buf)?;
        Ok(())
    }

    fn write_sectors(&mut self, lba: u64, data: &[u8]) -> Result<()> {
        assert_file_write_allowed(&self.gate)?;
        let ss = self.sector_size as u64;
        if data.len() as u64 % ss != 0 {
            bail!("data length must be a multiple of sector size");
        }
        let off = lba.checked_mul(ss).context("lba overflow")?;
        if off + data.len() as u64 > self.size {
            bail!("write past end of image");
        }
        self.file.seek(SeekFrom::Start(off))?;
        self.file.write_all(data)?;
        Ok(())
    }

    fn sync(&mut self) -> Result<()> {
        self.file.sync_all()?;
        Ok(())
    }

    fn info(&self) -> DiskInfo {
        DiskInfo {
            id: self.path.display().to_string(),
            path: self.path.display().to_string(),
            serial: self.serial.clone(),
            model: "FileImageTarget".into(),
            size_bytes: self.size,
            sector_size: self.sector_size,
            removable: true,
            is_system: false,
            contains_system_volume: false,
            partition_health: crate::enumerate::PartitionHealth::Healthy,
            bus: "file".into(),
        }
    }
}

/// Physical disk handle. Construction is allowed; **writes** are safety-gated.
pub struct PhysicalDiskTarget {
    info: DiskInfo,
    gate: WriteGate,
    #[cfg(windows)]
    handle: Option<windows_sys::Win32::Foundation::HANDLE>,
    #[cfg(all(unix, not(target_os = "macos")))]
    file: Option<File>,
    #[cfg(target_os = "macos")]
    _unused: (),
}

impl PhysicalDiskTarget {
    /// Open for I/O. Does **not** write. Caller must supply a `WriteGate` for writes.
    pub fn open(info: DiskInfo, gate: WriteGate) -> Result<Self> {
        Self::open_with_access(info, gate, true)
    }

    /// Read-only open for verify / inspect. Writes remain gated (dry-run default).
    pub fn open_readonly(info: DiskInfo) -> Result<Self> {
        Self::open_with_access(info, WriteGate::default(), false)
    }

    fn open_with_access(info: DiskInfo, gate: WriteGate, want_write: bool) -> Result<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Foundation::{INVALID_HANDLE_VALUE, GENERIC_READ, GENERIC_WRITE};
            use windows_sys::Win32::Storage::FileSystem::{
                CreateFileW, FILE_FLAG_NO_BUFFERING, FILE_FLAG_WRITE_THROUGH, FILE_SHARE_READ,
                FILE_SHARE_WRITE, OPEN_EXISTING,
            };

            let wide: Vec<u16> = std::ffi::OsStr::new(&info.path)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let access = if want_write {
                GENERIC_READ | GENERIC_WRITE
            } else {
                GENERIC_READ
            };
            let handle = unsafe {
                CreateFileW(
                    wide.as_ptr(),
                    access,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH,
                    std::ptr::null_mut(),
                )
            };
            if handle == INVALID_HANDLE_VALUE {
                let err = std::io::Error::last_os_error();
                bail!("CreateFileW({}) failed: {err}", info.path);
            }
            Ok(Self {
                info,
                gate,
                handle: Some(handle),
            })
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let mut opts = OpenOptions::new();
            opts.read(true);
            if want_write {
                opts.write(true);
            }
            let file = opts
                .open(&info.path)
                .with_context(|| format!("open {}", info.path))?;
            Ok(Self {
                info,
                gate,
                file: Some(file),
            })
        }
        #[cfg(target_os = "macos")]
        {
            let _ = (&info, &gate, want_write);
            bail!("PhysicalDiskTarget is not implemented on macOS in Phase 14");
        }
        #[cfg(not(any(windows, all(unix, not(target_os = "macos")), target_os = "macos")))]
        {
            let _ = (info, gate, want_write);
            bail!("unsupported platform for PhysicalDiskTarget");
        }
    }
}

#[cfg(windows)]
impl Drop for PhysicalDiskTarget {
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(h);
            }
        }
    }
}

impl BlockTarget for PhysicalDiskTarget {
    fn sector_size(&self) -> u32 {
        self.info.sector_size
    }

    fn size_bytes(&self) -> u64 {
        self.info.size_bytes
    }

    fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> Result<()> {
        let ss = self.sector_size() as u64;
        if buf.len() as u64 % ss != 0 {
            bail!("buffer must be sector-aligned");
        }
        let off = lba * ss;
        #[cfg(windows)]
        {
            use windows_sys::Win32::Storage::FileSystem::{ReadFile, SetFilePointerEx};
            let h = self.handle.context("disk not open")?;
            let mut new_pos = 0i64;
            let ok = unsafe { SetFilePointerEx(h, off as i64, &mut new_pos, 0) };
            if ok == 0 {
                bail!("SetFilePointerEx: {}", std::io::Error::last_os_error());
            }
            let mut read = 0u32;
            let ok = unsafe {
                ReadFile(
                    h,
                    buf.as_mut_ptr() as *mut _,
                    buf.len() as u32,
                    &mut read,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                bail!("ReadFile: {}", std::io::Error::last_os_error());
            }
            if read as usize != buf.len() {
                bail!("short read {read}/{}", buf.len());
            }
            Ok(())
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let f = self.file.as_mut().context("disk not open")?;
            f.seek(SeekFrom::Start(off))?;
            f.read_exact(buf)?;
            Ok(())
        }
        #[cfg(target_os = "macos")]
        {
            let _ = (lba, buf);
            bail!("unsupported");
        }
    }

    fn write_sectors(&mut self, lba: u64, data: &[u8]) -> Result<()> {
        // HARD GATE — never bypass.
        assert_physical_write_allowed(&self.info, &self.gate)?;
        let ss = self.sector_size() as u64;
        if data.len() as u64 % ss != 0 {
            bail!("data must be sector-aligned");
        }
        let off = lba * ss;
        #[cfg(windows)]
        {
            use windows_sys::Win32::Storage::FileSystem::{SetFilePointerEx, WriteFile};
            let h = self.handle.context("disk not open")?;
            let mut new_pos = 0i64;
            let ok = unsafe { SetFilePointerEx(h, off as i64, &mut new_pos, 0) };
            if ok == 0 {
                bail!("SetFilePointerEx: {}", std::io::Error::last_os_error());
            }
            let mut written = 0u32;
            let ok = unsafe {
                WriteFile(
                    h,
                    data.as_ptr() as *const _,
                    data.len() as u32,
                    &mut written,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                bail!("WriteFile: {}", std::io::Error::last_os_error());
            }
            if written as usize != data.len() {
                bail!("short write {written}/{}", data.len());
            }
            Ok(())
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let f = self.file.as_mut().context("disk not open")?;
            f.seek(SeekFrom::Start(off))?;
            f.write_all(data)?;
            Ok(())
        }
        #[cfg(target_os = "macos")]
        {
            let _ = (lba, data);
            bail!("unsupported");
        }
    }

    fn sync(&mut self) -> Result<()> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Storage::FileSystem::FlushFileBuffers;
            let h = self.handle.context("disk not open")?;
            let ok = unsafe { FlushFileBuffers(h) };
            if ok == 0 {
                bail!("FlushFileBuffers: {}", std::io::Error::last_os_error());
            }
            Ok(())
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            self.file.as_mut().context("disk not open")?.sync_all()?;
            Ok(())
        }
        #[cfg(target_os = "macos")]
        {
            bail!("unsupported");
        }
    }

    fn info(&self) -> DiskInfo {
        self.info.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn file_image_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("disk.img");
        let mut img = FileImageTarget::create(&path, 1024 * 1024, 512).unwrap();
        let mut sector = vec![0u8; 512];
        sector[0] = 0xAB;
        sector[511] = 0xCD;
        img.write_sectors(10, &sector).unwrap();
        let mut back = vec![0u8; 512];
        img.read_sectors(10, &mut back).unwrap();
        assert_eq!(back, sector);
    }

    #[test]
    fn dry_run_blocks_file_writes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("dry.img");
        let mut img = FileImageTarget::create(&path, 64 * 1024, 512)
            .unwrap()
            .with_gate(WriteGate::default());
        let sector = vec![0u8; 512];
        assert!(img.write_sectors(0, &sector).is_err());
    }
}
