//! Wasm execution tiering: wasmi interpreter for constrained / no_std targets.
//!
//! Hosts with Wasmtime (Phase 1) remain the high-performance JIT path. CNK uses
//! wasmi when `HwProfile::prefer_interpreter` is set or on bare-metal builds.

use alloc::vec::Vec;

use wasmi::{Engine, Linker, Module, Store};

use crate::hw::HwProfile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasmTierError {
    Compile,
    Link,
    Trap,
    MissingExport,
}

pub struct InterpreterRuntime {
    engine: Engine,
}

impl InterpreterRuntime {
    pub fn new(_profile: &HwProfile) -> Self {
        Self {
            engine: Engine::default(),
        }
    }

    /// Execute a minimal WAT/Wasm module exporting `run(i32) -> i32`.
    pub fn run_i32_kernel(&self, wasm: &[u8], input: i32) -> Result<i32, WasmTierError> {
        let module = Module::new(&self.engine, wasm).map_err(|_| WasmTierError::Compile)?;
        let mut store = Store::new(&self.engine, ());
        let linker = Linker::new(&self.engine);
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|_| WasmTierError::Link)?
            .start(&mut store)
            .map_err(|_| WasmTierError::Trap)?;
        let func = instance
            .get_typed_func::<i32, i32>(&store, "run")
            .map_err(|_| WasmTierError::MissingExport)?;
        func.call(&mut store, input).map_err(|_| WasmTierError::Trap)
    }
}

/// Tiny wasm: `(module (func (export "run") (param i32) (result i32) local.get 0 i32.const 1 i32.add))`
pub fn tiny_add1_wasm() -> Vec<u8> {
    // Pre-encoded wasm binary for the module above.
    Vec::from([
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01,
        0x7f, 0x03, 0x02, 0x01, 0x00, 0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00, 0x0a,
        0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x41, 0x01, 0x6a, 0x0b,
    ])
}

pub fn select_tier(profile: &HwProfile) -> &'static str {
    if profile.prefer_interpreter || profile.ram_bytes < 16 * 1024 * 1024 {
        "wasmi-interpreter"
    } else {
        "host-wasmtime-jit"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hw::HwProfile;

    #[test]
    fn interpreter_add1() {
        let rt = InterpreterRuntime::new(&HwProfile::minimal());
        let out = rt.run_i32_kernel(&tiny_add1_wasm(), 41).unwrap();
        assert_eq!(out, 42);
    }
}
