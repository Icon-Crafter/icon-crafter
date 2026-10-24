Please consult the project's documentation before contributing.

# Contributing

Thanks for helping with **Icon Crafter** — a Rust design platform (engine +
Tauri desktop app + Axum backend) built from an original implementation.

## Ground rules

1. **Never mark a feature done because the UI exists.** Every row in
   [`docs/FEATURE-CHECKLIST.md`](docs/FEATURE-CHECKLIST.md) names its evidence
   (a test, a log entry, a screenshot). Keep it honest.
2. **No placeholders in core paths.** If something is unimplemented it must
   return an explicit `unsupported`/validation error and be documented.
3. **Fix errors, don't silence them.** `-D warnings` is enforced in CI.
4. **Small, verifiable changes.** One coherent feature per PR beats a rewrite.

## Local setup

```powershell
# Toolchain: Rust stable (rustup), Node 22+, pnpm 10, WebView2 (Windows)
cargo test -p dp-model -p dp-geom -p dp-layout -p dp-text -p dp-render `
           -p dp-io -p dp-persist -p dp-editor -p dp-collab -p dp-prototype `
           -p dp-devmode -p dp-ai -p dp-plugins --no-default-features

pnpm --dir apps/ui install
pnpm --dir apps/ui test
pnpm --dir apps/ui build

cargo test -p iconcrafter-server --lib
```

Run the desktop app against the Vite dev server:

```powershell
pnpm --dir apps/ui dev            # terminal 1 (http://localhost:1420)
cargo run -p iconcrafter-desktop # terminal 2
```

Standalone/production behaviour (embedded UI, correct after packaging):

```powershell
pnpm --dir apps/ui build
cargo build -p iconcrafter-desktop --release --features custom-protocol
```

## Before opening a PR

```powershell
cargo fmt --all
cargo clippy -p dp-model -p dp-geom -p dp-layout -p dp-text -p dp-render `
             -p dp-io -p dp-persist -p dp-editor -p dp-collab -p dp-prototype `
             -p dp-devmode -p dp-ai -p dp-plugins --no-default-features --all-targets -- -D warnings
cargo test  -p dp-model -p dp-geom -p dp-layout -p dp-text -p dp-render `
            -p dp-io -p dp-persist -p dp-editor -p dp-collab -p dp-prototype `
            -p dp-devmode -p dp-ai -p dp-plugins --no-default-features
pnpm --dir apps/ui typecheck && pnpm --dir apps/ui test && pnpm --dir apps/ui build
```

CI runs exactly these checks (`.github/workflows/ci.yml`) plus dependency audit,
license policy and secret scanning (`security.yml`).

## Commit messages

Conventional commits are enforced by the installed git hook
(`commitlint.config.js`):

```
feat(editor): layer drag and drop reparenting
fix(render): cache clip masks per frame
docs(readme): install instructions
```

Prefixes `docs:`, `ci:` and `chore:` are treated as non-shipping by the release
automation.

## Branching and releases

- Work on a branch, open a PR; `main` is protected by the CI checks.
- Merging a PR does **not** create a release. A maintainer starts a version with
  the *Release version* workflow (**Actions → Release version → Run workflow**),
  which bumps the version, updates `CHANGELOG.md`, tags `vX.Y.Z` and dispatches
  the ship workflow that builds NSIS/MSI installers and publishes the release.
- Direct pushes to `main` are patch-shipped by the *Auto ship* workflow unless
  they only touch docs/CI.

Details: [`docs/RELEASING.md`](docs/RELEASING.md).

## Areas of the codebase

| Crate | Responsibility |
|---|---|
| `crates/dp-geom` | geometry kernel, Bezier paths, booleans, spatial index, snapping |
| `crates/dp-model` | document model, pages, styles, variables, components, transactions |
| `crates/dp-text` | font discovery/shaping/measurement (cosmic-text) |
| `crates/dp-layout` | deterministic auto layout |
| `crates/dp-render` | display list, culling, CPU rasterizer (tiny-skia) |
| `crates/dp-io` | SVG/PNG/JPEG/PDF/HTML export, SVG + image import |
| `crates/dp-persist` | `.icraft` container, autosave, versions |
| `crates/dp-editor` | transport-independent editor state machine and commands |
| `crates/dp-collab` | CRDT-style collaboration protocol |
| `crates/dp-prototype` | interaction graph and presentation runtime |
| `crates/dp-devmode` | inspection, deterministic codegen, revisions |
| `crates/dp-ai` | optional AI providers, validated operation proposals |
| `crates/dp-plugins` | manifest/permissions/op-scoped plugin sandbox |
| `apps/desktop` | Tauri shell and IPC command layer |
| `apps/backend` | Axum + PostgreSQL collaboration server |
| `apps/ui` | React/TypeScript presentation layer |

Architecture and contracts live in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
and [`docs/CONTRACT.md`](docs/CONTRACT.md).
