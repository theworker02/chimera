//! Self-healing autoscaler + priority traffic shedder.

use serde::{Deserialize, Serialize};

use crate::agent::TelemetrySample;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScaleDecision {
    pub function: String,
    pub tenant: String,
    pub from: u32,
    pub to: u32,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShedAction {
    Allow,
    Shed,
}

pub struct AutoScaler {
    pub scale_out_cpu: f32,
    pub scale_in_cpu: f32,
    pub max_instances: u32,
    pub min_instances: u32,
    pub queue_high: u32,
}

impl Default for AutoScaler {
    fn default() -> Self {
        Self {
            scale_out_cpu: 70.0,
            scale_in_cpu: 25.0,
            max_instances: 16,
            min_instances: 1,
            queue_high: 32,
        }
    }
}

impl AutoScaler {
    pub fn decide(
        &self,
        tenant: &str,
        function: &str,
        current: u32,
        sample: &TelemetrySample,
        queue_depth: u32,
    ) -> Option<ScaleDecision> {
        if sample.cpu_pct >= self.scale_out_cpu || queue_depth >= self.queue_high {
            let to = (current + 1).min(self.max_instances);
            if to > current {
                return Some(ScaleDecision {
                    function: function.into(),
                    tenant: tenant.into(),
                    from: current,
                    to,
                    reason: format!(
                        "scale-out cpu={:.1} queue={queue_depth}",
                        sample.cpu_pct
                    ),
                });
            }
        }
        if sample.cpu_pct <= self.scale_in_cpu && queue_depth == 0 && current > self.min_instances {
            let to = current - 1;
            return Some(ScaleDecision {
                function: function.into(),
                tenant: tenant.into(),
                from: current,
                to,
                reason: format!("scale-in cpu={:.1}", sample.cpu_pct),
            });
        }
        None
    }
}

/// Priority shedder: realtime (priority≥200) always allowed; low priority shed under saturation.
pub struct TrafficShedder {
    pub cpu_saturate: f32,
    pub realtime_priority: u8,
}

impl Default for TrafficShedder {
    fn default() -> Self {
        Self {
            cpu_saturate: 90.0,
            realtime_priority: 200,
        }
    }
}

impl TrafficShedder {
    pub fn admit(&self, priority: u8, sample: &TelemetrySample, queue_depth: u32) -> ShedAction {
        if priority >= self.realtime_priority {
            return ShedAction::Allow;
        }
        if sample.cpu_pct >= self.cpu_saturate || queue_depth > 64 {
            return ShedAction::Shed;
        }
        ShedAction::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(cpu: f32) -> TelemetrySample {
        TelemetrySample {
            cpu_pct: cpu,
            mem_avail_mb: 1024,
            thermal: cpu / 100.0,
            jitter_ms: 1.0,
            cache_hit: 0.9,
            load: cpu / 10.0,
        }
    }

    #[test]
    fn scale_out_on_spike() {
        let s = AutoScaler::default();
        let d = s.decide("t", "f", 1, &sample(85.0), 40).unwrap();
        assert_eq!(d.to, 2);
    }

    #[test]
    fn shed_low_priority_when_hot() {
        let sh = TrafficShedder::default();
        assert_eq!(sh.admit(10, &sample(95.0), 10), ShedAction::Shed);
        assert_eq!(sh.admit(200, &sample(95.0), 10), ShedAction::Allow);
    }
}
