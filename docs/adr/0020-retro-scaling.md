# ADR-0020: Retro-scaling execution policy

## Status
Accepted (Phase 11)

## Decision
`RetroScaler::plan(profile)` maps hardware profiles to `ExecTier` + caps:
- Constrained → Wasmi interpreter, low fuel/mem, precision degrade
- Capable → Wasmtime JIT, higher parallelism

Constrained paths **degrade** (downsample / fixed-point stand-in) instead of dropping tasks.

## Status
| Surface | Status |
|---|---|
| Policy module + tests | **working** |
| Automatic host Wasmtime↔wasmi hot-swap in the live node | **simulated / partial** (policy selects; node logs CNK preference) |
