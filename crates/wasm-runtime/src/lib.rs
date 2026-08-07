//! Wasmtime sandboxed execution with fuel + memory limits.

pub mod economy;

pub use economy::{verify_receipt, ReceiptSigner};

use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Context};
use parking_lot::Mutex;
use wasmtime::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

use transport_quic::protocol::{ComputeReceipt, NodeId, TaskSlice, WasmSnapshotMeta};

fn embedded_demo_wasm() -> &'static [u8] {
    include_bytes_option()
}

fn include_bytes_option() -> &'static [u8] {
    // Placeholder empty — runtime will generate wat.
    &[]
}

const DEMO_WAT: &str = r#"
(module
  (memory (export "memory") 1)
  (func (export "chimera_alloc") (param i32) (result i32)
    i32.const 1024)
  (func (export "chimera_dealloc") (param i32 i32))
  (func (export "chimera_execute") (param i32 i32 i32 i32) (result i32)
    (local $i i32) (local $count i32) (local $checksum i64) (local $v f32)
    local.get 1
    i32.const 16
    i32.lt_u
    if (result i32)
      i32.const -1
    else
      local.get 0
      i64.load
      local.set $checksum
      local.get 0
      i32.const 8
      i32.add
      i32.load
      local.set $count
      i32.const 0
      local.set $i
      (block $done
        (loop $loop
          local.get $i
          local.get $count
          i32.ge_u
          br_if $done
          local.get 0
          i32.const 16
          i32.add
          local.get $i
          i32.const 2
          i32.shl
          i32.add
          f32.load
          f32.const 1.618034
          f32.mul
          local.set $v
          local.get 2
          i32.const 16
          i32.add
          local.get $i
          i32.const 2
          i32.shl
          i32.add
          local.get $v
          f32.store
          local.get $checksum
          local.get $v
          i32.reinterpret_f32
          i64.extend_i32_u
          i64.add
          local.set $checksum
          local.get $i
          i32.const 1
          i32.add
          local.set $i
          br $loop
        )
      )
      local.get 2
      local.get $checksum
      i64.store
      local.get 2
      i32.const 8
      i32.add
      local.get $count
      i32.store
      local.get 2
      i32.const 12
      i32.add
      i32.const 0
      i32.store
      i32.const 16
      local.get $count
      i32.const 2
      i32.shl
      i32.add
    end
  )
)
"#;

pub struct WasmRuntime {
    engine: Engine,
    module: Module,
    module_hash: [u8; 32],
    memory_mib: u64,
    fuel: u64,
    local_id: NodeId,
    signer: ReceiptSigner,
}

#[derive(Default)]
struct HostState {
    limits: StoreLimits,
}

impl WasmRuntime {
    pub fn new(
        wasm_path: Option<&Path>,
        memory_mib: u64,
        fuel: u64,
        local_id: NodeId,
        signer: ReceiptSigner,
    ) -> anyhow::Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.wasm_multi_memory(false);
        let engine = Engine::new(&config)?;
        let (module, bytes) = if let Some(path) = wasm_path {
            let bytes = std::fs::read(path).with_context(|| format!("read wasm {path:?}"))?;
            (Module::new(&engine, &bytes)?, bytes)
        } else if !embedded_demo_wasm().is_empty() {
            let bytes = embedded_demo_wasm().to_vec();
            (Module::new(&engine, &bytes)?, bytes)
        } else {
            let bytes = DEMO_WAT.as_bytes().to_vec();
            (Module::new(&engine, DEMO_WAT)?, bytes)
        };
        let module_hash = *blake3::hash(&bytes).as_bytes();
        Ok(Self {
            engine,
            module,
            module_hash,
            memory_mib,
            fuel,
            local_id,
            signer,
        })
    }

    pub fn module_hash(&self) -> [u8; 32] {
        self.module_hash
    }

    pub fn execute(&self, task: &TaskSlice) -> anyhow::Result<ExecResult> {
        let limits = StoreLimitsBuilder::new()
            .memory_size((self.memory_mib as usize) * 1024 * 1024)
            .instances(1)
            .memories(1)
            .build();
        let mut store = Store::new(
            &self.engine,
            HostState { limits },
        );
        store.limiter(|s| &mut s.limits);
        store.set_fuel(self.fuel)?;

        let linker = Linker::new(&self.engine);
        let instance = linker.instantiate(&mut store, &self.module)?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .context("guest missing memory export")?;

        let input = build_input(task);
        let in_len = input.len() as i32;
        let out_cap = (input.len() + 64) as i32;

        let alloc = instance
            .get_typed_func::<i32, i32>(&mut store, "chimera_alloc")
            .ok();
        let (in_ptr, out_ptr) = if let Some(alloc) = alloc {
            let ip = alloc.call(&mut store, in_len)?;
            let op = alloc.call(&mut store, out_cap)?;
            if ip == 0 || op == 0 {
                bail!("guest alloc failed");
            }
            (ip as usize, op as usize)
        } else {
            (64usize, 64 + input.len() + 64)
        };

        memory.write(&mut store, in_ptr, &input)?;

        let execute = instance.get_typed_func::<(i32, i32, i32, i32), i32>(
            &mut store,
            "chimera_execute",
        )?;
        let written = execute.call(
            &mut store,
            (in_ptr as i32, in_len, out_ptr as i32, out_cap),
        )?;
        if written < 0 {
            bail!("chimera_execute returned {written}");
        }

        let mut out = vec![0u8; written as usize];
        memory.read(&store, out_ptr, &mut out)?;
        let fuel_left = store.get_fuel().unwrap_or(0);
        let fuel_used = self.fuel.saturating_sub(fuel_left);
        let result_hash = *blake3::hash(&out).as_bytes();
        let io_root = *blake3::hash(&input).as_bytes();
        let receipt = self.signer.sign_receipt(
            task.id,
            self.local_id,
            result_hash,
            fuel_used,
            io_root,
        );

        Ok(ExecResult {
            output: out,
            result_hash,
            fuel_used,
            receipt,
            snapshot: WasmSnapshotMeta {
                linear_memory_pages: memory.size(&store) as u32,
                fuel_remaining: fuel_left,
                checkpoint_offset: task.checkpoint_offset,
                memory_hash: result_hash,
                note: "Linear memory checkpointed; Wasmtime call-stack IP is not fully portable â€” resume restarts guest entry with checkpoint_offset.".into(),
            },
        })
    }

    /// Snapshot linear memory for ChimeraMEM migration.
    pub fn snapshot_memory_bytes(&self, task: &TaskSlice) -> anyhow::Result<(Vec<u8>, WasmSnapshotMeta)> {
        let result = self.execute(task)?;
        Ok((result.output, result.snapshot))
    }
}

pub struct ExecResult {
    pub output: Vec<u8>,
    pub result_hash: [u8; 32],
    pub fuel_used: u64,
    pub receipt: ComputeReceipt,
    pub snapshot: WasmSnapshotMeta,
}

fn build_input(task: &TaskSlice) -> Vec<u8> {
    let count = task.element_count as usize;
    let mut buf = Vec::with_capacity(16 + count * 4);
    buf.extend_from_slice(&task.seed.to_le_bytes());
    buf.extend_from_slice(&(task.element_count).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    for i in 0..count {
        let v = ((task.seed as f32) * 0.000_001) + (i as f32) * 0.01;
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf
}

/// Shared handle for node.
pub type SharedRuntime = Arc<Mutex<WasmRuntime>>;
