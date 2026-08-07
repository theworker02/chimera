//! chimera-boot — Boot-Sovereign USB flash / recovery CLI.
//!
//! Defaults to dry-run. Never writes physical disks without explicit gates.
//! See ADR-0023.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use chimera_boot::block::{BlockTarget, FileImageTarget, PhysicalDiskTarget};
use chimera_boot::enumerate::list_disks;
use chimera_boot::firmware::detect_firmware_mode;
use chimera_boot::flash::{
    flash_file_image, flash_physical, verify_blake3, FlashPlan, PartitionScheme, VolumeFilesystem,
};
use chimera_boot::progress::FlashProgress;
use chimera_boot::repair::{repair_file_image, repair_physical, RepairPlan};
use chimera_boot::safety::WriteGate;

#[derive(Parser, Debug)]
#[command(
    name = "chimera-boot",
    about = "Chimera Boot-Sovereign USB flasher (DATA LOSS RISK — dry-run ON by default)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// List physical disks (READ-ONLY)
    List,
    /// Flash ISO/payload (default dry-run)
    Flash {
        #[arg(long)]
        iso: Option<PathBuf>,
        #[arg(long)]
        target: String,
        #[arg(long)]
        image: bool,
        #[arg(long, default_value = "GPT")]
        scheme: String,
        #[arg(long, default_value = "FAT32")]
        filesystem: String,
        #[arg(long, default_value = "CHIMERA")]
        label: String,
        #[arg(long, default_value_t = false)]
        no_dry_run: bool,
        #[arg(long = "yes-i-understand-this-destroys-data", default_value_t = false)]
        destroy_confirm: bool,
        #[arg(long)]
        confirm_serial: Option<String>,
        #[arg(long)]
        payload: Option<PathBuf>,
        #[arg(long, default_value_t = 64)]
        image_mib: u64,
    },
    Verify {
        #[arg(long)]
        target: String,
        #[arg(long)]
        image: bool,
        #[arg(long)]
        hash: String,
        #[arg(long, default_value_t = 0)]
        bytes: u64,
    },
    Repair {
        #[arg(long)]
        target: String,
        #[arg(long)]
        image: bool,
        #[arg(long, default_value_t = false)]
        no_dry_run: bool,
        #[arg(long = "yes-i-understand-this-destroys-data", default_value_t = false)]
        destroy_confirm: bool,
        #[arg(long)]
        confirm_serial: Option<String>,
        #[arg(long)]
        inject_kernel: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::List => {
            println!("firmware_mode={:?}", detect_firmware_mode());
            let disks = list_disks()?;
            if disks.is_empty() {
                println!(
                    "(no physical drives enumerated — elevation may be required; use --image for lab)"
                );
            }
            for d in disks {
                println!(
                    "{}\tpath={}\tserial={}\tremovable={}\tsystem={}",
                    d.id,
                    d.path,
                    d.serial,
                    d.removable,
                    d.is_system || d.contains_system_volume
                );
            }
        }
        Cmd::Flash {
            iso,
            target,
            image,
            scheme,
            filesystem,
            label,
            no_dry_run,
            destroy_confirm,
            confirm_serial,
            payload,
            image_mib,
        } => {
            eprintln!("DANGER: dry_run={}", !no_dry_run);
            let scheme = match scheme.to_ascii_uppercase().as_str() {
                "GPT" => PartitionScheme::Gpt,
                "MBR" => PartitionScheme::Mbr,
                o => bail!("unknown scheme {o}"),
            };
            let filesystem = match filesystem.to_ascii_uppercase().as_str() {
                "FAT32" => VolumeFilesystem::Fat32,
                "NTFS" => VolumeFilesystem::Ntfs,
                o => bail!("unknown filesystem {o}"),
            };
            let plan = FlashPlan {
                iso_path: iso,
                payload: payload.map(|p| std::fs::read(p)).transpose()?,
                scheme,
                filesystem,
                volume_label: label,
                gate: WriteGate {
                    destroy_confirm,
                    dry_run: !no_dry_run,
                    typed_serial: confirm_serial,
                },
                efi_stub: None,
                mbr_bootstrap: None,
            };
            let progress: chimera_boot::ProgressCallback = Box::new(|p: &FlashProgress| {
                eprintln!("[{}] {:.1}% {} B/s", p.stage, p.pct(), p.bytes_per_sec);
            });
            if image {
                let path = PathBuf::from(&target);
                let mut img = if path.exists() {
                    FileImageTarget::open(&path, 512)?
                } else {
                    FileImageTarget::create(&path, image_mib * 1024 * 1024, 512)?
                };
                let r = flash_file_image(&mut img, &plan, Some(progress))?;
                println!("{}", serde_json::to_string_pretty(&r)?);
            } else {
                let disk = list_disks()?
                    .into_iter()
                    .find(|d| d.path == target || d.id == target)
                    .with_context(|| format!("disk not found: {target}"))?;
                let r = flash_physical(disk, &plan, Some(progress))?;
                println!("{}", serde_json::to_string_pretty(&r)?);
            }
        }
        Cmd::Verify {
            target,
            image,
            hash,
            bytes,
        } => {
            if image {
                let mut img = FileImageTarget::open(&target, 512)?;
                let n = if bytes == 0 { img.size_bytes() } else { bytes };
                let ok = verify_blake3(&mut img, n, &hash, None)?;
                println!("{{\"ok\":{ok},\"bytes\":{n}}}");
            } else {
                let disk = list_disks()?
                    .into_iter()
                    .find(|d| d.path == target || d.id == target)
                    .with_context(|| format!("disk not found: {target}"))?;
                let mut t = PhysicalDiskTarget::open_readonly(disk)?;
                let n = if bytes == 0 { t.size_bytes() } else { bytes };
                let ok = verify_blake3(&mut t, n, &hash, None)?;
                println!("{{\"ok\":{ok},\"bytes\":{n}}}");
            }
        }
        Cmd::Repair {
            target,
            image,
            no_dry_run,
            destroy_confirm,
            confirm_serial,
            inject_kernel,
        } => {
            let plan = RepairPlan {
                gate: WriteGate {
                    destroy_confirm,
                    dry_run: !no_dry_run,
                    typed_serial: confirm_serial,
                },
                mbr_bootstrap: None,
                inject_kernel: inject_kernel.map(std::fs::read).transpose()?,
                kernel_lba: 2048,
            };
            let dry = plan.gate.dry_run;
            if image {
                let mut img = FileImageTarget::open(&target, 512)?;
                repair_file_image(&mut img, &plan)?;
            } else {
                let disk = list_disks()?
                    .into_iter()
                    .find(|d| d.path == target || d.id == target)
                    .with_context(|| format!("disk not found: {target}"))?;
                repair_physical(disk, &plan)?;
            }
            println!("{{\"ok\":true,\"dry_run\":{dry}}}");
        }
    }
    Ok(())
}
