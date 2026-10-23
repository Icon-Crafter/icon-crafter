# Editor ⇄ engine contract (Tauri IPC)

The React UI is a **presentation layer**. This document is the canonical contract between
`apps/ui` (TypeScript) and the Rust engine (`apps/desktop`). Both sides must implement this
exactly; changes are proposed to the Lead, who updates this document and the Rust side.
The Rust types live in `apps/desktop/src/ipc.rs`; the TypeScript mirror in
`apps/ui/src/engine/contract.ts`.

All lengths are design pixels unless a name says otherwise (`screen_*`, `dpr`, `zoom`).
All colors are sRGB `rgba` objects `{ r, g, b, a }` with 0..1 floats.

## 1. Frame rendering

The engine owns the viewport (pan/zoom) and the document. The UI requests a raster:

```ts
// returns raw RGBA8 straight-alpha pixels, length = width*height*4, width/height from state.viewport*dpr
const pixels: ArrayBuffer = await invoke("render_frame", { quality: 1.0 });
```

The UI keeps one frame in flight; after an `EngineUpdate` with a new `frame_id`, it requests
a frame and paints it. During continuous gestures the UI may request `quality: 0.5`.

The overlay (selection, handles, guides, marquee, cursors) is **not** in the raster. Rust
returns it as JSON in `EngineState.overlay`; the UI draws it in a DOM/canvas2D overlay layer
using world→screen transforms from `state.viewport`.

World↔screen: `screen = (world - {x, y}) * zoom`; `world = {x, y} + screen / zoom`.

## 2. Engine state

```ts
interface EngineState {
  doc: DocMeta | null;
  revision: number;        // bump on any document mutation (undo included)
  frame_id: number;        // bump when pixels need re-requesting
  viewport: Viewport;      // { x, y, zoom, width, height, dpr }  (width/height CSS px)
  tool: ToolId;
  selection: string[];
  hover: string | null;
  overlay: Overlay;
  can_undo: boolean;
  can_redo: boolean;
  dirty: boolean;
  summary: DocSummary | null;
  selection_details: NodeDetail[];
  text_edit: TextEditState | null;
  errors: EngineError[];   // drained each call; { code, message, severity }
  status: string | null;   // transient status line ("Saved", "3 assets exported")
}

interface DocMeta { id: string; name: string; path: string | null; schema: number; revision: number; }

interface DocSummary {
  pages: { id: string; name: string }[];
  active_page: string;
  nodes: TreeNode[];             // flat pre-order of the active page (root frames and all descendants)
  styles: { paints: StyleEntry[]; texts: StyleEntry[]; effects: StyleEntry[] };
  components: ComponentEntry[];  // { node_id, name, kind: "component" | "component_set" }
  variables: VariableCollectionEntry[];
}

interface TreeNode {
  id: string; name: string; kind: NodeKind; depth: number;
  parent: string | null; visible: boolean; locked: boolean;
  child_count: number; is_component: boolean; is_instance: boolean;
  frame: Rect;                 // world-space AABB for the layers panel/thumbnails
}

interface StyleEntry { id: string; name: string; }
interface ComponentEntry { node_id: string; name: string; kind: "component" | "component_set"; }
interface VariableCollectionEntry { id: string; name: string; modes: { id: string; name: string }[]; }

type NodeKind =
  | "frame" | "group" | "component" | "component_set" | "instance"
  | "rectangle" | "ellipse" | "line" | "polygon" | "star" | "vector"
  | "text" | "image" | "boolean" | "slice" | "sticky" | "connector";

interface NodeDetail {   // full editable node state for the inspector / dev mode
  id: string; name: string; kind: NodeKind; parent: string | null;
  transform: Transform;          // { x, y, w, h, rotation, flip_x, flip_y }
  visible: boolean; locked: boolean; opacity: number; blend: string;
  fills: Fill[]; strokes: Stroke[]; effects: Effect[]; corner_radii: CornerRadii;
  fill_rule: "nonzero" | "evenodd";
  text: TextContent | null;
  layout: AutoLayout | null;
  constraints: Constraints | null;
  component: ComponentRole | null;
  is_inside_instance: boolean;
  absolute_bounds: Rect;         // world AABB
  resolved: ResolvedStyle | null; // computed colors/flags for Dev Mode (null in v1 until devmode lands)
}
```

