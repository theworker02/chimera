//! REST management API, Prometheus `/metrics`, MeshShell SPA, Freight, collab.

mod meshshell;
mod portal;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::audit::AuditLog;
use crate::collab::{CollabHub, TextOpKind};
use crate::freight::{FreightRegistry, PublisherKey};
use crate::fs::ChimeraFs;
use crate::gateway::{FunctionGateway, InvokeRequest};
use crate::join_token::{JoinToken, TokenIssuer};
use crate::ledger::CreditLedger;
use crate::metrics::MetricsHub;
use crate::observability::prometheus_text;
use crate::protocol::NodeId;
use crate::raft_kv::KvStore;
use crate::rbac::{require, Permission, Principal, Role};
use crate::versioning::{ProtocolVersion, WIRE_MAJOR, WIRE_MINOR};

#[derive(Clone)]
pub struct MgmtState {
    pub node_name: String,
    pub node_id: String,
    pub metrics: MetricsHub,
    pub audit: Arc<Mutex<AuditLog>>,
    pub tokens: Arc<TokenIssuer>,
    pub principal: Principal,
    pub intents: Arc<Mutex<Vec<IntentRecord>>>,
    pub assets: Arc<Mutex<Vec<AssetRecord>>>,
    pub gateway: Arc<FunctionGateway>,
    pub kv: KvStore,
    pub fs: ChimeraFs,
    pub freight: FreightRegistry,
    pub publisher: PublisherKey,
    pub ledger: CreditLedger,
    pub collab: CollabHub,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentRecord {
    pub id: String,
    pub declaration: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRecord {
    pub name: String,
    pub root_hex: String,
    pub size: u64,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    node: String,
    node_id: String,
    wire: String,
    peers: usize,
    completed_tasks: u64,
    cpu_pct: f32,
}

#[derive(Debug, Serialize)]
struct ClusterResponse {
    node: String,
    peers: usize,
    pending: usize,
    running: usize,
    completed: u64,
    fs_blocks: u64,
    mem_faults: u64,
    migrations: u64,
    verified_receipts: u64,
}

#[derive(Debug, Deserialize)]
struct IntentRequest {
    declaration: String,
}

#[derive(Debug, Deserialize)]
struct TokenRequest {
    role: String,
    ttl_secs: Option<u64>,
    node_hint: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AssetPinRequest {
    name: String,
    /// Hex-encoded bytes (demo; production would stream).
    data_hex: String,
}

fn auth_principal(headers: &HeaderMap, state: &MgmtState) -> Result<Principal, StatusCode> {
    // Demo auth: `Authorization: Bearer role:name` or default state principal.
    if let Some(val) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        let token = val.strip_prefix("Bearer ").unwrap_or(val);
        if let Some((role, name)) = token.split_once(':') {
            let role = match role {
                "admin" => Role::Admin,
                "operator" => Role::Operator,
                "submitter" => Role::Submitter,
                "reader" => Role::Reader,
                _ => return Err(StatusCode::UNAUTHORIZED),
            };
            return Ok(Principal {
                name: name.into(),
                role,
                asset_prefixes: vec![],
            });
        }
    }
    Ok(state.principal.clone())
}

async fn health(State(state): State<MgmtState>) -> Json<HealthResponse> {
    let m = state.metrics.snapshot();
    Json(HealthResponse {
        status: "ok".into(),
        node: state.node_name.clone(),
        node_id: state.node_id.clone(),
        wire: format!("{WIRE_MAJOR}.{WIRE_MINOR}"),
        peers: m.peers,
        completed_tasks: m.completed_tasks,
        cpu_pct: m.local_caps.cpu_util_pct,
    })
}

async fn metrics_endpoint(State(state): State<MgmtState>) -> impl IntoResponse {
    let body = prometheus_text(&state.node_name, &state.metrics.snapshot());
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        body,
    )
}

async fn cluster(
    State(state): State<MgmtState>,
    headers: HeaderMap,
) -> Result<Json<ClusterResponse>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    require(&p, Permission::ViewCluster).map_err(|_| StatusCode::FORBIDDEN)?;
    let m = state.metrics.snapshot();
    Ok(Json(ClusterResponse {
        node: state.node_name.clone(),
        peers: m.peers,
        pending: m.pending_tasks,
        running: m.running_tasks,
        completed: m.completed_tasks,
        fs_blocks: m.fs_blocks,
        mem_faults: m.mem_faults,
        migrations: m.migrations,
        verified_receipts: m.verified_receipts,
    }))
}

