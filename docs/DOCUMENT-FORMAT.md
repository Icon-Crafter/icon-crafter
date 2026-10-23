# Document format — Icon Crafter native project file

Schema version: **1** (current implementation target). This file defines the persisted
document model exactly as `dp-model::Document` serializes it. Any change to a serialized
field is a schema change: bump `schema`, add a migration step in `dp-persist`, and add a
round-trip + migration test.

## 4.1 Container

A native project is a **ZIP container** with extension `.icraft`:

```
document.json        — the serialized document (schema below), UTF-8, no BOM
assets/images/<id>.<ext> — image assets, content-addressed by id
assets/fonts/<id>.<ext>  — embedded fonts (optional, when licensed for embedding)
meta.json            — { "format": "iconcrafter", "container": 1, "created_by": "..." }
```

Rationale: readable JSON for the document, binary-safe asset storage without base64
overhead, single-file project (portable), ZIP CRC validation on open. A `.json` export of
the document alone is also supported (interchange); assets are referenced by id and are
**not** embedded in that mode (opening a JSON doc with missing assets produces explicit
missing-asset errors, not silent blanks).

Validation on open is strict and bounded: max document bytes 512 MiB, max node count
2,000,000, max path points 100,000 per path, max string length 2 MiB, JSON depth 64.
Unknown properties are **preserved** via a `#[serde(flatten)] extensions` map? — No:
unknown properties are rejected in v1 (strict schema) so silent data loss is impossible;
forward compatibility is handled by migration steps only. `document.json` carries
`schema`; opening a newer schema than supported fails with a clear error.

## 4.2 Common types

```jsonc
Color      { "r": 0.0..1.0, "g": ..., "b": ..., "a": 0.0..1.0 }   // linear sRGB reference space
Point      { "x": f32, "y": f32 }
Size       { "w": f32, "h": f32 }
Rect       { "x": f32, "y": f32, "w": f32, "h": f32 }
Transform  { "x": f32, "y": f32, "w": f32, "h": f32,
             "rotation": f32,          // degrees, clockwise, about the node's center
             "flip_x": bool, "flip_y": bool }
BlendMode  "normal" | "multiply" | "screen" | "overlay" | "darken" | "lighten" |
           "color-dodge" | "color-burn" | "hard-light" | "soft-light" | "difference" |
           "exclusion" | "hue" | "saturation" | "color" | "luminosity" | "pass-through"
```

All lengths are in **design pixels** (CSS px at 100% zoom). Rotation is applied after
translation about the node center; flips are applied before rotation.

## 4.3 Document

```jsonc
{
  "schema": 1,
  "id": "uuid",
  "name": "Untitled",
  "created_at": "RFC3339",
  "updated_at": "RFC3339",
  "revision": 0,                       // preserved for dirty-state bookkeeping
  "active_page": "page-uuid",
  "pages": [ Page, ... ],              // at least 1
  "styles": { "paints": {...}, "texts": {...}, "effects": {...} },
  "components": { "components": {...}, "component_sets": {...} },
  "variables": { "collections": [...], "variables": {...} },
  "assets": { "images": {...}, "fonts": [...] },
  "prototype": { "start_frames": [...], "flows": [...] },
  "settings": { "grid": {...}, "guides": [...], "unit": "px" }
}
```

### Page

```jsonc
{
  "id": "uuid",
  "name": "Page 1",
  "background": Color,                 // page background, opaque by default
  "nodes": { "node-uuid": Node, ... }, // flat node map
  "roots": [ "node-uuid", ... ]        // ordered roots (top-level frames/shapes)
}
```

Invariants (checked by `Document::validate()`; violations are bugs, not user errors):

1. Every id in `roots` and every child id exists in `nodes`.
2. `node.parent` is `null` iff the node is a root; otherwise `nodes[parent].children`
   contains `node.id` exactly once.
3. A node appears in exactly one parent's `children`, except instance-internal clones
   (owned by the instance).
4. `children` order is the paint order (first = bottom); `roots` likewise.
5. No cycles; parent chain depth ≤ 256.
6. Instance nodes never have document siblings with the same `component_tree_id`.

### Node

```jsonc
{
  "id": "uuid",
  "name": "Rectangle 1",
  "kind": "frame" | "group" | "component" | "component_set" | "instance" |
          "rectangle" | "ellipse" | "line" | "polygon" | "star" | "vector" |
          "text" | "image" | "boolean" | "slice" | "sticky" | "connector",
  "parent": "uuid" | null,
  "children": [ "uuid", ... ],
  "rev": 1,                            // increments on each mutation (cache key)
  "visible": true,
  "locked": false,
  "opacity": 1.0,
  "blend": "normal",
  "transform": Transform,
  "style": NodeStyle,
  "geometry": Geometry | null,
  "text": TextContent | null,
  "layout": AutoLayout | null,
  "constraints": Constraints | null,
  "component": ComponentRole | null,
  "prototype": PrototypeNode | null,
  "props": { "key": Value, ... },      // component/plugin properties (typed values)
  "plugin_data": { "plugin-id": "json-string", ... }
}
```

`NodeStyle`:

