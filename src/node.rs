//! Node orchestration — wires all Chimera phases.

use std::sync::Arc;
use std::time::Duration;

use tracing::{info, warn};

use crate::agent::LocalAgent;
use crate::brand::{payload_watermark, print_banner};
use crate::config::NodeConfig;
use crate::discovery::{GossipService, PeerTable};
use crate::economy::{verify_receipt, ReceiptSigner};
use crate::fault::{CheckpointStore, FaultManager};
use crate::fs::ChimeraFs;
use crate::intent::IntentCompiler;
use crate::mem::ChimeraMem;
use crate::metrics::MetricsHub;
use crate::pipeline::DataPipeline;
use crate::protocol::{
    now_ms, AgentDigest, Capabilities, NodeId, PeerInfo, TaskState, WireMsg,
};
use crate::runtime::WasmRuntime;
use crate::scheduler::Scheduler;
use crate::transport::{MeshTransport, StreamClass};
use crate::tui;

pub struct ChimeraNode {
    pub config: NodeConfig,
    pub id: NodeId,
}

impl ChimeraNode {
    pub fn new(config: NodeConfig) -> Self {
        let id = config.resolved_node_id();
        Self { config, id }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        print_banner();
        std::fs::create_dir_all(&self.config.data_dir)?;

        if let Some(ep) = &self.config.otlp_endpoint {
            let _ = crate::observability::otel::init(&self.config.name, ep);
        }

        #[cfg(feature = "cnk")]
        {
            let cnk = crate::cnk_host::CnkHost::bootstrap();
            info!(
                "CNK v{} online platform={} prefer_interpreter={}",
                crate::cnk_host::CnkHost::version(),
                cnk.profile.platform_name,
                cnk.profile.prefer_interpreter
            );
        }

        #[cfg(feature = "nexus")]
        {
            let lane = crate::nexus_rt::RealtimeLane::sixty_fps();
            lane.submit(1, 100, 8, 0.5, b"boot".to_vec());
            let _ = lane.tick(&[]);
            info!("Nexus realtime lane online (60fps frame budget)");
        }

        let metrics = MetricsHub::new();
        let audit = std::sync::Arc::new(parking_lot::Mutex::new(
            crate::audit::AuditLog::open(&self.config.data_dir)?,
        ));
        let tokens = std::sync::Arc::new(crate::join_token::TokenIssuer::new(
            self.config.mesh_id.clone(),
        ));
        let _ = audit.lock().append(
            &self.config.name,
            "node.start",
            &self.id.0.to_string(),
            "boot",
        );

        let peers = PeerTable::new(self.id);
        let fs = ChimeraFs::open(
            &self.config.data_dir,
            self.id,
            self.config.fs_block_size,
            self.config.fs_cache_blocks,
        )?;

        #[cfg(feature = "mgmt")]
        if !self.config.no_mgmt {
            let kv = crate::raft_kv::KvStore::leader(1, vec![]);
            let bypass = !self.config.enforce_credits;
            let ledger = crate::ledger::CreditLedger::new(kv.clone(), bypass);
            // Seed operator account for demos when enforcing credits.
            if self.config.enforce_credits {
                let _ = ledger.credit("admin", 10_000);
                let _ = ledger.credit("ops", 10_000);
            }
            let mut gw = crate::gateway::FunctionGateway::with_kv(self.id, kv.clone())
                .expect("function gateway");
            gw = gw.with_ledger(ledger.clone());
            let gateway = std::sync::Arc::new(gw);
            {
                let gw = gateway.clone();
                let metrics = metrics.clone();
                let scaler = crate::autoscaler::AutoScaler::default();
                tokio::spawn(async move {
                    let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
                    loop {
                        tick.tick().await;
                        let snap = metrics.snapshot();
                        let sample = crate::agent::TelemetrySample {
                            cpu_pct: snap.local_caps.cpu_util_pct,
                            mem_avail_mb: snap.local_caps.mem_avail_mb,
                            thermal: snap.local_caps.cpu_util_pct / 100.0,
                            jitter_ms: 1.0,
                            cache_hit: 0.9,
                            load: snap.pending_tasks as f32,
                        };
                        let qd = snap.pending_tasks as u32;
                        gw.update_load(sample.clone(), qd);
                        for spec in gw.list_functions("demo") {
                            if let Some(d) =
                                scaler.decide(&spec.tenant, &spec.name, spec.instances, &sample, qd)
                            {
                                let _ = gw.scale(&d.tenant, &d.function, d.to);
                                gw.push_log(format!("autoscaler: {}", d.reason));
                            }
                        }
                    }
                });
            }
            let state = crate::mgmt::MgmtState {
                node_name: self.config.name.clone(),
                node_id: self.id.0.to_string(),
                metrics: metrics.clone(),
                audit: audit.clone(),
                tokens: tokens.clone(),
                principal: crate::rbac::Principal::admin(),
                intents: std::sync::Arc::new(parking_lot::Mutex::new(Vec::new())),
                assets: std::sync::Arc::new(parking_lot::Mutex::new(Vec::new())),
                gateway,
                kv,
                fs: fs.clone(),
                freight: crate::freight::FreightRegistry::new(),
                publisher: crate::freight::PublisherKey::generate(),
                ledger,
                collab: crate::collab::CollabHub::new(),
            };
            let bind = self.config.mgmt_bind;
            tokio::spawn(async move {
                if let Err(e) = crate::mgmt::serve(bind, state).await {
                    warn!("mgmt api: {e:#}");
                }
            });
            info!(
                "MeshShell http://{}/meshshell (wire {}.{})",
                self.config.mgmt_bind,
                crate::versioning::WIRE_MAJOR,
                crate::versioning::WIRE_MINOR
            );
        }

        let signer = ReceiptSigner::generate();
        let runtime = WasmRuntime::new(
            self.config.wasm.as_deref(),
            self.config.wasm_memory_mib,
            self.config.wasm_fuel,
            self.id,
            signer.clone(),
        )?;
        let wasm_hash = runtime.module_hash();

        let mem = ChimeraMem::new(self.id, self.config.mem_page_size);
        let _region = mem.fabric.lock().create_region(16);

        let pipeline = DataPipeline::new(&self.config.data_dir)?;
        let scheduler = Scheduler::new(
            self.id,
            peers.clone(),
            fs.clone(),
            self.config.peer_timeout(),
        );
        let checkpoints = CheckpointStore::new(&self.config.data_dir)?;
        let fault = FaultManager::new(
            scheduler.clone(),
            checkpoints,
            self.config.throttle_cpu_pct,
        );

        // Seed ChimeraFS with a tiny demo asset.
        let demo = b"chimera-demo-dataset-v1";
        let asset = fs.ingest_bytes("demo.bin", demo)?;
        let _wm = payload_watermark(&asset.root, &self.config.name);
        info!(
            "ChimeraFS asset root={} blocks={}",
            hex::encode(&asset.root[..8]),
            asset.blocks.len()
        );
        let _ = audit.lock().append(
            &self.config.name,
            "asset.pin",
            &hex::encode(asset.root),
            "demo.bin",
        );

        let caps = metrics.sample_caps(0.0, 1.0);
        let local_peer = PeerInfo {
            id: self.id,
            name: self.config.name.clone(),
            tcp_addr: rewrite_bind(self.config.tcp_bind),
            quic_addr: rewrite_bind(self.config.quic_bind),
            caps: caps.clone(),
            last_seen_ms: now_ms(),
            agent_score: 0.5,
        };

        let (gossip, mut gossip_rx) = GossipService::new(
            local_peer.clone(),
            peers.clone(),
            self.config.multicast_group,
            self.config.multicast_port,
            self.config.heartbeat_interval(),
            self.config.peer_timeout(),
        );
        tokio::spawn(async move {
            if let Err(e) = gossip.run().await {
                warn!("gossip terminated: {e}");
            }
        });

        let (transport, mut inbound) =
            MeshTransport::new(self.id, self.config.quic_bind, self.config.tcp_bind);
        let transport = Arc::new(transport);
        {
            let t = transport.clone();
            tokio::spawn(async move {
                if let Err(e) = t.serve().await {
                    warn!("transport: {e}");
                }
            });
        }

        let mut agent = LocalAgent::new(self.id);
        agent.observe(LocalAgent::from_caps(&caps));

        if self.config.demo_slices > 0 {
            let job = scheduler.submit_demo(
                self.config.demo_slices,
                self.config.demo_elements,
                wasm_hash,
            );
            info!("demo job {} with {} slices", job.0, self.config.demo_slices);
        }

        let mut intents_compiled = 0u64;
        if let Some(decl) = &self.config.intent {
            let compiler = IntentCompiler::new(wasm_hash);
            let intent = IntentCompiler::parse(decl);
            let plan = compiler.compile(&intent);
            if !plan.local_only {
                // still fine to run locally first
            }
            let rid = mem.fabric.lock().create_region(plan.mem_pages);
            info!(
                "intent '{}' → {} tasks, mem region {rid}",
                intent.name,
                plan.tasks.len()
            );
            scheduler.submit(plan.tasks);
            intents_compiled += 1;
        }

        let mut verified_receipts = 0u64;
        let use_tui = self.config.use_tui();
        let metrics_tui = metrics.clone();

        // Control / compute loop
        let cfg = self.config.clone();
        let id = self.id;
        let loop_handle = tokio::spawn(async move {
            let mut hb = tokio::time::interval(cfg.heartbeat_interval());
            let mut work = tokio::time::interval(Duration::from_millis(50));
            let mut steal = tokio::time::interval(Duration::from_millis(750));
            let mut seq = 0u64;
            loop {
                tokio::select! {
                    _ = hb.tick() => {
                        let fs_stats = fs.stats();
                        let hit = if fs_stats.cache_hits + fs_stats.cache_misses == 0 {
                            1.0
                        } else {
                            fs_stats.cache_hits as f32
                                / (fs_stats.cache_hits + fs_stats.cache_misses) as f32
                        };
                        let load = scheduler.load_score();
                        let caps = metrics.sample_caps(load, hit);
                        agent.observe(LocalAgent::from_caps(&caps));
                        if fault.maybe_throttle_migrate(caps.cpu_util_pct)
                            || agent.should_preempt_migrate()
                        {
                            if let Some(target) = scheduler.pick_steal_target() {
                                let maybe_task = {
                                    let (_, running) = scheduler.snapshot_tasks();
                                    running.into_iter().next()
                                };
                                if let Some(task) = maybe_task {
                                    if let Ok((blob, meta)) = runtime.snapshot_memory_bytes(&task) {
                                        {
                                            mem.migration.lock().begin(
                                                task.id,
                                                target,
                                                meta.clone(),
                                                blob,
                                            );
                                        }
                                        let _ = transport.send(
                                            target,
                                            WireMsg::MigrateOffer { task_id: task.id, snapshot: meta },
                                            StreamClass::Control,
                                        ).await;
                                    }
                                }
                            }
                        }
                        let digest = agent.last_digest.clone();
                        seq += 1;
                        let msg = WireMsg::Heartbeat {
                            from: id,
                            caps: caps.clone(),
                            agent_digest: digest.clone(),
                            seq,
                        };
                        transport.broadcast_control(msg).await;
                        let mem_stats = mem.stats();
                        let (br, bw) = pipeline.stats();
                        metrics.update(|m| {
                            m.local_caps = caps;
                            m.peers = peers.len();
                            m.pending_tasks = scheduler.pending_count();
                            m.running_tasks = scheduler.running_count();
                            m.completed_tasks = scheduler.completed_count();
                            m.bytes_read = br;
                            m.bytes_written = bw;
                            m.fs_cache_hits = fs_stats.cache_hits;
                            m.fs_cache_misses = fs_stats.cache_misses;
                            m.fs_blocks = fs_stats.blocks_stored;
                            m.mem_regions = mem_stats.regions;
                            m.mem_local_pages = mem_stats.local_pages;
                            m.mem_faults = mem_stats.remote_faults;
                            m.migrations = mem_stats.migrations;
                            m.agent_willingness = digest.willingness;
                            m.agent_healing = digest.healing_pressure;
                            m.verified_receipts = verified_receipts;
                            m.intents_compiled = intents_compiled;
                        });
                    }
                    _ = work.tick() => {
                        if let Some(task) = scheduler.next_local() {
                            let span = tracing::info_span!(
                                "task.execute",
                                task = %task.id.0,
                                index = task.index
                            );
                            let _g = span.enter();
                            match runtime.execute(&task) {
                                Ok(result) => {
                                    let header = crate::pipeline::ChunkHeader {
                                        task_id: task.id,
                                        index: task.index,
                                        byte_len: result.output.len() as u64,
                                        content_hash: result.result_hash,
                                    };
                                    let _ = pipeline.write_chunk(&header, &result.output);
                                    let _ = fault.checkpoint_task(&task, &result.output);
                                    if verify_receipt(&result.receipt) {
                                        verified_receipts += 1;
                                    }
                                    scheduler.complete(task.id, result.result_hash);
                                    info!(
                                        "slice {}/{} done fuel={} hash={}",
                                        task.index + 1,
                                        task.total,
                                        result.fuel_used,
                                        hex::encode(&result.result_hash[..6])
                                    );
                                }
                                Err(e) => {
                                    warn!("task {} failed: {e:#}", task.id.0);
                                    scheduler.fail_requeue(task.id);
                                }
                            }
                        }
                    }
                    _ = steal.tick() => {
                        if scheduler.pending_count() == 0 {
                            if let Some(target) = scheduler.pick_steal_target() {
                                let _ = transport.send(
                                    target,
                                    WireMsg::StealRequest { from: id, capacity: 2 },
                                    StreamClass::Compute,
                                ).await;
                            }
                        } else if scheduler.load_score() > 4.0 {
                            if let Some(target) = peers.underutilized(cfg.peer_timeout()).map(|p| p.id) {
                                let offered = scheduler.offer_steal(2);
                                if !offered.is_empty() {
                                    let _ = transport.send(
                                        target,
                                        WireMsg::StealOffer { tasks: offered },
                                        StreamClass::Compute,
                                    ).await;
                                }
                            }
                        }
                    }
                    Some(ev) = gossip_rx.recv() => {
                        match ev {
                            crate::discovery::GossipEvent::PeerJoined(p)
                            | crate::discovery::GossipEvent::PeerUpdated(p) => {
                                transport.remember_peer(p.id, p.tcp_addr, p.quic_addr);
                                fs.dht.lock().touch_peer(p.id);
                                info!("peer {} @ {}", p.name, p.tcp_addr);
                            }
                            crate::discovery::GossipEvent::PeerLost(nid) => {
                                warn!("peer lost {nid}");
                                fault.on_peer_lost(nid);
                                fs.dht.lock().remove_peer(nid);
                            }
                        }
                    }
                    Some((from, msg, _class)) = inbound.recv() => {
                        handle_inbound(
                            from,
                            msg,
                            id,
                            &scheduler,
                            &transport,
                            &fs,
                            &mem,
                            &mut verified_receipts,
                        ).await;
                    }
                }
            }
        });

        if use_tui {
            // Run TUI on blocking thread; node loop continues on runtime.
            let tui_result = tokio::task::spawn_blocking(move || tui::try_run(metrics_tui)).await?;
            tui_result?;
        } else {
            info!("node {} running (no TUI) — Ctrl+C to stop", id);
            tokio::signal::ctrl_c().await.ok();
        }

        loop_handle.abort();
        Ok(())
    }
}

