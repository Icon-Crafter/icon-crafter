# Development log

Chronological record of work, decisions and verification. Add an entry for every
meaningful step; keep claims tied to commands or manual checks.

## Session 2 — 2026-10-23 (evening): engine foundation

### Repository inspection (Phase 1)
- Repository was empty (0 files) on a Windows 10 machine (NT 10.0.26300), Rust 1.98.0
  stable-msvc, Node 25.2.1, pnpm 10.18.3, Python 3.14.6, git 2.55.0, WebView2 runtime
  present. Docker is **not** installed (documented gap; backend integration tests are
  `#[ignore]`).
- Network access to crates.io and npmjs confirmed; dependency versions pinned after
  reading current crates.io metadata (not guessed).

### Architecture & docs (Phase 2)
- Created `docs/ARCHITECTURE.md`, `DOCUMENT-FORMAT.md`, `COLLAB-PROTOCOL.md`,
  `CONTRACT.md`, `DESIGN-SYSTEM.md`, `FEATURE-CHECKLIST.md`, `TESTING.md`,
  `RELEASE-CHECKLIST.md`, `DEVELOPMENT-LOG.md`.
- Decisions: single Cargo workspace; Rust owns document/geometry/layout/scene and builds
  a display list; wgpu backend behind `dp-render` feature `gpu`; tiny-skia CPU rasterizer
  is the deterministic reference for export/thumbnails; React/TS is presentation only.
- Collaboration: per-field LWW for node properties, RGA for ordered child lists, HLC
  stamps, tombstoned deletes, SQLite journal + server snapshot keying (not whole-document
  last-write-wins).

### Scaffold
- 12 `crates/dp-*` engine crates, `apps/desktop` (Tauri 2 shell, config, capabilities,
  generated brand icons), `apps/backend` (Axum stub, sqlx/argon2 deps), `apps/ui`
  (React/TS by the frontend teammate).
- Brand: "Icon Crafter" graphite + coral + teal, original sparkle mark rendered to PNG/ICO
  by `tools/gen_icons.py` (pure Python, reproducible).

### dp-geom (44 tests, `cargo test -p dp-geom` → 44 passed)
- Transform math (`Mat2D`), `Point`/`Size`/`Insets`/`Rect`/`CornerRadii`.
- Bezier `VectorPath` (anchors with handles, adaptive flattening, exact cubic bounds,
  even-odd/non-zero containment, outline distance, split/remove/reverse, kurbo bridge).
- Shape primitives (rectangle with per-corner radii, ellipse, line, polygon, star).
- Boolean ops via `i_overlay` 9.1.0 (flattened tolerance 0.02, documented as polygonal).
- Spatial hash grid with huge-query fallback; snapping engine (grid/guides/object
  edges+centres, threshold, guides emitted for overlay).
- Bugs found by tests and fixed: cubic extrema polynomial coefficients; polygon-width and
  ellipse-area test assumptions.

### dp-model (65 tests, `cargo test -p dp-model` → 65 passed)
- Paint model (solid/linear/radial/angular/image), fills with style+variable refs,
  strokes (alignment/cap/join/dash), effects, blend modes, node styles.
- Nodes (17 kinds), transforms (`T(centre)·R·S·T(-w/2,-h/2)`), geometry variants
  (vector/polygon/star/boolean), text content and typography, auto-layout descriptors,
  constraints, component roles/overrides, prototype interactions.
- Styles registry, variable collections/modes with alias resolution and cycle guard,
  component registry, assets.
- Document schema v1 with canonical JSON serialization, migrations hook, validation
  (≈70 rule codes), transactions/history with coalescing, and command operations:
  insert/patch/nudge/delete/duplicate/reorder/group/ungroup/boolean/align/distribute.
- Bugs found by tests and fixed: spring easing endpoints not exact (now pinned 0/1);
  duplicate subtree children double-linked during insert (children rebuilt on insert);
  non-canonical JSON round-trip (serialize through `serde_json::Value`); `locate_node`
  returns `(&Page,&Node)`; `AutoLayout::vertical` takes `f64` padding.

### dp-text (13 tests, `cargo test -p dp-text` → 13 passed)
- `FontCatalog` over `cosmic-text`/fontdb with case-insensitive family resolution and
  explicit *missing-font* reporting (cosmic-text falls back silently).
- Shaping via `FontSystem`/`Buffer` with per-character `Attrs` (family, weight, italic,
  letter spacing in EM units), `Wrap::WordOrGlyph`, ellipsis for `max_lines`, and
  `Align` mapping including justified text.
- `TextLayout`/`TextLine`/`Glyph` carry document-space positions, byte clusters, font id,
  flags; `GlyphBitmap` rasterization through `SwashCache` (straight-alpha RGBA8) for the
  CPU rasterizer and future glyph atlases.
- Layout cache (bounded, cleared wholesale on overflow) so drag frames don't reshape.
- Behaviours found by tests: empty text must return zero size (cosmic-text reports one
  empty line); hug-width text does not wrap (only auto-height/fixed boxes wrap);
  `max_lines` needs a second no-ellipsis probe to report overflow honestly.

### dp-layout (12 tests, `cargo test -p dp-layout` → 12 passed)
- Deterministic auto layout: horizontal/vertical, gap/padding, hug/fill/fixed sizing,
  min/max clamp, primary alignment (start/center/end/space-between), counter alignment
  (start/center/end/stretch/baseline), item reverse, wrapping rows, nested layouts.
- Hug containers are measured bottom-up; fill children are re-measured against the final
  container cross size (text re-shapes at the resolved width).
