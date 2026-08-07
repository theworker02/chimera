//! Stable C ABI — no allocations on the tick path (pre-created world).

use std::os::raw::{c_char, c_int, c_uint};
use std::ptr;
use std::time::Duration;

use crate::ecs::{NexusWorld, Transform};
use crate::frame::{FrameScheduler, RealtimeTask, DEFAULT_FRAME_BUDGET_NS};
use crate::prediction::PredictionEngine;

pub struct NexusNode {
    pub world: NexusWorld,
    pub scheduler: FrameScheduler,
    pub prediction: PredictionEngine,
    pub last_outcomes: usize,
    pub frame_ns: u64,
}

impl NexusNode {
    fn new(budget_ns: u64) -> Self {
        Self {
            world: NexusWorld::new(),
            scheduler: FrameScheduler::new(Duration::from_nanos(budget_ns)),
            prediction: PredictionEngine::new(),
            last_outcomes: 0,
            frame_ns: budget_ns,
        }
    }
}

static mut LAST_ERROR: [u8; 128] = [0; 128];

fn set_err(msg: &str) {
    unsafe {
        let bytes = msg.as_bytes();
        let n = bytes.len().min(127);
        LAST_ERROR = [0; 128];
        LAST_ERROR[..n].copy_from_slice(&bytes[..n]);
    }
}

#[no_mangle]
pub extern "C" fn chimera_nexus_version() -> c_uint {
    crate::NEXUS_VERSION
}

/// Create a nexus node. `budget_ns == 0` → default 16.6ms.
#[no_mangle]
pub extern "C" fn chimera_nexus_init(budget_ns: u64) -> *mut NexusNode {
    let b = if budget_ns == 0 {
        DEFAULT_FRAME_BUDGET_NS
    } else {
        budget_ns
    };
    Box::into_raw(Box::new(NexusNode::new(b)))
}

#[no_mangle]
pub unsafe extern "C" fn chimera_nexus_shutdown(node: *mut NexusNode) {
    if !node.is_null() {
        drop(Box::from_raw(node));
    }
}

/// Tick one frame. Returns number of task outcomes; never blocks beyond budget accounting.
#[no_mangle]
pub unsafe extern "C" fn chimera_nexus_tick(node: *mut NexusNode) -> c_int {
    if node.is_null() {
        set_err("null node");
        return -1;
    }
    let n = &mut *node;
    let (_budget, outcomes) = n.scheduler.tick_frame(&[]);
    n.last_outcomes = outcomes.len();
    outcomes.len() as c_int
}

#[no_mangle]
pub unsafe extern "C" fn chimera_nexus_submit_rt(
    node: *mut NexusNode,
    task_id: u64,
    cost_hint_ns: u64,
    deadline_ms: u32,
    local_result_ptr: *const u8,
    local_result_len: usize,
) -> c_int {
    if node.is_null() {
        return -1;
    }
    let n = &mut *node;
    let mut local = vec![0u8; local_result_len.min(256)];
    if !local_result_ptr.is_null() && local_result_len > 0 {
        let src = std::slice::from_raw_parts(local_result_ptr, local_result_len.min(256));
        local[..src.len()].copy_from_slice(src);
        local.truncate(src.len());
    }
    n.scheduler.enqueue(RealtimeTask {
        id: task_id,
        cost_hint_ns,
        deadline_offset: Duration::from_millis(deadline_ms as u64),
        peer_latency_ms: 0.0,
        payload: Vec::new(),
        local_fallback_result: local,
    });
    0
}

#[no_mangle]
pub unsafe extern "C" fn chimera_nexus_spawn_entity(node: *mut NexusNode) -> u64 {
    if node.is_null() {
        return 0;
    }
    (*node).world.spawn("c-abi")
}

#[no_mangle]
pub unsafe extern "C" fn chimera_nexus_set_transform(
    node: *mut NexusNode,
    entity: u64,
    x: f32,
    y: f32,
    z: f32,
) -> c_int {
    if node.is_null() {
        return -1;
    }
    (*node).world.set_transform(
        entity,
        Transform {
            x,
            y,
            z,
            ..Default::default()
        },
        "c-abi",
    );
    0
}

#[no_mangle]
pub unsafe extern "C" fn chimera_nexus_get_transform(
    node: *mut NexusNode,
    entity: u64,
    out_x: *mut f32,
    out_y: *mut f32,
    out_z: *mut f32,
) -> c_int {
    if node.is_null() || out_x.is_null() || out_y.is_null() || out_z.is_null() {
        return -1;
    }
    match (*node).world.transform(entity) {
        Some(t) => {
            *out_x = t.x;
            *out_y = t.y;
            *out_z = t.z;
            0
        }
        None => -2,
    }
}

#[no_mangle]
pub extern "C" fn chimera_nexus_last_error(buf: *mut c_char, len: usize) -> c_int {
    if buf.is_null() || len == 0 {
        return -1;
    }
    unsafe {
        let err = &raw const LAST_ERROR;
        let n = (*err).iter().position(|&b| b == 0).unwrap_or(127);
        let copy = n.min(len - 1);
        ptr::copy_nonoverlapping((*err).as_ptr(), buf as *mut u8, copy);
        *buf.add(copy) = 0;
    }
    0
}