```jsonc
{
  "fills": [ Fill, ... ],              // bottom to top; empty = no fill
  "strokes": [ Stroke, ... ],
  "effects": [ Effect, ... ],
  "corner_radii": { "tl": f32, "tr": f32, "br": f32, "bl": f32 }, // per-corner, clamped to size/2
  "fill_rule": "nonzero" | "evenodd",
  "stroke_align": "center" | "inside" | "outside",   // default per stroke overrides
  "inset_stroke": false                            // legacy helper resolved during layout
}
```

`Fill`:

```jsonc
{
  "visible": true,
  "opacity": 1.0,
  "blend": "normal",
  "paint": Paint,                      // inline paint (see below)
  "style_id": "uuid" | null,           // if set, resolves to styles.paints[style_id]
  "variable_id": "uuid" | null         // if set and resolvable, wins over style/inline
}
```

`Paint` (serde tag `"type"`):

```jsonc
{ "type": "solid", "color": Color }
{ "type": "linear", "stops": [ {"position": 0.0, "color": Color}, ... ],
  "from": Point, "to": Point,                  // unit space of the node bounds (0..1)
  "transform": [[a,b,c],[d,e,f]] | null }      // optional 2x3 gradient transform
{ "type": "radial", "stops": [...], "center": Point, "radius": f32,
  "transform": ... | null }
{ "type": "angular", "stops": [...], "center": Point, "angle": f32 }
{ "type": "image", "asset_id": "uuid", "fit": "fill" | "fit" | "crop" | "tile",
  "transform": [[a,b,c],[d,e,f]] | null }
```

`Stroke`:

```jsonc
{ "visible": true, "opacity": 1.0, "blend": "normal", "paint": Paint,
  "style_id": null, "variable_id": null,
  "weight": 1.0, "align": "center" | "inside" | "outside",
  "cap": "butt" | "round" | "square",
  "join": "miter" | "round" | "bevel",
  "miter_limit": 4.0,
  "dash": [ f32, ... ] | null }
```

`Effect` (tag `"type"`): `drop_shadow` / `inner_shadow` with `{ color, offset: Point,
blur: f32, spread: f32, blend }`, `layer_blur` / `background_blur` with `{ radius: f32 }`.

`Geometry` (tag `"type"`, only for kinds that need parameters):

```jsonc
{ "type": "vector", "path": VectorPath }                    // vector kind
{ "type": "polygon", "point_count": 3..1000 }
{ "type": "star", "point_count": 3..1000, "inner_ratio": 0.0..1.0 }
{ "type": "boolean", "operation": "union" | "subtract" | "intersect" | "exclude",
  "fill_rule": "nonzero" | "evenodd" }                       // operands = children, evaluated live
```

`VectorPath`:

```jsonc
{
  "contours": [
    {
      "closed": true,
      "anchors": [
        { "p": Point, "in": Point, "out": Point,       // control points, node-local absolute
          "smooth": true }                              // smooth = auto-mirrored handles
      ]
    }
  ]
}
```

Anchors define cubic segments: segment `i` goes from `anchors[i].p` with control
`anchors[i].out` to `anchors[i+1].p` with control `anchors[i+1].in`. A closed contour adds
a segment from the last anchor back to the first. The first anchor's `in` and the last
anchor's `out` are unused on an open contour (stored but ignored) — this keeps handle
editing reversible.

`TextContent`:

```jsonc
{
  "characters": "Hello world",
  "style": {
    "font_family": "Inter", "font_weight": 400, "italic": false,
    "font_size": 14.0,
    "line_height": { "unit": "px" | "auto" | "percent", "value": 20.0 },
    "letter_spacing": 0.0,
    "text_case": "original" | "upper" | "lower" | "title",
    "decoration": "none" | "underline" | "strikethrough",
    "fills": [ Fill, ... ]
  },
  "paragraph": {
    "align": "left" | "center" | "right" | "justified",
    "vertical_align": "top" | "center" | "bottom",
    "max_lines": null | u32,
    "paragraph_spacing": 0.0
  },
  "auto_size": "none" | "height" | "width_and_height",
  "on_path": null | { "path_node_id": "uuid", "start_offset": f32, "side": "left" | "right" }
}
```

`AutoLayout`:

```jsonc
{
  "direction": "horizontal" | "vertical",
  "wrap": false,
  "gap": 8.0,
  "padding": { "top": 8, "right": 8, "bottom": 8, "left": 8 },
  "primary_align": "start" | "center" | "end" | "space_between",
  "counter_align": "start" | "center" | "end" | "stretch" | "baseline",
  "sizing": { "horizontal": "fixed" | "hug" | "fill",
              "vertical":   "fixed" | "hug" | "fill" },
  "min_size": { "w": f32, "h": f32 },
  "max_size": { "w": f32, "h": f32 },
  "item_reverse": false
}
```

`Constraints` (for children of a non-auto-layout container):

```jsonc
{ "horizontal": "left" | "right" | "left_right" | "center" | "scale",
  "vertical":   "top" | "bottom" | "top_bottom" | "center" | "scale" }
```

`ComponentRole` (tag `"type"`):

