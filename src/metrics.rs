//! Live cluster metrics for TUI / agents.

use std::sync::Arc;

use parking_lot::RwLock;
use sysinfo::System;

use crate::protocol::Capabilities;

#[derive(Debug, Clone, Default)]
pub struct ClusterMetrics {
    pub local_caps: Capabilities,
    pub peers: usize,
    pub pending_tasks: usize,
    pub running_tasks: usize,
    pub completed_tasks: u64,
    pub throughput_slices_per_min: f32,
    pub bytes_read: u64,
    pub bytes_written: u64,
    pub fs_cache_hits: u64,
    pub fs_cache_misses: u64,
    pub fs_blocks: u64,
    pub mem_regions: usize,
    pub mem_local_pages: usize,
    pub mem_faults: u64,
    pub migrations: u64,
    pub agent_willingness: f32,
    pub agent_healing: f32,
    pub verified_receipts: u64,
    pub intents_compiled: u64,
}

#[derive(Clone)]
pub struct MetricsHub {
    inner: Arc<RwLock<ClusterMetrics>>,
    sys: Arc<RwLock<System>>,
}

impl MetricsHub {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self {
            inner: Arc::new(RwLock::new(ClusterMetrics::default())),
            sys: Arc::new(RwLock::new(sys)),
        }
    }

    pub fn snapshot(&self) -> ClusterMetrics {
        self.inner.read().clone()
    }

    pub fn update<F: FnOnce(&mut ClusterMetrics)>(&self, f: F) {
        f(&mut self.inner.write());
    }

    pub fn sample_caps(&self, load_score: f32, cache_hit: f32) -> Capabilities {
        let mut sys = self.sys.write();
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        let cpu = sys.global_cpu_usage();
        let cores = sys.cpus().len().max(1) as u32;
        let total = sys.total_memory() / (1024 * 1024);
        let avail = sys.available_memory() / (1024 * 1024);
        let thermal = (cpu / 100.0).clamp(0.0, 1.0);
        Capabilities {
            cpu_cores: cores,
            cpu_util_pct: cpu,
            mem_total_mb: total,
            mem_avail_mb: avail,
            gpu_hint: false,
            load_score,
            thermal_pressure: thermal,
            cache_hit_rate: cache_hit,
            network_jitter_ms: 1.0,
        }
    }
}

impl Default for MetricsHub {
    fn default() -> Self {
        Self::new()
    }
}
