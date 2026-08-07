//! Platform layers: host shim + bare-metal skeletons.

use crate::hw::HwProfile;

#[derive(Debug, Clone)]
pub struct BootReport {
    pub platform: &'static str,
    pub ram_bytes: u64,
    pub prefer_interpreter: bool,
    pub notes: &'static str,
}

pub fn boot(profile: HwProfile) -> BootReport {
    #[cfg(feature = "host")]
    {
        return host::boot_host(profile);
    }
    #[cfg(all(not(feature = "host"), feature = "uefi"))]
    {
        return uefi::boot_stub(profile);
    }
    #[cfg(all(not(feature = "host"), not(feature = "uefi"), feature = "cortex-m"))]
    {
        return cortex_m::boot_stub(profile);
    }
    #[cfg(all(
        not(feature = "host"),
        not(feature = "uefi"),
        not(feature = "cortex-m"),
        feature = "riscv"
    ))]
    {
        return riscv::boot_stub(profile);
    }
    #[allow(unreachable_code)]
    {
        BootReport {
            platform: profile.platform_name,
            ram_bytes: profile.ram_bytes,
            prefer_interpreter: profile.prefer_interpreter,
            notes: "bare CNK core — no platform feature selected",
        }
    }
}

#[cfg(feature = "host")]
pub mod host {
    use super::*;
    use crate::executor::{NanoExecutor, NanoTask};
    use crate::memory::ImmutableRegion;
    use crate::replay::kinds;

    pub fn boot_host(profile: HwProfile) -> BootReport {
        let mut exec = NanoExecutor::new(profile.clone());
        let _ = exec.run(NanoTask {
            id: 0,
            seed: 1,
            elements: 64,
            consensus_math: true,
        });
        exec.log.append(kinds::CHECKPOINT, b"host-boot");
        let mut region = ImmutableRegion::new(256, 64);
        let _ = region.write(0, b"CNK-HOST");
        region.seal();
        BootReport {
            platform: "host-std",
            ram_bytes: profile.ram_bytes,
            prefer_interpreter: profile.prefer_interpreter,
            notes: "Host-simulated boot OK (Windows/desktop). UEFI/MCU boots are scaffolding only.",
        }
    }
}

#[cfg(feature = "uefi")]
pub mod uefi {
    use super::*;
    pub fn boot_stub(profile: HwProfile) -> BootReport {
        BootReport {
            platform: "uefi-x86_64-stub",
            ram_bytes: profile.ram_bytes,
            prefer_interpreter: true,
            notes: "UEFI stub — link with x86_64-unknown-uefi; real boot untested",
        }
    }
}

#[cfg(feature = "cortex-m")]
pub mod cortex_m {
    use super::*;
    pub fn boot_stub(profile: HwProfile) -> BootReport {
        BootReport {
            platform: "cortex-m-stub",
            ram_bytes: profile.ram_bytes,
            prefer_interpreter: true,
            notes: "Cortex-M stub — use thumbv7em-none-eabihf; requires PAC + linker script",
        }
    }
}

#[cfg(feature = "riscv")]
pub mod riscv {
    use super::*;
    pub fn boot_stub(profile: HwProfile) -> BootReport {
        BootReport {
            platform: "riscv-stub",
            ram_bytes: profile.ram_bytes,
            prefer_interpreter: true,
            notes: "RISC-V stub — use riscv32imac-unknown-none-elf; requires board support",
        }
    }
}
