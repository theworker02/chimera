# Chimera Brand Guidelines

## Metaphor
Chimera is a resilient mesh: independent nodes interlocking into one creature. The mark is a hexagonal node graph with a luminous core — decentralized strength without a master.

## Palette
| Token | Hex | Use |
|---|---|---|
| Void Black | `#0A0A0C` | Backgrounds, voids, terminal chrome |
| Electric Cyan | `#00F0FF` | Primary brand, links, live metrics |
| Warning Amber | `#FFB800` | Accents, alerts, secondary CTAs |
| Neutral Gray | `#A0A4AB` | Supporting copy |

## Typography
- Display / wordmark: geometric sans (Segoe UI / Helvetica / Inter as fallbacks in SVG)
- Terminal: monospace for protocol / CLI

## Logo system
| Asset | Path | Notes |
|---|---|---|
| Standalone mark | `assets/brand/chimera-mark.svg` | GitHub avatar / favicon source |
| App / CLI icon | `assets/brand/chimera-icon.svg` | Rounded 512² |
| Horizontal lockup | `assets/brand/chimera-lockup-horizontal.svg` | Docs headers |
| Stacked wordmark | `assets/brand/chimera-wordmark-stacked.svg` | Tall placements |
| README banner | `assets/brand/chimera-banner.svg` | Hero |

Export targets: Figma / Illustrator via SVG import. Prefer SVG in git; rasterize PNG @1x/@2x only for stores that require it.

## Clear space
Keep clear space ≥ one node radius around the mark. Never recolor cyan to purple gradients. Never place the mark on low-contrast mid-grays.

## CLI / TUI
Launch banners use electric cyan ANSI (`38;2;0;240;255`) on void-black terminals. See `chimera::brand::ascii_banner`.

## Watermarking
Distributed assets and Wasm outputs carry a BLAKE3 brand watermark (`chimera::brand::payload_watermark`) binding content hash + node hint + palette salt.
