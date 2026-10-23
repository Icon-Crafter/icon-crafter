# Icon Crafter

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
