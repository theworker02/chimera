//! Flash telemetry — throughput samples; thermal/SMART only when available.

use serde::{Deserialize, Serialize};

use crate::progress::FlashProgress;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelemetrySample {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub bytes_per_sec: u64,
    pub stage: String,
    /// Media temperature °C if obtainable via SMART; `None` = unavailable (not fabricated).
    pub media_temp_c: Option<f32>,
}

#[derive(Debug, Default)]
pub struct TelemetryStream {
    pub samples: Vec<TelemetrySample>,
    pub max_samples: usize,
}

impl TelemetryStream {
    pub fn new(max_samples: usize) -> Self {
        Self {
            samples: Vec::new(),
            max_samples: max_samples.max(8),
        }
    }

    pub fn on_progress(&mut self, p: &FlashProgress) {
        self.samples.push(TelemetrySample {
            bytes_done: p.bytes_done,
            bytes_total: p.bytes_total,
            bytes_per_sec: p.bytes_per_sec,
            stage: p.stage.clone(),
            media_temp_c: read_smart_temp_c(), // None unless platform provides it
        });
        if self.samples.len() > self.max_samples {
            self.samples.remove(0);
        }
    }

    pub fn sparkline_bps(&self) -> Vec<u64> {
        self.samples.iter().map(|s| s.bytes_per_sec).collect()
    }
}

/// SMART temperature — Phase 14 returns `None` (unavailable) rather than inventing values.
pub fn read_smart_temp_c() -> Option<f32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::FlashProgress;

    #[test]
    fn records_throughput_without_fake_thermal() {
        let mut t = TelemetryStream::new(16);
        t.on_progress(&FlashProgress {
            bytes_done: 1000,
            bytes_total: 2000,
            stage: "iso-write".into(),
            bytes_per_sec: 50_000_000,
        });
        assert_eq!(t.samples.len(), 1);
        assert!(t.samples[0].media_temp_c.is_none());
        assert_eq!(t.sparkline_bps()[0], 50_000_000);
    }
}
