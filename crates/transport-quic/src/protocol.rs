//! Shared wire protocol types (postcard over QUIC/TCP).

use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub Uuid);

impl NodeId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn short(&self) -> String {
        self.0.to_string()[..8].to_string()
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.short())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub Uuid);

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(pub Uuid);

impl JobId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Resource / capability advertisement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub cpu_cores: u32,
    pub cpu_util_pct: f32,
    pub mem_total_mb: u64,
    pub mem_avail_mb: u64,
    pub gpu_hint: bool,
    pub load_score: f32,
    pub thermal_pressure: f32,
    pub cache_hit_rate: f32,
    pub network_jitter_ms: f32,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            cpu_cores: 1,
            cpu_util_pct: 0.0,
            mem_total_mb: 0,
            mem_avail_mb: 0,
            gpu_hint: false,
            load_score: 0.0,
            thermal_pressure: 0.0,
            cache_hit_rate: 1.0,
            network_jitter_ms: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: NodeId,
    pub name: String,
    pub tcp_addr: SocketAddr,
    pub quic_addr: SocketAddr,
    pub caps: Capabilities,
    pub last_seen_ms: u64,
    pub agent_score: f32,
}

impl PeerInfo {
    pub fn is_stale(&self, timeout: Duration, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.last_seen_ms) > timeout.as_millis() as u64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskState {
    Pending,
    Running,
    Checkpointed,
    Completed,
    Failed,
    Migrating,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSlice {
    pub id: TaskId,
    pub job_id: JobId,
    pub index: u32,
    pub total: u32,
    pub seed: u64,
    pub element_count: u32,
    pub wasm_hash: [u8; 32],
    /// ChimeraFS block hashes this slice depends on.
    pub data_deps: Vec<[u8; 32]>,
    pub state: TaskState,
    pub assigned_to: Option<NodeId>,
    pub checkpoint_offset: u64,
    pub fuel_used: u64,
    pub intent_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WireMsg {
    /// Control-plane (highest priority on QUIC).
    /// Phase 7: version negotiation hello (sent before/with first gossip).
    ProtocolHello {
        from: NodeId,
        version: crate::versioning::ProtocolVersion,
    },
    Heartbeat {
        from: NodeId,
        caps: Capabilities,
        agent_digest: AgentDigest,
        seq: u64,
    },
    GossipAnnounce {
        peer: PeerInfo,
        known_peers: Vec<NodeId>,
    },
    StealRequest {
        from: NodeId,
        capacity: u32,
    },
    StealOffer {
        tasks: Vec<TaskSlice>,
    },
    TaskAssign {
        task: TaskSlice,
    },
    TaskProgress {
        task_id: TaskId,
        checkpoint_offset: u64,
        fuel_used: u64,
        pct: f32,
    },
    TaskComplete {
        task_id: TaskId,
        result_hash: [u8; 32],
        fuel_used: u64,
        receipt: ComputeReceipt,
    },
    TaskFailed {
        task_id: TaskId,
        reason: String,
    },
    Checkpoint {
        task_id: TaskId,
        offset: u64,
        blob: Vec<u8>,
    },
    Reclaim {
        task_id: TaskId,
        reason: String,
    },
    /// ChimeraFS
    BlockHave {
        hashes: Vec<[u8; 32]>,
    },
    BlockGet {
        hash: [u8; 32],
    },
    BlockPut {
        hash: [u8; 32],
        data: Vec<u8>,
    },
    DhtFind {
        key: [u8; 32],
    },
    DhtPeers {
        key: [u8; 32],
        holders: Vec<NodeId>,
    },
    /// ChimeraMEM
    PageFetch {
        region: u64,
        page: u64,
    },
    PageData {
        region: u64,
        page: u64,
        data: Vec<u8>,
        owner: NodeId,
    },
    PageOwn {
        region: u64,
        page: u64,
        owner: NodeId,
        lease_seq: u64,
    },
    MigrateOffer {
        task_id: TaskId,
        snapshot: WasmSnapshotMeta,
    },
    MigrateAccept {
        task_id: TaskId,
    },
    MigrateChunk {
        task_id: TaskId,
        seq: u32,
        data: Vec<u8>,
    },
    /// Phase 4
    IntentPropagate {
        intent: IntentSpec,
    },
    AgentVote {
        proposal: AgentProposal,
    },
    ReceiptVerify {
        receipt: ComputeReceipt,
        accepted: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentDigest {
    pub willingness: f32,
    pub energy_cost: f32,
    pub predicted_latency_ms: f32,
    pub healing_pressure: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProposal {
    pub from: NodeId,
    pub task_id: TaskId,
    pub score: f32,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentSpec {
    pub id: Uuid,
    pub name: String,
    /// Free-form declarative constraints, e.g. "latency<200ms privacy=local render=hd"
    pub declaration: String,
    pub latency_budget_ms: Option<u64>,
    pub privacy_local_only: bool,
    pub prefer_gpu: bool,
    pub max_peers: u32,
    pub slice_hint: u32,
    pub elements_per_slice: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeReceipt {
    pub task_id: TaskId,
    pub executor: NodeId,
    pub transcript_hash: [u8; 32],
    pub fuel_consumed: u64,
    pub io_merkle_root: [u8; 32],
    pub public_key: [u8; 32],
    pub signature: Vec<u8>,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmSnapshotMeta {
    pub linear_memory_pages: u32,
    pub fuel_remaining: u64,
    pub checkpoint_offset: u64,
    pub memory_hash: [u8; 32],
    /// Stack/IP restore is best-effort; Wasmtime linear memory is authoritative.
    pub note: String,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn encode_msg(msg: &WireMsg) -> anyhow::Result<Vec<u8>> {
    Ok(postcard::to_allocvec(msg)?)
}

pub fn decode_msg(bytes: &[u8]) -> anyhow::Result<WireMsg> {
    Ok(postcard::from_bytes(bytes)?)
}

/// Length-prefixed frame helpers.
pub fn frame(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}
