//! chimera-usb — zero-footprint portable mesh daemon (USB-drive-relative).
//!
//! **Status: working** as a portable user-space binary.
//! True driverless plug-and-play (no OS process launch) is **roadmap**.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::Parser;
use chimera::join_token::TokenIssuer;
use chimera::metrics::MetricsHub;
use chimera::protocol::NodeId;
use chimera::retro_scale::{RetroScaler, ScalingProfile};
use chimera::tee::{SimulatedTee, TeeProvider};

#[derive(Parser, Debug)]
#[command(
    name = "chimera-usb",
    about = "Portable Chimera worker — no installer, config next to the binary"
)]
struct Args {
    /// Override portable root (default: directory containing this executable)
    #[arg(long)]
    root: Option<PathBuf>,

    /// Node display name
    #[arg(long, default_value = "usb-node")]
    name: String,

    /// Optional join token (hex JSON from chimeractl token issue)
    #[arg(long)]
    join_token: Option<String>,

    /// Measure cold-start init and exit (prints real milliseconds)
    #[arg(long, default_value_t = false)]
    benchmark_startup: bool,

    /// Mesh management API hint
    #[arg(long, default_value = "127.0.0.1:7600")]
    mesh_api: String,
}

fn portable_root(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(r) = explicit {
        return Ok(r);
    }
    let exe = std::env::current_exe().context("current_exe")?;
    Ok(exe
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".")))
}

fn ensure_config(root: &PathBuf) -> Result<()> {
    let path = root.join("chimera-usb.toml");
    if !path.exists() {
        std::fs::write(
            &path,
            "# Portable Chimera USB node — lives next to chimera-usb.exe\nname = \"usb-node\"\nmesh_id = \"chimera-local\"\n",
        )?;
    }
    Ok(())
}

struct ColdStartReport {
    node_id: String,
    elapsed_ms: f64,
    elapsed_ns: u128,
    tier: String,
    ram_mib_cap: u64,
    data_dir: String,
}

fn cold_start(root: &PathBuf, args: &Args) -> Result<ColdStartReport> {
    let t0 = Instant::now();
    let data = root.join("data");
    std::fs::create_dir_all(&data)?;
    ensure_config(root)?;

    let metrics = MetricsHub::new();
    let caps = metrics.sample_caps(0.1, 0.9);
    let profile = ScalingProfile::from_caps(caps.cpu_cores.max(1), caps.mem_total_mb.max(16));
    let policy = RetroScaler::plan(&profile);

    let tee = SimulatedTee::new(b"chimera-usb-enclave");
    let nonce = [0x55u8; 16];
    let att = tee.attest(&nonce, args.name.as_bytes())?;
    let _ = tee.verify(&att, Some(&nonce));

    if let Some(tok) = &args.join_token {
        let decoded = TokenIssuer::decode(tok).context("join token decode")?;
        TokenIssuer::verify(&decoded).context("join token verify")?;
    }

    let id = NodeId::new();
    let elapsed = t0.elapsed();
    Ok(ColdStartReport {
        node_id: id.0.to_string(),
        elapsed_ms: elapsed.as_secs_f64() * 1000.0,
        elapsed_ns: elapsed.as_nanos(),
        tier: format!("{:?}", policy.tier),
        ram_mib_cap: policy.wasm_memory_mib,
        data_dir: data.display().to_string(),
    })
}

fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let args = Args::parse();
    let root = portable_root(args.root.clone())?;

    if args.benchmark_startup {
        let report = cold_start(&root, &args)?;
        println!(
            "chimera-usb startup_ms={:.3} startup_ns={} node={} tier={} wasm_mem_mib={} data={}",
            report.elapsed_ms,
            report.elapsed_ns,
            report.node_id,
            report.tier,
            report.ram_mib_cap,
            report.data_dir
        );
        println!(
            "note: measured cold-start of portable init path (not a hardcoded claim). Full mesh join is additional."
        );
        return Ok(());
    }

    let report = cold_start(&root, &args)?;
    println!("chimera-usb ready as {}", args.name);
    println!("  root={}", root.display());
    println!("  node={}", report.node_id);
    println!("  init={:.3}ms tier={}", report.elapsed_ms, report.tier);
    println!("  mesh_api_hint={}", args.mesh_api);
    println!("  limits: OS must execute this binary; driverless USB autostart is roadmap.");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(30));
        let _ = MetricsHub::new().sample_caps(0.1, 0.9);
    }
}
