# Icon Crafter — Architecture

Status: **living document**. Last substantive revision: session 1 (vertical-slice foundation).
This document describes the intended architecture and the reasoning behind it. Where
reality diverges, the divergence is listed under _Known gaps_ at the bottom — the code and
[FEATURE-CHECKLIST.md](FEATURE-CHECKLIST.md) are the source of truth for what is implemented.

## 1. Product scope

Icon Crafter is a professional interface-design platform: a native desktop design editor
(vector editing, typography, auto layout, components/variables, prototyping, Dev Mode),
a collaborative multi-workspace product (whiteboard, slides, illustration, website
publishing), and a Rust backend (accounts, projects, permissions, real-time sync,
comments, version history, publishing).

Original branding, original implementation. No Figma source code, assets or trade dress.

## 2. Technology decisions

| Concern | Choice | Rationale |
|---|---|---|
| Language | Rust (edition 2024, MSRV 1.90) | Mandated; one language for engine and server. |
| Desktop shell | Tauri 2 | Native webview, small binary, Rust command layer, no bundled Chromium. |
| UI layer | React + TypeScript (Vite) | Presentation only. No document logic in JS. |
| Engine | Rust crates under `crates/dp-*` | Document model, geometry, layout, text, rendering, persistence, collaboration. |
| GPU | `wgpu` (optional `gpu` feature in `dp-render`) | Accelerated scene rasterization where it measurably wins; CPU rasterizer is the deterministic reference. |
| CPU raster | `tiny-skia` | Deterministic, testable reference renderer used for export, thumbnails, and viewport tiles. |
| Text | `cosmic-text` (+ fontdb/swash) | Shaping, bidi, fallback, glyph rasterization, font discovery. |
| Vector geometry | `kurbo` (Bézier math), `i_overlay` (boolean ops) | Mature, pure-Rust. |
| Server | Axum 0.8 + Tokio | Mandated; tower ecosystem. |
| Database | PostgreSQL via `sqlx` (compile-time-checked queries with offline cache is **not** used; runtime queries instead) | Mandated; migrations via `sqlx::migrate!`. |
| Object storage | S3-compatible (`aws-sdk-s3`, optional `s3` feature) | Mandated; local filesystem provider for dev. |
| Coordination | Redis, optional feature | Presence, rate-limit counters, distributed server fan-out when scaling beyond one node. |
| Local state | SQLite (`rusqlite`, bundled) | Recent files, autosave journal, offline cache, font index. |
| Cross-cutting | `serde`, `serde_json`, `tracing` (+OTel optional), `uuid`, `thiserror` | Mandated / conventional. |

### Workspace layout

```
Cargo.toml                     # single workspace, default-members = engine crates only
rust-toolchain.toml            # stable + rustfmt + clippy
crates/
  dp-geom/                     # math: points, rects, affine transforms, Bézier paths, shape
                               # primitives, booleans, hit testing, spatial index, snapping
  dp-model/                    # document model: nodes, paints, text, auto-layout descriptors,
                               # styles, components, variables, pages, transactions, history,
                               # schema versioning + migration
  dp-text/                     # font discovery/indexing, shaping, fallback, paragraph layout,
                               # measurement, glyph runs, text-on-path
  dp-layout/                   # auto layout engine (hug/fill/fixed, wrapping, constraints)
  dp-render/                   # scene builder, display list, culling, tile cache,
                               # CPU rasterizer (tiny-skia), optional wgpu backend
  dp-io/                       # SVG import/export, PNG/JPEG export, PDF, HTML/CSS export,
                               # asset extraction, clipboard bridge payloads
  dp-persist/                  # native container format, autosave journal, version snapshots
                               # and deltas, SQLite cache, migrations registry
  dp-collab/                   # transport-independent collaboration protocol: op model,
                               # Lamport/vector clocks, replicated tree (RGA), presence, cursors
  dp-prototype/                # interaction graph, validated runtime, expressions, springs
  dp-devmode/                  # measurements, resolved styles, deterministic codegen
                               # (CSS/React/SwiftUI/Compose), annotations, revision diff
  dp-plugins/                  # plugin manifest, permissions, sandboxed op API, lifecycle
  dp-ai/                       # optional provider abstraction, prompt templates, validated
                               # operation proposals (never bypasses normal op pipeline)
apps/
  desktop/                     # Tauri shell (Rust): engine session, IPC, window chrome
  ui/                          # React/TS presentation layer (Vite)
  backend/                     # Axum server: REST + WebSocket sync, auth, RBAC, storage,
                               # publishing, migrations
docker/                        # docker-compose for PostgreSQL, MinIO, Redis
brand/                         # logo/marks (SVG)
tools/                         # reproducible asset generators (icons)
fixtures/                      # documents used by integration tests and benchmarks
docs/                          # this documentation set
```

