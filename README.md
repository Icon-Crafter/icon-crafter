# Icon Crafter

[![CI](https://github.com/Icon-Crafter/icon-crafter/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Icon-Crafter/icon-crafter/actions/workflows/ci.yml)
[![Security](https://github.com/Icon-Crafter/icon-crafter/actions/workflows/security.yml/badge.svg?branch=main)](https://github.com/Icon-Crafter/icon-crafter/actions/workflows/security.yml)
[![Release](https://github.com/Icon-Crafter/icon-crafter/actions/workflows/ship.yml/badge.svg)](https://github.com/Icon-Crafter/icon-crafter/actions/workflows/ship.yml)
[![Latest release](https://img.shields.io/github/v/release/Icon-Crafter/icon-crafter?label=release)](https://github.com/Icon-Crafter/icon-crafter/releases/latest)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)

Icon Crafter is an original, professional design platform: a Rust-native design
document engine with GPU/CPU rendering, a Tauri 2 desktop editor, a collaborative
multiplayer backend, and companion workspaces for whiteboarding, slides,
illustration, website design, prototyping and developer handoff.

It is an independent implementation. It does not contain Figma source code,
assets, or branding; it implements equivalent functionality with original
engineering and original visual design.

## Status

This repository is under active incremental development. The authoritative
status of every capability lives in [`docs/FEATURE-CHECKLIST.md`](docs/FEATURE-CHECKLIST.md).
Nothing is considered done until it compiles, is exercised by a test where
appropriate, works in the running application, and survives save/reload.

Start with:

- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — system design and trade-offs
- [`docs/DOCUMENT-FORMAT.md`](docs/DOCUMENT-FORMAT.md) — the native document schema
- [`docs/COLLAB-PROTOCOL.md`](docs/COLLAB-PROTOCOL.md) — realtime synchronisation
- [`docs/TESTING.md`](docs/TESTING.md) — how to build, test and verify
- [`docs/RELEASING.md`](docs/RELEASING.md) — CI, versioning and installer automation
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — workflow, commit conventions and checks
- [`docs/DEVELOPMENT-LOG.md`](docs/DEVELOPMENT-LOG.md) — chronological engineering log

## Requirements

| Tool | Version | Purpose |
| --- | --- | --- |
| Rust (stable) | 1.90+ | every engine crate, desktop shell, server |
| Node.js | 22+ | frontend tooling |
| pnpm | 10+ | frontend dependencies |
| PostgreSQL | 16+ | server persistence (only for the backend) |
| Docker (optional) | 24+ | reproducible backend environment |
| WebView2 | any current | Tauri runtime on Windows |

## Quick start

```bash
# Build and test the engine crates
cargo test --workspace --exclude iconcrafter-desktop --exclude iconcrafter-server

# Frontend assets
cd apps/ui && pnpm install && pnpm build

# Run the desktop editor (requires the built frontend)
cargo run -p iconcrafter-desktop
```

Backend instructions live in [`docs/TESTING.md`](docs/TESTING.md).

## Repository layout

```
crates/           Rust engine + product crates (no UI framework in these)
apps/desktop/     Tauri 2 desktop shell (Rust) + IPC command layer
apps/ui/          React + TypeScript editor frontend (presentation layer only)
apps/backend/     Axum/Tokio collaboration + publishing server
docs/             Architecture, format, protocol, testing, release docs
fixtures/         Sample documents used by tests and benchmarks
docker/           Local backend stack
```

## License

MIT OR Apache-2.0.

## Optional AI and plugins

The editor is fully usable offline. Two optional subsystems are built in:

- **AI assistance** (`dp-ai`) — a deterministic offline provider ships by
  default; an OpenAI-compatible provider is available when built with the
  `http` feature and configured through `ICONCRAFTER_AI_ENDPOINT`,
  `ICONCRAFTER_AI_MODEL`, `ICONCRAFTER_AI_KEY`. AI output is schema-validated
  and applied as undoable transactions; destructive changes need confirmation.
- **Plugins** (`dp-plugins`) — validated manifests, declared permissions and
  user grants, op-scoped sandbox, lifecycle and typed settings. Execution is
  host-provided (no third-party JS/WASM runtime yet); see
  `crates/dp-plugins/examples/hello-plugin/` and `docs/FEATURE-CHECKLIST.md`.

## Build and run (verified commands)

Prerequisites: Rust stable (1.90+), Node 22+/pnpm 10+, Windows WebView2 runtime
(preinstalled on Windows 11). Docker is required only for the backend database/storage.

```powershell
# Frontend
pnpm --dir apps/ui install
pnpm --dir apps/ui build        # strict tsc + vite; writes apps/ui/dist

# Desktop engine + shell
cargo build -p iconcrafter-desktop
# Dev flow: debug builds load http://localhost:1420, so keep this running in another terminal:
pnpm --dir apps/ui dev

# Self-contained app (embeds apps/ui/dist, no dev server needed):
cargo build -p iconcrafter-desktop --release --features custom-protocol
# → target/release/iconcrafter-desktop.exe
```

```powershell
# Tests (engine, 13 crates)
cargo test --no-default-features -p dp-geom -p dp-model -p dp-text -p dp-layout `
  -p dp-render -p dp-io -p dp-persist -p dp-editor -p dp-collab -p dp-prototype `
  -p dp-devmode -p dp-ai -p dp-plugins

# Lint gate
cargo clippy --no-default-features --all-targets -- -D warnings

# Backend (needs Docker + PostgreSQL; see apps/backend/README.md)
docker compose up -d postgres minio redis
cargo test -p iconcrafter-server --offline --lib
cargo run -p iconcrafter-server
```

On a machine where Docker/PostgreSQL is unavailable, the backend compiles and its unit
tests run, but DB-backed integration tests and the live WS/HTTP flows remain unverified —
the checklist marks them accordingly instead of pretending they passed.

## Installers (Windows, verified)

Build both installers with:

``powershell
pnpm --dir apps/ui install          # one-time: installs the Tauri CLI devDependency
pwsh tools/package-windows.ps1     # UI build + NSIS + MSI
``

Artifacts land in 	arget/release/bundle/nsis/Icon Crafter_0.1.0_x64-setup.exe (NSIS, per-user install,
~5.2 MiB) and 	arget/release/bundle/msi/Icon Crafter_0.1.0_x64_en-US.msi (~8.3 MiB). Silent install,
launch and uninstall were verified (see docs/RELEASE-CHECKLIST.md and docs/TESTING.md). The installers are
currently **unsigned**, so Windows SmartScreen shows a warning; code signing requires a certificate.
