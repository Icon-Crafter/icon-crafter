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

## Session 4 (2026-10-24)

### Disk incident and build hygiene
- A full engine test build filled the C: drive (8.77 GB `target`, ~10 MB free, rustc
  `os error 112`). Freed it with `cargo clean`, removed leftover `target` (311 MB) and cleaned
  the user temp directory reported by the backend teammate (33 GB); free space recovered to
  44 GB.
- Root workspace profile changed to `opt-level = 1`, `debug = 0`, `incremental = false` so
  debug/test builds stay small on this machine. This only affects development artifacts, not
  release output.
- `.gitattributes` added (`* text=auto eol=lf`, binary asset patterns) to stop CRLF churn.

### Backend committed
- `apps/backend` committed as `a0cda62` after re-running its tests: `cargo test -p
  iconcrafter-server --offline --lib` → 132 passed / 0 failed (5 PostgreSQL integration tests
  remain `#[ignore]` because Docker is not installed on this host). Docker compose, Dockerfile,
  README and `.env.example` ship with it.

### dp-prototype: runtime engine
- New `crates/dp-prototype` (was a stub) now contains the deterministic prototype runtime used
  by both the editor preview and (future) shared prototype links:
  - `expr.rs`: pure expression language (numbers/bools/strings, variables by name, arithmetic,
    comparisons, short-circuit logic, parentheses; 64-deep safety budget) with parsing and
    evaluation errors.
  - `runtime.rs`: `PrototypeRuntime` with frame stack/history, overlay stack (position,
    backdrop, open-progress), transitions (instant/dissolve/slide/smart, duration/easing/
    spring), delayed and after-delay triggers, multiple actions per interaction, runtime
    variables resolved from the document (aliases included), and `validate_flow` which checks
    flow/action targets before playback.
  - `PrototypeRuntime::start`/`start_flow`, `trigger`/`click`/`press_key`, `tick` (drives
    transitions + timers), `back`, `close_overlay`, `set_variable`, `evaluate*`, `drain_events`
    returning `RuntimeEvent`s the UI turns into pixels.
- Tests: `cargo test -p dp-prototype` → 18 passed, 0 failed.

### Lint gate
- `cargo clippy --fix` applied ~60 machine fixes across the engine crates and the remaining
  warnings were resolved manually (intentional `Mat2D::mul` inherent method, intentionally
  large `Edit` enum, rasterizer checked division, dead code in page integrity check / SVG
  importer / parser). The engine now passes:
  `cargo clippy -p dp-geom -p dp-model -p dp-text -p dp-layout -p dp-render -p dp-io -p
  dp-persist -p dp-editor -p dp-prototype --all-targets -- -D warnings`.
- Engine suite re-run after the lint sweep: 44 + 65 + 13 + 12 + 6/11 + 9 + 13 + 19 = 192 tests
  green, plus `dp-prototype` 18 → 210 engine tests total.

### Next
- Launch the Tauri app against the real UI bundle, run the manual checklist (create/move/edit
  shapes and text, layers, inspector, zoom/pan, undo, save/reopen, PNG/SVG export), record
  evidence and flip F17/E-rows to verified.
- Wire `dp-prototype` playback into the editor IPC (`prototype_play/stop`) and the preview UI.
- Continue with components/styles/variables authoring UI and collaboration transport.

## Session 5 (2026-10-24) — collaboration core

- Real end-to-end run of the Tauri app against the Vite dev server (WebView2 at
  `localhost:1420`, remote debugging on 9222). Verified the shell renders, `bootstrap` /
  `engine_state` / `pointer` / `command` round-trip, and that drawing a rectangle through real
  IPC produces engine revision 2 with undo to revision 3.
- Engine fixes from live use: new shapes get visible default fills (`#D9D9D9`; frames white,
  lines black stroke), click-with-shape-tool creates a 100×100 layer instead of a 1×1, the
  workspace backdrop is `#101318` instead of the white page background, text nodes get a
  `#1F2430` fill, and the node being text-edited is hidden from the display list so the DOM
  textarea is the only visible text.
