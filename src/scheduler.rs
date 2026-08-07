//! Randomized work-stealing scheduler with prefetch hooks.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use rand::seq::SliceRandom;
use tracing::info;

use crate::discovery::PeerTable;
use crate::fs::ChimeraFs;
use crate::protocol::{now_ms, JobId, NodeId, TaskId, TaskSlice, TaskState};

#[derive(Clone)]
pub struct Scheduler {
    local_id: NodeId,
    queue: Arc<Mutex<VecDeque<TaskSlice>>>,
    running: Arc<Mutex<Vec<TaskSlice>>>,
    completed: Arc<Mutex<u64>>,
    peers: PeerTable,
    fs: ChimeraFs,
    peer_timeout: Duration,
}

impl Scheduler {
    pub fn new(local_id: NodeId, peers: PeerTable, fs: ChimeraFs, peer_timeout: Duration) -> Self {
        Self {
            local_id,
            queue: Arc::new(Mutex::new(VecDeque::new())),
            running: Arc::new(Mutex::new(Vec::new())),
            completed: Arc::new(Mutex::new(0)),
            peers,
            fs,
            peer_timeout,
        }
    }

    pub fn submit(&self, tasks: Vec<TaskSlice>) {
        let mut q = self.queue.lock();
        for mut t in tasks {
            t.state = TaskState::Pending;
            if t.assigned_to.is_none() {
                t.assigned_to = Some(self.local_id);
            }
            q.push_back(t);
        }
    }

    pub fn submit_demo(&self, slices: u32, elements: u32, wasm_hash: [u8; 32]) -> JobId {
        let job = JobId::new();
        let mut tasks = Vec::with_capacity(slices as usize);
        for i in 0..slices {
            tasks.push(TaskSlice {
                id: TaskId::new(),
                job_id: job,
                index: i,
                total: slices,
                seed: now_ms() ^ (i as u64).wrapping_mul(0x9E37),
                element_count: elements,
                wasm_hash,
                data_deps: vec![],
                state: TaskState::Pending,
                assigned_to: Some(self.local_id),
                checkpoint_offset: 0,
                fuel_used: 0,
                intent_id: None,
            });
        }
        self.submit(tasks);
        job
    }

    pub fn pending_count(&self) -> usize {
        self.queue.lock().len()
    }

    pub fn running_count(&self) -> usize {
        self.running.lock().len()
    }

    pub fn completed_count(&self) -> u64 {
        *self.completed.lock()
    }

    pub fn snapshot_tasks(&self) -> (Vec<TaskSlice>, Vec<TaskSlice>) {
        (
            self.queue.lock().iter().cloned().collect(),
            self.running.lock().clone(),
        )
    }

    /// Prefetch ChimeraFS deps then pop next local task.
    pub fn next_local(&self) -> Option<TaskSlice> {
        let mut q = self.queue.lock();
        let mut t = q.pop_front()?;
        drop(q);
        if !t.data_deps.is_empty() {
            let _ = self.fs.prefetch(&t.data_deps);
        }
        t.state = TaskState::Running;
        t.assigned_to = Some(self.local_id);
        self.running.lock().push(t.clone());
        Some(t)
    }

    pub fn complete(&self, task_id: TaskId, _result_hash: [u8; 32]) {
        self.running.lock().retain(|t| t.id != task_id);
        *self.completed.lock() += 1;
    }

    pub fn fail_requeue(&self, task_id: TaskId) {
        let mut running = self.running.lock();
        if let Some(pos) = running.iter().position(|t| t.id == task_id) {
            let mut t = running.remove(pos);
            t.state = TaskState::Pending;
            self.queue.lock().push_back(t);
        }
    }

    pub fn reclaim_from(&self, node: NodeId) -> Vec<TaskSlice> {
        let mut reclaimed = Vec::new();
        {
            let mut running = self.running.lock();
            let keep: Vec<_> = running
                .drain(..)
                .filter_map(|mut t| {
                    if t.assigned_to == Some(node) {
                        t.state = TaskState::Pending;
                        t.assigned_to = Some(self.local_id);
                        reclaimed.push(t);
                        None
                    } else {
                        Some(t)
                    }
                })
                .collect();
            *running = keep;
        }
        {
            let mut q = self.queue.lock();
            for t in q.iter_mut() {
                if t.assigned_to == Some(node) {
                    t.assigned_to = Some(self.local_id);
                    t.state = TaskState::Pending;
                }
            }
        }
        if !reclaimed.is_empty() {
            info!("reclaimed {} tasks from lost peer {node}", reclaimed.len());
            self.queue.lock().extend(reclaimed.iter().cloned());
        }
        reclaimed
    }

    pub fn offer_steal(&self, n: usize) -> Vec<TaskSlice> {
        let mut q = self.queue.lock();
        if q.is_empty() || n == 0 {
            return vec![];
        }
        let take = n.min(q.len().saturating_add(1) / 2).max(1).min(q.len());
        let mut offered = Vec::with_capacity(take);
        for _ in 0..take {
            if let Some(t) = q.pop_back() {
                offered.push(t);
            }
        }
        offered
    }

    pub fn accept_stolen(&self, tasks: Vec<TaskSlice>) {
        self.submit(tasks);
    }

    pub fn pick_steal_target(&self) -> Option<NodeId> {
        let mut peers = self.peers.alive(self.peer_timeout);
        peers.shuffle(&mut rand::thread_rng());
        peers.into_iter().map(|p| p.id).next()
    }

    pub fn load_score(&self) -> f32 {
        (self.pending_count() + self.running_count()) as f32
    }
}