Rules:

* One workspace. `default-members` excludes the heavy apps so `cargo check`/`cargo test`
  stay fast; build apps with `-p iconcrafter-desktop` / `-p iconcrafter-server`.
* Dependency ownership: shared dependencies live in root `[workspace.dependencies]`
  (Lead owns root `Cargo.toml`). Crate-specific dependencies are declared in the owning
  crate's manifest. Adding a shared dependency requires a Lead change.
* Dependency direction is acyclic and one-way:
  `geom ← model ← {text → layout, render, io, prototype, devmode, plugins, ai}`,
  `persist → model`, `collab → model`, apps → crates. `dp-geom` depends on no `dp-*` crate.
* No UI framework mix on the desktop: the webview is the only UI surface for the editor.
  Rust-native UI crates (egui) are not used; the engine has no windowing dependencies.
  Specialized tools (profiling overlays, benchmarks) are CLI binaries, not GUI frameworks.

## 3. Runtime topology

### Desktop

```
┌──────────────────────────── Tauri window ────────────────────────────┐
│ React/TS UI (apps/ui)                                                │
│   panels, dialogs, command palette, shortcuts — presentation only    │
│   canvas element: blits RGBA frame produced by Rust + draws overlay   │
└───────────────▲──────────────────────────────┬───────────────────────┘
        invoke(commands) / binary frames      │ pointer/wheel/key events
┌───────────────┴──────────────────────────────▼───────────────────────┐
│ Tauri command layer (apps/desktop/src/ipc.rs)                        │
│                                                                      │
│  EngineSession  (single-threaded owner, Mutex<Session>)             │
│   ├─ Document       (dp-model)  + History (undo/redo transactions)   │
│   ├─ Viewport, Selection, Tool state machine (Rust)                  │
│   ├─ LayoutEngine   (dp-layout) → resolved rects, cached by revision │
│   ├─ TextEngine     (dp-text)  → shaped runs, cached by style+text    │
│   ├─ Renderer       (dp-render) → tile cache → RGBA viewport frame   │
│   ├─ HitTester      (dp-geom spatial index + document tree)          │
│   └─ Persistence    (dp-persist) autosave, recent files, recovery     │
└──────────────────────────────────────────────────────────────────────┘
```

The React layer is deliberately thin: it owns chrome, panels, dialogs, settings and
shortcut dispatch, but **never** mutates document state directly. Every pointer/keyboard
interaction that changes geometry is sent to the Rust session, which runs the active tool's
state machine and applies validated transactions. The UI receives:

1. a binary RGBA viewport frame (`invoke('render_frame')` → `ArrayBuffer`), and
2. a JSON `EngineUpdate` (revision, viewport, selection, overlay, tool state, errors) after
   every command — used to draw selection handles, guides, marquee, cursors and to refresh
   panels.

### Backend

```
Internet ── TLS ──> Axum router ──> services (auth, teams, projects, files, versions,
                                   comments, shares, publish, storage, ws)
                      │
                      ├─ PostgreSQL (accounts, teams, projects, permissions, comments,
                      │              versions metadata, publish records, audit log)
                      ├─ S3-compatible object store (images, fonts, exports, site assets,
                      │                             document blobs & deltas)
                      └─ Redis (optional): presence, rate limits, ws fan-out
```