async fn handle_inbound(
    from: NodeId,
    msg: WireMsg,
    local: NodeId,
    scheduler: &Scheduler,
    transport: &MeshTransport,
    fs: &ChimeraFs,
    mem: &ChimeraMem,
    verified_receipts: &mut u64,
) {
    match msg {
        WireMsg::Heartbeat { from, caps, agent_digest, .. } => {
            let _ = (from, caps, agent_digest);
        }
        WireMsg::StealRequest { from, capacity } => {
            let tasks = scheduler.offer_steal(capacity as usize);
            if !tasks.is_empty() {
                let _ = transport
                    .send(from, WireMsg::StealOffer { tasks }, StreamClass::Compute)
                    .await;
            }
        }
        WireMsg::StealOffer { tasks } => {
            scheduler.accept_stolen(tasks);
        }
        WireMsg::TaskAssign { mut task } => {
            task.assigned_to = Some(local);
            task.state = TaskState::Pending;
            scheduler.submit(vec![task]);
        }
        WireMsg::TaskComplete { receipt, .. } => {
            if verify_receipt(&receipt) {
                *verified_receipts += 1;
            }
        }
        WireMsg::BlockGet { hash } => {
            if let Ok(Some(data)) = fs.store.get_block(&hash) {
                let _ = transport
                    .send(
                        from,
                        WireMsg::BlockPut { hash, data },
                        StreamClass::Bulk,
                    )
                    .await;
            }
        }
        WireMsg::BlockPut { hash, data } => {
            if blake3::hash(&data).as_bytes() == &hash {
                let _ = fs.store.put_block(&data);
                fs.dht.lock().announce(hash);
            }
        }
        WireMsg::DhtFind { key } => {
            let holders = fs.dht.lock().find(&key);
            let _ = transport
                .send(
                    from,
                    WireMsg::DhtPeers { key, holders },
                    StreamClass::Control,
                )
                .await;
        }
        WireMsg::PageFetch { region, page } => {
            let exported = mem.fabric.lock().export_page(region, page);
            if let Some((owner, data, _lease)) = exported {
                let _ = transport
                    .send(
                        from,
                        WireMsg::PageData {
                            region,
                            page,
                            data,
                            owner,
                        },
                        StreamClass::Bulk,
                    )
                    .await;
            }
        }
        WireMsg::PageData {
            region,
            page,
            data,
            owner,
        } => {
            mem.fabric
                .lock()
                .fill_page(region, page, owner, data, 0);
        }
        WireMsg::MigrateOffer { task_id, snapshot } => {
            let _ = snapshot;
            let _ = transport
                .send(from, WireMsg::MigrateAccept { task_id }, StreamClass::Control)
                .await;
        }
        WireMsg::MigrateAccept { task_id } => {
            let chunks: Vec<(u32, Vec<u8>)> = {
                let mut mig = mem.migration.lock();
                let mut out = Vec::new();
                while let Some(c) = mig.next_chunk(task_id, 16 * 1024) {
                    out.push(c);
                }
                mig.finish(task_id);
                out
            };
            for (seq, data) in chunks {
                let _ = transport
                    .send(
                        from,
                        WireMsg::MigrateChunk {
                            task_id,
                            seq,
                            data,
                        },
                        StreamClass::Bulk,
                    )
                    .await;
            }
        }
        WireMsg::MigrateChunk { .. } => {
            // Receiver reassembles in migration manager (simplified accept path).
        }
        WireMsg::Reclaim { task_id, .. } => {
            scheduler.fail_requeue(task_id);
        }
        WireMsg::IntentPropagate { intent } => {
            info!("received intent {}", intent.name);
        }
        WireMsg::AgentVote { proposal } => {
            info!("agent vote from {} score={:.3}", proposal.from, proposal.score);
        }
        _ => {}
    }
}

fn rewrite_bind(addr: std::net::SocketAddr) -> std::net::SocketAddr {
    // Advertise localhost for LAN demos when bound to 0.0.0.0
    if addr.ip().is_unspecified() {
        std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), addr.port())
    } else {
        addr
    }
}

// silence unused import warning helpers
#[allow(dead_code)]
fn _caps_digest(c: Capabilities, d: AgentDigest) {
    let _ = (c, d);
}
