# Feature checklist

Status legend: **planned** · **in-progress** · **implemented** (code exists, builds, unit
tested) · **verified** (verified end-to-end in the running app) · **blocked** (with reason)
· **limited** (implemented subset with documented limitation).

Rule: a feature is never marked implemented/verified because a button exists. Each row
names the verification evidence in the development log or a test.

Last updated: session 7 (design-system engine authoring: components, instances, shared styles, variables/modes; instance rendering; 253 engine tests green).

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
| F17 | Desktop IPC layer (Tauri commands = contract) | verified | `cargo check -p iconcrafter-desktop` passes; real window launch: WebView2 loaded the real UI (`TAURI IPC` badge), `engine_state`/`pointer`/`command` round-trips, rectangle create (rev 2) + undo (rev 3) verified live (2026-10-24) |
| F18 | `dp-prototype`: deterministic runtime (interaction graph, triggers, actions, transitions, timers, variables, expression language, flow validation) | verified | `cargo test -p dp-prototype` → 18 passed (2026-10-24); preview UI and shareable links pending |
| F19 | `dp-collab`: HLC/Lamport clocks + actor ids, RGA sibling ordering with tombstones, per-field LWW registers, delete/insert resurrection, page + document-name registers, validated op envelopes, presence/conflict payloads, converged document materialization | verified (engine) | `cargo test -p dp-collab` → 19 passed (2026-10-24); desktop↔server transport and live multi-client wiring pending |
| F20 | `dp-devmode`: inspection snapshots (bounds/matrix/styles/typography/effects/component/variable refs), deterministic CSS + React/TS + SwiftUI + Compose generation with explicit manual-adaptation warnings, annotation/handoff tracking, structural revision diff | verified (engine) | `cargo test -p dp-devmode` → 14 passed (2026-10-24); strict clippy clean; Dev Mode IPC commands `dev_inspect`/`dev_codegen`/`dev_handoff`/`dev_add_annotation`/`dev_mark_ready`/`dev_compare` compile in the desktop crate; panel UI pending |

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
| E12 | Property inspector: transform, size, rotation, opacity, blend, fills, strokes, effects, corner radii | in-progress | engine `apply_patch`/opacity/text props implemented; UI inspector milestone 2; inspector effects fields pending |
| E13 | Undo/redo with coalescing | verified | `History` coalescing + editor tests (drag undo, delete undo, text commit undo) |
| E14 | Rulers, guides, grid, snap lines | limited | snapping + snap-line overlay in engine; rulers/guides UI pending |
| E15 | Save/reopen native `.icraft` + JSON, autosave journal, crash recovery | implemented | `save_and_reopen_roundtrip` test; recovery journal in dp-persist; recovery prompt UI pending |
| E16 | PNG/SVG export with scale/transparent/selection options | implemented | `png_and_svg_export_produce_files` test; transparent PNG + selection scope honoured by the scoped exporters; PDF also |
| E17 | Command palette + customizable shortcuts + context menus + tooltips | in-progress | shell exists (apps/ui); build fix + real wiring in milestone 2 |
| E18 | Selection toolbar (align, distribute, boolean, opacity, group) | in-progress | engine commands implemented + tested; UI toolbar milestone 2 |
| E19 | Responsive to large documents (tile cache, culling, benchmarked) | limited | viewport culling + layout/text caches implemented; tile cache and benchmarks not yet |
| E20 | Booleans rendered as evaluated geometry; drop shadows rasterized | implemented | renderer evaluates boolean children (flattened, polygonal) and rasterizes offset blurred shape shadows; tests `boolean_subtract_renders_evaluated_geometry`, `boolean_union_merges_operand_outlines`, `drop_shadows_are_rasterized_behind_shapes`; inner shadows / layer / background blur still warned |

## Vector editing (Phase 4)

| # | Feature | Status | Evidence / notes |
|---|---|---|---|
| V1 | Bézier pen tool with handle editing, add/remove/convert anchors | planned | |
| V2 | Multiple contours, open/closed, join/split | planned | |
| V3 | Boolean union/subtract/intersect/exclude (curves flattened → polygonal, documented) | implemented | `dp-geom::boolean` + editor `boolean` command + renderer evaluates operand children into one path (`boolean_subtract_renders_evaluated_geometry`, `boolean_union_merges_operand_outlines`); operands stay editable children; UI booleans via selection toolbar (E18) |
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
| C1 | Create component, place instances, nested components | implemented | engine+editor: `create_component` (same-parent selection, master sized to world bounds), `place_instance`, instance rendering expands the master subtree on canvas; component sets, variants and swaps still explicit `unsupported` |
| C2 | Overrides (property/text/instance swap), detach, reset | in-progress | `detach_instance` works (instance → frame); overrides are not applied yet — instances render the master verbatim; reset planned with overrides |
| C3 | Variants + component properties | planned | |
| C4 | Color/text/effect styles | implemented | `create_style_from_node`, `apply_style`, `rename_style`, `delete_style`; nodes carry `style_id` references, deletion surfaces broken references through validation instead of silently cleaning up |
| C5 | Variables, collections, modes, aliases, light/dark themes | implemented | `create_variable_collection`, `create_color_variable`, `create_float_variable`, `set_variable_value`, `set_active_mode`; engine resolves per-mode + aliases (32-deep guard); string/boolean/alias authoring UI pending |
| C6 | Libraries: publish/consume across files, update + conflict reports | planned | |
| C7 | Broken-reference detection, usage reporting | in-progress | validation reports broken style/variable/component references; `design_system_counts` exposes counts; per-node usage report UI planned |

## Prototyping (Phase 5)

| # | Feature | Status | Evidence / notes |
|---|---|---|---|
| P1 | Interaction graph (frames/components), triggers click/hover/press/drag | in progress | engine triggers click/hover/press/key/after-delay + runtime firing tested (dp-prototype); drag trigger + authoring UI not built |
| P2 | Actions: navigate, overlay open/close, scroll to, back | implemented | runtime executes all six actions; tested incl overlay stack and back navigation; preview UI pending |
| P3 | Transitions: instant/dissolve/slide/smart animate, duration/easing/spring | implemented | transition progress, spring easing and tick-driven completion tested; smart-animate pair matching (UI side) pending |
| P4 | Variables + conditional logic, expressions, multiple actions | implemented | runtime variables, alias resolution, arithmetic/logic expression parser (8 expr tests) and multi-action chains tested |
| P5 | Device/viewport presets, responsive preview, presentation mode, keyboard | in progress | device presets + keyboard triggers in engine; presentation mode UI pending |
| P6 | Shareable preview link (backend) | planned | requires backend share links wired to prototype viewer |

## Dev Mode (Phase 5)

| # | Feature | Status | Evidence / notes |
|---|---|---|---|
| D1 | Inspect dimensions/spacing/constraints/colors/type/effects/components/variables | implemented (engine) | `dp-devmode::inspect` resolves layout, variables, styles, typography, component refs; 14 tests; UI panel pending |
| D2 | Deterministic CSS / React+TS / SwiftUI / Compose generation | implemented (engine) | `dp-devmode::codegen`, deterministic-output test, warnings + `requires_manual_adaptation`; SwiftUI/Compose always flagged manual |
| D3 | Asset export panel + copy measurements/styles | in progress | scoped PNG/SVG/PDF export dialogs work end-to-end (engine + UI); copy-measurements affordances in the UI still pending |
| D4 | Annotations, ready-for-dev status, links to code/docs, revision compare | implemented (engine) | `dp-devmode::workbench` (annotations/resolve/link/readiness/structural diff) tested; UI panel pending |

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