Real-time protocol: WebSocket at `/ws/doc/:doc_id`, binary-ish JSON messages; the server is
an op relay/sequencer with persistence (`dp-collab` ops), not an authoritative CRDT node —
clients converge using `dp-collab`. PostgreSQL stores the op journal and periodic
snapshots.

## 4. Document model in one page

See [DOCUMENT-FORMAT.md](DOCUMENT-FORMAT.md) for the exact schema. Key points:

* A `Document` owns `pages`. A page owns a **flat node map** (`NodeId → Node`) plus an
  ordered root list. Each node stores its ordered children ids. Flat maps make diffing,
  op application, undo and collaboration straightforward and avoid deep-tree cloning.
* Every node has a stable `NodeId` (UUID v4), a monotonically increasing `rev` for cache
  invalidation, and structured properties (`NodeProps`) grouped by concern: transform,
  geometry, fills/strokes/effects, text, layout, component, constraints.
* Geometry primitives are first-class (`ShapeGeometry`); free-form vectors store
  `VectorPath` with cubic Bézier anchors; boolean groups store an operator + child
  operands (real geometry, evaluated by `dp-geom`).
* Cascading style references resolve through `PaintStyleId`/`TextStyleId`/`EffectStyleId`
  and design variables (`VariableId` with modes/aliases), resolved at read time.
* Components are nodes (`Component`, `ComponentSet`) and instances reference the
  component id; overrides are explicit per-property patches, never destructive.

### Transactions and undo

All mutations go through semantic operations (`DocOp`) applied to a `Transaction`:

* `apply(op)` captures the minimal before/after state needed to invert and structure the
  op for collaboration (node snapshots for insert/remove, property patches for updates).
* Undo pops a transaction and applies the inverse records in reverse order; redo reapplies.
* Gestures (drag, resize, pen) run as one coalesced transaction opened at pointer-down and
  committed at pointer-up; cancelled gestures roll back.
* Ops are also the wire format for collaboration and for the autosave journal, so saving,
  undo and sync share one code path.

## 5. Rendering strategy

Non-goals for this design: flattening the document into a bitmap as the source of truth
(it is not — geometry stays editable), and rendering in JavaScript (it does not).

Pipeline: `Document + layout results → SceneBuilder (cull, resolve styles) → DisplayList
→ Renderer`.

* **DisplayList** is a backend-neutral, serializable description: layers with transforms,
  fills/strokes/effects, paths (kurbo), text runs (glyph ids + positions), image draws,
  clips, masks, blur groups, blend modes.
* **Tiles.** The viewport is rendered in 256 px device-space tiles, cached by
  `(page, tile index, zoom bucket, document revision)`. Panning composites cached tiles
  without re-rasterizing; zoom invalidates; editing invalidates only the tiles covered by
  the changed bounds. The current frame is a composited tile composite.
* **CPU rasterizer** (`tiny-skia`) is the reference implementation: deterministic output
  for exports, tests and thumbnails, identical rendering independent of GPU.
* **GPU backend** (`wgpu`, optional) accelerates tile rasterization and full-resolution
  export when available. It must be pixel-comparable with the CPU path (golden-image tests
  with tolerance), otherwise it stays opt-in.
* **Quality ladder during interaction:** while a drag is in flight, re-render dirty tiles
  only (optionally at half resolution for very large scenes) and promote to full quality on
  pointer-up. The quality scale is a viewport parameter, not a separate code path.
* **Text** is shaped by `dp-text`; glyph rasterization is cached in an atlas keyed by
  (font, size bucket, glyph id), shared across tiles.
* **Culling & indexing:** `dp-geom::spatial::SpatialGrid` indexes node bounds per page;
  scene building queries only visible candidates, then applies clip/mask containment.
  Blur/mask groups inflate bounds.
* **Frame transport:** the desktop engine returns a raw premultiplied RGBA8 frame
  (`tauri::ipc::Response`) at device pixel ratio; the UI blits it with
  `putImageData`/`createImageBitmap` and draws the interaction overlay (selection, handles,
  guides, marquee, collaborator cursors) in a separate canvas at the same size. PNG
  encoding is only for export/thumbnail requests, never per frame.

