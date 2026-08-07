//! Progress reporting for flash / verify pipelines.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashProgress {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub stage: String,
    /// Instantaneous throughput estimate (bytes/sec), 0 if unknown.
    pub bytes_per_sec: u64,
}

pub type ProgressCallback = Box<dyn FnMut(&FlashProgress) + Send>;

impl FlashProgress {
    pub fn pct(&self) -> f64 {
        if self.bytes_total == 0 {
            0.0
        } else {
            100.0 * self.bytes_done as f64 / self.bytes_total as f64
        }
    }
}
