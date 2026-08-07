//! chimeractl — manage Chimera mesh nodes via the REST management API.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde_json::Value;

#[derive(Parser, Debug)]
#[command(name = "chimeractl", about = "Chimera enterprise mesh control plane CLI")]
struct Cli {
    /// Management API base URL
    #[arg(long, env = "CHIMERA_API", default_value = "http://127.0.0.1:7600")]
    api: String,

    /// Auth bearer: role:name (e.g. admin:ops)
    #[arg(long, env = "CHIMERA_AUTH", default_value = "admin:ops")]
    auth: String,

    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show cluster health snapshot
    Status,
    /// Print Prometheus metrics text
    Metrics,
    /// Spin up a local mesh fabric (N nodes + gateway)
    Up {
        #[arg(long, default_value_t = 1)]
        nodes: u32,
        #[arg(long, default_value_t = 7600)]
        mgmt_port: u16,
        #[arg(long, default_value = "./data/fabric")]
        data_dir: PathBuf,
        /// Path to chimera binary (default: look beside chimeractl / PATH / cargo)
        #[arg(long)]
        chimera_bin: Option<PathBuf>,
        /// Keep running in foreground until Ctrl-C
        #[arg(long, default_value_t = true)]
        foreground: bool,
    },
    /// Deploy a Wasm module (or built-in demo)
    Deploy {
        /// Path to .wasm, or the literal `demo` for the embedded add1 module
        path: String,
        #[arg(long, default_value = "demo")]
        tenant: String,
        #[arg(long, default_value = "fn")]
        name: String,
        #[arg(long, default_value_t = 16)]
        memory_mib: u64,
        #[arg(long, default_value_t = 5_000_000)]
        fuel: u64,
    },
    /// Invoke a deployed function
    Invoke {
        #[arg(long, default_value = "demo")]
        tenant: String,
        #[arg(long, default_value = "fn")]
        function: String,
        /// Hex-encoded input bytes (default: single byte 41 for demo add1 → 42)
        #[arg(long, default_value = "29")]
        input_hex: String,
        #[arg(long, default_value_t = 1)]
        priority: u8,
    },
    /// Tail function gateway logs
    Logs {
        #[arg(long, default_value_t = false)]
        follow: bool,
        #[arg(long, default_value_t = 1500)]
        interval_ms: u64,
    },
    /// Scale function instances
    Scale {
        #[arg(long, default_value = "demo")]
        tenant: String,
        #[arg(long, default_value = "fn")]
        name: String,
        instances: u32,
    },
    /// Freight decentralized package registry
    Freight {
        #[command(subcommand)]
        action: FreightCmd,
    },
    /// KV get/set
    Kv {
        #[command(subcommand)]
        action: KvCmd,
    },
    /// List / submit intents
    Intent {
        #[command(subcommand)]
        action: IntentCmd,
    },
    /// List / pin assets
    Asset {
        #[command(subcommand)]
        action: AssetCmd,
    },
    /// Issue or verify mesh join tokens
    Token {
        #[command(subcommand)]
        action: TokenCmd,
    },
    /// Audit trail helpers
    Audit {
        #[command(subcommand)]
        action: AuditCmd,
    },
    /// Boot-Sovereign USB flash / recovery (DANGEROUS — see --help)
    Usb {
        #[command(subcommand)]
        action: UsbCmd,
    },
    /// Tail live health every N ms
    Watch {
        #[arg(long, default_value_t = 2000)]
        interval_ms: u64,
    },
}

#[derive(Subcommand, Debug)]
enum FreightCmd {
    /// Publish a Wasm package (path or `demo`)
    Publish {
        path: String,
        #[arg(long, default_value = "pkg")]
        name: String,
        #[arg(long, default_value = "0.1.0")]
        version: String,
        #[arg(long, default_value = "")]
        description: String,
    },
    Search {
        #[arg(default_value = "")]
        query: String,
    },
    Install {
        name: String,
        #[arg(long, default_value = "0.1.0")]
        version: String,
        #[arg(long, default_value = "freight")]
        tenant: String,
    },
    Run {
        name: String,
        #[arg(long, default_value = "freight")]
        tenant: String,
        #[arg(long, default_value = "29")]
        input_hex: String,
    },
}

