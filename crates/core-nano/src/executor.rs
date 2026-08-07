//! Nano-kernel task executor with degradation policies.

use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::determinism::FixedPoint;
use crate::hw::HwProfile;
use crate::replay::{kinds, TxLog};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NanoTask {
    pub id: u64,
    pub seed: u64,
    pub elements: u32,
    /// When true, use FixedPoint path instead of SoftF32.
    pub consensus_math: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskOutcome {
    pub id: u64,
    pub checksum: u64,
    pub elements_done: u32,
    pub degraded: bool,
}

pub struct NanoExecutor {
    pub profile: HwProfile,
    pub log: TxLog,
    /// Precision / downsample knobs for constrained profiles.
    pub max_elements: u32,
    pub force_fixed_point: bool,
}

impl NanoExecutor {
    pub fn new(profile: HwProfile) -> Self {
        let (max_elements, force_fixed_point) = degradation_policy(&profile);
        Self {
            profile,
            log: TxLog::new(),
            max_elements,
            force_fixed_point,
        }
    }

    pub fn run(&mut self, task: NanoTask) -> TaskOutcome {
        self.log
            .append(kinds::TASK_SPAWN, &task.id.to_le_bytes());

        let n = task.elements.min(self.max_elements);
        let use_fp = task.consensus_math || self.force_fixed_point;
        let degraded = n < task.elements || use_fp != task.consensus_math && self.force_fixed_point;

        let mut checksum = task.seed ^ 0xC0FFEE;
        // Always use FixedPoint on no_std (no libm sin); SoftF32 path is host-only demo.
        let use_fp = use_fp || cfg!(not(feature = "std"));
        if use_fp {
            let mut acc = FixedPoint::from_i32(0);
            for i in 0..n {
                let v = FixedPoint::from_f32_truncated((task.seed as f32) * 1e-6 + i as f32 * 0.01);
                let out = v.mul(FixedPoint::from_f32_truncated(1.618034));
                acc = acc.add(out);
                checksum = checksum
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    .wrapping_add(acc.0 as u64);
            }
        } else {
            #[cfg(feature = "std")]
            {
                for i in 0..n {
                    let v = (task.seed as f32) * 1e-6 + i as f32 * 0.01;
                    let out = (v * 1.618_034).sin().abs();
                    checksum = checksum
                        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                        .wrapping_add(out.to_bits() as u64);
                }
            }
        }

        let outcome = TaskOutcome {
            id: task.id,
            checksum,
            elements_done: n,
            degraded,
        };
        let payload = postcard::to_allocvec(&outcome).unwrap_or_default();
        self.log.append(kinds::TASK_COMPLETE, &payload);
        outcome
    }

    /// Recover executor counter-state from log (demo of deterministic replay).
    pub fn recover_completed_ids(log: &TxLog) -> Result<Vec<u64>, crate::replay::ReplayError> {
        log.replay(Vec::new(), |ids, e| {
            if e.kind == kinds::TASK_COMPLETE {
                if let Ok(o) = postcard::from_bytes::<TaskOutcome>(&e.payload) {
                    ids.push(o.id);
                }
            }
        })
    }
}

fn degradation_policy(profile: &HwProfile) -> (u32, bool) {
    if profile.ram_bytes < 64 * 1024 {
        (256, true)
    } else if profile.ram_bytes < 1024 * 1024 {
        (2048, true)
    } else if !profile.isa.has_float {
        (4096, true)
    } else {
        (u32::MAX, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hw::{HwProfile, IsaHints};

    #[test]
    fn constrained_degrades() {
        let profile = HwProfile {
            ram_bytes: 32 * 1024,
            isa: IsaHints {
                has_float: false,
                ..IsaHints::none()
            },
            ..HwProfile::minimal()
        };
        let mut ex = NanoExecutor::new(profile);
        let out = ex.run(NanoTask {
            id: 1,
            seed: 9,
            elements: 10_000,
            consensus_math: false,
        });
        assert!(out.degraded);
        assert!(out.elements_done <= 256);
        let ids = NanoExecutor::recover_completed_ids(&ex.log).unwrap();
        assert_eq!(ids, vec![1]);
    }
}