async fn list_intents(
    State(state): State<MgmtState>,
    headers: HeaderMap,
) -> Result<Json<Vec<IntentRecord>>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    require(&p, Permission::ViewCluster).map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(state.intents.lock().clone()))
}

async fn submit_intent(
    State(state): State<MgmtState>,
    headers: HeaderMap,
    Json(req): Json<IntentRequest>,
) -> Result<Json<IntentRecord>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    require(&p, Permission::SubmitWorkload).map_err(|_| StatusCode::FORBIDDEN)?;
    let rec = IntentRecord {
        id: uuid::Uuid::new_v4().to_string(),
        declaration: req.declaration.clone(),
        status: "accepted".into(),
    };
    state.intents.lock().push(rec.clone());
    let _ = state.audit.lock().append(
        &p.name,
        "intent.submit",
        &rec.id,
        &req.declaration,
    );
    Ok(Json(rec))
}

async fn list_assets(
    State(state): State<MgmtState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AssetRecord>>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    require(&p, Permission::ReadAsset).map_err(|_| StatusCode::FORBIDDEN)?;
    let assets: Vec<_> = state
        .assets
        .lock()
        .iter()
        .filter(|a| p.can_read_asset(&a.name) || p.can_read_asset(&a.root_hex))
        .cloned()
        .collect();
    Ok(Json(assets))
}

async fn pin_asset(
    State(state): State<MgmtState>,
    headers: HeaderMap,
    Json(req): Json<AssetPinRequest>,
) -> Result<Json<AssetRecord>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    require(&p, Permission::WriteAsset).map_err(|_| StatusCode::FORBIDDEN)?;
    let data = hex::decode(&req.data_hex).map_err(|_| StatusCode::BAD_REQUEST)?;
    let root = *blake3::hash(&data).as_bytes();
    let rec = AssetRecord {
        name: req.name.clone(),
        root_hex: hex::encode(root),
        size: data.len() as u64,
    };
    state.assets.lock().push(rec.clone());
    let _ = state
        .audit
        .lock()
        .append(&p.name, "asset.pin", &rec.root_hex, &rec.name);
    Ok(Json(rec))
}

async fn issue_token(
    State(state): State<MgmtState>,
    headers: HeaderMap,
    Json(req): Json<TokenRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    require(&p, Permission::IssueTokens).map_err(|_| StatusCode::FORBIDDEN)?;
    let role = match req.role.as_str() {
        "admin" => Role::Admin,
        "operator" => Role::Operator,
        "submitter" => Role::Submitter,
        "reader" => Role::Reader,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let tok = state
        .tokens
        .issue(role, req.ttl_secs.unwrap_or(3600), req.node_hint);
    let encoded = state.tokens.encode(&tok);
    let _ = state
        .audit
        .lock()
        .append(&p.name, "token.issue", &tok.claims.token_id, &req.role);
    Ok(Json(serde_json::json!({
        "token": encoded,
        "expires_ms": tok.claims.expires_ms,
        "role": req.role,
    })))
}

async fn verify_join(
    State(state): State<MgmtState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let raw = body
        .get("token")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let tok = TokenIssuer::decode(raw).map_err(|_| StatusCode::BAD_REQUEST)?;
    TokenIssuer::verify(&tok).map_err(|_| StatusCode::UNAUTHORIZED)?;
    let _ = state.audit.lock().append(
        "mesh",
        "token.verify",
        &tok.claims.token_id,
        "ok",
    );
    Ok(Json(serde_json::json!({
        "ok": true,
        "role": format!("{:?}", tok.claims.role),
        "mesh_id": tok.claims.mesh_id,
    })))
}

async fn protocol_info() -> Json<ProtocolVersion> {
    Json(ProtocolVersion::local())
}

async fn audit_tail(
    State(state): State<MgmtState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    require(&p, Permission::AuditRead).map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(serde_json::json!({
        "path": state.audit.lock().path().display().to_string(),
        "entries": state.audit.lock().len(),
    })))
}

async fn portal() -> Html<&'static str> {
    Html(portal::DASHBOARD_HTML)
}

async fn get_asset(
    State(state): State<MgmtState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<AssetRecord>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    require(&p, Permission::ReadAsset).map_err(|_| StatusCode::FORBIDDEN)?;
    if !p.can_read_asset(&name) {
        return Err(StatusCode::FORBIDDEN);
    }
    let found = state
        .assets
        .lock()
        .iter()
        .find(|a| a.name == name)
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)?;
    let _ = state
        .audit
        .lock()
        .append(&p.name, "asset.read", &found.root_hex, &name);
    Ok(Json(found))
}