#[derive(Subcommand, Debug)]
enum KvCmd {
    Get { key: String },
    Set {
        key: String,
        #[arg(long)]
        value_hex: String,
    },
}

#[derive(Subcommand, Debug)]
enum IntentCmd {
    List,
    Submit {
        #[arg(long)]
        declaration: String,
    },
}

#[derive(Subcommand, Debug)]
enum AssetCmd {
    List,
    Pin {
        #[arg(long)]
        name: String,
        #[arg(long)]
        data_hex: String,
    },
    Get {
        name: String,
    },
}

#[derive(Subcommand, Debug)]
enum TokenCmd {
    Issue {
        #[arg(long, default_value = "operator")]
        role: String,
        #[arg(long, default_value_t = 3600)]
        ttl_secs: u64,
        #[arg(long)]
        node_hint: Option<String>,
    },
    Verify {
        #[arg(long)]
        token: String,
    },
}

#[derive(Subcommand, Debug)]
enum AuditCmd {
    Info,
    Verify {
        #[arg(long)]
        path: String,
    },
}

#[derive(Subcommand, Debug)]
enum UsbCmd {
    /// List physical disks (READ-ONLY — safe)
    List,
    /// Flash ISO/payload to a target (default: dry-run ON)
    Flash {
        #[arg(long)]
        iso: Option<PathBuf>,
        /// Physical disk path (e.g. \\.\PhysicalDrive1) OR file image path with --image
        #[arg(long)]
        target: String,
        /// Use a file-backed image instead of a physical disk (lab / safe)
        #[arg(long)]
        image: bool,
        #[arg(long, default_value = "GPT")]
        scheme: String,
        #[arg(long, default_value = "FAT32")]
        filesystem: String,
        #[arg(long, default_value = "CHIMERA")]
        label: String,
        /// Opt out of dry-run (REQUIRED for any real write). Default: dry-run ON.
        #[arg(long, default_value_t = false)]
        no_dry_run: bool,
        #[arg(long = "yes-i-understand-this-destroys-data", default_value_t = false)]
        destroy_confirm: bool,
        /// Type the disk serial to confirm (alternative/additional gate)
        #[arg(long)]
        confirm_serial: Option<String>,
        #[arg(long)]
        efi_stub: Option<PathBuf>,
        #[arg(long)]
        mbr_bootstrap: Option<PathBuf>,
        /// Inject raw payload bytes from file (nano-kernel image) when --iso omitted
        #[arg(long)]
        payload: Option<PathBuf>,
        /// Size for --image create if file does not exist (MiB)
        #[arg(long, default_value_t = 64)]
        image_mib: u64,
    },
    /// Verify BLAKE3 of first N bytes on target against --hash
    Verify {
        #[arg(long)]
        target: String,
        #[arg(long)]
        image: bool,
        #[arg(long)]
        hash: String,
        #[arg(long, default_value_t = 0)]
        bytes: u64,
    },
    /// Non-destructive bootsector / kernel inject (same safety gates)
    Repair {
        #[arg(long)]
        target: String,
        #[arg(long)]
        image: bool,
        /// Opt out of dry-run (REQUIRED for real repair writes)
        #[arg(long, default_value_t = false)]
        no_dry_run: bool,
        #[arg(long = "yes-i-understand-this-destroys-data", default_value_t = false)]
        destroy_confirm: bool,
        #[arg(long)]
        confirm_serial: Option<String>,
        #[arg(long)]
        inject_kernel: Option<PathBuf>,
        #[arg(long)]
        mbr_bootstrap: Option<PathBuf>,
    },
    /// Stream write telemetry (throughput; SMART thermal omitted if unavailable)
    Telemetry {
        #[arg(long, default_value_t = true)]
        stream: bool,
    },
}

