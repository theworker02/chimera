# RFC-0007: Chimera Sovereign surfaces

## chimera-usb
Portable binary; config/state beside the executable. `--benchmark-startup` prints **measured** init time.

## MeshShell Sovereign Dash
`GET /meshshell` → WebGL canvas (`dashboard.js`). Fed by `/health`.

## Security
- Simulated TEE: `chimera::tee`
- mTLS lab: `chimera::mtls`
- Retro-scale: `chimera::retro_scale`
- Continuity: `chimera::continuity`
