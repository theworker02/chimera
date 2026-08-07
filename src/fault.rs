//! Heartbeats, checkpointing, and fault re-routing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tracing::{info, warn};

use crate::protocol::{TaskId, TaskSlice};
use crate::scheduler::Scheduler;

#[derive(Clone)]
pub struct CheckpointStore {
    root: PathBuf,
    memory: Arc<Mutex<HashMap<TaskId, Vec<u8>>>>,
}

impl CheckpointStore {
    pub fn new(root: impl AsRef<Path>) -> anyhow::Result<Self> {
        let root = root.as_ref().join("checkpoints");
        std::fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            memory: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn save(&self, task_id: TaskId, offset: u64, blob: &[u8]) -> anyhow::Result<()> {
        self.memory.lock().insert(task_id, blob.to_vec());
        let path = self.root.join(format!("{}_{offset}.ckpt", task_id.0));
        std::fs::write(path, blob)?;
        Ok(())
    }

    pub fn load(&self, task_id: TaskId) -> Option<Vec<u8>> {
        if let Some(v) = self.memory.lock().get(&task_id).cloned() {
            return Some(v);
        }
        let prefix = format!("{}", task_id.0);
        let rd = std::fs::read_dir(&self.root).ok()?;
        for ent in rd.flatten() {
            let name = ent.file_name().to_string_lossy().to_string();
            if name.starts_with(&prefix) {
                return std::fs::read(ent.path()).ok();
            }
        }
        None
    }
}

#[derive(Clone)]
pub struct FaultManager {
    scheduler: Scheduler,
    checkpoints: CheckpointStore,
    throttle_cpu_pct: f32,
}

impl FaultManager {
    pub fn new(scheduler: Scheduler, checkpoints: CheckpointStore, throttle_cpu_pct: f32) -> Self {
        Self {
            scheduler,
            checkpoints,
            throttle_cpu_pct,
        }
    }

    pub fn checkpoints(&self) -> &CheckpointStore {
        &self.checkpoints
    }

    pub fn on_peer_lost(&self, node: crate::protocol::NodeId) {
        let reclaimed = self.scheduler.reclaim_from(node);
        for t in &reclaimed {
            if let Some(blob) = self.checkpoints.load(t.id) {
                info!(
                    "restoring checkpoint for task {} ({} bytes)",
                    t.id.0,
                    blob.len()
                );
            }
        }
    }

    pub fn maybe_throttle_migrate(&self, cpu_pct: f32) -> bool {
        if cpu_pct >= self.throttle_cpu_pct {
            warn!("thermal/cpu pressure {cpu_pct:.1}% — marking tasks for migration");
            true
        } else {
            false
        }
    }

    pub fn checkpoint_task(&self, task: &TaskSlice, blob: &[u8]) -> anyhow::Result<()> {
        self.checkpoints
            .save(task.id, task.checkpoint_offset, blob)
    }
}

pub async fn heartbeat_loop<F>(interval: Duration, mut tick: F)
where
    F: FnMut() + Send + 'static,
{
    let mut iv = tokio::time::interval(interval);
    loop {
        iv.tick().await;
        tick();
    }
}
