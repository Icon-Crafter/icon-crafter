# Feature checklist

Status legend: **planned** · **in-progress** · **implemented** (code exists, builds, unit
tested) · **verified** (verified end-to-end in the running app) · **blocked** (with reason)
· **limited** (implemented subset with documented limitation).

Rule: a feature is never marked implemented/verified because a button exists. Each row
names the verification evidence in the development log or a test.

Last updated: session 3 (engine vertical slice complete and green; desktop IPC compiles; backend delivered and unit-tested; frontend milestone 2 in flight).

## Foundation

| # | Feature | Status | Evidence / notes |
|---|---|---|---|
| F1 | Rust workspace (15 members incl. dp-editor), pinned toolchain, shared profiles | verified | `cargo metadata --no-deps`, commits a1765cb/1a6725a/8013f97 |
| F2 | Tauri desktop shell + config + capabilities + icons | implemented | `cargo check -p iconcrafter-desktop` passes with the full IPC layer; app launch verification pending the real UI bundle |
| F3 | Axum backend: auth/teams/projects/files/comments/versions/shares/assets/publish/WS | limited | 132 unit tests re-run green after the clippy refactor (`cargo test -p iconcrafter-server --lib`); `--features s3`/`--features otel` check clean; PostgreSQL integration tests not run (Docker missing) |
| F4 | Docker compose for Postgres/MinIO/Redis | limited | `docker-compose.yml` + Dockerfile written and reviewed; never executed because Docker is not installed on the dev host |
| F5 | Architecture, document format, collaboration protocol docs | implemented | `docs/ARCHITECTURE.md`, `DOCUMENT-FORMAT.md`, `COLLAB-PROTOCOL.md` |
| F6 | Contract + design system docs | implemented | `docs/CONTRACT.md` (§8 implementation notes), `DESIGN-SYSTEM.md` |
| F7 | CI-ready commands documented (fmt/clippy/test/build) | implemented | `docs/TESTING.md`, root README |
| F8 | Feature checklist / dev log / release checklist | implemented | this file + `DEVELOPMENT-LOG.md`, `RELEASE-CHECKLIST.md` |
| F9 | `dp-geom`: transforms, Bezier paths, primitives, booleans, spatial index, snapping | verified | `cargo test -p dp-geom` → 44 passed (2026-10-23) |
| F10 | `dp-model`: document model, styles, variables, components, prototype model, transactions/undo, command ops, validation, canonical schema-v1 JSON | verified | `cargo test -p dp-model` → 65 passed (2026-10-23) |
| F11 | `dp-text`: system font catalog + missing-font detection, cosmic-text shaping, wrapping/max-lines, glyph runs and rasterization, layout cache | verified | `cargo test -p dp-text` → 13 passed (2026-10-23) |
| F12 | `dp-layout`: deterministic auto layout (hug/fill/fixed, gap/padding, alignment, wrap, item-reverse, min/max, nested, text-driven hug) | verified | `cargo test -p dp-layout` → 12 passed (2026-10-23) |
| F13 | `dp-render`: display list + scene builder + culling + tiny-skia CPU rasterizer + PNG encoding (wgpu declared, not implemented) | verified | `cargo test -p dp-render` → 6 + 11 passed (2026-10-23); wgpu backend honestly absent |
| F14 | `dp-io`: SVG export/import (editable vectors), PNG/JPEG raster export with scale, PDF export | verified | `cargo test -p dp-io` → 9 passed (2026-10-23) |
| F15 | `dp-persist`: `.icraft` container, atomic save, recovery journal, schema validation | verified | `cargo test -p dp-persist` → 13 passed (2026-10-23) |
| F16 | `dp-editor`: state machine, tools/selection/drags, snapping, commands, text sessions, undo, render/overlay, save/open/export | verified | `cargo test -p dp-editor` → 19 passed (2026-10-23) |
| F17 | Desktop IPC layer (Tauri commands = contract) | implemented | `cargo check -p iconcrafter-desktop` passes; end-to-end launch pending UI milestone 2 |

## Design editor — Phase 3 vertical slice