- IPC contract fixes: `render_frame.quality` accepts the numeric tier, `set_preference` returns
  the updated preferences record, exports honour `node_ids` + `transparent`, SVG import honours
  `page_id`/`parent_id`, and `bootstrap.capabilities` advertises real feature flags
  (`export_scope`, `import_svg`, `canvas_text_edit`, `prototype`, …) so the UI can disable what
  the engine cannot do.
- Frontend milestone 2 landed alongside the engine fixes: capability-gated menus/dialogs,
  scoped export dialog, SVG import targeting, 85 unit tests, strict `tsc` + Vite build, and
  live-loop verification in the Tauri window.
- `dp-collab` implemented (previously a stub): `clock.rs` (actor ids + hybrid logical clocks),
  `rga.rs` (replicated growable arrays with tombstones and deterministic concurrent inserts),
  `op.rs` (per-field LWW property ops, insert/delete/reparent/page/name ops, validated
  envelopes, presence and conflict payloads), `replica.rs` (seed → apply → materialize a
  converged `Document`). `cargo test -p dp-collab` → 19 passed; strict clippy pending.
- Next: wire `dp-collab` into the desktop session and the Axum WebSocket relay (server already
  has an op journal keyed by `(file_id, batch_id)`), then component/style/variable authoring UI.

## Session 6 (2026-10-24) — Dev Mode engine + IPC

- `dp-devmode` implemented from stub: `inspect.rs` (selection inspection resolving layout,
  style/variable paints, typography, effects, component/variable refs), `codegen.rs`
  (deterministic CSS / React+TS / SwiftUI / Compose with honest manual-adaptation warnings),
  `workbench.rs` (annotations, resolve/link, readiness status, structural revision diff).
  `cargo test -p dp-devmode` → 14 passed; strict clippy clean.
- Dev Mode wired through the editor and IPC: `EditorState` gained `dev_mode` + `handoff`
  state, `inspect_selection`, `generate_code`, `compare_revision`, `dev_add_annotation`,
  `dev_mark_ready`; new Tauri commands `set_dev_mode`, `dev_inspect`, `dev_codegen`,
  `dev_handoff`, `dev_add_annotation`, `dev_mark_ready`, `dev_compare`; contract documented
  in `docs/CONTRACT.md` §9. `cargo check -p iconcrafter-desktop` exits 0.
- Known limitations written down rather than hidden: generated code is a visual
  approximation; SwiftUI/Compose always require adaptation; auto layout is flattened to
  offsets; vectors/images/booleans need asset export; handoff data is session-local until
  collaboration sync lands; Dev Mode panel UI is the next frontend milestone.
- Next: Dev Mode panel UI (inspect/codegen/handoff tabs), then collaboration transport
  wiring (desktop ↔ Axum WS relay), component/style/variable authoring UI.

## Session 7 (2026-10-24) — Design systems in the engine

- `dp-model` gained undoable document-level registry edits (`Edit::SetStyles` /
  `SetComponents` / `SetVariables` / `SetSettings`) and a new `design_system` module:
  `create_component` (same-parent selection, master sized to world bounds, world positions
  preserved through inverse matrices), `detach_instance`, `create_instance`, shared
  paint/text/effect styles created from nodes or values and applied by id, variable
  collections plus colour/float variables, per-mode values and active-mode switching.
  `cargo test -p dp-model` → 72 passed.
- `dp-render` now expands component instances on canvas: the master subtree is rendered at
  the instance transform with clip/opacity/culling preserved. Masters on other pages and
  instance overrides are explicit limitations (warned, never silently wrong).
  `cargo test -p dp-render` → 17 passed (6 unit + 11 integration).
