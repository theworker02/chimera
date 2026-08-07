//! Chimera Boot-Sovereign (`chimera-boot`) — safety-gated USB flash / recovery.
//!
//! # Safety
//! Physical disk **writes** require ALL of:
//! 1. `--yes-i-understand-this-destroys-data` (or typed serial confirmation)
//! 2. Removable-media check that **hard-refuses** fixed/system disks
//! 3. Explicit `--no-dry-run` (dry-run is **ON** by default)
//!
//! All unit/integration tests use [`FileImageTarget`] only. Real-hardware
//! flashing is **UNTESTED** in CI and development.

pub mod block;
pub mod boot;
pub mod crc32;
pub mod enumerate;
pub mod fat32;
pub mod firmware;
pub mod flash;
pub mod gpt;
pub mod mbr;
pub mod ntfs;
pub mod progress;
pub mod repair;
pub mod safety;
pub mod telemetry;

pub use block::{BlockTarget, FileImageTarget, PhysicalDiskTarget};
pub use enumerate::{DiskInfo, PartitionHealth};
pub use flash::{FlashPlan, FlashResult, PartitionScheme, VolumeFilesystem};
pub use progress::{FlashProgress, ProgressCallback};
pub use safety::{SafetyError, WriteGate};

/// Library version (Phase 14).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
