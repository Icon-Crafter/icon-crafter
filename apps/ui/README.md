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
  components/
    Toolbar, LeftPanel, Inspector, CanvasStage, StatusBar, App shell
    panels/          Layers, Pages, Assets, Components
    inspector/       Transform/Layout/Fill/Stroke/Effects/Typography/Component/Export
    primitives/      IconButton, Button, menus, number/color/segmented fields, Dialog
    CommandPalette, ContextMenus, Dialogs, Toasts
  icons/             lucide-react re-exports + editor-specific custom SVG glyphs
  i18n/              centralized English strings + label helpers
  styles/            tokens.css (design system variables) + app.css
  utils/             color, number, fuzzy-match, layer-tree utilities
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

### Not implemented / honest placeholders

- Share and present are intentionally not built; the controls explain this in a toast
  rather than opening a fake dialog.
- Applying shared styles from the Assets panel is not wired yet (engine has no command);
  rows explain this instead of pretending to apply.
- Component insertion and boolean operations depend on engine support; when the engine
  reports `unsupported`, a toast is shown.
- Image import is disabled proactively from `capabilities.import_image` (false in this
  build) and explains itself through a tooltip/toast.
- Export follows the engine v1 scoping rules: explicit `node_ids`, otherwise the current
  selection, otherwise the whole page. The export dialog shows the current scope and lets
  you choose page vs selection; deselect layers to export the page when a selection
  exists. Transparent output is PNG-only.
- Native window decorations are used (no fake custom title-bar buttons).

### Engine capability contract (integrated with the real IPC)

The UI reads `bootstrap.capabilities` and disables actions it cannot honour:

| capability        | UI effect                                              |
| ----------------- | ------------------------------------------------------ |
| `export_pdf`      | PDF row in the export dialog and menu                  |
| `export_scope`    | page/selection scope cards + transparent background    |
| `import_svg`      | Import SVG menu/palette rows and command               |
| `import_image`    | Place image menu row and command                       |
| `prototype`       | reserved for prototype/dev-mode commands               |
| `canvas_text_edit`| on-canvas DOM text editor                              |
| `dev_mode`        | dev-only badges/notes                                  |

Earlier bridge gaps are resolved and the workarounds were removed: `render_frame` accepts
a numeric `quality`; `set_preference` returns the updated preferences; exporters honour
`node_ids` and `transparent`; `import_svg` honours `page_id`/`parent_id`; and the
capability flags above replace the previous "try and toast" behaviour where possible.

## Verification

- `pnpm test` — 85 unit tests: store reducers, shortcut resolver/persistence, palette
  fuzzy filtering and grouping, overlay geometry, color/number formatting, layer-tree
  flattening/search.
- `pnpm build` — strict type-check + production bundle; current output: 388.80 kB JS
  (115.63 kB gzip), 41.78 kB CSS (7.32 kB gzip), 0.61 kB HTML, no build warnings. The dev
  harness is excluded from the production bundle (verified: no `devTransport` /
  `iconcrafter.dev.files` strings in `dist/`).
