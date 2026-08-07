# gitops-operator

Kubernetes Helm chart + Compose deployment scaffolding for Chimera.

**Status:** scaffolding (artifacts under `deploy/`). Not a production operator controller yet.

```bash
helm template chimera ./deploy/k8s
docker compose -f docker-compose.yml config
```