- `dp-editor` dispatch now implements `create_component`, `place_instance`,
  `detach_instance`, style create/apply/rename/delete, variable collection/colour/float
  creation, `set_variable_value`, `set_active_mode` and `dev_mode_toggle`; snapshot
  components expose the registry `component_id` for placement.
  `cargo test -p dp-editor` → 22 passed.
- Full engine suite: **253 passed** (44+72+13+12+17+9+13+22+19+18+14); strict clippy clean
  for the touched crates. Contract addendum in `docs/CONTRACT.md` §10.
- Known limitations (documented, not hidden): component sets/variants/instance overrides and
  swap are not implemented (`unsupported`); libraries/shared-library sync planned; masters
  must be on the same page to render; style deletion leaves broken references that validation
  reports; variable aliases work in the engine but have no authoring UI yet.
- Next: frontend milestone 4 (design-system panels: components/library, style list + apply,
  variables collections/modes) against these commands; then collaboration transport wiring
  (desktop ↔ Axum WS relay) and prototype UI.

## Session 8 (2026-10-24) — Renderer fidelity: evaluated booleans + drop shadows

- `dp-render` evaluates boolean nodes into real geometry: visible children are outlined,
  transformed into the boolean node's local space, folded with the node's `BooleanOp` through
  `dp_geom::boolean_op`, and drawn as one path; operand children stay editable in the document
  and are not drawn twice. Unsupported operands (images/text/etc.) fall back to drawing the
  operands with a warning. `cargo test -p dp-render` → 20 passed (6 unit + 14 integration,
  new `boolean_subtract_renders_evaluated_geometry`, `boolean_union_merges_operand_outlines`).
- Drop shadows are rasterized: the shape outline is filled with the shadow colour, offset by
  the effect offset, optionally expanded with a stroke for spread, and blurred with a
  separable triple box blur on premultiplied RGBA before being composited with the shadow
  blend mode. New test `drop_shadows_are_rasterized_behind_shapes`. Remaining effect
  limitations are explicit warnings: inner shadows, layer blur, background blur, text shadows.
- Docs updated: FEATURE-CHECKLIST E12/E16/E20 + V3; ARCHITECTURE §8 trade-offs; TESTING
  session 8 commands (expected 256 engine tests).
- Next: design-system panels UI (frontend milestone 4 in flight), then collaboration transport.

## Session 9 (2026-10-24) — Prototype presentation in the editor

- `dp-editor` gained prototype presentation: `playback.rs` stores the session as a replayable
  action log (`Click(node)`, `Back`, `CloseOverlay`); `prototype_play` starts a
  `dp_prototype::PrototypeRuntime`, fits the viewport to the frame, clears selection and hides
  editing chrome; `prototype_back` / `prototype_stop` navigate; every interaction rebuilds the
  runtime from the document and replays the log (deterministic, undo-independent), ticking 10 s
  to finish transitions. Pointer input while presenting clicks the topmost interactive node at
  the cursor (bounds hit test); Escape stops, Backspace/ArrowLeft go back; wheel zoom remains.
- `render` switches to the prototype background and strips selection/handles/marquee/snap
  lines/text-edit/tool-hint from the overlay while presenting (presentation shows the prototype,
  not the editor chrome). Snapshot gained an additive `prototype` field.
- Tauri IPC: `prototype_play{frame_id?}`, `prototype_stop`, `prototype_back`; handler registered.
  `cargo test -p dp-editor` → 23 passed (new `prototype_presentation_navigates_and_goes_back`).
- Known limitations (in docs/CONTRACT.md §11): neighbouring artboards are not clipped away in
  presentation yet; after-delay triggers need the preview animation loop; expression actions and
  keyboard triggers have no presentation UI/routing yet; hotspot hit testing uses world AABBs.
- Next: frontend milestone 4 (design-system panels) + presentation overlay UI; then collaboration
  transport wiring (desktop ↔ Axum WS relay).