- Groups without auto layout hug the union of their children and normalise child
  positions. Hidden children are excluded from flow but still placed for hit testing.
- World matrices compose parent transforms; `NodeLayout::contains` hit tests through the
  inverse matrix. Constraints are applied by the command layer on resize (documented).


### Teammates
- `frontend`: scaffolded `apps/ui` (Vite + React 19 + TS strict, zustand, vitest) with the
  editor shell, design tokens, canvas raster blit + overlay, transport/contract dev
  harness, command palette, panels; contract gaps (bootstrap shape, `set_viewport`,
  `select_nodes`, page commands, command argument conventions) resolved and recorded in
  `docs/CONTRACT.md` §7.
- `backend-api`: building the Axum/Postgres backend (auth, teams/projects/files,
  comments, shares, versions, assets, WS relay/journal, docker compose).

### Verification commands run this session
- `cargo metadata --no-deps` → all 14 members parse.
- `cargo test -p dp-geom` → 44 passed.
- `cargo test -p dp-model` → 65 passed.
- `cargo fix` warning cleanup + `git commit` ("engine: geometry kernel and design
  document model").

### Next
- `dp-text` (font discovery, shaping, measurement), `dp-layout` (deterministic auto
  layout), `dp-render` display list + CPU rasterizer, then the desktop engine and IPC
  layer to make the vertical slice interactive.

## Session 1 — 2026-10-23: bootstrap
- Inspected the empty repository and environment; created the workspace scaffold,
  architecture documentation, brand assets and the initial commit.

## Session 3 — 2026-10-23: engine vertical slice, desktop IPC, backend delivered

### New engine crates (all on disk, all test suites green)
- `dp-render` — display list (`DisplayList`/`SceneItem`), `SceneBuilder` (document +
  layout → resolved paints/strokes/text, clipping AABBs, culling), tiny-skia CPU
  rasterizer (`RenderTarget`, deterministic straight-RGBA output, PNG encoding), and a
  pan/zoom `Viewport`. wgpu is declared but intentionally not implemented yet (no fake GPU
  path). Tests: 6 viewport unit tests + 11 integration tests (`cargo test -p dp-render`).
- `dp-io` — SVG export (editable paths + real `<text>`), SVG import (usvg → editable cubic
  vector nodes, solid paints, warnings for gradients/text/images), PNG/JPEG raster export
  with scale, PDF export via `svg2pdf`. Tests: 9 (`cargo test -p dp-io`).
- `dp-persist` — native `.icraft` container (ZIP + `document.json` + `meta.json`), atomic
  writes, autosave recovery journal, schema validation, project load/save. Tests: 13
  (`cargo test -p dp-persist`).
- `dp-editor` — transport-independent editor state machine: document/history, viewport,
  tools, selection, hit testing, drag interactions (move/marquee/create), snapping,
  keyboard shortcuts, text-edit sessions, clipboard, render + overlay, save/open/export,
  PNG/SVG/PDF export, SVG import, and the whole contract command table. Tests: 19
  integration tests (`cargo test -p dp-editor`).
- `apps/desktop/src/ipc.rs` — Tauri 2 IPC: bootstrap, document lifecycle, save, engine
  state, tool/pointer/wheel/key input (flat contract payloads), command dispatch, raw RGBA
  `render_frame`, node details, PNG/SVG/PDF export, SVG import, preferences; honest
  `unsupported` for image import. `cargo check -p iconcrafter-desktop` passes.

### Contract alignment (docs/CONTRACT.md §8)
Overlay is world-space except the text caret hint; one-shot engine errors; render_frame is
raw RGBA8; documented limitations for import/export/overlays.

### Backend delivered by `backend-api` (compiles, unit-tested)
41 files (~10k lines): Axum 0.8 routes for auth/teams/projects/files/docs/versions/comments/
shares/assets/publish/activity/health, WS relay + op journal, Argon2id sessions with hashed
tokens + CSRF, RBAC per file role, rate limiting, storage trait (local FS + optional S3),
additive SQL migration (16 tables), docker compose + Dockerfile, integration test suite.
Verified before the disk incident: `cargo check -p iconcrafter-server` clean, 132 unit tests
passed (5 PG integration ignored), `--features s3` and `--features otel` check clean, clippy
`-D warnings` clean, fmt clean. PostgreSQL integration tests have NOT run (Docker is not
installed on this host); the README marks that gap.

### Environment incident
`C:` ran out of space mid-build (~10 MB free, 8.77 GB target). `cargo clean` + deleting the
leftover target directory restored ~19 GB; the dev profile was changed to
`debug = 0`, `incremental = false` to keep artifacts small. Backend teammate had already
deleted `target/debug/incremental` (3.42 GB) while investigating.

### Verification commands run this session
- `cargo test -p dp-render` → 6 + 11 passed.
- `cargo test -p dp-io --no-default-features` → 9 passed.
- `cargo test -p dp-persist` → 13 passed.
- `cargo test -p dp-editor --no-default-features` → 19 passed.
- `cargo check -p iconcrafter-desktop` → finished.
- Backend: see the report above.

### Known gaps / next session
- Frontend milestone 2 in flight (real transport, textarea editing, inspector wiring, visual
  polish, `pnpm build` fix). `apps/ui/dist` currently holds a placeholder so Tauri builds.
- App launch + end-to-end manual checklist not yet run (needs the real UI bundle).
- Backend unit tests must be re-run after the clippy refactor; PG integration awaits Docker.
- Image import, wgpu backend, tile cache/benchmarks, collaboration wiring are explicitly
  pending (see `docs/FEATURE-CHECKLIST.md`).
