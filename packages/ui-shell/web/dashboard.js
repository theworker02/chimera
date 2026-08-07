/* Sovereign WebGL dashboard — raw WebGL1, no build toolchain.
   Status: working in-browser. Vulkan / embedded native display = roadmap. */

(function () {
  const canvas = document.getElementById("glCanvas");
  if (!canvas) return;
  const gl = canvas.getContext("webgl", { antialias: true, alpha: true });
  const hudCpu = document.getElementById("hudCpu");
  const hudPeers = document.getElementById("hudPeers");
  const hudWire = document.getElementById("hudWire");
  const hudHs = document.getElementById("hudHs");

  let peers = 0;
  let cpu = 0;
  let handshake = 0;
  let nodes = [];

  function seedNodes(n) {
    nodes = [];
    const count = Math.max(3, Math.min(12, n + 3));
    for (let i = 0; i < count; i++) {
      const a = (i / count) * Math.PI * 2;
      nodes.push({
        x: Math.cos(a) * 0.55,
        y: Math.sin(a) * 0.45,
        phase: Math.random() * Math.PI * 2,
      });
    }
  }
  seedNodes(0);

  if (!gl) {
    canvas.replaceWith(Object.assign(document.createElement("pre"), {
      className: "out",
      textContent: "WebGL unavailable — topology overlays disabled",
    }));
    return;
  }

  const vsSrc = `
    attribute vec2 a_pos;
    attribute float a_glow;
    varying float v_glow;
    void main() {
      v_glow = a_glow;
      gl_Position = vec4(a_pos, 0.0, 1.0);
      gl_PointSize = 10.0 + a_glow * 14.0;
    }
  `;
  const fsSrc = `
    precision mediump float;
    varying float v_glow;
    uniform vec3 u_color;
    void main() {
      float d = length(gl_PointCoord - vec2(0.5));
      float a = smoothstep(0.5, 0.1, d) * (0.4 + v_glow);
      gl_FragColor = vec4(u_color, a);
    }
  `;

  function compile(type, src) {
    const s = gl.createShader(type);
    gl.shaderSource(s, src);
    gl.compileShader(s);
    return s;
  }
  const prog = gl.createProgram();
  gl.attachShader(prog, compile(gl.VERTEX_SHADER, vsSrc));
  gl.attachShader(prog, compile(gl.FRAGMENT_SHADER, fsSrc));
  gl.linkProgram(prog);
  gl.useProgram(prog);
  const aPos = gl.getAttribLocation(prog, "a_pos");
  const aGlow = gl.getAttribLocation(prog, "a_glow");
  const uColor = gl.getUniformLocation(prog, "u_color");
  const buf = gl.createBuffer();
  gl.enable(gl.BLEND);
  gl.blendFunc(gl.SRC_ALPHA, gl.ONE);

  let last = performance.now();
  let frames = 0;
  let fps = 0;

  function frame(now) {
    const dt = (now - last) / 1000;
    last = now;
    frames++;
    if (frames % 30 === 0) fps = Math.round(1 / Math.max(dt, 0.001));

    handshake = Math.max(0, handshake - dt * 0.35);
    const linkPulse = 0.35 + 0.65 * Math.sin(now * 0.004) * 0.5 + 0.5;

    gl.viewport(0, 0, canvas.width, canvas.height);
    gl.clearColor(0.04, 0.045, 0.06, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);

    // Lines as degenerate point pairs (simple): draw nodes then fake links via extra points along edges
    const data = [];
    for (let i = 0; i < nodes.length; i++) {
      const n = nodes[i];
      const glow = 0.5 + 0.5 * Math.sin(now * 0.003 + n.phase) + handshake;
      data.push(n.x, n.y, Math.min(1.5, glow));
      const j = (i + 1) % nodes.length;
      const m = nodes[j];
      for (let t = 0.2; t < 1; t += 0.2) {
        data.push(
          n.x + (m.x - n.x) * t,
          n.y + (m.y - n.y) * t,
          0.15 + linkPulse * 0.25 + handshake * 0.4
        );
      }
    }

    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(data), gl.DYNAMIC_DRAW);
    gl.enableVertexAttribArray(aPos);
    gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 12, 0);
    gl.enableVertexAttribArray(aGlow);
    gl.vertexAttribPointer(aGlow, 1, gl.FLOAT, false, 12, 8);
    gl.uniform3f(uColor, 0.0, 0.94, 1.0);
    gl.drawArrays(gl.POINTS, 0, data.length / 3);

    if (hudHs) {
      hudHs.textContent = handshake > 0.05 ? `pulse ${(handshake * 100) | 0}% · ${fps}fps` : `idle · ${fps}fps`;
    }
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);

  async function poll() {
    try {
      const h = await fetch("/health").then((r) => r.json());
      cpu = h.cpu_pct || 0;
      peers = h.peers || 0;
      if (hudCpu) hudCpu.textContent = cpu.toFixed(1) + "%";
      if (hudPeers) hudPeers.textContent = String(peers);
      if (hudWire) hudWire.textContent = h.wire || "—";
      seedNodes(peers);
      handshake = 1;
    } catch {
      if (hudHs) hudHs.textContent = "link lost";
    }
  }
  poll();
  setInterval(poll, 2000);

  // Handshake visual when switching to dash tab
  document.querySelectorAll(".tabs button").forEach((btn) => {
    btn.addEventListener("click", () => {
      if (btn.dataset.view === "dash") handshake = 1;
    });
  });
})();
