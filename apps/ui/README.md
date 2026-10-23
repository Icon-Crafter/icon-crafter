# Icon Crafter — UI

React 19 + TypeScript (strict) + Vite presentation layer for the Icon Crafter design
platform. The UI is deliberately thin: every document mutation goes to the Rust engine
through the IPC contract in [`docs/CONTRACT.md`](../../docs/CONTRACT.md). The UI never
mutates geometry in JavaScript.

## Requirements

- Node.js 20+ (tested on 25.x)
- pnpm 10 (`corepack enable` if you do not have it)

## Commands

From `apps/ui`:

| Command | Description |
| --- | --- |
| `pnpm install` | Install dependencies (run once at the repo root too). |
| `pnpm dev` | Browser development harness on <http://localhost:1420>. No Rust required. |
| `pnpm build` | Type-check (`tsc --noEmit`) and produce a production bundle in `dist/`. |
| `pnpm preview` | Serve the built bundle locally. |
| `pnpm typecheck` | Strict TypeScript check only. |
| `pnpm test` | Vitest unit suite (`vitest run`). |
| `pnpm test:watch` | Vitest in watch mode. |

From the repository root (`apps/desktop` is the Tauri shell):

```powershell
pnpm tauri dev    # launches the native window; Vite dev server is started from here
pnpm tauri build  # packaged desktop app
```

`apps/desktop/src-tauri/tauri.conf.json` already expects `devUrl = http://localhost:1420`
and `frontendDist = ../ui/dist`.

## Architecture

```
src/
  engine/
    contract.ts      TypeScript mirror of docs/CONTRACT.md (snake_case IPC payloads)
    transport.ts     EngineTransport interface + TauriTransport (invoke + binary frames)
    devTransport.ts  DEV-ONLY in-memory engine used by the browser harness
    input.ts         EnginePump: serializes commands, coalesces pointer/wheel moves
    store.ts         zustand app store (snapshot, connection, toasts, dialogs, shortcuts)
    uiStore.ts       transient UI state (context menus, rename targets)
    commands.ts      every user action: run/tool/save/open/export/dialog/shortcut handlers
    shortcuts.ts     portable binding map, resolver, persistence, conflict detection
    palette.ts       command palette catalogue + fuzzy filtering
    overlayGeometry.ts  pure screen-space helpers for the overlay canvas and widgets
    prototype.ts     pure prototype authoring/presentation helpers (triggers, actions, hotspots)
    designSystem.ts  pure style/variable helpers (style kind detection, value parsing/formatting)
    devMode.ts       Dev Mode pure model: revision captures, measurement text, diff labels
    devModeStore.ts  Dev Mode panel state (inspect/code/handoff/captures) via dev_* IPC
  components/
    Toolbar, LeftPanel, RightSidebar (Inspector or Dev panel), CanvasStage, StatusBar, App shell
    Presentation.tsx full-window prototype player (frame canvas + hotspot overlay)
    panels/          Layers, Pages, Assets (styles), Components, Variables
    inspector/       Transform/Layout/Fill/Stroke/Effects/Typography/Component/Styles/Prototype/Export
    dev/             Dev Mode panel: Inspect, Code, Annotate, Compare tabs
    primitives/      IconButton, Button, menus, number/color/segmented fields, Dialog
    CommandPalette, ContextMenus, Dialogs, Toasts, ConfirmDialog
  icons/             lucide-react re-exports + editor-specific custom SVG glyphs
  i18n/              centralized English strings + label helpers
  styles/            tokens.css (design system variables) + app.css + dev.css + panels.css
  utils/             color, number, fuzzy-match, layer-tree, file-save utilities
```

### Data flow

1. `connect()` creates the transport (`TauriTransport` inside Tauri, dev harness in a
   browser) and reads `bootstrap` + `engine_state`.
2. The canvas calls `render_frame` (raw RGBA8 `ArrayBuffer`, `viewport.width * dpr` ×
   `viewport.height * dpr`) and blits it with `putImageData` into a backing canvas that is
   scaled to CSS pixels. A second canvas draws the JSON overlay (selection quads, resize
   handles and cursors, marquee, snap lines, guides, peer cursors, text caret).
3. Pointer/wheel/keyboard input is sent as flat `pointer` / `wheel` / `key` payloads.
   Commands are sent through `command {name, args}`. Every response is a new
   `EngineState`; the store ingests it and the canvas requests a frame only when
   `frame_id` changed.
