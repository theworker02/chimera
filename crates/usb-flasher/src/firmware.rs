//! Host firmware mode detection (UEFI vs legacy BIOS).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FirmwareMode {
    Uefi,
    LegacyBios,
    Unknown,
}

/// Detect firmware mode on the running host (best-effort).
pub fn detect_firmware_mode() -> FirmwareMode {
    #[cfg(windows)]
    {
        windows_get_firmware_type()
    }
    #[cfg(target_os = "linux")]
    {
        if std::path::Path::new("/sys/firmware/efi").exists() {
            FirmwareMode::Uefi
        } else {
            FirmwareMode::LegacyBios
        }
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        FirmwareMode::Unknown
    }
}

#[cfg(windows)]
fn windows_get_firmware_type() -> FirmwareMode {
    // GetFirmwareType(FirmwareType*) — kernel32.
    type FnGetFirmwareType = unsafe extern "system" fn(*mut u32) -> i32;
    let lib = unsafe {
        windows_sys::Win32::System::LibraryLoader::LoadLibraryA(b"kernel32.dll\0".as_ptr() as _)
    };
    if lib.is_null() {
        return FirmwareMode::Unknown;
    }
    let proc = unsafe {
        windows_sys::Win32::System::LibraryLoader::GetProcAddress(
            lib,
            b"GetFirmwareType\0".as_ptr() as _,
        )
    };
    let Some(proc) = proc else {
        return FirmwareMode::Unknown;
    };
    let f: FnGetFirmwareType = unsafe { std::mem::transmute(proc) };
    let mut t = 0u32;
    let ok = unsafe { f(&mut t) };
    // FirmwareTypeUnknown=0, Bios=1, Uefi=2
    match (ok != 0, t) {
        (true, 1) => FirmwareMode::LegacyBios,
        (true, 2) => FirmwareMode::Uefi,
        _ => FirmwareMode::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_returns_variant() {
        let m = detect_firmware_mode();
        // On Windows CI/dev hosts this is usually Uefi; accept any non-panic result.
        let _ = m;
    }
}
