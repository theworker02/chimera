//! Hardware-agnostic retro-scaling — pick Wasm tier + caps from a profile.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecTier {
    /// Wasmtime JIT — capable hosts. **Status: working**
    WasmtimeJit,
    /// CNK wasmi interpreter — constrained / portable. **Status: working**
    WasmiInterpreter,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScalingProfile {
    pub name: String,
    pub cpu_cores: u32,
    pub ram_mib: u64,
    pub prefer_interpreter: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecPolicy {
    pub tier: ExecTier,
    pub wasm_memory_mib: u64,
    pub fuel: u64,
    pub max_parallel_tasks: u32,
    /// Fixed-point / downsample factor for constrained paths (1.0 = full precision).
    pub precision_scale: f32,
    pub degrade: bool,
}

impl ScalingProfile {
    pub fn tiny() -> Self {
        Self {
            name: "tiny".into(),
            cpu_cores: 1,
            ram_mib: 64,
            prefer_interpreter: true,
        }
    }

    pub fn big() -> Self {
        Self {
            name: "big".into(),
            cpu_cores: 16,
            ram_mib: 32_768,
            prefer_interpreter: false,
        }
    }

    pub fn from_caps(cpu_cores: u32, ram_mib: u64) -> Self {
        Self {
            name: "host".into(),
            cpu_cores: cpu_cores.max(1),
            ram_mib: ram_mib.max(16),
            prefer_interpreter: ram_mib < 256 || cpu_cores <= 1,
        }
    }
}

pub struct RetroScaler;

impl RetroScaler {
    /// Select tier and caps. Constrained profiles **degrade** (lower fuel/mem/precision)
    /// instead of dropping work.
    pub fn plan(profile: &ScalingProfile) -> ExecPolicy {
        let constrained = profile.prefer_interpreter
            || profile.ram_mib < 256
            || profile.cpu_cores <= 1;
        if constrained {
            ExecPolicy {
                tier: ExecTier::WasmiInterpreter,
                wasm_memory_mib: profile.ram_mib.min(16).max(2),
                fuel: 500_000,
                max_parallel_tasks: 1,
                precision_scale: 0.5,
                degrade: true,
            }
        } else {
            let mem = (profile.ram_mib / 32).clamp(32, 512);
            ExecPolicy {
                tier: ExecTier::WasmtimeJit,
                wasm_memory_mib: mem,
                fuel: 50_000_000,
                max_parallel_tasks: profile.cpu_cores.max(2),
                precision_scale: 1.0,
                degrade: false,
            }
        }
    }

    /// Apply precision degradation to a sample buffer (downsampling stand-in).
    pub fn degrade_samples(samples: &[f32], scale: f32) -> Vec<f32> {
        if scale >= 0.999 {
            return samples.to_vec();
        }
        // Round to coarser fixed-point steps
        let step = (1.0 / scale.max(0.05)).round().max(1.0);
        samples
            .iter()
            .map(|v| ((*v) * step).round() / step)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_selects_interpreter_and_degrades() {
        let p = RetroScaler::plan(&ScalingProfile::tiny());
        assert_eq!(p.tier, ExecTier::WasmiInterpreter);
        assert!(p.degrade);
        assert!(p.wasm_memory_mib <= 16);
        assert_eq!(p.max_parallel_tasks, 1);
    }

    #[test]
    fn big_selects_jit() {
        let p = RetroScaler::plan(&ScalingProfile::big());
        assert_eq!(p.tier, ExecTier::WasmtimeJit);
        assert!(!p.degrade);
        assert!(p.max_parallel_tasks >= 2);
    }

    #[test]
    fn constrained_does_not_drop_work() {
        let p = RetroScaler::plan(&ScalingProfile::tiny());
        // Policy always returns a runnable plan (fuel > 0)
        assert!(p.fuel > 0);
        let out = RetroScaler::degrade_samples(&[1.234, 5.678], p.precision_scale);
        assert_eq!(out.len(), 2);
    }
}
