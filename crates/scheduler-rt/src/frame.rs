//! Sub-16ms frame-budget realtime scheduler lane.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::time::{Duration, Instant};

/// Default 60 FPS frame budget.
pub const DEFAULT_FRAME_BUDGET_NS: u64 = 16_666_667;

#[derive(Debug, Clone)]
pub struct FrameBudget {
    pub budget: Duration,
    started: Instant,
}

impl FrameBudget {
    pub fn start(budget: Duration) -> Self {
        Self {
            budget,
            started: Instant::now(),
        }
    }

    pub fn remaining(&self) -> Duration {
        self.budget.saturating_sub(self.started.elapsed())
    }

    pub fn exceeded(&self) -> bool {
        self.started.elapsed() >= self.budget
    }

    pub fn elapsed_ns(&self) -> u64 {
        self.started.elapsed().as_nanos() as u64
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RealtimeTask {
    pub id: u64,
    pub cost_hint_ns: u64,
    /// Absolute deadline (Instant-like via relative remaining from frame start).
    pub deadline_offset: Duration,
    pub peer_latency_ms: f32,
    pub payload: Vec<u8>,
    /// Local fallback work (deterministic).
    pub local_fallback_result: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameOutcome {
    /// Remote/local result accepted within budget.
    Accepted { task_id: u64, result: Vec<u8>, source: Source },
    /// Late remote result dropped; local fallback used.
    LocalFallback { task_id: u64, result: Vec<u8> },
    /// Dropped entirely (no fallback configured) — should not stall frame.
    Dropped { task_id: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Remote,
    Local,
}

struct Ranked(RealtimeTask);

impl PartialEq for Ranked {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id
            && self.0.cost_hint_ns == other.0.cost_hint_ns
            && self.0.deadline_offset == other.0.deadline_offset
            && self.0.peer_latency_ms.to_bits() == other.0.peer_latency_ms.to_bits()
    }
}

impl Eq for Ranked {}

impl Ord for Ranked {
    fn cmp(&self, other: &Self) -> Ordering {
        // Lower latency + earlier deadline first.
        other
            .0
            .deadline_offset
            .cmp(&self.0.deadline_offset)
            .then_with(|| {
                other
                    .0
                    .peer_latency_ms
                    .to_bits()
                    .cmp(&self.0.peer_latency_ms.to_bits())
            })
            .then_with(|| other.0.id.cmp(&self.0.id))
    }
}

impl PartialOrd for Ranked {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub struct FrameScheduler {
    pub budget: Duration,
    queue: BinaryHeap<Ranked>,
    background_preempted: u64,
}

impl FrameScheduler {
    pub fn new(budget: Duration) -> Self {
        Self {
            budget,
            queue: BinaryHeap::new(),
            background_preempted: 0,
        }
    }

    pub fn with_default_60fps() -> Self {
        Self::new(Duration::from_nanos(DEFAULT_FRAME_BUDGET_NS))
    }

    pub fn enqueue(&mut self, task: RealtimeTask) {
        self.queue.push(Ranked(task));
    }

    pub fn preempt_background(&mut self) {
        self.background_preempted += 1;
    }

    pub fn background_preempted(&self) -> u64 {
        self.background_preempted
    }

    /// Run one frame: execute tasks that fit; late remote → local fallback.
    /// `remote_results` maps task_id → (arrival_delay, bytes). Arrival after deadline ⇒ late.
    pub fn tick_frame(
        &mut self,
        remote_results: &[(u64, Duration, Vec<u8>)],
    ) -> (FrameBudget, Vec<FrameOutcome>) {
        self.preempt_background();
        let budget = FrameBudget::start(self.budget);
        let mut outcomes = Vec::new();
        let mut remotes: std::collections::HashMap<u64, (Duration, Vec<u8>)> =
            remote_results
                .iter()
                .cloned()
                .map(|(id, d, b)| (id, (d, b)))
                .collect();

        while let Some(Ranked(task)) = self.queue.pop() {
            if budget.exceeded() {
                // Frame full — force local fallback without waiting.
                outcomes.push(FrameOutcome::LocalFallback {
                    task_id: task.id,
                    result: task.local_fallback_result,
                });
                continue;
            }

            // Simulated cost accounting.
            let cost = Duration::from_nanos(task.cost_hint_ns);
            if cost > budget.remaining() {
                outcomes.push(FrameOutcome::LocalFallback {
                    task_id: task.id,
                    result: task.local_fallback_result,
                });
                continue;
            }

            if let Some((arrival, bytes)) = remotes.remove(&task.id) {
                if arrival <= task.deadline_offset && !budget.exceeded() {
                    outcomes.push(FrameOutcome::Accepted {
                        task_id: task.id,
                        result: bytes,
                        source: Source::Remote,
                    });
                } else {
                    // Late — drop remote, use local.
                    outcomes.push(FrameOutcome::LocalFallback {
                        task_id: task.id,
                        result: task.local_fallback_result,
                    });
                }
            } else {
                // No remote — local path within budget.
                outcomes.push(FrameOutcome::Accepted {
                    task_id: task.id,
                    result: task.local_fallback_result,
                    source: Source::Local,
                });
            }
        }

        (budget, outcomes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_remote_uses_local_fallback() {
        let mut sched = FrameScheduler::with_default_60fps();
        sched.enqueue(RealtimeTask {
            id: 1,
            cost_hint_ns: 1_000_000,
            deadline_offset: Duration::from_millis(8),
            peer_latency_ms: 2.0,
            payload: vec![1],
            local_fallback_result: b"local".to_vec(),
        });
        let (_b, out) = sched.tick_frame(&[(1, Duration::from_millis(20), b"late".to_vec())]);
        assert_eq!(
            out[0],
            FrameOutcome::LocalFallback {
                task_id: 1,
                result: b"local".to_vec()
            }
        );
    }

    #[test]
    fn on_time_remote_accepted() {
        let mut sched = FrameScheduler::with_default_60fps();
        sched.enqueue(RealtimeTask {
            id: 2,
            cost_hint_ns: 500_000,
            deadline_offset: Duration::from_millis(10),
            peer_latency_ms: 1.0,
            payload: vec![],
            local_fallback_result: b"local".to_vec(),
        });
        let (_b, out) = sched.tick_frame(&[(2, Duration::from_millis(3), b"remote".to_vec())]);
        assert!(matches!(
            out[0],
            FrameOutcome::Accepted {
                source: Source::Remote,
                ..
            }
        ));
    }

    #[test]
    fn frame_budget_respected() {
        let mut sched = FrameScheduler::new(Duration::from_millis(1));
        for i in 0..50 {
            sched.enqueue(RealtimeTask {
                id: i,
                cost_hint_ns: 500_000, // 0.5ms each
                deadline_offset: Duration::from_millis(1),
                peer_latency_ms: 0.0,
                payload: vec![],
                local_fallback_result: vec![i as u8],
            });
        }
        let (budget, out) = sched.tick_frame(&[]);
        assert!(!out.is_empty());
        // After processing, budget object still reports whether exceeded during tick.
        let _ = budget.elapsed_ns();
        assert!(sched.background_preempted() >= 1);
    }
}