#[derive(Debug, Deserialize)]
struct DeployRequest {
    tenant: String,
    name: String,
    wasm_hex: String,
    memory_mib: Option<u64>,
    fuel: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ScaleRequest {
    tenant: String,
    name: String,
    instances: u32,
}

#[derive(Debug, Deserialize)]
struct KvSetRequest {
    key: String,
    value_hex: String,
}

async fn fn_deploy(
    State(state): State<MgmtState>,
    headers: HeaderMap,
    Json(req): Json<DeployRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    let wasm = hex::decode(&req.wasm_hex).map_err(|_| StatusCode::BAD_REQUEST)?;
    let spec = state
        .gateway
        .deploy(
            &p,
            &req.tenant,
            &req.name,
            &wasm,
            req.memory_mib.unwrap_or(16),
            req.fuel.unwrap_or(5_000_000),
        )
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let _ = state.audit.lock().append(
        &p.name,
        "fn.deploy",
        &format!("{}/{}", req.tenant, req.name),
        &hex::encode(&spec.wasm_hash[..8]),
    );
    Ok(Json(serde_json::json!({
        "tenant": spec.tenant,
        "name": spec.name,
        "wasm_hash": hex::encode(spec.wasm_hash),
        "instances": spec.instances,
    })))
}

async fn fn_invoke_json(
    State(state): State<MgmtState>,
    headers: HeaderMap,
    Json(req): Json<InvokeRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    let res = state
        .gateway
        .invoke(&p, &req)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let _ = state.audit.lock().append(
        &p.name,
        "fn.invoke",
        &format!("{}/{}", req.tenant, req.function),
        &format!("{}ms", res.duration_ms),
    );
    Ok(Json(serde_json::json!({
        "output_hex": res.output_hex,
        "fuel_used": res.fuel_used,
        "peer": res.peer,
        "duration_ms": res.duration_ms,
    })))
}

async fn fn_list(
    State(state): State<MgmtState>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    require(&p, Permission::ViewCluster).map_err(|_| StatusCode::FORBIDDEN)?;
    let tenant = q.get("tenant").cloned().unwrap_or_else(|| "demo".into());
    let list = state.gateway.list_functions(&tenant);
    Ok(Json(serde_json::to_value(list).unwrap_or_default()))
}

async fn fn_scale(
    State(state): State<MgmtState>,
    headers: HeaderMap,
    Json(req): Json<ScaleRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    require(&p, Permission::ManageNodes).map_err(|_| StatusCode::FORBIDDEN)?;
    state
        .gateway
        .scale(&req.tenant, &req.name, req.instances)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::json!({ "ok": true, "instances": req.instances })))
}

async fn fn_logs(
    State(state): State<MgmtState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    require(&p, Permission::ViewCluster).map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(serde_json::json!({ "lines": state.gateway.tail_logs(100) })))
}

async fn kv_get(
    State(state): State<MgmtState>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    require(&p, Permission::ReadAsset).map_err(|_| StatusCode::FORBIDDEN)?;
    match state.kv.get(&key) {
        Some(v) => Ok(Json(serde_json::json!({ "key": key, "value_hex": hex::encode(v) }))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn kv_set(
    State(state): State<MgmtState>,
    headers: HeaderMap,
    Json(req): Json<KvSetRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    require(&p, Permission::WriteAsset).map_err(|_| StatusCode::FORBIDDEN)?;
    let val = hex::decode(&req.value_hex).map_err(|_| StatusCode::BAD_REQUEST)?;
    state.kv.set(&req.key, val).map_err(|_| StatusCode::CONFLICT)?;
    Ok(Json(serde_json::json!({ "ok": true, "key": req.key })))
}

#[derive(Debug, Deserialize)]
struct FsUploadRequest {
    name: String,
    data_hex: String,
}

async fn fs_list(
    State(state): State<MgmtState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    require(&p, Permission::ReadAsset).map_err(|_| StatusCode::FORBIDDEN)?;
    let items: Vec<_> = state
        .fs
        .list_assets()
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "name": a.name,
                "root_hex": hex::encode(a.root),
                "size": a.size,
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "items": items })))
}