4. Command failures returned in `state.errors` become toasts. Unsupported features are
   reported honestly as "Not available yet" — never silently ignored.
5. Text editing is delegated to the engine (`text_edit_begin/input/set_caret/commit/
   cancel`) with a transparent DOM textarea positioned from `engine.text_edit`.
6. Shortcuts are resolved by `engine/shortcuts.ts` (customizable, persisted in
   `localStorage` under `iconcrafter.shortcuts.v1`), with focus management so typing in
   fields never triggers editor shortcuts.

### Browser development harness (dev only)

`src/engine/devTransport.ts` implements the same `EngineTransport` interface with a small
in-memory document (frames, rectangles, ellipses, stars, text), selection, move/resize/
rotate, undo/redo, layers, pages, inspector patches, a fake rasterizer and localStorage
saves. It exists so the UI can be developed and reviewed before the Rust IPC is wired.

- `transport.ts` picks it only when `import.meta.env.DEV` is true **and**
  `window.__TAURI_INTERNALS__` is absent; inside Tauri the real transport is always used.
- It is loaded by dynamic `import()` from the DEV branch, so it is not part of the
  production interaction path inside Tauri.
- The header shows a `Dev harness` badge while it is active.
- Dev saves go to `localStorage`; nothing is written to disk.

### Dev Mode (inspection, code generation, annotations, compare)

Toggle **Dev Mode** in the toolbar (right side, shortcut `Shift+D`). While it is on, the
right sidebar becomes a Dev Mode panel with four tabs; the left panel, canvas and all
editing commands keep working. Turning it off restores the design Inspector.

| Tab | What it does | Engine calls |
| --- | --- | --- |
| Inspect | Metrics (position/size/rotation/opacity/blend), parent-relative offset, constraints/auto-layout summary, fills with swatch/hex/style/variable refs, strokes, effects, radii, typography, component role/overrides and refs. `Copy measurements`, `Copy first fill colour` (disabled with a reason when no solid fill) and `Copy snapshot JSON`. Refreshes debounced on selection/revision changes, plus a manual Refresh. | `set_dev_mode`, `dev_inspect`, `dev_snapshot` |
| Code | CSS / React + TS / SwiftUI / Jetpack Compose tabs. Fetches on switch, shows a prominent "Requires manual adaptation" banner plus every engine warning. Copy and Download (`generated.<ext>` via the dialog plugin). Loading/error states, never presented as production code. | `dev_codegen` |
| Annotate | Adds a note/implementation/measurement annotation (body required, optional link) to the selected layer, lists document annotations with author/timestamps, resolves them (`Mark resolved`), and updates the handoff status (draft → ready for review → ready for development → implemented) with a "marked by" field prefilled from `preferences["user.name"]`. Warns while open annotations exist. | `dev_handoff`, `dev_add_annotation`, `dev_resolve_annotation`, `dev_mark_ready` |
| Compare | Captures the current revision as canonical JSON (max 5, labelled, persisted in `localStorage`), and diffs the current document against a capture. Shows added/removed/changed counts, node rows and changed-field chips. The engine compares *current* as before and *captured* as after, so the panel re-labels added/removed for the "since capture" direction and states that direction explicitly. | `dev_snapshot`, `dev_compare` |

Honest limitations:

- Generated code is a static starting point from resolved values; layout/constraints,
  prototype links and assets are not generated, and the panel says so.
- Dev Mode requires `capabilities.dev_mode === true`; the harness and the current Rust
  build both set it, and the toggle is disabled with a tooltip otherwise.
- The browser harness simulates deterministic Dev Mode data (inspect node snapshots,
  four code generators, annotations, snapshots and diffs) so the whole panel is
  explorable without Rust; it reports its own "browser harness" warnings openly.

### Design systems, components, variables and prototypes

The left panel exposes real design-system libraries; every button maps to an engine command.

