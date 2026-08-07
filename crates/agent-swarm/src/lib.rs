//! Decentralized agent-in-the-loop coordinators (rule-based scoring).

use std::collections::VecDeque;

use transport_quic::protocol::{AgentDigest, AgentProposal, Capabilities, NodeId, TaskId};

const TELEMETRY_RING: usize = 128;

#[derive(Debug, Clone, Default)]
pub struct TelemetrySample {
    pub cpu_pct: f32,
    pub mem_avail_mb: u64,
    pub thermal: f32,
    pub jitter_ms: f32,
    pub cache_hit: f32,
    pub load: f32,
}

pub struct LocalAgent {
    pub node_id: NodeId,
    ring: VecDeque<TelemetrySample>,
    pub last_digest: AgentDigest,
}

impl LocalAgent {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            ring: VecDeque::with_capacity(TELEMETRY_RING),
            last_digest: AgentDigest::default(),
        }
    }

    pub fn observe(&mut self, sample: TelemetrySample) {
        if self.ring.len() >= TELEMETRY_RING {
            self.ring.pop_front();
        }
        self.ring.push_back(sample);
        self.last_digest = self.score();
    }

    /// Sub-millisecond scoring expert system.
    pub fn score(&self) -> AgentDigest {
        let Some(last) = self.ring.back() else {
            return AgentDigest {
                willingness: 0.5,
                energy_cost: 0.5,
                predicted_latency_ms: 10.0,
                healing_pressure: 0.0,
            };
        };
        let pred_thermal = self.predict_thermal();
        let willingness = (1.0 - last.cpu_pct / 100.0)
            * (0.5 + 0.5 * last.cache_hit)
            * (1.0 - pred_thermal * 0.5)
            * (1.0 / (1.0 + last.load * 0.1));
        let energy_cost = (last.cpu_pct / 100.0) * 0.7 + pred_thermal * 0.3;
        let predicted_latency_ms = 2.0 + last.jitter_ms + last.load * 1.5;
        let healing_pressure = if pred_thermal > 0.8 || last.cpu_pct > 90.0 {
            1.0
        } else if self.trend_degrading() {
            0.6
        } else {
            0.0
        };
        AgentDigest {
            willingness: willingness.clamp(0.0, 1.0),
            energy_cost: energy_cost.clamp(0.0, 1.0),
            predicted_latency_ms,
            healing_pressure,
        }
    }

    pub fn predict_thermal(&self) -> f32 {
        if self.ring.len() < 3 {
            return self.ring.back().map(|s| s.thermal).unwrap_or(0.0);
        }
        let n = self.ring.len();
        let a = self.ring[n - 3].thermal;
        let b = self.ring[n - 2].thermal;
        let c = self.ring[n - 1].thermal;
        (c + (c - a) * 0.5).clamp(0.0, 1.0).max(b).min(1.0)
    }

    pub fn trend_degrading(&self) -> bool {
        if self.ring.len() < 5 {
            return false;
        }
        let n = self.ring.len();
        let early: f32 = self.ring.iter().take(n / 2).map(|s| s.cpu_pct).sum::<f32>()
            / (n / 2) as f32;
        let late: f32 = self.ring.iter().skip(n / 2).map(|s| s.cpu_pct).sum::<f32>()
            / (n - n / 2) as f32;
        late > early + 8.0
    }

    pub fn should_preempt_migrate(&self) -> bool {
        self.last_digest.healing_pressure > 0.7
    }

    pub fn propose_for_task(&self, task_id: TaskId, caps: &Capabilities) -> AgentProposal {
        let digest = self.score();
        let score = digest.willingness * 0.6
            + (1.0 - digest.energy_cost) * 0.2
            + (1.0 / (1.0 + digest.predicted_latency_ms / 50.0)) * 0.2
            + if caps.gpu_hint { 0.05 } else { 0.0 };
        AgentProposal {
            from: self.node_id,
            task_id,
            score,
            reason: format!(
                "will={:.2} energy={:.2} lat={:.1}ms heal={:.2}",
                digest.willingness,
                digest.energy_cost,
                digest.predicted_latency_ms,
                digest.healing_pressure
            ),
        }
    }

    pub fn from_caps(caps: &Capabilities) -> TelemetrySample {
        TelemetrySample {
            cpu_pct: caps.cpu_util_pct,
            mem_avail_mb: caps.mem_avail_mb,
            thermal: caps.thermal_pressure,
            jitter_ms: caps.network_jitter_ms,
            cache_hit: caps.cache_hit_rate,
            load: caps.load_score,
        }
    }
}

#[cfg(feature = "ml-agent")]
pub mod ml {
    //! Optional tract/burn micro-model plumbing — not required for default builds.
    pub fn inference_stub(_features: &[f32]) -> f32 {
        0.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn agent_scores_after_observe() {
        let mut agent = LocalAgent::new(NodeId(Uuid::nil()));
        agent.observe(TelemetrySample {
            cpu_pct: 10.0,
            mem_avail_mb: 4096,
            thermal: 0.1,
            jitter_ms: 1.0,
            cache_hit: 0.9,
            load: 0.2,
        });
        let d = agent.score();
        assert!(d.willingness > 0.0);
        assert!(d.willingness <= 1.0);
    }
}
