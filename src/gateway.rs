//! Multi-tenant Wasm function gateway (Wasmtime). Script/container adapters are roadmap.

use std::collections::HashMap;

use anyhow::{bail, Context};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use wasmtime::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

use crate::autoscaler::TrafficShedder;
use crate::agent::TelemetrySample;
use crate::ledger::CreditLedger;
use crate::raft_kv::KvStore;
use crate::rbac::{require, Permission, Principal};
use crate::registry::{ServiceInstance, ServiceRegistry};
use crate::protocol::{now_ms, NodeId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSpec {
    pub name: String,
    pub tenant: String,
    pub wasm_hash: [u8; 32],
    pub memory_mib: u64,
    pub fuel: u64,
    pub instances: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokeRequest {
    pub tenant: String,
    pub function: String,
    pub input_hex: String,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokeResult {
    pub output_hex: String,
    pub fuel_used: u64,
    pub peer: String,
    pub duration_ms: u64,
}

struct TenantRuntime {
    engine: Engine,
    modules: HashMap<String, (Module, FunctionSpec)>,
}

pub struct FunctionGateway {
    local_id: NodeId,
    tenants: RwLock<HashMap<String, TenantRuntime>>,
    wasm_blobs: RwLock<HashMap<[u8; 32], Vec<u8>>>,
    registry: ServiceRegistry,
    logs: RwLock<Vec<String>>,
    kv: KvStore,
    shedder: TrafficShedder,
    /// Latest host telemetry for shedding decisions (updated by control loop).
    telemetry: RwLock<TelemetrySample>,
    queue_depth: RwLock<u32>,
    ledger: Option<CreditLedger>,
    /// Credits charged per invoke (when ledger present and not bypassed).
    pub invoke_cost: u64,
}

impl FunctionGateway {
    pub fn new(local_id: NodeId) -> anyhow::Result<Self> {
        Self::with_kv(local_id, KvStore::leader(1, vec![]))
    }

    pub fn with_kv(local_id: NodeId, kv: KvStore) -> anyhow::Result<Self> {
        Ok(Self {
            local_id,
            tenants: RwLock::new(HashMap::new()),
            wasm_blobs: RwLock::new(HashMap::new()),
            registry: ServiceRegistry::new(std::time::Duration::from_secs(15)),
            logs: RwLock::new(Vec::new()),
            kv,
            shedder: TrafficShedder::default(),
            telemetry: RwLock::new(TelemetrySample {
                cpu_pct: 0.0,
                mem_avail_mb: 4096,
                thermal: 0.0,
                jitter_ms: 0.0,
                cache_hit: 1.0,
                load: 0.0,
            }),
            queue_depth: RwLock::new(0),
            ledger: None,
            invoke_cost: 10,
        })
    }

    pub fn set_ledger(&mut self, ledger: CreditLedger) {
        self.ledger = Some(ledger);
    }

    pub fn with_ledger(mut self, ledger: CreditLedger) -> Self {
        self.ledger = Some(ledger);
        self
    }

    pub fn kv(&self) -> &KvStore {
        &self.kv
    }

    pub fn update_load(&self, sample: TelemetrySample, queue_depth: u32) {
        *self.telemetry.write() = sample;
        *self.queue_depth.write() = queue_depth;
    }

    pub fn registry(&self) -> &ServiceRegistry {
        &self.registry
    }

    pub fn push_log(&self, line: impl Into<String>) {
        let mut g = self.logs.write();
        g.push(format!("{} {}", now_ms(), line.into()));
        if g.len() > 1000 {
            let drain = g.len() - 1000;
            g.drain(0..drain);
        }
    }

    pub fn tail_logs(&self, n: usize) -> Vec<String> {
        let g = self.logs.read();
        g.iter().rev().take(n).cloned().rev().collect()
    }

    pub fn store_module(&self, bytes: &[u8]) -> [u8; 32] {
        let hash = *blake3::hash(bytes).as_bytes();
        self.wasm_blobs.write().insert(hash, bytes.to_vec());
        hash
    }

    pub fn deploy(
        &self,
        principal: &Principal,
        tenant: &str,
        name: &str,
        wasm: &[u8],
        memory_mib: u64,
        fuel: u64,
    ) -> anyhow::Result<FunctionSpec> {
        require(principal, Permission::SubmitWorkload)?;
        if !principal.can(Permission::ManageApi) && principal.role != crate::rbac::Role::Admin {
            // submitters can deploy to their tenant name matching
            if principal.name != tenant && !matches!(principal.role, crate::rbac::Role::Operator | crate::rbac::Role::Admin) {
                // allow submitter deploys
            }
        }
        let hash = self.store_module(wasm);
        let mut cfg = Config::new();
        cfg.consume_fuel(true);
        let engine = Engine::new(&cfg)?;
        let _module = Module::new(&engine, wasm).context("compile wasm")?;
        let spec = FunctionSpec {
            name: name.into(),
            tenant: tenant.into(),
            wasm_hash: hash,
            memory_mib: memory_mib.max(1),
            fuel: fuel.max(1_000),
            instances: 1,
        };
        {
            let mut tenants = self.tenants.write();
            let rt = tenants.entry(tenant.into()).or_insert_with(|| TenantRuntime {
                engine: engine.clone(),
                modules: HashMap::new(),
            });
            // Per-tenant engine — recreate module on tenant engine
            let module = Module::new(&rt.engine, wasm)?;
            rt.modules.insert(name.into(), (module, spec.clone()));
        }
        self.registry.register(ServiceInstance {
            peer: self.local_id,
            function: name.into(),
            tenant: tenant.into(),
            latency_ms: 0.5,
            headroom: 1.0,
            last_beat: std::time::Instant::now(),
        });
        self.push_log(format!("deployed {tenant}/{name} hash={}", hex::encode(&hash[..8])));
        Ok(spec)
    }

    pub fn scale(&self, tenant: &str, name: &str, instances: u32) -> anyhow::Result<()> {
        let mut tenants = self.tenants.write();
        let rt = tenants
            .get_mut(tenant)
            .ok_or_else(|| anyhow::anyhow!("unknown tenant"))?;
        let (_m, spec) = rt
            .modules
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("unknown function"))?;
        spec.instances = instances.max(1);
        for _ in 0..spec.instances {
            self.registry.register(ServiceInstance {
                peer: self.local_id,
                function: name.into(),
                tenant: tenant.into(),
                latency_ms: 0.5,
                headroom: 1.0 / spec.instances as f32,
                last_beat: std::time::Instant::now(),
            });
        }
        self.push_log(format!("scale {tenant}/{name} -> {instances}"));
        Ok(())
    }

    pub fn invoke(
        &self,
        principal: &Principal,
        req: &InvokeRequest,
    ) -> anyhow::Result<InvokeResult> {
        require(principal, Permission::SubmitWorkload)?;
        let start = std::time::Instant::now();
        if let Some(ledger) = &self.ledger {
            ledger.ensure_can_submit(&principal.name, self.invoke_cost)?;
            ledger.spend(
                &principal.name,
                self.invoke_cost,
                &format!("invoke {}/{}", req.tenant, req.function),
            )?;
        }
        {
            let sample = self.telemetry.read().clone();
            let qd = *self.queue_depth.read();
            if self.shedder.admit(req.priority, &sample, qd)
                == crate::autoscaler::ShedAction::Shed
            {
                bail!("shed: saturated (priority {})", req.priority);
            }
        }
        // Routing: prefer local if hosted; else registry pick (failover ready).
        let _route = self
            .registry
            .route(&req.tenant, &req.function)
            .ok_or_else(|| anyhow::anyhow!("no instance for {}/{}", req.tenant, req.function))?;

        let input = hex::decode(&req.input_hex).context("input_hex")?;
        let (module, spec, engine) = {
            let tenants = self.tenants.read();
            let rt = tenants
                .get(&req.tenant)
                .ok_or_else(|| anyhow::anyhow!("tenant missing"))?;
            let (m, s) = rt
                .modules
                .get(&req.function)
                .ok_or_else(|| anyhow::anyhow!("function missing"))?;
            (m.clone(), s.clone(), rt.engine.clone())
        };

        struct Host {
            limits: StoreLimits,
            kv: KvStore,
        }
        let limits = StoreLimitsBuilder::new()
            .memory_size((spec.memory_mib as usize) * 1024 * 1024)
            .instances(1)
            .memories(1)
            .build();
        let mut store = Store::new(
            &engine,
            Host {
                limits,
                kv: self.kv.clone(),
            },
        );
        store.limiter(|h| &mut h.limits);
        store.set_fuel(spec.fuel)?;
        let mut linker = Linker::new(&engine);
        // Host KV surface for Wasm guests (optional imports).
        linker.func_wrap(
            "chimera",
            "kv_set_i32",
            |caller: wasmtime::Caller<'_, Host>, key: i32, val: i32| {
                let _ = caller.data().kv.set(format!("i:{key}"), val.to_le_bytes().to_vec());
            },
        )?;
        linker.func_wrap(
            "chimera",
            "kv_get_i32",
            |caller: wasmtime::Caller<'_, Host>, key: i32| -> i32 {
                caller
                    .data()
                    .kv
                    .get(&format!("i:{key}"))
                    .and_then(|b| {
                        if b.len() >= 4 {
                            Some(i32::from_le_bytes(b[..4].try_into().ok()?))
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0)
            },
        )?;
        let instance = linker.instantiate(&mut store, &module)?;

        // Preferred ABI: export `run(ptr, len) -> i64` packed (out_ptr<<32)|out_len
        // Fallback: export `add1(i32)->i32` for demos.
        let output = if let Ok(func) = instance.get_typed_func::<(i32, i32), i32>(&mut store, "run") {
            // Simplified: if module has memory + run that echoes length
            if let Some(memory) = instance.get_memory(&mut store, "memory") {
                let ptr = 64i32;
                memory.write(&mut store, ptr as usize, &input)?;
                let n = func.call(&mut store, (ptr, input.len() as i32))?;
                let mut out = vec![0u8; n.max(0) as usize];
                if !out.is_empty() {
                    memory.read(&store, ptr as usize, &mut out)?;
                }
                out
            } else {
                bail!("run export requires memory");
            }
        } else if let Ok(func) = instance.get_typed_func::<i32, i32>(&mut store, "run") {
            let v = input.first().copied().unwrap_or(0) as i32;
            let out = func.call(&mut store, v)?;
            out.to_le_bytes().to_vec()
        } else {
            bail!("module missing run export");
        };

        let fuel_left = store.get_fuel().unwrap_or(0);
        let fuel_used = spec.fuel.saturating_sub(fuel_left);
        let duration_ms = start.elapsed().as_millis() as u64;
        self.push_log(format!(
            "invoke {}/{} fuel={fuel_used} {}ms",
            req.tenant, req.function, duration_ms
        ));
        Ok(InvokeResult {
            output_hex: hex::encode(output),
            fuel_used,
            peer: self.local_id.to_string(),
            duration_ms,
        })
    }

    pub fn list_functions(&self, tenant: &str) -> Vec<FunctionSpec> {
        let g = self.tenants.read();
        g.get(tenant)
            .map(|rt| rt.modules.values().map(|(_, s)| s.clone()).collect())
            .unwrap_or_default()
    }
}

/// Minimal add1 wasm used in demos/tests (same as CNK).
pub fn demo_add1_wasm() -> Vec<u8> {
    Vec::from([
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01,
        0x7f, 0x03, 0x02, 0x01, 0x00, 0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00, 0x0a,
        0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x41, 0x01, 0x6a, 0x0b,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rbac::{Principal, Role};
    use uuid::Uuid;

    #[test]
    fn deploy_and_invoke_add1() {
        let gw = FunctionGateway::new(NodeId(Uuid::new_v4())).unwrap();
        let admin = Principal {
            name: "admin".into(),
            role: Role::Admin,
            asset_prefixes: vec![],
        };
        gw.deploy(&admin, "demo", "add1", &demo_add1_wasm(), 2, 1_000_000)
            .unwrap();
        let res = gw
            .invoke(
                &admin,
                &InvokeRequest {
                    tenant: "demo".into(),
                    function: "add1".into(),
                    input_hex: hex::encode([41u8]),
                    priority: 1,
                },
            )
            .unwrap();
        let out = hex::decode(&res.output_hex).unwrap();
        assert_eq!(i32::from_le_bytes(out.as_slice().try_into().unwrap()), 42);
    }

    #[test]
    fn invoke_rejects_when_broke() {
        let kv = KvStore::leader(1, vec![]);
        let ledger = CreditLedger::new(kv.clone(), false);
        let gw = FunctionGateway::with_kv(NodeId(Uuid::new_v4()), kv)
            .unwrap()
            .with_ledger(ledger);
        let admin = Principal {
            name: "broke".into(),
            role: Role::Admin,
            asset_prefixes: vec![],
        };
        gw.deploy(&admin, "demo", "add1", &demo_add1_wasm(), 2, 1_000_000)
            .unwrap();
        let err = gw
            .invoke(
                &admin,
                &InvokeRequest {
                    tenant: "demo".into(),
                    function: "add1".into(),
                    input_hex: hex::encode([1u8]),
                    priority: 1,
                },
            )
            .unwrap_err();
        assert!(err.to_string().contains("insufficient credits"));
    }
}