| Surface | What it does | Engine calls |
| --- | --- | --- |
| Layers & Pages | Select, rename, show/hide, lock, reorder and navigate pages (unchanged from milestone 1). | `select_nodes`, `rename_node`, `toggle_visible`, `toggle_locked`, page commands |
| Assets | Paint, text and effect styles from `summary.styles`. Apply a style to the current selection, rename inline, delete with confirmation, or create a new style from the selected layer (kind detected the same way the engine does: text → text style, fill → paint style, effects → effect style). | `create_style_from_node`, `apply_style`, `rename_style`, `delete_style` |
| Components | Main components and component sets from `summary.components`. Create a component from the selection, insert an instance at the viewport centre, select the source node, and detach instances from the layer context menu. | `create_component`, `place_instance`, `detach_instance` |
| Variables | Collections, modes and values from `summary.variables`. Create a collection, add colour/float variables, edit the value for the active mode, switch the active mode per collection (variable-bound fills repaint immediately), and see aliases as references rather than invented values. | `create_variable_collection`, `create_color_variable`, `create_float_variable`, `set_variable_value`, `set_active_mode` |
| Inspector → Styles | Applies the matching style category to the selection, creates a style from the layer, and manages existing styles. | `apply_style`, `create_style_from_node`, `rename_style`, `delete_style` |
| Inspector → Prototype | Lists the selected layer's interactions (trigger + action summary), adds interactions (on click / hover / press / after delay → navigate to / open overlay / close overlay / back / scroll to) and removes them. | `add_interaction`, `remove_interaction` |
| Present | Toolbar **Present** (or `Ctrl/Cmd+Enter`) plays the selected frame, or the first frame on the page. The editor shell is replaced by a full-window presentation canvas: the frame is rendered with `render_frame`, engine hotspots are outlined with labels, clicks go through the engine hit-test via `pointer`, **Back** appears when `can_back`, and `Esc` exits. | `prototype_play`, `prototype_stop`, `prototype_back` |

Honest limitations:

- The engine currently returns hotspots only for the presented frame, not for opened
  overlays. Overlay navigation works through `pointer` hit-testing; the UI draws only the
  hotspot rectangles the engine actually reports.
- Scroll-to interactions are authored and stored, but the engine does not animate scroll
  yet; the form says so instead of faking a preview.
- Deleting a style/component is confirmed locally; the engine validates the command.
- The browser harness reproduces styles, components, variables (with two modes), prototype
  interactions and presentation, so the whole flow is explorable without Rust.

### Not implemented / honest placeholders

- Share is intentionally not built; the control explains this in a toast rather than
  opening a fake dialog.
- Boolean operations and other commands that report `unsupported` produce an honest toast.
- Image import is disabled proactively from `capabilities.import_image` (false in this
  build) and explains itself through a tooltip/toast.
- Export follows the engine v1 scoping rules: explicit `node_ids`, otherwise the current
  selection, otherwise the whole page. The export dialog shows the current scope and lets
  you choose page vs selection; deselect layers to export the page when a selection
  exists. Transparent output is PNG-only.
- Native window decorations are used (no fake custom title-bar buttons).
- Dev Mode code output is a starting point, not production code; the panel says so.

### Engine capability contract (integrated with the real IPC)

The UI reads `bootstrap.capabilities` and disables actions it cannot honour:

| capability        | UI effect                                              |
| ----------------- | ------------------------------------------------------ |
| `export_pdf`      | PDF row in the export dialog and menu                  |
| `export_scope`    | page/selection scope cards + transparent background    |
| `import_svg`      | Import SVG menu/palette rows and command               |
| `import_image`    | Place image menu row and command                       |
| `prototype`       | Present button, prototype commands and presentation layer  |
| `canvas_text_edit`| on-canvas DOM text editor                              |
| `dev_mode`        | dev-only badges/notes                                  |

Earlier bridge gaps are resolved and the workarounds were removed: `render_frame` accepts
a numeric `quality`; `set_preference` returns the updated preferences; exporters honour
`node_ids` and `transparent`; `import_svg` honours `page_id`/`parent_id`; and the
capability flags above replace the previous "try and toast" behaviour where possible.

## Verification

- `pnpm test` — 138 unit tests: store reducers (editor + Dev Mode), shortcut
  resolver/persistence, palette fuzzy filtering and grouping, overlay geometry,
  color/number formatting, layer-tree flattening/search, Dev Mode measurement
  formatting, diff direction/counts, capture persistence and dedupe, prototype
  interaction payloads/labels/hotspot hit-testing, style-kind detection and variable
  value parsing/formatting.
- `pnpm build` — strict type-check + production bundle; current output: 469.47 kB JS
  (135.26 kB gzip), 59.21 kB CSS (9.78 kB gzip), 1.05 kB dynamic plugin-fs chunk,
  0.61 kB HTML, no build warnings. The dev harness is excluded from the production
  bundle (verified: no `devTransport` / `iconcrafter.dev.files` strings in `dist/`).