struct Client {
    base: String,
    auth: String,
    http: reqwest::Client,
}

impl Client {
    fn new(base: String, auth: String) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            auth,
            http: reqwest::Client::new(),
        }
    }

    async fn get(&self, path: &str) -> Result<Value> {
        let url = format!("{}{path}", self.base);
        let res = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.auth))
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        let status = res.status();
        let body = res.text().await?;
        if !status.is_success() {
            bail!("{status}: {body}");
        }
        Ok(serde_json::from_str(&body).unwrap_or(Value::String(body)))
    }

    async fn get_text(&self, path: &str) -> Result<String> {
        let url = format!("{}{path}", self.base);
        let res = self.http.get(&url).send().await?;
        Ok(res.text().await?)
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value> {
        let url = format!("{}{path}", self.base);
        let res = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.auth))
            .json(&body)
            .send()
            .await?;
        let status = res.status();
        let text = res.text().await?;
        if !status.is_success() {
            bail!("{status}: {text}");
        }
        Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
    }
}

/// Minimal add1 wasm (export `run(i32)->i32`) — same bytes as chimera::gateway::demo_add1_wasm.
fn demo_add1_wasm() -> Vec<u8> {
    Vec::from([
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01,
        0x7f, 0x03, 0x02, 0x01, 0x00, 0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00, 0x0a,
        0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x41, 0x01, 0x6a, 0x0b,
    ])
}

fn resolve_chimera_bin(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.with_file_name(if cfg!(windows) {
            "chimera.exe"
        } else {
            "chimera"
        });
        if sibling.exists() {
            return Ok(sibling);
        }
    }
    if let Ok(p) = which_in_path("chimera") {
        return Ok(p);
    }
    // Fall back to `cargo run` via a wrapper script path marker
    Ok(PathBuf::from("cargo"))
}

fn which_in_path(name: &str) -> Result<PathBuf> {
    let path = std::env::var_os("PATH").context("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.into()
        });
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    bail!("{name} not found on PATH");
}

fn spawn_node(
    bin: &Path,
    name: &str,
    tcp: u16,
    quic: u16,
    mgmt: u16,
    data_dir: &Path,
) -> Result<Child> {
    std::fs::create_dir_all(data_dir)?;
    let mut cmd = if bin.file_name().and_then(|s| s.to_str()) == Some("cargo")
        || bin.file_name().and_then(|s| s.to_str()) == Some("cargo.exe")
    {
        let mut c = Command::new(bin);
        c.args([
            "run",
            "-q",
            "--bin",
            "chimera",
            "--",
            "--name",
            name,
            "--no-tui",
            "--tcp-bind",
            &format!("127.0.0.1:{tcp}"),
            "--quic-bind",
            &format!("127.0.0.1:{quic}"),
            "--mgmt-bind",
            &format!("127.0.0.1:{mgmt}"),
            "--data-dir",
            &data_dir.to_string_lossy(),
        ]);
        c
    } else {
        let mut c = Command::new(bin);
        c.args([
            "--name",
            name,
            "--no-tui",
            "--tcp-bind",
            &format!("127.0.0.1:{tcp}"),
            "--quic-bind",
            &format!("127.0.0.1:{quic}"),
            "--mgmt-bind",
            &format!("127.0.0.1:{mgmt}"),
            "--data-dir",
            &data_dir.to_string_lossy(),
        ]);
        c
    };
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn().with_context(|| format!("spawn {}", bin.display()))
}

