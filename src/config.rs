use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use uuid::Uuid;

use crate::protocol::NodeId;

/// Chimera node configuration (CLI + runtime).
#[derive(Debug, Clone, Parser)]
#[command(name = "chimera", about = "Decentralized P2P compute & rendering grid", long_about = None)]
pub struct NodeConfig {
    /// Human-readable node name
    #[arg(long, default_value = "chimera-node")]
    pub name: String,

    /// Stable node identity (generated if omitted)
    #[arg(long)]
    pub node_id: Option<String>,

    /// TCP listen address for work-stealing streams
    #[arg(long, default_value = "0.0.0.0:7400")]
    pub tcp_bind: SocketAddr,

    /// QUIC listen address
    #[arg(long, default_value = "0.0.0.0:7401")]
    pub quic_bind: SocketAddr,

    /// UDP multicast group for peer gossip
    #[arg(long, default_value = "239.255.74.10")]
    pub multicast_group: Ipv4Addr,

    /// UDP multicast port
    #[arg(long, default_value_t = 7410)]
    pub multicast_port: u16,

    /// Directory for mmap'd datasets, ChimeraFS cache, checkpoints
    #[arg(long, default_value = "./data")]
    pub data_dir: PathBuf,

    /// Optional Wasm module path (defaults to embedded demo guest)
    #[arg(long)]
    pub wasm: Option<PathBuf>,

    /// Maximum Wasm linear memory (MiB)
    #[arg(long, default_value_t = 64)]
    pub wasm_memory_mib: u64,

    /// Fuel budget per task slice
    #[arg(long, default_value_t = 50_000_000)]
    pub wasm_fuel: u64,

    /// Gossip / heartbeat interval (ms)
    #[arg(long, default_value_t = 1000)]
    pub heartbeat_ms: u64,

    /// Peer considered dead after this many missed heartbeats
    #[arg(long, default_value_t = 5)]
    pub heartbeat_misses: u32,

    /// Thermal throttle threshold (percent CPU)
    #[arg(long, default_value_t = 95.0)]
    pub throttle_cpu_pct: f32,

    /// Launch interactive TUI dashboard
    #[arg(long, default_value_t = true)]
    pub tui: bool,

    /// Disable TUI (log-only mode)
    #[arg(long, default_value_t = false)]
    pub no_tui: bool,

    /// Submit a local demo job with N slices on start
    #[arg(long, default_value_t = 0)]
    pub demo_slices: u32,

    /// Elements per demo slice
    #[arg(long, default_value_t = 4096)]
    pub demo_elements: u32,

    /// Declarative intent string to compile into a job plan
    #[arg(long)]
    pub intent: Option<String>,

    /// ChimeraFS block size (bytes)
    #[arg(long, default_value_t = 65536)]
    pub fs_block_size: usize,

    /// Max ChimeraFS RAM cache blocks
    #[arg(long, default_value_t = 256)]
    pub fs_cache_blocks: usize,

    /// ChimeraMEM page size (bytes)
    #[arg(long, default_value_t = 4096)]
    pub mem_page_size: usize,

    /// Management REST API bind address (Phase 7; requires `mgmt` feature)
    #[arg(long, default_value = "127.0.0.1:7600")]
    pub mgmt_bind: SocketAddr,

    /// Disable management HTTP API
    #[arg(long, default_value_t = false)]
    pub no_mgmt: bool,

    /// Mesh id embedded in join tokens
    #[arg(long, default_value = "chimera-local")]
    pub mesh_id: String,

    /// Optional OTLP endpoint (requires `--features otel`)
    #[arg(long)]
    pub otlp_endpoint: Option<String>,

    /// Disable credit ledger enforcement (default on for local WorldOS meshes)
    #[arg(long, default_value_t = true)]
    pub ledger_bypass: bool,

    /// Enforce credit ledger (inverse of bypass)
    #[arg(long, default_value_t = false)]
    pub enforce_credits: bool,
}

impl NodeConfig {
    pub fn resolved_node_id(&self) -> NodeId {
        match &self.node_id {
            Some(s) => NodeId(Uuid::parse_str(s).unwrap_or_else(|_| Uuid::new_v4())),
            None => NodeId(Uuid::new_v4()),
        }
    }

    pub fn heartbeat_interval(&self) -> Duration {
        Duration::from_millis(self.heartbeat_ms)
    }

    pub fn peer_timeout(&self) -> Duration {
        Duration::from_millis(self.heartbeat_ms * u64::from(self.heartbeat_misses))
    }

    pub fn use_tui(&self) -> bool {
        self.tui && !self.no_tui
    }
}