| # | Feature | Status | Evidence / notes |
|---|---|---|---|
| E1 | Infinite canvas: zoom/pan (wheel, trackpad, space+drag, middle mouse) | implemented | engine viewport + input + IPC; `wheel_pans_and_ctrl_wheel_zooms` test; UI wiring in milestone 2 |
| E2 | Zoom to selection / fit / 100% / selection info | implemented | engine commands `zoom_fit`/`zoom_selection`/`zoom_100`; UI buttons |
| E3 | Frames, groups, rectangles, ellipses, lines, polygons, stars | implemented | create-drag in `input.rs`; scene builder + layout; editor tests |
| E4 | Text creation + on-canvas editing | in-progress | engine text nodes + edit sessions + undo verified; DOM textarea overlay in frontend milestone 2 |
| E5 | Vector paths + pen tool | limited | vector geometry model/render/SVG import done; interactive pen tool not implemented |
| E6 | Selection: click, shift-click, marquee, deep select, lock/hide respect | implemented | editor tests `click_selects_node_under_cursor`, `marquee_selects_intersecting_nodes` |
| E7 | Move, resize with handles, rotate, nudge, snap to objects/grid/guides | implemented | move/nudge/snap tested; handle drag math in `input.rs`; guides UI pending |
| E8 | Copy/paste/duplicate/cut/clipboard | implemented | `duplicate_nodes` + clipboard commands; editor tests |
| E9 | Group/ungroup, reorder, z-order commands, lock/hide | implemented | ops layer + command table; editor tests |
| E10 | Layers panel with tree, rename, drag reorder, search | in-progress | engine summary + rename/visibility/lock commands; UI panel milestone 2 |
| E11 | Pages management | implemented | `add_page`/`rename_page`/`delete_page`/`set_active_page` + editor test |
| E12 | Property inspector: transform, size, rotation, opacity, blend, fills, strokes, effects, corner radii | in-progress | engine `apply_patch`/opacity/text props implemented; UI inspector milestone 2 |
| E13 | Undo/redo with coalescing | verified | `History` coalescing + editor tests (drag undo, delete undo, text commit undo) |
| E14 | Rulers, guides, grid, snap lines | limited | snapping + snap-line overlay in engine; rulers/guides UI pending |
| E15 | Save/reopen native `.icraft` + JSON, autosave journal, crash recovery | implemented | `save_and_reopen_roundtrip` test; recovery journal in dp-persist; recovery prompt UI pending |
| E16 | PNG/SVG export with scale/transparent/selection options | implemented | `png_and_svg_export_produce_files` test; PDF also; transparent flag documented as not yet honoured |
| E17 | Command palette + customizable shortcuts + context menus + tooltips | in-progress | shell exists (apps/ui); build fix + real wiring in milestone 2 |
| E18 | Selection toolbar (align, distribute, boolean, opacity, group) | in-progress | engine commands implemented + tested; UI toolbar milestone 2 |
| E19 | Responsive to large documents (tile cache, culling, benchmarked) | limited | viewport culling + layout/text caches implemented; tile cache and benchmarks not yet |

## Vector editing (Phase 4)

| # | Feature | Status | Evidence / notes |
|---|---|---|---|
| V1 | Bézier pen tool with handle editing, add/remove/convert anchors | planned | |
| V2 | Multiple contours, open/closed, join/split | planned | |
| V3 | Boolean union/subtract/intersect/exclude (curves flattened → polygonal, documented) | planned | |
| V4 | Stroke alignment/cap/join/dash controls | planned | |
| V5 | Shape builder equivalent | planned | |
| V6 | Text on path | planned | |
| V7 | SVG import/export round-trip of editable geometry | planned | |
| V8 | Custom brushes / illustration workspace | planned | |

## Typography (Phase 4)

| # | Feature | Status | Evidence / notes |
|---|---|---|---|
| T1 | Font discovery/indexing, font loading, fallback + missing-font warnings | planned | |
| T2 | Family/size/weight/style/line-height/letter-spacing controls | planned | |
| T3 | Alignment, paragraph spacing, list controls, decoration, case | planned | |
| T4 | Auto/fixed text box + overflow handling | planned | |
| T5 | Shared text styles | planned | |
| T6 | Reliable shaping/measurement (bidi, fallback), cached | planned | |

## Auto layout (Phase 4)

| # | Feature | Status | Evidence / notes |
|---|---|---|---|
| L1 | H/V layout, gap, padding, alignment | planned | |
| L2 | Hug/fill/fixed, min/max, wrapping | planned | |
| L3 | Nested layouts, layout-aware instances | planned | |
| L4 | Constraints/anchors for non-auto-layout children | planned | |
| L5 | Deterministic engine + unit tests for nested/complex cases | planned | |
| L6 | Visual layout debugging | planned | |