async fn wait_healthy(client: &Client, timeout: Duration) -> Result<()> {
    let start = std::time::Instant::now();
    loop {
        if client.get("/health").await.is_ok() {
            return Ok(());
        }
        if start.elapsed() > timeout {
            bail!("management API did not become healthy within {timeout:?}");
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = Client::new(cli.api.clone(), cli.auth.clone());

    match cli.cmd {
        Commands::Up {
            nodes,
            mgmt_port,
            data_dir,
            chimera_bin,
            foreground,
        } => {
            let bin = resolve_chimera_bin(chimera_bin)?;
            let n = nodes.max(1);
            println!("chimera fabric: spawning {n} node(s) via {}", bin.display());
            let mut children = Vec::new();
            for i in 0..n {
                let name = format!("fabric-{i}");
                let tcp = (7400 + i * 2) as u16;
                let quic = (7401 + i * 2) as u16;
                let mgmt = mgmt_port + i as u16;
                let dir = data_dir.join(&name);
                let child = spawn_node(&bin, &name, tcp, quic, mgmt, &dir)?;
                println!(
                    "  {name} pid={} tcp={tcp} quic={quic} mgmt=http://127.0.0.1:{mgmt}",
                    child.id()
                );
                children.push(child);
            }
            let primary = Client::new(
                format!("http://127.0.0.1:{mgmt_port}"),
                cli.auth.clone(),
            );
            wait_healthy(&primary, Duration::from_secs(90)).await?;
            println!("fabric ready — gateway at http://127.0.0.1:{mgmt_port}");
            println!("quickstart:");
            println!("  chimeractl --api http://127.0.0.1:{mgmt_port} deploy demo --name add1");
            println!("  chimeractl --api http://127.0.0.1:{mgmt_port} invoke --function add1 --input-hex 29");
            if foreground {
                println!("Ctrl-C to tear down…");
                tokio::signal::ctrl_c().await?;
                for mut c in children {
                    let _ = c.kill();
                }
            } else {
                // Detach: leave children running (caller owns lifecycle)
                for mut c in children {
                    let _ = c.try_wait();
                    std::mem::forget(c);
                }
            }
        }
        Commands::Deploy {
            path,
            tenant,
            name,
            memory_mib,
            fuel,
        } => {
            let wasm = if path == "demo" {
                demo_add1_wasm()
            } else {
                std::fs::read(&path).with_context(|| format!("read {path}"))?
            };
            let v = client
                .post(
                    "/v1/functions",
                    serde_json::json!({
                        "tenant": tenant,
                        "name": name,
                        "wasm_hex": hex::encode(wasm),
                        "memory_mib": memory_mib,
                        "fuel": fuel,
                    }),
                )
                .await?;
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        Commands::Invoke {
            tenant,
            function,
            input_hex,
            priority,
        } => {
            let v = client
                .post(
                    "/v1/functions/invoke",
                    serde_json::json!({
                        "tenant": tenant,
                        "function": function,
                        "input_hex": input_hex,
                        "priority": priority,
                    }),
                )
                .await?;
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        Commands::Logs {
            follow,
            interval_ms,
        } => {
            let mut last = 0usize;
            loop {
                let v = client.get("/v1/functions/logs").await?;
                if let Some(lines) = v.get("lines").and_then(|x| x.as_array()) {
                    for line in lines.iter().skip(last) {
                        if let Some(s) = line.as_str() {
                            println!("{s}");
                        }
                    }
                    last = lines.len();
                }
                if !follow {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(interval_ms)).await;
            }
        }
        Commands::Scale {
            tenant,
            name,
            instances,
        } => {
            let v = client
                .post(
                    "/v1/functions/scale",
                    serde_json::json!({
                        "tenant": tenant,
                        "name": name,
                        "instances": instances,
                    }),
                )
                .await?;
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        Commands::Freight { action } => match action {
            FreightCmd::Publish {
                path,
                name,
                version,
                description,
            } => {
                let wasm = if path == "demo" {
                    demo_add1_wasm()
                } else {
                    std::fs::read(&path).with_context(|| format!("read {path}"))?
                };
                let v = client
                    .post(
                        "/v1/freight/publish",
                        serde_json::json!({
                            "name": name,
                            "version": version,
                            "wasm_hex": hex::encode(wasm),
                            "description": description,
                        }),
                    )
                    .await?;
                println!("{}", serde_json::to_string_pretty(&v)?);
            }
            FreightCmd::Search { query } => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &client
                            .get(&format!(
                                "/v1/freight/search?q={}",
                                urlencoding_lite(&query)
                            ))
                            .await?
                    )?
                );
            }
            FreightCmd::Install {
                name,
                version,
                tenant,
            } => {
                let v = client
                    .post(
                        "/v1/freight/install",
                        serde_json::json!({
                            "name": name,
                            "version": version,
                            "tenant": tenant,
                        }),
                    )
                    .await?;
                println!("{}", serde_json::to_string_pretty(&v)?);
            }
            FreightCmd::Run {
                name,
                tenant,
                input_hex,
            } => {
                let v = client
                    .post(
                        "/v1/freight/run",
                        serde_json::json!({
                            "name": name,
                            "tenant": tenant,
                            "input_hex": input_hex,
                        }),
                    )
                    .await?;
                println!("{}", serde_json::to_string_pretty(&v)?);
            }
        },
        Commands::Kv { action } => match action {
            KvCmd::Get { key } => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&client.get(&format!("/v1/kv/{key}")).await?)?
                );
            }
            KvCmd::Set { key, value_hex } => {
                let v = client
                    .post(
                        "/v1/kv",
                        serde_json::json!({ "key": key, "value_hex": value_hex }),
                    )
                    .await?;
                println!("{}", serde_json::to_string_pretty(&v)?);
            }
        },
        Commands::Status => {
            let h = client.get("/health").await?;
            let c = client.get("/v1/cluster").await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "health": h,
                    "cluster": c,
                }))?
            );
        }
        Commands::Metrics => {
            println!("{}", client.get_text("/metrics").await?);
        }
        Commands::Intent { action } => match action {
            IntentCmd::List => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&client.get("/v1/intents").await?)?
                );
            }
            IntentCmd::Submit { declaration } => {
                let v = client
                    .post(
                        "/v1/intents",
                        serde_json::json!({ "declaration": declaration }),
                    )
                    .await?;
                println!("{}", serde_json::to_string_pretty(&v)?);
            }
        },
        Commands::Asset { action } => match action {
            AssetCmd::List => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&client.get("/v1/assets").await?)?
                );
            }
            AssetCmd::Pin { name, data_hex } => {
                let v = client
                    .post(
                        "/v1/assets",
                        serde_json::json!({ "name": name, "data_hex": data_hex }),
                    )
                    .await?;
                println!("{}", serde_json::to_string_pretty(&v)?);
            }
            AssetCmd::Get { name } => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&client.get(&format!("/v1/assets/{name}")).await?)?
                );
            }
        },
        Commands::Token { action } => match action {
            TokenCmd::Issue {
                role,
                ttl_secs,
                node_hint,
            } => {
                let mut body = serde_json::json!({ "role": role, "ttl_secs": ttl_secs });
                if let Some(h) = node_hint {
                    body["node_hint"] = Value::String(h);
                }
                println!(
                    "{}",
                    serde_json::to_string_pretty(&client.post("/v1/tokens", body).await?)?
                );
            }
            TokenCmd::Verify { token } => {
                let v = client
                    .post("/v1/join/verify", serde_json::json!({ "token": token }))
                    .await?;
                println!("{}", serde_json::to_string_pretty(&v)?);
            }
        },
        Commands::Audit { action } => match action {
            AuditCmd::Info => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&client.get("/v1/audit").await?)?
                );
            }
            AuditCmd::Verify { path } => {
                let n = verify_audit_file(&path)?;
                println!("ok entries={n} path={path}");
            }
        },
        Commands::Usb { action } => run_usb(action)?,
        Commands::Watch { interval_ms } => loop {
            let h = client.get("/health").await?;
            println!(
                "{} status={} peers={} cpu={}",
                chrono::Utc::now().to_rfc3339(),
                h.get("status").and_then(|v| v.as_str()).unwrap_or("?"),
                h.get("peers").and_then(|v| v.as_u64()).unwrap_or(0),
                h.get("cpu_pct").and_then(|v| v.as_f64()).unwrap_or(0.0)
            );
            tokio::time::sleep(Duration::from_millis(interval_ms)).await;
        },
    }
    Ok(())
}

