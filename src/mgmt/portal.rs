//! Minimal embedded web dashboard (single HTML page).

pub const DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>Chimera Mesh Portal</title>
<style>
  :root { --void:#0A0A0C; --cyan:#00F0FF; --amber:#FFB800; --muted:#8A9099; }
  * { box-sizing: border-box; }
  body { margin:0; font-family: Segoe UI, system-ui, sans-serif; background:var(--void); color:#e8eaed; }
  header { padding:1.25rem 1.5rem; border-bottom:1px solid #1c1f26; display:flex; align-items:baseline; gap:1rem; }
  header h1 { margin:0; color:var(--cyan); letter-spacing:.12em; font-size:1.25rem; }
  header span { color:var(--muted); font-size:.85rem; }
  main { padding:1.5rem; display:grid; gap:1rem; grid-template-columns: repeat(auto-fit,minmax(220px,1fr)); }
  .card { background:#12141a; border:1px solid #1c1f26; border-radius:8px; padding:1rem 1.1rem; }
  .card h2 { margin:0 0 .5rem; font-size:.75rem; text-transform:uppercase; letter-spacing:.08em; color:var(--amber); }
  .val { font-size:1.75rem; color:var(--cyan); font-variant-numeric: tabular-nums; }
  .sub { color:var(--muted); font-size:.85rem; margin-top:.35rem; }
  footer { padding:1rem 1.5rem; color:var(--muted); font-size:.8rem; }
  a { color:var(--cyan); }
</style>
</head>
<body>
<header>
  <h1>CHIMERA</h1>
  <span>enterprise mesh portal · live</span>
</header>
<main>
  <div class="card"><h2>Status</h2><div class="val" id="status">…</div><div class="sub" id="node">—</div></div>
  <div class="card"><h2>Peers</h2><div class="val" id="peers">0</div><div class="sub">online</div></div>
  <div class="card"><h2>CPU</h2><div class="val" id="cpu">0%</div><div class="sub">local util</div></div>
  <div class="card"><h2>Tasks done</h2><div class="val" id="done">0</div><div class="sub" id="queue">pending 0 · running 0</div></div>
  <div class="card"><h2>FS blocks</h2><div class="val" id="fs">0</div><div class="sub">CAS stored</div></div>
  <div class="card"><h2>MEM faults</h2><div class="val" id="mem">0</div><div class="sub" id="mig">migrations 0</div></div>
  <div class="card"><h2>Receipts</h2><div class="val" id="rx">0</div><div class="sub">verified</div></div>
  <div class="card"><h2>Wire</h2><div class="val" id="wire">—</div><div class="sub"><a href="/meshshell">MeshShell</a> · <a href="/metrics">/metrics</a> · <a href="/health">/health</a></div></div>
</main>
<footer>Refresh 2s · auth via <code>Authorization: Bearer role:name</code></footer>
<script>
async function tick(){
  try {
    const h = await fetch('/health').then(r=>r.json());
    const c = await fetch('/v1/cluster',{headers:{'Authorization':'Bearer reader:portal'}}).then(r=>r.json());
    document.getElementById('status').textContent = h.status;
    document.getElementById('node').textContent = h.node + ' · ' + (h.node_id||'').slice(0,8);
    document.getElementById('peers').textContent = c.peers;
    document.getElementById('cpu').textContent = (h.cpu_pct||0).toFixed(1) + '%';
    document.getElementById('done').textContent = c.completed;
    document.getElementById('queue').textContent = 'pending '+c.pending+' · running '+c.running;
    document.getElementById('fs').textContent = c.fs_blocks;
    document.getElementById('mem').textContent = c.mem_faults;
    document.getElementById('mig').textContent = 'migrations '+c.migrations;
    document.getElementById('rx').textContent = c.verified_receipts;
    document.getElementById('wire').textContent = h.wire;
  } catch(e) {
    document.getElementById('status').textContent = 'err';
  }
}
tick(); setInterval(tick, 2000);
</script>
</body>
</html>
"##;
