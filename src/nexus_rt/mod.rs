//! Real-time frame-budget lane bridging Chimera node ↔ Nexus.

use std::time::Duration;

use chimera_nexus::frame::{FrameOutcome, FrameScheduler, RealtimeTask, Source};

#[derive(Clone)]
pub struct RealtimeLane {
    inner: std::sync::Arc<parking_lot::Mutex<FrameScheduler>>,
}

impl RealtimeLane {
    pub fn new(budget: Duration) -> Self {
        Self {
            inner: std::sync::Arc::new(parking_lot::Mutex::new(FrameScheduler::new(budget))),
        }
    }

    pub fn sixty_fps() -> Self {
        Self::new(Duration::from_nanos(
            chimera_nexus::frame::DEFAULT_FRAME_BUDGET_NS,
        ))
    }

    pub fn submit(
        &self,
        id: u64,
        cost_hint_ns: u64,
        deadline_ms: u64,
        peer_latency_ms: f32,
        local_fallback: Vec<u8>,
    ) {
        self.inner.lock().enqueue(RealtimeTask {
            id,
            cost_hint_ns,
            deadline_offset: Duration::from_millis(deadline_ms),
            peer_latency_ms,
            payload: Vec::new(),
            local_fallback_result: local_fallback,
        });
    }

    pub fn tick(&self, remote: &[(u64, Duration, Vec<u8>)]) -> Vec<FrameOutcome> {
        let mut g = self.inner.lock();
        let (_b, out) = g.tick_frame(remote);
        out
    }

    pub fn accepted_remote(outcomes: &[FrameOutcome]) -> usize {
        outcomes
            .iter()
            .filter(|o| {
                matches!(
                    o,
                    FrameOutcome::Accepted {
                        source: Source::Remote,
                        ..
                    }
                )
            })
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lane_fallback() {
        let lane = RealtimeLane::sixty_fps();
        lane.submit(9, 1000, 5, 1.0, b"L".to_vec());
        let out = lane.tick(&[(9, Duration::from_millis(50), b"R".to_vec())]);
        assert!(matches!(out[0], FrameOutcome::LocalFallback { .. }));
    }
}
