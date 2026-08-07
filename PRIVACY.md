# Privacy Policy

**Effective date:** 2026-08-07  
**Project:** [Chimera](https://github.com/theworker02/chimera)  
**Contact:** [matthewlooney5@gmail.com](mailto:matthewlooney5@gmail.com)

This policy describes how privacy works for the Chimera open-source project and
the software in this repository. It is written for a decentralized peer-to-peer
tool, not a hosted SaaS product.

## Summary

- Chimera is open-source software you run yourself (or that peers run on their
  own machines).
- **The project authors do not operate a central Chimera cloud that collects
  your personal data by default.**
- When you run a node, telemetry and logs stay under **your** control unless
  **you** configure export to a third party.
- Using GitHub, crates.io, npm, or similar registries is governed by **those
  services’** privacy policies, not this one.

## What Chimera is

Chimera is a peer-to-peer compute and storage mesh. Nodes discover peers, move
workloads, and exchange data over networks you join. There is no required
master server operated by the Chimera maintainers.

## Data the software may process locally

Depending on how you configure and use Chimera, a node may process:

- **Operational metrics** you enable (CPU, memory, task counts, mesh health)
- **Workload inputs/outputs** you submit (Wasm modules, files, function payloads)
- **Mesh metadata** needed for routing (peer IDs, addresses, capability ads,
  content hashes)
- **Audit / receipt logs** you enable for compliance or debugging
- **Local credentials** you create (join tokens, keys) stored on disk you control

By default, that information remains on the machines participating in your mesh
and on paths you configure. Chimera does not silently phone home to the
maintainers.

## Optional telemetry you configure

If you turn on OpenTelemetry or similar exporters, metrics/traces/logs go to
**the backends you point them at** (for example your own Prometheus, Grafana, or
OTLP collector). The Chimera project does not receive that data unless you
explicitly send it somewhere the maintainers operate (we do not provide a
default collector endpoint).

## Peer-to-peer sharing

When you join a mesh, other peers can see network-visible information required
for the protocol to work (for example advertised addresses, peer identifiers,
capability scores, and content-addressed block inventories). Do not put secrets
in public gossip fields. Treat untrusted meshes like an untrusted network.

## Third-party services

If you interact with this project through third parties, their policies apply,
including but not limited to:

- [GitHub Privacy Statement](https://docs.github.com/en/site-policy/privacy-policies/github-privacy-statement)
- [crates.io / Rust policies](https://crates.io/policies)
- npm, PyPI, Docker Hub, or other registries you use to download packages

Cloning the repository, starring it, opening issues, or downloading release
assets may expose account or IP information to those providers under their
terms.

## Cookies and websites

The project documentation site hosted on GitHub Pages is static. We do not set
Chimera-specific tracking cookies. GitHub Pages and your browser may still
apply their own technical logging.

## Children

Chimera is developer tooling and is not directed at children under 13. We do
not knowingly collect personal information from children.

## No sale of personal data

The Chimera maintainers do not sell personal data.

## Your choices

- Run Chimera offline or on a private mesh.
- Disable or avoid optional telemetry exporters.
- Delete local data directories, logs, and keys on machines you control.
- Leave a mesh by shutting down or reconfiguring your node.

## Privacy requests

For privacy questions about **this open-source project** (not about a mesh
someone else operates), contact
[matthewlooney5@gmail.com](mailto:matthewlooney5@gmail.com) or open an issue at
[github.com/theworker02/chimera/issues](https://github.com/theworker02/chimera/issues).

If your request concerns data held by GitHub, crates.io, or another provider,
contact that provider directly.

## Changes

We may update this policy as the project evolves. Material changes will be
reflected in this file in the repository, with an updated effective date.
