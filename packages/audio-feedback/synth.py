"""Procedural PCM/WAV tones for Chimera mesh events."""
from __future__ import annotations

import math
import struct
import wave
from pathlib import Path


def _tone(freq: float, ms: int, volume: float = 0.3, rate: int = 22050) -> list[float]:
    n = int(rate * ms / 1000)
    out = []
    for i in range(n):
        t = i / rate
        env = min(1.0, i / (0.01 * rate)) * min(1.0, (n - i) / (0.02 * rate))
        out.append(volume * env * math.sin(2 * math.pi * freq * t))
    return out


def write_wav(path: Path, samples: list[float], rate: int = 22050) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "w") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(rate)
        frames = b"".join(struct.pack("<h", int(max(-1, min(1, s)) * 32767)) for s in samples)
        w.writeframes(frames)


EVENT_TONES = {
    "peer_join": (523.25, 120),
    "peer_lost": (220.0, 180),
    "task_complete": (659.25, 90),
    "alert": (880.0, 250),
}


def synthesize_event(name: str, out_dir: Path | None = None) -> Path:
    freq, ms = EVENT_TONES.get(name, (440.0, 100))
    samples = _tone(freq, ms)
    out = (out_dir or Path("out")) / f"{name}.wav"
    write_wav(out, samples)
    return out


if __name__ == "__main__":
    for ev in EVENT_TONES:
        p = synthesize_event(ev, Path("generated"))
        print("wrote", p)
