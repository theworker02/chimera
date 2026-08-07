//! Chimera Nexus — distributed neural & game-engine interop.
//!
//! # Honesty
//! No Godot/Unreal engine is linked in CI. The C ABI, WIT, ECS, frame scheduler,
//! and prediction/replay are unit-tested on host. Engine embedding is scaffolding.

#![allow(clippy::missing_safety_doc)]

pub mod abi;
pub mod ecs;
pub mod frame;
pub mod prediction;

pub use ecs::{ComponentKind, EntityId, NexusWorld, Transform};
pub use frame::{FrameBudget, FrameOutcome, FrameScheduler, RealtimeTask};
pub use prediction::{PredictionEngine, PredictedOp};

/// Nexus library version (semver-ish packed).
pub const NEXUS_VERSION: u32 = 0x0001_0000;