async fn fs_upload(
    State(state): State<MgmtState>,
    headers: HeaderMap,
    Json(req): Json<FsUploadRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    require(&p, Permission::WriteAsset).map_err(|_| StatusCode::FORBIDDEN)?;
    let data = hex::decode(&req.data_hex).map_err(|_| StatusCode::BAD_REQUEST)?;
    let asset = state
        .fs
        .ingest_bytes(&req.name, &data)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    state.assets.lock().push(AssetRecord {
        name: asset.name.clone(),
        root_hex: hex::encode(asset.root),
        size: asset.size,
    });
    Ok(Json(serde_json::json!({
        "name": asset.name,
        "root_hex": hex::encode(asset.root),
        "size": asset.size,
    })))
}

async fn fs_by_hash(
    State(state): State<MgmtState>,
    headers: HeaderMap,
    Path(hash): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    require(&p, Permission::ReadAsset).map_err(|_| StatusCode::FORBIDDEN)?;
    let (asset, data) = state
        .fs
        .read_asset_by_root(&hash)
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::json!({
        "name": asset.name,
        "root_hex": hex::encode(asset.root),
        "size": asset.size,
        "data_hex": hex::encode(data),
    })))
}

#[derive(Debug, Deserialize)]
struct FreightPublishBody {
    name: String,
    version: String,
    wasm_hex: String,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FreightInstallBody {
    name: String,
    version: String,
    tenant: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FreightRunBody {
    name: String,
    tenant: Option<String>,
    input_hex: String,
}

async fn freight_search(
    State(state): State<MgmtState>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let query = q.get("q").cloned().unwrap_or_default();
    let items = state.freight.search(&query);
    Json(serde_json::json!({ "items": items }))
}

async fn freight_publish(
    State(state): State<MgmtState>,
    headers: HeaderMap,
    Json(req): Json<FreightPublishBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    let wasm = hex::decode(&req.wasm_hex).map_err(|_| StatusCode::BAD_REQUEST)?;
    let hash = *blake3::hash(&wasm).as_bytes();
    let manifest = state.publisher.sign_manifest(
        &req.name,
        &req.version,
        hash,
        vec![],
        req.description.as_deref().unwrap_or(""),
    );
    let m = state
        .freight
        .publish(&p, manifest, &wasm, Some(&state.fs))
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::to_value(m).unwrap_or_default()))
}

async fn freight_publish_demo(
    State(state): State<MgmtState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    let wasm = crate::gateway::demo_add1_wasm();
    let hash = *blake3::hash(&wasm).as_bytes();
    let manifest = state.publisher.sign_manifest(
        "add1",
        "0.1.0",
        hash,
        vec![],
        "demo add1 for MeshShell / Freight",
    );
    let m = state
        .freight
        .publish(&p, manifest, &wasm, Some(&state.fs))
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::to_value(m).unwrap_or_default()))
}

async fn freight_install(
    State(state): State<MgmtState>,
    headers: HeaderMap,
    Json(req): Json<FreightInstallBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    let tenant = req.tenant.unwrap_or_else(|| "freight".into());
    let m = state
        .freight
        .install(&p, &req.name, &req.version, &state.gateway, &tenant)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(serde_json::to_value(m).unwrap_or_default()))
}

async fn freight_run(
    State(state): State<MgmtState>,
    headers: HeaderMap,
    Json(req): Json<FreightRunBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    let tenant = req.tenant.unwrap_or_else(|| "freight".into());
    let res = state
        .freight
        .run(&p, &req.name, &tenant, &req.input_hex, &state.gateway)
        .map_err(|e| {
            if e.to_string().contains("insufficient credits") {
                StatusCode::PAYMENT_REQUIRED
            } else {
                StatusCode::BAD_REQUEST
            }
        })?;
    Ok(Json(serde_json::to_value(res).unwrap_or_default()))
}

async fn ledger_balance(
    State(state): State<MgmtState>,
    headers: HeaderMap,
    Path(account): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let _p = auth_principal(&headers, &state)?;
    Ok(Json(serde_json::json!({
        "account": account,
        "balance": state.ledger.balance(&account),
        "bypass": state.ledger.bypass.get(),
    })))
}

#[derive(Debug, Deserialize)]
struct LedgerCreditBody {
    account: String,
    amount: u64,
}

async fn ledger_credit(
    State(state): State<MgmtState>,
    headers: HeaderMap,
    Json(req): Json<LedgerCreditBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let p = auth_principal(&headers, &state)?;
    require(&p, Permission::ManageNodes).map_err(|_| StatusCode::FORBIDDEN)?;
    state
        .ledger
        .credit(&req.account, req.amount)
        .map_err(|_| StatusCode::CONFLICT)?;
    Ok(Json(serde_json::json!({
        "account": req.account,
        "balance": state.ledger.balance(&req.account),
    })))
}