`Fill`, `Stroke`, `Effect`, `Transform`, `TextContent`, `AutoLayout`, `Constraints`,
`ComponentRole`, `Rect` mirror [DOCUMENT-FORMAT.md](DOCUMENT-FORMAT.md) (camelCase in TS,
snake_case in Rust; serde rename is not used — TypeScript uses the serialized snake_case
names to avoid a translation layer; see `contract.ts` for the exact shapes).

`Overlay`:

```ts
interface Overlay {
  selection: SelectionBox[];     // { id, corners: [Point,Point,Point,Point] } world-space, rotated
  hover: SelectionBox | null;
  handles: Handle[];             // { kind: "corner"|"edge"|"rotate", pos: Point, cursor: string }
  marquee: Rect | null;
  snap_lines: SnapLine[];        // { axis: "x"|"y", pos: number, from: number, to: number }
  guides: Guide[];               // { axis: "x"|"y", pos: number }
  cursors: PeerCursor[];         // { actor, name, color, world: Point } (collab later)
  tool_hint: string | null;
  text_edit: { caret: Point; selection: [number, number] } | null;
}
```

## 3. Commands (Tauri `invoke`)

| command | args | returns | notes |
|---|---|---|---|
| `bootstrap` | – | `Bootstrap` | app version, recent files, preferences, capabilities |
| `new_document` | `{ kind: DocKind }` | `EngineState` | creates default page + starter frame |
| `open_document` | `{ path: string }` | `EngineState` | validates + migrates |
| `close_document` | – | `EngineState` | prompts UI-side for unsaved |
| `save_document` | `{ path?: string }` | `SaveResult` | autosave already runs in engine |
| `engine_state` | – | `EngineState` | re-sync panels after UI reload |
| `set_tool` | `{ tool: ToolId }` | `EngineState` | |
| `pointer` | `PointerInput` | `EngineState` | down/move/up/dblclick, screen px |
| `wheel` | `WheelInput` | `EngineState` | engine applies zoom/pan |
| `key` | `KeyInput` | `EngineState` | only when engine owns focus (text/pen) |
| `command` | `{ name: string, args?: unknown }` | `EngineState` | discrete commands, see §4 |
| `render_frame` | `{ quality?: number }` | `ArrayBuffer` | raw RGBA |
| `node_details` | `{ ids: string[] }` | `NodeDetail[]` | inspector/dev-mode |
| `export_png` | `ExportRaster` | `ExportResult` | dialog handled UI-side |
| `export_svg` | `ExportVector` | `ExportResult` | |
| `export_pdf` | `ExportPdf` | `ExportResult` | |
| `import_svg` | `{ path, page_id?, parent_id? }` | `EngineState` | real import |
| `import_image` | `{ path }` | `EngineState` | adds image asset + fill |
| `set_preference` | `{ key: string, value: unknown }` | `Bootstrap` | persisted in SQLite |

```ts
interface Viewport { x: number; y: number; zoom: number; width: number; height: number; dpr: number; }
type DocKind = "design" | "whiteboard" | "slides" | "site" | "illustration";
type ToolId =
  | "move" | "hand" | "frame" | "slice" | "rectangle" | "ellipse" | "line"
  | "polygon" | "star" | "pen" | "node" | "text" | "image" | "comment" | "connector";
type SaveResult = { path: string; revision: number; bytes: number };

interface PointerInput {
  kind: "down" | "move" | "up" | "double_click" | "leave";
  button: 0 | 1 | 2;                       // 0 left, 1 middle, 2 right
  screen_x: number; screen_y: number;      // CSS px relative to the canvas element
  shift: boolean; alt: boolean; ctrl: boolean; meta: boolean;
  pressure?: number;
  timestamp_ms: number;
}
interface WheelInput {
  delta_x: number; delta_y: number;
  screen_x: number; screen_y: number;
  shift: boolean; ctrl: boolean; alt: boolean; meta: boolean;
}
interface KeyInput {
  kind: "down" | "up";
  key: string; code: string;
  shift: boolean; alt: boolean; ctrl: boolean; meta: boolean;
  repeat: boolean;
}
interface TextEditState {
  node_id: string;
  screen_rect: Rect;            // where the UI editor overlay goes
  characters: string;
  caret: number;                // UTF-16 index into characters (v1 limitation, documented)
  selection: [number, number] | null;
  font: { family: string; weight: number; italic: boolean; size: number; line_height: number; letter_spacing: number; align: string };
  color: { r: number; g: number; b: number; a: number };
}
interface ExportRaster { node_ids?: string[]; scale: number; transparent: boolean; path: string; }
interface ExportVector { node_ids?: string[]; path: string; }
interface ExportPdf { node_ids?: string[]; path: string; }
interface ExportResult { path: string; bytes: number; width?: number; height?: number; format: string; }
```

