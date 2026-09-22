# Acquisition Brief â€” Chimera

**Date:** 2026-09-22  
**Repository:** https://github.com/theworker02/chimera  
**Default branch:** `main`  
**Primary language:** Rust  
**Status:** Diligence briefing only. **No acquisition has occurred** by virtue of this file.  
**License:** Proprietary â€” sale, written commercial license, or completed asset transfer required (see root `LICENSE`).  
**Valuation:** Not stated.  
**Contact:** GitHub [@theworker02](https://github.com/theworker02) Â· [thanks.dev/u/gh/theworker02](https://thanks.dev/u/gh/theworker02)

> Cloning or forking this repository does **not** grant production, redistribution, SaaS, OEM, or commercial rights.

---

## 1. Executive thesis

<img src="./assets/brand/chimera-banner.svg" alt="Chimera Ã¢â‚¬â€ decentralized peer-to-peer compute mesh" width="100%"/> <a href="https://github.com/theworker02/chimera/actions"><img src="https://img.shields.io/github/actions/workflow/status/theworker02/chimera/ci.yml?branch=main&label=CI" alt="CI"/></a> <a href="./LICENSE"><img src="https://img.shields.io/badge/license-Proprietary%20(source--available)-00F0FF?labelColor=0A0A0C" alt="License: MIT"/></a>

**Why a buyer cares:** Chimera packages transferable product IP â€” source, docs, in-repo brand assets, and a diligence room under `docs/acquisition/` â€” under a clear proprietary posture so diligence can proceed without mistaking the repo for open source.

---

## 2. Product snapshot

| Item | Detail |
|------|--------|
| Product | Chimera |
| Repo | `theworker02/chimera` |
| Language | Rust |
| Open source? | **No** â€” proprietary |
| Rightsholder | theworker02 |
| Diligence pack | `docs/acquisition/` |

### Capability highlights (from current materials)

- [`chimera-nano-kernel`](https://crates.io/crates/chimera-nano-kernel)
- [`chimera-crypto-quantum`](https://crates.io/crates/chimera-crypto-quantum)
- [`chimera-transport-quic`](https://crates.io/crates/chimera-transport-quic)
- [`chimera-dht-routing`](https://crates.io/crates/chimera-dht-routing)
- [`chimera-wasm-runtime`](https://crates.io/crates/chimera-wasm-runtime)
- [`chimera-memory-fabric`](https://crates.io/crates/chimera-memory-fabric)
- [`chimera-storage-cas`](https://crates.io/crates/chimera-storage-cas)
- [`chimera-fuser-mount`](https://crates.io/crates/chimera-fuser-mount)
- [`chimera-consensus-dag`](https://crates.io/crates/chimera-consensus-dag)
- [`chimera-network-bridge`](https://crates.io/crates/chimera-network-bridge)
- [`chimera-compiler-jit`](https://crates.io/crates/chimera-compiler-jit)
- [`chimera-nexus`](https://crates.io/crates/chimera-nexus)

---

## 3. Problem / opportunity

Teams evaluating Chimera typically need either (a) a commercial right to run or embed it, or (b) outright ownership of the Product IP for strategic build-out. Public GitHub visibility without a proprietary license creates false assumptions about free production use. This brief and the linked data room make the commercial path explicit.

---

## 4. What ships today

Honest maturity: treat repository contents, README claims, tests, and release tags as the source of truth. Do not assume production customers, ARR, filed patents, or SLAs unless separately evidenced in diligence.

Typical transferable surfaces:

- Source tree and build/test scripts present in-repo
- Documentation and design notes
- Acquisition / diligence markdown under `docs/acquisition/`
- Branding assets committed to the repository (if any)

---

## 5. Demo / evaluation path (buyer)

Minimal path (no secrets required unless README says otherwise):

```
```mermaid
flowchart LR
  subgraph Mesh["P2P Mesh"]
    A[Node A<br/>Agent + Scheduler]
    B[Node B<br/>Agent + Scheduler]
    C[Node C<br/>Agent + Scheduler]
  end
  A <-- QUIC/TCP postcard --> B
  B <-- gossip multicast --> C
  A <-- steal / migrate --> C
  subgraph Fabric["Data + Memory"]
    FS[ChimeraFS CAS/DHT]
    MEM[ChimeraMEM DSM]
  end
  A --> FS
  B --> MEM
  C --> FS
  Intent[Declarative Intent] --> A
```
```mermaid
flowchart TB
  Intent["Intent / Agent"] -->|plan| Sched["Scheduler<br/>work-steal"]
  Intent -->|plan| Wasm["Wasmtime<br/>sandbox"]
  Intent -->|plan| Mem["ChimeraMEM<br/>soft DSM"]
  Sched <-->|"steal"| Wasm
  Wasm -->|"migrate"| Mem
  Sched -->|"prefetch"| FS["ChimeraFS<br/>BLAKE3 CAS / gossip DHT / VirtualMount / FUSE*"]
  Wasm -->|"I/O"| FS
  Mem -->|"pages"| FS
```
```bash
# From crates.io (after a real release)
cargo install chimeractl
cargo install chimera-boot
cargo install chimera-mesh --bin chimera

# Or fetch GitHub Release binaries (matches release.yml artifact names)
cargo binstall chimeractl
cargo binstall chimera-boot

```

Extended evaluation: `docs/acquisition/BUYER_EVALUATION.md`. Written NDA / evaluation grants may be required for private materials.

---

## 6. What a transaction typically includes

Subject to definitive schedules:

| Included (typical) | Excluded (typical) |
|--------------------|--------------------|
| Repo materials + asserted original IP | Seller personal accounts / unrelated repos |
| Docs + diligence room at closing | Third-party dependency source under separate licenses |
| In-repo brand marks as assigned | Secrets without rotation plan |
| Know-how captured in docs | Fabricated revenue, user, or adoption metrics |

---

## 7. Suggested deal structures

| Structure | When it fits |
|-----------|--------------|
| Non-exclusive commercial license | Deploy/run under seat or environment terms |
| Exclusive field-of-use license | Buyer wants exclusivity; seller may retain entity |
| Asset / IP assignment | Buyer wants ownership of Materials outright |
| OEM / redistribution | Separate agreement â€” not implied here |

Commercial terms (price, earnouts, escrow) are negotiated under NDA with counsel.

---

## 8. Buyer diligence checklist

- [ ] Confirm Rightsholder identity and authority to sell/license
- [ ] Inventory Materials (`docs/acquisition/ASSET_INVENTORY.md`)
- [ ] Review IP posture (`IP_PROVENANCE.md`) and dependencies (`DEPENDENCY_INVENTORY.md`)
- [ ] Run evaluation script (`BUYER_EVALUATION.md`)
- [ ] Review risks (`RISK_REGISTER.md`)
- [ ] Agree transfer scope (`TRANSFER_MANIFEST.md`) and handoff (`HANDOFF_CHECKLIST.md`)
- [ ] Supersede root `LICENSE` at closing via definitive agreement

---

## 9. Related documents

| Document | Purpose |
|----------|---------|
| `LICENSE` | Proprietary â€” no default grant |
| `docs/acquisition/README.md` | Data-room index |
| `docs/acquisition/EXECUTIVE_SUMMARY.md` | One-page thesis |
| `README.md` | Product overview |
| `SECURITY.md` | Vulnerability reporting |
| `COMMERCIAL.md` | Licensing contact path |
| `.github/FUNDING.yml` | Sponsors / thanks.dev |

---

## 10. Disclaimer

This package is informational and **does not** create a binding offer, grant of rights, or investment advice. Engage counsel for any transaction.

---

*Document version: 2.0.0 / 2026-09-22 Â· Classification: acquisition briefing*