## Components & design systems (Phase 4)

| # | Feature | Status | Evidence / notes |
|---|---|---|---|
| C1 | Create component/component set, instances, nested components | planned | |
| C2 | Overrides (property/text/instance swap), detach, reset | planned | |
| C3 | Variants + component properties | planned | |
| C4 | Color/text/effect styles | planned | |
| C5 | Variables, collections, modes, aliases, light/dark themes | planned | |
| C6 | Libraries: publish/consume across files, update + conflict reports | planned | |
| C7 | Broken-reference detection, usage reporting | planned | |

## Prototyping (Phase 5)

| # | Feature | Status | Evidence / notes |
|---|---|---|---|
| P1 | Interaction graph (frames/components), triggers click/hover/press/drag | planned | |
| P2 | Actions: navigate, overlay open/close, scroll to, back | planned | |
| P3 | Transitions: instant/dissolve/slide/smart animate, duration/easing/spring | planned | |
| P4 | Variables + conditional logic, expressions, multiple actions | planned | |
| P5 | Device/viewport presets, responsive preview, presentation mode, keyboard | planned | |
| P6 | Shareable preview link (backend) | planned | |

## Dev Mode (Phase 5)

| # | Feature | Status | Evidence / notes |
|---|---|---|---|
| D1 | Inspect dimensions/spacing/constraints/colors/type/effects/components/variables | planned | |
| D2 | Deterministic CSS / React+TS / SwiftUI / Compose generation | planned | |
| D3 | Asset export panel + copy measurements/styles | planned | |
| D4 | Annotations, ready-for-dev status, links to code/docs, revision compare | planned | |

## Collaboration & backend (Phase 6)

| # | Feature | Status | Evidence / notes |
|---|---|---|---|
| B1 | Accounts (register/login/logout), Argon2id, session tokens, secure cookies | planned | |
| B2 | Teams/projects/files/folders, roles, invites, permissions (server enforced) | planned | |
| B3 | Autosave + shares + link access controls + password-protected links | planned | |
| B4 | Comments anchored to objects, threads, resolve, activity history | planned | |
| B5 | WebSocket real-time sync (dp-collab), presence, cursors, reconnect replay | planned | |
| B6 | Version snapshots + deltas, named versions, restore, compare, branches/merge | planned | |
| B7 | S3-compatible storage (uploads, exports, assets) + local dev provider | planned | |
| B8 | Rate limiting, audit log, migrations, backups guidance | planned | |
| B9 | Shareable file/prototype links + public viewer page | planned | |

## Additional workspaces (Phase 7)

| # | Feature | Status | Evidence / notes |
|---|---|---|---|
| W1 | Whiteboard: sticky notes, shapes, connectors, freehand, tables, mind maps, voting, timer, templates, reactions | planned | |
| W2 | Slides: creation/reorder, templates/themes, notes, transitions, present, PDF export | planned | |
| W3 | Drawing/illustration: vector brushes, pressure, layers, texture/pattern fills, SVG/PNG/PDF export | planned | |
| W4 | Website design: breakpoints, sections, content collections, responsive preview, HTML/CSS export | planned | |
| W5 | Website publishing backend: actual deploy workflow, custom-domain config | planned | |

## Platform features (Phase 7)

| # | Feature | Status | Evidence / notes |
|---|---|---|---|
| X1 | AI subsystem: provider abstraction, validated op proposals, confirmation/undo, optional | planned | |
| X2 | Plugin architecture: manifests, permissions, sandboxed ops, commands/panels, lifecycle, docs + examples | planned | |
| X3 | Integrations: GitHub/issues/design APIs/AI tool hooks | planned | |
| X4 | Import/export: SVG, PNG, JPEG, PDF, HTML/CSS, image import, clipboard | planned | |
| X5 | PowerPoint subset import/export (documented) | planned | |

## Quality & performance (Phase 8)

| # | Feature | Status | Evidence / notes |
|---|---|---|---|
| Q1 | Unit + integration tests across crates and server | planned | |
| Q2 | Benchmarks for large documents (10k nodes), reported numbers | planned | |
| Q3 | Dependency audit, secret scan, security review | planned | |
| Q4 | Installers (Windows MSI/NSIS), upgrade path, reproducibility | planned | |
| Q5 | Accessibility pass (keyboard, focus, labels) | planned | |
| Q6 | Offline/no-network operation for core editor | planned | |
