//! Write safety gates — physical targets MUST pass before any destructive I/O.

use thiserror::Error;

use crate::enumerate::DiskInfo;

/// Explicit confirmation + dry-run policy for destructive operations.
#[derive(Debug, Clone)]
pub struct WriteGate {
    /// `--yes-i-understand-this-destroys-data`
    pub destroy_confirm: bool,
    /// Default **true**. Real writes require `false` via `--no-dry-run`.
    pub dry_run: bool,
    /// Optional typed serial must match `DiskInfo::serial` when provided.
    pub typed_serial: Option<String>,
}

impl Default for WriteGate {
    fn default() -> Self {
        Self {
            destroy_confirm: false,
            dry_run: true,
            typed_serial: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum SafetyError {
    #[error("dry-run is ON (default). Pass --no-dry-run to enable real writes")]
    DryRunActive,
    #[error("missing destructive confirmation (--yes-i-understand-this-destroys-data) or typed serial")]
    MissingConfirmation,
    #[error("refusing non-removable / fixed disk")]
    NotRemovable,
    #[error("refusing system or boot volume disk")]
    SystemDisk,
    #[error("typed serial mismatch (expected {expected}, got {got})")]
    SerialMismatch { expected: String, got: String },
    #[error("writes to file images do not require physical gates, but dry-run still blocks when requested")]
    FileDryRun,
}

/// Evaluate whether a **physical** disk may be written.
pub fn assert_physical_write_allowed(disk: &DiskInfo, gate: &WriteGate) -> Result<(), SafetyError> {
    if gate.dry_run {
        return Err(SafetyError::DryRunActive);
    }
    if !disk.removable {
        return Err(SafetyError::NotRemovable);
    }
    if disk.is_system || disk.contains_system_volume {
        return Err(SafetyError::SystemDisk);
    }
    let confirmed = gate.destroy_confirm
        || gate
            .typed_serial
            .as_ref()
            .is_some_and(|s| s == &disk.serial);
    if !confirmed {
        return Err(SafetyError::MissingConfirmation);
    }
    if let Some(typed) = &gate.typed_serial {
        if typed != &disk.serial {
            return Err(SafetyError::SerialMismatch {
                expected: disk.serial.clone(),
                got: typed.clone(),
            });
        }
    }
    Ok(())
}

/// File-backed images only need an explicit opt-out of dry-run when the caller
/// wants to actually mutate the image (tests always pass `dry_run: false`).
pub fn assert_file_write_allowed(gate: &WriteGate) -> Result<(), SafetyError> {
    if gate.dry_run {
        return Err(SafetyError::FileDryRun);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enumerate::{DiskInfo, PartitionHealth};

    fn sample_removable() -> DiskInfo {
        DiskInfo {
            id: "img0".into(),
            path: "/virtual/img0".into(),
            serial: "TEST-SERIAL-001".into(),
            model: "FileImage".into(),
            size_bytes: 64 * 1024 * 1024,
            sector_size: 512,
            removable: true,
            is_system: false,
            contains_system_volume: false,
            partition_health: PartitionHealth::Unknown,
            bus: "file".into(),
        }
    }

    #[test]
    fn default_gate_blocks() {
        let d = sample_removable();
        let g = WriteGate::default();
        assert!(matches!(
            assert_physical_write_allowed(&d, &g),
            Err(SafetyError::DryRunActive)
        ));
    }

    #[test]
    fn refuses_fixed_even_with_flags() {
        let mut d = sample_removable();
        d.removable = false;
        let g = WriteGate {
            destroy_confirm: true,
            dry_run: false,
            typed_serial: None,
        };
        assert!(matches!(
            assert_physical_write_allowed(&d, &g),
            Err(SafetyError::NotRemovable)
        ));
    }

    #[test]
    fn refuses_system_disk() {
        let mut d = sample_removable();
        d.is_system = true;
        let g = WriteGate {
            destroy_confirm: true,
            dry_run: false,
            typed_serial: None,
        };
        assert!(matches!(
            assert_physical_write_allowed(&d, &g),
            Err(SafetyError::SystemDisk)
        ));
    }

    #[test]
    fn allows_removable_with_all_gates() {
        let d = sample_removable();
        let g = WriteGate {
            destroy_confirm: true,
            dry_run: false,
            typed_serial: Some("TEST-SERIAL-001".into()),
        };
        assert!(assert_physical_write_allowed(&d, &g).is_ok());
    }
}
