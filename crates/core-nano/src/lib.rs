//! Chimera Nano-Kernel (CNK) — silicon-agnostic execution matrix.
//!
//! # Architecture
//! - **Core** (`#![no_std]` + `alloc`): executor, framing, TLSF allocator,
//!   deterministic replay, softfloat, immutable memory regions.
//! - **Platform layers**: `host` (std shim for Windows/desktop), plus feature-gated
//!   stubs for UEFI / Cortex-M / RISC-V (scaffolding — not production firmware).
//!
//! # Honesty
//! Real UEFI/Cortex-M/RISC-V boots are **untested** on this host. The host-simulated
//! boot path and embedded `cargo check` targets are the verified deliverables.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::type_complexity)]

extern crate alloc;

pub mod alloc_pool;
pub mod determinism;
pub mod executor;
pub mod frame;
pub mod hw;
pub mod memory;
pub mod platform;
pub mod replay;

#[cfg(feature = "net")]
pub mod net;

#[cfg(feature = "pq")]
pub mod security;

#[cfg(feature = "wasm-tier")]
pub mod wasm_tier;

pub use alloc_pool::{BlockPool, PoolError};
pub use determinism::{canonicalize_f32_bits, FixedPoint, SoftF32};
pub use executor::{NanoExecutor, NanoTask, TaskOutcome};
pub use frame::{decode_frame, encode_frame, FrameHeader, MeshFrame};
pub use hw::{HwProfile, IsaHints};
pub use memory::{ImmutableRegion, RegionError};
pub use replay::{ReplayError, TxEntry, TxLog};

/// CNK protocol / ABI version.
pub const CNK_VERSION: u16 = 1;

/// Boot the nano-kernel on the active platform (host-simulated when `host` is on).
pub fn boot(profile: HwProfile) -> platform::BootReport {
    platform::boot(profile)
}
