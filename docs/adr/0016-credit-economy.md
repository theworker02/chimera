# ADR-0016: Compute credit economy

## Status
Accepted (Phase 10)

## Context
Mesh workloads need a lightweight barter unit so nodes can charge for execution without an external chain.

## Decision
- Credit balances and signed double-entry transactions live in the Raft KV store (`ledger:bal:*`, `ledger:tx:*`).
- Earn: `CreditLedger::earn_from_receipt` credits an account after a verified Phase-4 compute receipt (fuel × rate).
- Spend: gateway invoke charges `invoke_cost` credits; insufficient balance → reject.
- Local meshes default to **bypass** (`--ledger-bypass` / not `--enforce-credits`) so demos work offline without funding accounts.

## Honesty
This is an accounting layer, not a cryptocurrency or planetary settlement network. No on-chain bridges.

## Consequences
Tests cover earn, spend, and broke rejection. Operators enable enforcement with `--enforce-credits`.