## 4. Discrete command names (`invoke("command", { name, args })`)

History: `undo`, `redo`, `delete`, `duplicate`, `copy`, `paste`, `cut`, `select_all`,
`select_none`, `select_inverse`, `group`, `ungroup`, `mask_with_selection`, `create_component`,
`create_component_set`, `detach_instance`, `swap_instance`.

Ordering: `bring_to_front`, `bring_forward`, `send_backward`, `send_to_back`,
`move_to_parent`, `reorder` (`{ node_id, new_parent_id, index }`).

View: `zoom_in`, `zoom_out`, `zoom_to`, `zoom_fit`, `zoom_selection`, `zoom_100`,
`pan_by`, `set_viewport`.

Edit: `apply_patch` (`{ ids, patch: NodePatch, label, coalesce_key? }`), `rename_node`,
`toggle_visible`, `toggle_locked`, `set_opacity`, `nudge` (`{ dx, dy, ids? }`),
`align` (`{ ids?, align: "left"|"hcenter"|"right"|"top"|"vcenter"|"bottom" }`),
`distribute` (`{ ids?, axis: "horizontal"|"vertical", spacing: "gap"|"center" }`),
`boolean` (`{ ids?, op: "union"|"subtract"|"intersect"|"exclude" }`),
`flatten`, `outline_stroke`, `text_edit_begin`, `text_edit_input`,
`text_edit_set_caret`, `text_edit_commit`, `text_edit_cancel`.

Prototype: `set_interaction`, `remove_interaction`, `prototype_play`, `prototype_stop`
(implemented in the prototype phase; unknown commands return a structured `unsupported`
error instead of failing silently).

Dev mode: `dev_mode_toggle`, `dev_mode_mark_ready`, `dev_mode_export_asset`,
`dev_mode_generate_code` (`{ language }`), `dev_mode_add_annotation`.

`NodePatch` is a partial `NodeDetail`-shaped object; only provided fields are applied.
`coalesce_key` merges consecutive patches with the same key into one undo step.

All commands return a full `EngineState` (except render/export/details). Unknown command
names, invalid arguments and disallowed ops return `EngineError { code, message }` and
leave the document untouched; they never panic the app.

## 5. Errors and status

* `EngineError.code` is a stable machine-readable string (`"validation"`, `"io"`,
  `"permission"`, `"unsupported"`, `"conflict"`, `"internal"`).
* The UI shows errors in a non-blocking banner (with "details" expander) and keeps
  running; destructive failures never close the document without a save prompt.
* `status` is a transient human message; the UI displays it in the status bar for ~4 s.
* Engine panics are caught at the command boundary and converted to `internal` errors
  (the document snapshot is preserved by the autosave journal).

## 6. UI-owned responsibilities

* Chrome: toolbar, sidebars, status bar, dialogs, command palette, context menus, toasts.
* Keyboard shortcut definitions (customizable, persisted UI-side via `set_preference`).
* Text input composition (IME) through a DOM textarea; the engine validates and stores text.
* File dialogs (Tauri dialog plugin) and drag-and-drop file paths.
* Theme (dark by default), density, and font-size preferences.
* Presentation shells for prototype preview / Dev Mode (Rust supplies the data/render).

## 7. v1.1 addendum — bootstrap, viewport and command argument conventions (frozen)

This section is authoritative where §4 is thinner. All JSON field names are `snake_case`
(Rust serde defaults; the UI must not camelCase IPC payloads).

### 7.1 Bootstrap

```ts
interface Bootstrap {
  app_version: string;                    // CARGO_PKG_VERSION
  recent_files: { path: string; name: string }[];
  preferences: Record<string, unknown>;   // opaque UI prefs (theme/density/shortcuts)
  capabilities: Record<string, unknown>;  // honest feature flags: { ai, plugins, dev_mode, export_pdf, collab }
}
```

### 7.2 Viewport

`set_viewport { width, height, dpr }` — CSS pixels plus devicePixelRatio of the canvas
area; the engine keeps its own pan (`x`,`y`) and `zoom` and returns a fresh frame. The UI
calls this on resize and DPR change.

### 7.3 Selection

`select_nodes { ids: string[], additive?: boolean }` — selects by id (unknown ids are
ignored, not an error). Engine keeps the canonical selection; UI mirrors `EngineState.selection`.

### 7.4 Pages

