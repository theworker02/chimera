const AUTH = "admin:ops";
const API = "";

async function api(path, opts = {}) {
  const res = await fetch(API + path, {
    ...opts,
    headers: {
      Authorization: `Bearer ${AUTH}`,
      "Content-Type": "application/json",
      ...(opts.headers || {}),
    },
  });
  const text = await res.text();
  let body;
  try { body = JSON.parse(text); } catch { body = text; }
  if (!res.ok) throw new Error(`${res.status}: ${typeof body === "string" ? body : JSON.stringify(body)}`);
  return body;
}

function hexFromBuffer(buf) {
  return [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function bufferFromHex(hex) {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.substr(i * 2, 2), 16);
  return out;
}

// Tabs
document.querySelectorAll(".tabs button").forEach((btn) => {
  btn.addEventListener("click", () => {
    document.querySelectorAll(".tabs button").forEach((b) => b.classList.remove("active"));
    document.querySelectorAll(".view").forEach((v) => v.classList.remove("active"));
    btn.classList.add("active");
    document.getElementById("view-" + btn.dataset.view).classList.add("active");
  });
});

async function refreshStatus() {
  try {
    const h = await api("/health");
    document.getElementById("nodeStatus").textContent =
      `${h.node} · peers ${h.peers} · cpu ${h.cpu_pct?.toFixed?.(1) ?? h.cpu_pct}% · wire ${h.wire}`;
  } catch (e) {
    document.getElementById("nodeStatus").textContent = "offline: " + e.message;
  }
}

async function refreshFs() {
  const list = document.getElementById("fsList");
  list.innerHTML = "";
  try {
    const files = await api("/v1/fs");
    (files.items || files || []).forEach((f) => {
      const li = document.createElement("li");
      const name = f.name || f.path || "asset";
      const hash = f.root_hex || f.hash || "";
      li.innerHTML = `<span>${name}<br/><small>${hash.slice(0, 16)}… · ${f.size || 0} B</small></span>`;
      const dl = document.createElement("button");
      dl.textContent = "Download";
      dl.onclick = async () => {
        const data = await api(`/v1/fs/by-hash/${hash}`);
        const bytes = bufferFromHex(data.data_hex || "");
        const blob = new Blob([bytes]);
        const a = document.createElement("a");
        a.href = URL.createObjectURL(blob);
        a.download = name;
        a.click();
      };
      li.appendChild(dl);
      list.appendChild(li);
    });
  } catch (e) {
    list.innerHTML = `<li>${e.message}</li>`;
  }
}

async function uploadFile(file) {
  const buf = await file.arrayBuffer();
  await api("/v1/fs/upload", {
    method: "POST",
    body: JSON.stringify({ name: file.name, data_hex: hexFromBuffer(buf) }),
  });
  await refreshFs();
}

const drop = document.getElementById("dropzone");
const fileInput = document.getElementById("fileInput");
drop.addEventListener("click", () => fileInput.click());
fileInput.addEventListener("change", async () => {
  for (const f of fileInput.files) await uploadFile(f);
});
["dragenter", "dragover"].forEach((ev) =>
  drop.addEventListener(ev, (e) => { e.preventDefault(); drop.classList.add("drag"); })
);
["dragleave", "drop"].forEach((ev) =>
  drop.addEventListener(ev, (e) => { e.preventDefault(); drop.classList.remove("drag"); })
);
drop.addEventListener("drop", async (e) => {
  for (const f of e.dataTransfer.files) await uploadFile(f);
});

let selectedPkg = null;

async function refreshPkgs(q = "") {
  const pkgs = await api(`/v1/freight/search?q=${encodeURIComponent(q)}`);
  const list = document.getElementById("pkgList");
  list.innerHTML = "";
  (pkgs.items || pkgs || []).forEach((p) => {
    const li = document.createElement("li");
    li.innerHTML = `<span><strong>${p.name}</strong>@${p.version}<br/><small>${p.description || ""}</small></span>`;
    const install = document.createElement("button");
    install.textContent = "Install";
    install.onclick = async () => {
      await api("/v1/freight/install", {
        method: "POST",
        body: JSON.stringify({ name: p.name, version: p.version, tenant: "freight" }),
      });
      selectedPkg = p;
      document.getElementById("runOut").textContent = `installed ${p.name}@${p.version}`;
    };
    li.appendChild(install);
    list.appendChild(li);
  });
}

document.getElementById("freightSearch").onclick = () =>
  refreshPkgs(document.getElementById("freightQuery").value);
document.getElementById("freightDemo").onclick = async () => {
  await api("/v1/freight/publish-demo", { method: "POST", body: "{}" });
  await refreshPkgs("");
};
document.getElementById("runPkg").onclick = async () => {
  if (!selectedPkg) {
    document.getElementById("runOut").textContent = "install a package first";
    return;
  }
  const out = await api("/v1/freight/run", {
    method: "POST",
    body: JSON.stringify({
      name: selectedPkg.name,
      tenant: "freight",
      input_hex: document.getElementById("invokeIn").value || "29",
    }),
  });
  document.getElementById("runOut").textContent = JSON.stringify(out, null, 2);
};

async function refreshTopo() {
  try {
    const c = await api("/v1/cluster");
    document.getElementById("topoOut").textContent = JSON.stringify(c, null, 2);
  } catch (e) {
    document.getElementById("topoOut").textContent = e.message;
  }
}

// Collab WebSocket
const notes = document.getElementById("notes");
const wsState = document.getElementById("wsState");
let ws;
let applyingRemote = false;
function connectWs() {
  const proto = location.protocol === "https:" ? "wss" : "ws";
  ws = new WebSocket(`${proto}://${location.host}/v1/collab/ws?session=notes`);
  ws.onopen = () => { wsState.textContent = "ws: connected"; };
  ws.onclose = () => {
    wsState.textContent = "ws: reconnecting…";
    setTimeout(connectWs, 1500);
  };
  ws.onmessage = (ev) => {
    try {
      const msg = JSON.parse(ev.data);
      if (msg.text != null) {
        applyingRemote = true;
        notes.value = msg.text;
        applyingRemote = false;
      }
    } catch {}
  };
}
let noteTimer;
notes.addEventListener("input", () => {
  if (applyingRemote || !ws || ws.readyState !== 1) return;
  clearTimeout(noteTimer);
  noteTimer = setTimeout(() => {
    ws.send(JSON.stringify({ type: "set", text: notes.value }));
  }, 80);
});

refreshStatus();
refreshFs();
refreshPkgs("");
refreshTopo();
connectWs();
setInterval(() => { refreshStatus(); refreshTopo(); }, 3000);