fn run_usb(action: UsbCmd) -> Result<()> {
    use chimera_boot::block::{BlockTarget, FileImageTarget};
    use chimera_boot::enumerate::list_disks;
    use chimera_boot::firmware::detect_firmware_mode;
    use chimera_boot::flash::{
        flash_file_image, flash_physical, verify_blake3, FlashPlan, PartitionScheme,
        VolumeFilesystem,
    };
    use chimera_boot::progress::FlashProgress;
    use chimera_boot::repair::{repair_file_image, repair_physical, RepairPlan};
    use chimera_boot::safety::WriteGate;
    use chimera_boot::telemetry::TelemetryStream;

    match action {
        UsbCmd::List => {
            println!("firmware_mode={:?}", detect_firmware_mode());
            println!("WARNING: listing is read-only. Never flash without reviewing serials.");
            let disks = list_disks()?;
            if disks.is_empty() {
                println!(
                    "(no physical drives enumerated — on Windows, raw PhysicalDrive access often needs elevation; use --image for lab flashing)"
                );
            }
            for d in disks {
                println!(
                    "{}\tpath={}\tserial={}\tsize_mib={}\tsector={}\tremovable={}\tsystem={}\tbus={}\tmodel={}",
                    d.id,
                    d.path,
                    d.serial,
                    d.size_bytes / (1024 * 1024),
                    d.sector_size,
                    d.removable,
                    d.is_system || d.contains_system_volume,
                    d.bus,
                    d.model
                );
            }
        }
        UsbCmd::Flash {
            iso,
            target,
            image,
            scheme,
            filesystem,
            label,
            no_dry_run,
            destroy_confirm,
            confirm_serial,
            efi_stub,
            mbr_bootstrap,
            payload,
            image_mib,
        } => {
            eprintln!(
                "DANGER: USB flash can destroy all data on the target. dry_run={}",
                !no_dry_run
            );
            let scheme = match scheme.to_ascii_uppercase().as_str() {
                "GPT" => PartitionScheme::Gpt,
                "MBR" => PartitionScheme::Mbr,
                other => bail!("unknown scheme {other} (GPT|MBR)"),
            };
            let filesystem = match filesystem.to_ascii_uppercase().as_str() {
                "FAT32" => VolumeFilesystem::Fat32,
                "NTFS" => VolumeFilesystem::Ntfs,
                other => bail!("unknown filesystem {other} (FAT32|NTFS)"),
            };
            let gate = WriteGate {
                destroy_confirm,
                dry_run: !no_dry_run,
                typed_serial: confirm_serial,
            };
            let payload_bytes = match payload {
                Some(p) => Some(std::fs::read(&p).with_context(|| format!("read {}", p.display()))?),
                None => None,
            };
            let plan = FlashPlan {
                iso_path: iso,
                payload: payload_bytes,
                scheme,
                filesystem,
                volume_label: label,
                gate,
                efi_stub,
                mbr_bootstrap,
            };
            let mut telem = TelemetryStream::new(64);
            let progress: chimera_boot::ProgressCallback =
                Box::new(move |p: &FlashProgress| {
                    eprintln!(
                        "[{}] {:.1}%  {} B/s  {}/{}",
                        p.stage,
                        p.pct(),
                        p.bytes_per_sec,
                        p.bytes_done,
                        p.bytes_total
                    );
                });
            let _ = &mut telem;
            if image {
                let path = PathBuf::from(&target);
                let mut img = if path.exists() {
                    FileImageTarget::open(&path, 512)?
                } else {
                    FileImageTarget::create(&path, image_mib * 1024 * 1024, 512)?
                };
                let r = flash_file_image(&mut img, &plan, Some(progress))?;
                println!("{}", serde_json::to_string_pretty(&r)?);
            } else {
                let disks = list_disks()?;
                let disk = disks
                    .into_iter()
                    .find(|d| d.path == target || d.id == target)
                    .with_context(|| format!("disk not found: {target}"))?;
                let r = flash_physical(disk, &plan, Some(progress))?;
                println!("{}", serde_json::to_string_pretty(&r)?);
            }
        }
        UsbCmd::Verify {
            target,
            image,
            hash,
            bytes,
        } => {
            if image {
                let mut img = FileImageTarget::open(&target, 512)?;
                let n = if bytes == 0 { img.size_bytes() } else { bytes };
                let ok = verify_blake3(&mut img, n, &hash, None)?;
                println!("{{\"ok\":{ok},\"bytes\":{n}}}");
            } else {
                use chimera_boot::block::PhysicalDiskTarget;
                let disks = list_disks()?;
                let disk = disks
                    .into_iter()
                    .find(|d| d.path == target || d.id == target)
                    .with_context(|| format!("disk not found: {target}"))?;
                let mut t = PhysicalDiskTarget::open_readonly(disk)?;
                let n = if bytes == 0 { t.size_bytes() } else { bytes };
                let ok = verify_blake3(&mut t, n, &hash, None)?;
                println!("{{\"ok\":{ok},\"bytes\":{n},\"note\":\"read-only verify\"}}");
            }
        }
        UsbCmd::Repair {
            target,
            image,
            no_dry_run,
            destroy_confirm,
            confirm_serial,
            inject_kernel,
            mbr_bootstrap,
        } => {
            let gate = WriteGate {
                destroy_confirm,
                dry_run: !no_dry_run,
                typed_serial: confirm_serial,
            };
            let kernel = match inject_kernel {
                Some(p) => Some(std::fs::read(&p)?),
                None => None,
            };
            let plan = RepairPlan {
                gate,
                mbr_bootstrap,
                inject_kernel: kernel,
                kernel_lba: 2048,
            };
            if image {
                let mut img = FileImageTarget::open(&target, 512)?;
                let dry = plan.gate.dry_run;
                repair_file_image(&mut img, &plan)?;
                println!("{{\"ok\":true,\"mode\":\"image\",\"dry_run\":{dry}}}");
            } else {
                let disks = list_disks()?;
                let disk = disks
                    .into_iter()
                    .find(|d| d.path == target || d.id == target)
                    .with_context(|| format!("disk not found: {target}"))?;
                let dry = plan.gate.dry_run;
                repair_physical(disk, &plan)?;
                println!("{{\"ok\":true,\"mode\":\"physical\",\"dry_run\":{dry}}}");
            }
        }
        UsbCmd::Telemetry { stream } => {
            println!(
                "{{\"stream\":{stream},\"media_temp_c\":null,\"note\":\"SMART thermal unavailable — not fabricated\"}}"
            );
            if stream {
                println!("stage=idle bytes_per_sec=0 (attach during flash for live samples)");
            }
        }
    }
    Ok(())
}

fn urlencoding_lite(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}

fn verify_audit_file(path: &str) -> Result<u64> {
    use std::io::{BufRead, BufReader};
    let f = std::fs::File::open(path)?;
    let mut tip = [0u8; 32];
    let mut count = 0u64;
    for line in BufReader::new(f).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let ev: Value = serde_json::from_str(&line)?;
        let prev = hex_array32(ev.get("prev_hash"))?;
        let hash = hex_array32(ev.get("hash"))?;
        if prev != tip {
            bail!("hash chain break at seq {:?}", ev.get("seq"));
        }
        tip = hash;
        count += 1;
    }
    Ok(count)
}

fn hex_array32(v: Option<&Value>) -> Result<[u8; 32]> {
    let arr = v.and_then(|x| x.as_array()).context("missing hash")?;
    if arr.len() != 32 {
        bail!("hash len");
    }
    let mut out = [0u8; 32];
    for (i, n) in arr.iter().enumerate() {
        out[i] = n.as_u64().unwrap_or(0) as u8;
    }
    Ok(out)
}