```jsonc
{ "type": "component", "key": "library-key", "description": "",
  "variant_properties": { "Size": "Large" } }     // component_set children
{ "type": "component_set", "key": "library-key" }
{ "type": "instance", "component_id": "uuid",
  "overrides": [ { "target": "node-id-inside-instance", "prop": "name|visible|...",
                   "value": Value } ],
  "swapped_component_id": null | "uuid" }
```

`PrototypeNode` and interaction types live in `dp-model::prototype` and are documented in
[PROTOTYPE.md](PROTOTYPE.md) once the runtime lands. v1 stores:

```jsonc
{ "interactions": [ { "id": "uuid", "trigger": {...}, "actions": [...],
                      "transition": {...} | null } ] }
```

## 4.4 Styles, components, variables, assets

```jsonc
styles: {
  "paints": { "uuid": { "id": "uuid", "name": "Brand/500", "paint": Paint,
                        "description": "" } },
  "texts":  { "uuid": { "id": "uuid", "name": "Heading/H1", "style": TextStyle,
                        "description": "" } },
  "effects":{ "uuid": { "id": "uuid", "name": "Card", "effects": [ Effect ],
                        "description": "" } }
}
components: {
  "components": { "node-uuid": { "node_id": "uuid", "key": "...",
                                 "name": "...", "description": "",
                                 "created_at": "RFC3339" } },
  "component_sets": { "node-uuid": { "node_id": "uuid", "key": "...", "name": "..." } }
}
variables: {
  "collections": [ { "id": "uuid", "name": "Theme",
                     "modes": [ { "id": "uuid", "name": "Light" },
                                { "id": "uuid", "name": "Dark" } ],
                     "default_mode": "uuid" } ],
  "variables": { "uuid": { "id": "uuid", "collection_id": "uuid",
      "name": "color/bg", "description": "",
      "resolved_type": "color" | "float" | "string" | "boolean",
      "values": { "mode-uuid": { "type": "color", "color": Color } },
      "alias": { "variable_id": "uuid" } | null } }
}
assets: {
  "images": { "uuid": { "id": "uuid", "name": "photo.png", "hash": "sha256-hex",
                         "width": 1920, "height": 1080, "mime": "image/png",
                         "file": "assets/images/uuid.png" } },
  "fonts":  [ { "id": "uuid", "family": "Inter", "style": "Regular",
                "hash": "sha256-hex", "file": "assets/fonts/uuid.ttf",
                "embed": true } ]
}
```

Resolution order for a paint binding: `variable_id` (resolved through the active variable
mode and alias chain, max depth 32) → `style_id` → inline `paint`. Broken references
(missing variable/style/component/asset) are detected by `Document::validate()` and
surfaced in the UI as explicit warnings; they never crash rendering or abort a save.

## 4.5 Versioning and migration

* `Document::SCHEMA` is compiled in. Saving writes `SCHEMA`.
* `dp-persist` keeps a migration registry: `fn migrate(from: u32, value: serde_json::Value)
  -> Result<serde_json::Value>`; each step is a pure function with fixture tests
  (`fixtures/migrations/v{n}-sample.json`).
* Opening a file with `schema > SCHEMA` fails with `UnsupportedSchema { found, supported }`.
* Opening `schema < SCHEMA` runs migrations in order, reports the applied steps, and marks
  the document dirty so the user can save the upgraded version.
* The autosave journal (SQLite, `dp-persist`) stores the last N op batches, not document
  copies, so crash recovery replays onto the last opened snapshot. Journal format carries
  the same `schema` and is discarded if incompatible.

## 4.6 Revision history / snapshots (persistence strategy)

* Named and automatic versions are stored as `(snapshot, delta*)`:
  snapshot = full `document.json` (compressed), deltas = ordered op batches
  (`Vec<DocOp>` JSON, compressed) with `(base_version, revision, author, created_at)`.
* A restore = replay snapshot + deltas in memory, then append a new head version.
* Full snapshots are taken every 200 op batches or 10 minutes of active editing, whichever
  first; deltas since the last snapshot bound recovery cost.
* Branching = a version DAG: each version has `parent`. Merge replays the branch's ops onto
  the target and reports conflicts per node/property (same-node concurrent property writes
  with overlapping fields), never silently discarding either side.

## 4.7 Clipboard

Clipboard payloads are the same node subtrees plus referenced styles/variables/assets,
serialized as `application/x-iconcrafter-nodes+json` and (for interop) `text/plain`/PNG.
Pasting into another document remaps ids deterministically (new UUIDs for nodes,
preserved component keys for cross-document instance resolution) and validates references.

## 4.8 Known limitations of schema v1

* No embedded font licensing checks; embedding is opt-in per font asset and the UI warns.
* Boolean geometry is evaluated live from children (non-destructive); manual flatten
  produces a `vector` node.
* Component instance overrides target node ids *inside the instance*; propagation from a
  changed main component is implemented in the components phase and is tracked in the
  feature checklist.
* Text-on-path references a sibling path node; circular references are rejected.
* Schema v1 is not Figma-compatible and does not claim to read `.fig` files.
