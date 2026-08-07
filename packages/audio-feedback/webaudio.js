// WebAudio procedural blips for MeshShell events.
export function playEvent(name) {
  const ctx = new (window.AudioContext || window.webkitAudioContext)();
  const o = ctx.createOscillator();
  const g = ctx.createGain();
  const map = { peer_join: 523, peer_lost: 220, task_complete: 659, alert: 880 };
  o.frequency.value = map[name] || 440;
  o.type = "sine";
  g.gain.value = 0.08;
  o.connect(g); g.connect(ctx.destination);
  o.start();
  g.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.15);
  o.stop(ctx.currentTime + 0.16);
}