`set_active_page { page_id }`, `add_page { name? }`, `rename_page { page_id, name }`,
`delete_page { page_id }` — deleting the last remaining page returns a validation error.

### 7.5 Command arguments

Where `ids?` is omitted the current selection is used; commands that need more ids than
are selected return a `validation` error and never touch the document.

| Command | Arguments |
|---|---|
| `rename_node` | `{ id, name }` |
| `toggle_visible` / `toggle_locked` | `{ ids? }` |
| `set_opacity` | `{ ids?, opacity }` (0..1, clamped) |
| `nudge` | `{ dx, dy, ids? }` |
| `align` | `{ ids?, align: "left"\|"hcenter"\|"right"\|"top"\|"vcenter"\|"bottom" }` (>=2 ids) |
| `distribute` | `{ ids?, axis: "horizontal"\|"vertical", spacing: number }` (spacing `0` = equal gaps; >=3 ids) |
| `boolean` | `{ ids?, op: "union"\|"subtract"\|"intersect"\|"exclude" }` (first id is the base for subtract) |
| `pan_by` | `{ dx, dy }` (world units) |
| `zoom_to` | `{ zoom }` (clamped 0.01..64, anchored at viewport centre) |
| `text_edit_begin` | `{ node_id }` |
| `text_edit_input` | `{ characters, caret, selection: [number, number] \| null }` |
| `text_edit_set_caret` | `{ caret, selection: [number, number] \| null }` |
| `text_edit_commit` / `text_edit_cancel` | `{}` |

The earlier §4 `distribute` spelling `spacing: "gap"|"center"` is superseded by the
numeric form above. Group/boolean operand order follows the engine selection order; the
UI must show that order (it is the z-order of the selection).

## 8. v1.2 implementation notes — session 3 (engine IPC landing)

These notes describe how the Rust implementation (`crates/dp-editor`, `apps/desktop/src/ipc.rs`)
realizes the contract above. They are authoritative where they add detail.

### 8.1 Overlay coordinate spaces

The overlay is produced in Rust. World values are authoritative; the UI converts to screen
with `worldToScreen` (already in `contract.ts`).

* `overlay.selection[].corners` — WORLD-space rotated corners (`tl, tr, br, bl`), currently
  the world AABB of each node (rotation-aware outlines are a later refinement; hit testing
  is rotation-correct).
* `overlay.hover` — a WORLD-space `SelectionBox` carrying the hovered node id.
* `overlay.handles[].pos` — WORLD-space handle centres; `kind` is `corner` | `edge` |
  `rotate` (the engine's `nw/n/ne/e/se/s/sw/w` kinds are collapsed to `corner`/`edge`).
* `overlay.snap_lines` — WORLD space: `axis: "x"` is a vertical line (`pos` = x,
  `from`/`to` = y range), `axis: "y"` is horizontal.
* `overlay.marquee` — WORLD-space rect.
* `overlay.guides`, `overlay.cursors` — reserved for collaboration; empty in this build.
* `overlay.text_edit` — screen-space caret hint `{ caret: {x,y}, selection: [start,end] }`.
  v1 approximates caret x/y from character counts (no glyph-metric pass yet); the
  authoritative editing surface is the top-level `text_edit` (DOM textarea over `screen_rect`).
* `overlay.tool_hint` is `null` when there is nothing to show.

### 8.2 render_frame

`render_frame` returns raw RGBA8 bytes (straight alpha), length
`round(viewport.width*dpr) * round(viewport.height*dpr) * 4`, row-major, top-left origin.
The overlay is NOT included; the UI draws it from `EngineState.overlay`. `quality` is
accepted and ignored by the CPU reference renderer in this build.

### 8.3 Command failures

Validation failures are reported inside the returned `EngineState.errors` array (one-shot:
they do not persist into the next snapshot), with `code` mapped to the §5 strings. Unknown
or not-yet-implemented commands return `unsupported` and must be surfaced visibly by the
UI — never silently ignored, never a fake success.

### 8.4 Explicit limitations in this build

* `import_image` returns an honest `unsupported` error; image assets land with the
  asset-storage milestone.
* `import_svg` preserves editable cubic vector geometry, solid fills and strokes; gradients
  are reduced to their first stop and text is converted to outlines by the importer
  (warnings are reported in `status`).
* Export uses the selection bounds when a selection exists, otherwise the whole page.
  `export_*.args.node_ids` is accepted but currently equivalent to selection/page.
* One document per app instance (multi-window is a later milestone).

