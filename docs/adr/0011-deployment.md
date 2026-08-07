# ADR-0011: Deployment artifacts

## Status
Accepted

## Context
Enterprises expect Docker Compose and Kubernetes starting points.

## Decision
Ship `Dockerfile`, `docker-compose.yml` (3-node), and `deploy/k8s/` manifests + minimal Helm chart.
A Kubernetes **operator** is roadmap-only.

## Honesty
Artifacts are **syntactically authored** but **not executed** on the Chimera Windows build host (no Docker/K8s). Validate in CI where runners allow.
