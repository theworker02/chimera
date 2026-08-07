//! Chimera Omniverse — umbrella mesh composing 28 modular crates.
//!
//! Prefer depending on individual crates under `crates/` / `packages/` when you
//! only need one capability. This package wires the full node.

// --- Phase 12 re-exports (extracted crates) ---
pub use agent_swarm as agent;
pub use audit_ledger as audit;
pub use compiler_jit as retro_scale;
pub use compliance_tee as tee;
pub use consensus_dag as raft_kv;
pub use network_bridge as bridge;
pub use rbac_auth as rbac;
pub use transport_quic::mtls;
pub use transport_quic::protocol;
pub use transport_quic::transport;

pub mod dht {
    pub use dht_routing::dht::*;
}
pub mod registry {
    pub use dht_routing::registry::*;
}

// Keep umbrella-local modules that still compose multiple crates
pub mod autoscaler;
pub mod brand;
#[cfg(feature = "cnk")]
pub mod cnk_host;
pub mod collab;
pub mod continuity;
#[cfg(feature = "nexus")]
pub mod nexus_rt;
pub mod config;
pub mod discovery;
pub mod economy;
pub mod fault;
pub mod freight;
pub mod fs;
pub mod gateway;
pub mod intent;
pub mod join_token;
pub mod ledger;
pub mod mem;
pub mod metrics;
#[cfg(feature = "mgmt")]
pub mod mgmt;
pub mod node;
pub mod observability;
pub mod pipeline;
pub mod runtime;
pub mod scheduler;
pub mod tui;
pub mod versioning;

pub use config::NodeConfig;
pub use node::ChimeraNode;
pub use protocol::{NodeId, PeerInfo, TaskId, TaskSlice};
pub use versioning::ProtocolVersion;

// Convenience aliases matching the Phase 12 module map
pub mod modules {
    //! Stable names for the Omniverse Rust crates (non-Rust packages live under `packages/`).
    pub use agent_swarm as agent_swarm;
    pub use audit_ledger as audit_ledger;
    pub use compiler_jit as compiler_jit;
    pub use compliance_tee as compliance_tee;
    pub use consensus_dag as consensus_dag;
    pub use crypto_quantum as crypto_quantum;
    pub use dht_routing as dht_routing;
    pub use fuser_mount as fuser_mount;
    pub use inference_engine as inference_engine;
    pub use memory_fabric as memory_fabric;
    pub use network_bridge as network_bridge;
    pub use policy_engine as policy_engine;
    pub use rbac_auth as rbac_auth;
    pub use storage_cas as storage_cas;
    pub use telemetry_otel as telemetry_otel;
    pub use transport_quic as transport_quic;
    pub use wasm_runtime as wasm_runtime;
    // Also composed via workspace path crates (not path-deps of this lib alone):
    // core-nano (chimera-nano-kernel), scheduler-rt (chimera-nexus), usb-daemon, cli-tool,
    // usb-flasher (chimera-boot — Phase 14)
    // packages/: sdk-python, sdk-ts, sdk-go, gitops-operator, ui-shell, dashboard-hud, audio-feedback
}
