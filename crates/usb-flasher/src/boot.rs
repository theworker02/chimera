//! UEFI ESP layout helpers + legacy MBR bootstrap injection.
//!
//! **No precompiled bootloader blobs are shipped.** Callers supply:
//! - `BOOTX64.EFI` (or equivalent) via filesystem path
//! - Optional 440-byte legacy bootstrap
//!
//! This module builds the `/EFI/BOOT/` directory tree **inside a FAT32
//! file-image volume** for tests, or records the intended layout for physical
//! media after the volume is mounted by the OS.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::block::BlockTarget;
use crate::mbr::{inject_bootstrap, MBR_BOOTSTRAP_LEN};

/// Intended ESP paths (relative to volume root).
pub const EFI_BOOT_DIR: &str = "EFI/BOOT";
pub const EFI_BOOT_FILE: &str = "EFI/BOOT/BOOTX64.EFI";

#[derive(Debug, Clone)]
pub struct EspLayoutPlan {
    pub efi_stub_source: PathBuf,
    pub dest_rel: String,
}

impl EspLayoutPlan {
    pub fn from_stub(path: impl Into<PathBuf>) -> Self {
        Self {
            efi_stub_source: path.into(),
            dest_rel: EFI_BOOT_FILE.into(),
        }
    }
}

/// Validate that a user-supplied EFI stub exists and is non-empty.
pub fn validate_efi_stub(path: &Path) -> Result<()> {
    let meta = std::fs::metadata(path).with_context(|| format!("EFI stub {}", path.display()))?;
    if !meta.is_file() || meta.len() == 0 {
        bail!("EFI stub must be a non-empty file (no bundled bootloaders in chimera-boot)");
    }
    Ok(())
}

/// Copy EFI stub into a host directory tree representing the ESP (for file-image lab).
/// Creates `esp_root/EFI/BOOT/BOOTX64.EFI`.
pub fn materialize_esp_tree(esp_root: &Path, plan: &EspLayoutPlan) -> Result<PathBuf> {
    validate_efi_stub(&plan.efi_stub_source)?;
    let dest = esp_root.join(&plan.dest_rel);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(&plan.efi_stub_source, &dest)?;
    Ok(dest)
}

/// Inject legacy MBR bootstrap from a user file (exactly 440 bytes).
/// Default / missing file → documented no-op (zeros already in MBR writer).
pub fn inject_legacy_bootstrap(target: &mut dyn BlockTarget, bootstrap_path: Option<&Path>) -> Result<()> {
    let Some(path) = bootstrap_path else {
        return Ok(()); // no-op default
    };
    let bytes = std::fs::read(path).context("read bootstrap")?;
    if bytes.len() != MBR_BOOTSTRAP_LEN {
        bail!(
            "legacy bootstrap must be exactly {MBR_BOOTSTRAP_LEN} bytes (got {}). \
             chimera-boot ships no default bootloader blob.",
            bytes.len()
        );
    }
    inject_bootstrap(target, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::FileImageTarget;
    use crate::mbr::{read_mbr, write_mbr, MbrTable};
    use tempfile::tempdir;

    #[test]
    fn esp_tree_from_user_stub() {
        let dir = tempdir().unwrap();
        let stub = dir.path().join("fake.efi");
        std::fs::write(&stub, b"MZ-fake-efi-stub").unwrap();
        let esp = dir.path().join("esp");
        let dest = materialize_esp_tree(&esp, &EspLayoutPlan::from_stub(&stub)).unwrap();
        assert!(dest.ends_with("BOOTX64.EFI"));
        assert_eq!(std::fs::read(&dest).unwrap(), b"MZ-fake-efi-stub");
    }

    #[test]
    fn bootstrap_inject_roundtrip() {
        let dir = tempdir().unwrap();
        let mut img =
            FileImageTarget::create(dir.path().join("boot.img"), 1024 * 1024, 512).unwrap();
        write_mbr(&mut img, &MbrTable::default()).unwrap();
        let mut boot = vec![0u8; 440];
        boot[0] = 0xEB;
        boot[1] = 0xFE;
        let boot_path = dir.path().join("boot.bin");
        std::fs::write(&boot_path, &boot).unwrap();
        inject_legacy_bootstrap(&mut img, Some(&boot_path)).unwrap();
        let mbr = read_mbr(&mut img).unwrap();
        assert_eq!(mbr.bootstrap[0], 0xEB);
        assert_eq!(mbr.bootstrap[1], 0xFE);
    }
}