#[derive(Debug, Deserialize)]
struct CollabQuery {
    session: Option<String>,
}

async fn collab_ws(
    ws: WebSocketUpgrade,
    State(state): State<MgmtState>,
    Query(q): Query<CollabQuery>,
) -> impl IntoResponse {
    let session = q.session.unwrap_or_else(|| "notes".into());
    ws.on_upgrade(move |socket| handle_collab_socket(socket, state, session))
}

async fn handle_collab_socket(socket: WebSocket, state: MgmtState, session: String) {
    let (mut sender, mut receiver) = socket.split();
    let node = NodeId(
        uuid::Uuid::parse_str(&state.node_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
    );
    let snap = state.collab.get_or_create(&session);
    let _ = sender
        .send(Message::Text(
            serde_json::json!({ "text": snap.doc.render(), "session": session }).to_string().into(),
        ))
        .await;

    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(t) = msg {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
                if let Some(text) = v.get("text").and_then(|x| x.as_str()) {
                    let s = state.collab.apply(
                        &session,
                        node,
                        TextOpKind::Set {
                            text: text.into(),
                        },
                    );
                    let _ = sender
                        .send(Message::Text(
                            serde_json::json!({ "text": s.doc.render() })
                                .to_string()
                                .into(),
                        ))
                        .await;
                }
            }
        }
    }
}

async fn meshshell_index() -> Html<&'static str> {
    Html(meshshell::INDEX_HTML)
}

async fn meshshell_js() -> Response {
    (
        [(header::CONTENT_TYPE, HeaderValue::from_static("application/javascript; charset=utf-8"))],
        meshshell::APP_JS,
    )
        .into_response()
}

async fn meshshell_css() -> Response {
    (
        [(header::CONTENT_TYPE, HeaderValue::from_static("text/css; charset=utf-8"))],
        meshshell::STYLE_CSS,
    )
        .into_response()
}

async fn meshshell_dash_js() -> Response {
    (
        [(header::CONTENT_TYPE, HeaderValue::from_static("application/javascript; charset=utf-8"))],
        meshshell::DASHBOARD_JS,
    )
        .into_response()
}

pub fn router(state: MgmtState) -> Router {
    Router::new()
        .route("/", get(portal))
        .route("/meshshell", get(meshshell_index))
        .route("/meshshell/", get(meshshell_index))
        .route("/meshshell/index.html", get(meshshell_index))
        .route("/meshshell/app.js", get(meshshell_js))
        .route("/meshshell/style.css", get(meshshell_css))
        .route("/meshshell/dashboard.js", get(meshshell_dash_js))
        .route("/health", get(health))
        .route("/metrics", get(metrics_endpoint))
        .route("/v1/cluster", get(cluster))
        .route("/v1/protocol", get(protocol_info))
        .route("/v1/intents", get(list_intents).post(submit_intent))
        .route("/v1/assets", get(list_assets).post(pin_asset))
        .route("/v1/assets/{name}", get(get_asset))
        .route("/v1/tokens", post(issue_token))
        .route("/v1/join/verify", post(verify_join))
        .route("/v1/audit", get(audit_tail))
        .route("/v1/functions", get(fn_list).post(fn_deploy))
        .route("/v1/functions/invoke", post(fn_invoke_json))
        .route("/v1/functions/scale", post(fn_scale))
        .route("/v1/functions/logs", get(fn_logs))
        .route("/v1/kv/{key}", get(kv_get))
        .route("/v1/kv", post(kv_set))
        .route("/v1/fs", get(fs_list))
        .route("/v1/fs/upload", post(fs_upload))
        .route("/v1/fs/by-hash/{hash}", get(fs_by_hash))
        .route("/v1/freight/search", get(freight_search))
        .route("/v1/freight/publish", post(freight_publish))
        .route("/v1/freight/publish-demo", post(freight_publish_demo))
        .route("/v1/freight/install", post(freight_install))
        .route("/v1/freight/run", post(freight_run))
        .route("/v1/ledger/{account}", get(ledger_balance))
        .route("/v1/ledger/credit", post(ledger_credit))
        .route("/v1/collab/ws", get(collab_ws))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub async fn serve(bind: SocketAddr, state: MgmtState) -> anyhow::Result<()> {
    let app = router(state);
    info!("management API listening on http://{bind}");
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Helper so callers can map join decode errors.
pub type DecodedJoin = JoinToken;
