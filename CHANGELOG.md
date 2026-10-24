# Changelog

All notable changes to Icon Crafter are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Release automation:
- `.github/workflows/version.yml` raises the workspace version, syncs
  `apps/ui/package.json` and `apps/desktop/tauri.conf.json`, promotes the
  `Unreleased` section below and tags `vX.Y.Z`.
- `.github/workflows/ship.yml` builds the installers for that tag and publishes
  a GitHub Release.
- `.github/workflows/auto-ship.yml` patch-bumps and ships direct pushes to
  `main`.

## [Unreleased]

### Added

- **Image import with embedded assets**: PNG/JPEG/WebP/GIF import becomes an
  editable image layer; assets travel inside `.icraft` (`data_base64`) and are
  rendered by the CPU renderer and SVG export.
- **Windows installers**: reproducible NSIS + MSI packaging
  (`tools/package-windows.ps1`) with verified silent install/launch/uninstall.
- **AI assistant (offline by default)**: validated document-op proposals,
  one-undo apply, destructive-change confirmation; optional OpenAI-compatible
  provider via `ICONCRAFTER_AI_*` environment variables.
- **Plugin subsystem**: manifest validation, declared permissions, op-scoped
  sandbox, registry, lifecycle and error isolation (no client-side code runs).
- **Website export**: HTML/CSS bundle with real text, inline SVG vectors and
  prototype hot-spots.
- **Figma-style layer drag & drop**: drag a layer onto a container to nest it,
  or between rows to reorder, with drop indicators.

### Changed

- Desktop rendering performance: clip-mask cache, scaled rendering during
  gestures, demultiplied IPC transfer, rAF-coalesced state commits
  (full render 77 ms → 47 ms, drag ≈13 → ≈85 fps — docs/PERFORMANCE.md).
- Canvas viewport now resyncs with the measured CSS size, fixing stretched
  frames and selection outlines that could not line up with artboards.

## [0.1.0] - 2026-10-24

### Added

- Rust workspace: geometry kernel, document model with undoable transactions,
  text shaping, deterministic auto-layout, CPU rasterizer, SVG/PNG/JPEG/PDF
  IO, `.icraft` persistence, convergent collaboration protocol, prototype
  runtime, Dev Mode inspection/codegen, AI and plugin subsystems.
- Tauri 2 desktop application with a React/TypeScript presentation layer:
  layers/assets/components/variables panels, inspector, command palette,
  Dev Mode panel, prototype presentation, AI assistant and plugin manager.
- Axum backend with Argon2id auth, RBAC, projects/files/comments/versions/
  shares/assets and a WebSocket op relay.
- 340+ engine tests, 150+ frontend tests, strict clippy and rustfmt gates.

[Unreleased]: https://github.com/Icon-Crafter/icon-crafter/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Icon-Crafter/icon-crafter/releases/tag/v0.1.0
