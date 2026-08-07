//! Boot-time hardware profiler / static capability descriptors.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IsaHints {
    pub has_float: bool,
    pub simd_sse2: bool,
    pub simd_avx2: bool,
    pub simd_avx512: bool,
    pub simd_neon: bool,
    pub simd_rvv: bool,
    pub atomics: bool,
}

impl IsaHints {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn host_detect() -> Self {
        #[cfg(feature = "std")]
        {
            detect_std()
        }
        #[cfg(not(feature = "std"))]
        {
            // Static descriptors for bare-metal — fill from build scripts / PAC later.
            Self {
                has_float: cfg!(any(target_feature = "f", target_arch = "x86_64")),
                atomics: true,
                ..Self::default()
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HwProfile {
    pub ram_bytes: u64,
    pub cpu_cores: u32,
    pub isa: IsaHints,
    pub platform_name: &'static str,
    /// Prefer wasmi interpreter vs host JIT.
    pub prefer_interpreter: bool,
}

impl HwProfile {
    pub fn minimal() -> Self {
        Self {
            ram_bytes: 64 * 1024,
            cpu_cores: 1,
            isa: IsaHints::none(),
            platform_name: "minimal",
            prefer_interpreter: true,
        }
    }

    pub fn host() -> Self {
        #[cfg(feature = "std")]
        {
            let ram = detect_ram_bytes();
            Self {
                ram_bytes: ram,
                cpu_cores: std::thread::available_parallelism()
                    .map(|n| n.get() as u32)
                    .unwrap_or(1),
                isa: IsaHints::host_detect(),
                platform_name: "host-std",
                prefer_interpreter: false,
            }
        }
        #[cfg(not(feature = "std"))]
        {
            Self::minimal()
        }
    }
}

#[cfg(feature = "std")]
fn detect_ram_bytes() -> u64 {
    // Best-effort; sysinfo is in the parent crate — avoid heavy deps here.
    8 * 1024 * 1024 * 1024
}

#[cfg(feature = "std")]
fn detect_std() -> IsaHints {
    #[allow(unused_mut)]
    let mut h = IsaHints {
        has_float: true,
        atomics: true,
        ..IsaHints::default()
    };
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        h.simd_sse2 = is_x86_feature_detected!("sse2");
        h.simd_avx2 = is_x86_feature_detected!("avx2");
        // avx512f may not be on all hosts — detect safely.
        h.simd_avx512 = is_x86_feature_detected!("avx512f");
    }
    #[cfg(target_arch = "aarch64")]
    {
        h.simd_neon = true;
    }
    h
}
