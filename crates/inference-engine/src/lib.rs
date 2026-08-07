//! Lightweight rule/statistical anomaly detector for mesh telemetry.
//! Status: working (statistical). Heavy ML runtimes are roadmap.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryWindow {
    pub cpu_pct: Vec<f32>,
    pub latency_ms: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnomalyReport {
    pub score: f32,
    pub reason: String,
    pub is_anomaly: bool,
}

pub struct InferenceEngine {
    pub cpu_z_threshold: f32,
    pub latency_z_threshold: f32,
}

impl Default for InferenceEngine {
    fn default() -> Self {
        Self { cpu_z_threshold: 2.5, latency_z_threshold: 2.5 }
    }
}

impl InferenceEngine {
    pub fn detect(&self, w: &TelemetryWindow) -> AnomalyReport {
        let (cpu_z, cpu_mean) = z_last(&w.cpu_pct);
        let (lat_z, _) = z_last(&w.latency_ms);
        let mut reasons = Vec::new();
        if cpu_z.abs() >= self.cpu_z_threshold {
            reasons.push(format!("cpu z={cpu_z:.2} mean={cpu_mean:.1}"));
        }
        if lat_z.abs() >= self.latency_z_threshold {
            reasons.push(format!("latency z={lat_z:.2}"));
        }
        let score = cpu_z.abs().max(lat_z.abs());
        AnomalyReport {
            is_anomaly: !reasons.is_empty(),
            score,
            reason: if reasons.is_empty() { "ok".into() } else { reasons.join("; ") },
        }
    }
}

fn z_last(xs: &[f32]) -> (f32, f32) {
    if xs.len() < 3 { return (0.0, xs.last().copied().unwrap_or(0.0)); }
    let mean = xs.iter().sum::<f32>() / xs.len() as f32;
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / xs.len() as f32;
    let std = var.sqrt().max(1e-3);
    let last = *xs.last().unwrap();
    ((last - mean) / std, mean)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spikes_flagged() {
        let mut cpu: Vec<f32> = (0..20).map(|_| 20.0).collect();
        cpu.push(95.0);
        let r = InferenceEngine::default().detect(&TelemetryWindow { cpu_pct: cpu, latency_ms: vec![1.0; 21] });
        assert!(r.is_anomaly);
    }
}