Performance budgets (initial targets, to be measured and reported in
[PERFORMANCE.md](PERFORMANCE.md)): idle scene 10k nodes → first frame < 250 ms; pan with
warm tiles < 8 ms CPU per frame; single node drag on 1k-node page < 16 ms/frame raster;
hit test p95 < 1 ms; document save+reopen round-trip < 1 s for 1 MB doc; undo of a 5k-node
operation < 50 ms. These are targets, not claims; benchmarks land in Phase 8.

## 6. Collaboration model (summary)

Full protocol in [COLLAB-PROTOCOL.md](COLLAB-PROTOCOL.md).

* Replicated tree: nodes are identified by stable ids; the children lists are RGA
  (replicated growable arrays) keyed by `(lamport, actor)` so concurrent inserts/reorders
  converge deterministically.
* Properties are per-field LWW registers stamped with a hybrid logical clock
  `(lamport, actor, wall_clock)`; concurrent writes to different fields of the same node
  both survive (no whole-node last-write-wins).
* Deletes are tombstones (retained for a configurable retention window, compacted by a
  leader snapshot); restoring a deleted node is a resurrection op.
* Presence, cursors, selection are ephemeral (not persisted), relayed over the same socket.
* Recovery: every client persists its unacknowledged op batch locally (SQLite journal) and
  replays after reconnect; server assigns a monotonic `server_seq` for ordering, and
  clients can request missing ranges (`sync_from(seq)`).

## 7. Security & reliability posture (summary)

* Passwords: Argon2id hashes; sessions are opaque 256-bit tokens, stored hashed
  (SHA-256) in Postgres, delivered via `HttpOnly`, `SameSite=Lax`, `Secure` cookies for
  web previews and via secure storage for desktop tokens.
* Authorization always server-side; every route resolves the actor from the session and
  checks object-level permissions (`view`/`comment`/`edit`/`admin`) through a single
  `authorize(user, project, capability)` gate.
* Uploads: content-type allow-list, size limits, per-user rate limits, images re-encoded in
  Rust (strips metadata), S3 keys are server-generated (never user paths).
* Documents/plugins/imports are untrusted: schema validation + limits (node count, string
  length, path point count), no script execution in the document model; plugin ops run
  through the same validated transaction pipeline with declared permissions.
* Logging: structured `tracing` with secret redaction; OTel export optional; audit events
  (login, share changes, permission changes, version restore) are persisted.

## 8. Known gaps / trade-offs (honest list)

* Real-time collaboration will ship incrementally: protocol + server relay + presence first,
  full CRDT client integration for the desktop editor after the single-user editor is solid.
  Until integrated, the editor is single-user with autosave.
* Boolean operations flatten curves to polylines before exact boolean and emit polygonal
  results (curves within tolerance are lost). Documented in `dp-geom::boolean` and tested;
  exact curve-boolean is a future upgrade. The CPU renderer now evaluates boolean children
  into one path for display while the document keeps the editable operands.
* Drop shadows are rasterized as an offset, triple-box-blurred copy of the shape outline
  (spread expanded with a stroke) — an approximation of the vector shadow, not an exact
  gaussian. Inner shadows, layer blur, background blur and text shadows are not rendered yet;
  the scene builder emits explicit warnings for them instead of silently drawing nothing.
* wgpu backend is behind a feature flag and must prove pixel parity with the CPU path
  before it becomes the default; measurements will decide.
* Plugins use a declaration-validated, op-scoped sandbox in Rust rather than arbitrary
  JS/WASM execution in the first iterations; a WASM runtime can replace the op runner
  without changing the API surface. Documented per release.
* PDF export uses `svg2pdf` over the SVG exporter (vector text is outlined or embedded via
  subset fonts; exact font embedding is a tracked limitation).
* PowerPoint import/export implements a documented subset (no SmartArt, no animations
  beyond slide transitions) — see docs/POWERPOINT.md when implemented.
* Font matching depends on system fonts + bundled fonts; missing fonts render with a
  documented fallback and a warning list (never silently substituted without notice).
* Publishing requires configured infrastructure (Postgres + S3 + a site host); the desktop
  app never pretends a local preview is a public URL.
