/**
 * Icon Crafter engine contract — TypeScript mirror of `docs/CONTRACT.md`.
 *
 * This file is the ONLY place where engine payload shapes are declared. The
 * Rust side (`apps/desktop/src/ipc.rs`) is canonical; when the contract
 * changes, both sides change together (the Lead updates CONTRACT.md).
 *
 * Naming: the engine serializes snake_case field names and the UI consumes
 * them as-is. Do not add a camelCase translation layer — the contract
 * deliberately avoids one (see CONTRACT.md §2).
 *
 * Colors are sRGB `{r,g,b,a}` with 0..1 floats. Lengths are design pixels
 * unless a name says `screen_` or `dpr`.
 */

// ---------------------------------------------------------------------------
// Common values
// ---------------------------------------------------------------------------

export interface Point {
  x: number;
  y: number;
}

export interface Size {
  w: number;
  h: number;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Color {
  r: number;
  g: number;
  b: number;
  a: number;
}

export interface Transform {
  x: number;
  y: number;
  w: number;
  h: number;
  /** Degrees, clockwise, about the node center. */
  rotation: number;
  flip_x: boolean;
  flip_y: boolean;
}

export type BlendMode =
  | "normal"
  | "multiply"
  | "screen"
  | "overlay"
  | "darken"
  | "lighten"
  | "color-dodge"
  | "color-burn"
  | "hard-light"
  | "soft-light"
  | "difference"
  | "exclusion"
  | "hue"
  | "saturation"
  | "color"
  | "luminosity"
  | "pass-through";

export type FillRule = "nonzero" | "evenodd";
export type StrokeAlign = "center" | "inside" | "outside";
export type StrokeCap = "butt" | "round" | "square";
export type StrokeJoin = "miter" | "round" | "bevel";

export interface CornerRadii {
  tl: number;
  tr: number;
  br: number;
  bl: number;
}

export type NodeKind =
  | "frame"
  | "group"
  | "component"
  | "component_set"
  | "instance"
  | "rectangle"
  | "ellipse"
  | "line"
  | "polygon"
  | "star"
  | "vector"
  | "text"
  | "image"
  | "boolean"
  | "slice"
  | "sticky"
  | "connector";

export type DocKind = "design" | "whiteboard" | "slides" | "site" | "illustration";

export type ToolId =
  | "move"
  | "hand"
  | "frame"
  | "slice"
  | "rectangle"
  | "ellipse"
  | "line"
  | "polygon"
  | "star"
  | "pen"
  | "node"
  | "text"
  | "image"
  | "comment"
  | "connector";

// ---------------------------------------------------------------------------
// Paint / style
// ---------------------------------------------------------------------------

export interface GradientStop {
  position: number;
  color: Color;
}

export type GradientTransform = [[number, number, number], [number, number, number]];

export type Paint =
  | { type: "solid"; color: Color }
  | {
      type: "linear";
      stops: GradientStop[];
      from: Point;
      to: Point;
      transform: GradientTransform | null;
    }
  | {
      type: "radial";
      stops: GradientStop[];
      center: Point;
      radius: number;
      transform: GradientTransform | null;
    }
  | { type: "angular"; stops: GradientStop[]; center: Point; angle: number }
  | {
      type: "image";
      asset_id: string;
      fit: "fill" | "fit" | "crop" | "tile";
      transform: GradientTransform | null;
    };

export type PaintType = Paint["type"];

export interface Fill {
  visible: boolean;
  opacity: number;
  blend: BlendMode;
  paint: Paint;
  style_id: string | null;
  variable_id: string | null;
}

export interface Stroke {
  visible: boolean;
  opacity: number;
  blend: BlendMode;
  paint: Paint;
  style_id: string | null;
  variable_id: string | null;
  weight: number;
  align: StrokeAlign;
  cap: StrokeCap;
  join: StrokeJoin;
  miter_limit: number;
  dash: number[] | null;
}

export type Effect =
  | {
      type: "drop_shadow" | "inner_shadow";
      color: Color;
      offset: Point;
      blur: number;
      spread: number;
      blend: BlendMode;
    }
  | { type: "layer_blur" | "background_blur"; radius: number };

export type EffectType = Effect["type"];

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

export type TextCase = "original" | "upper" | "lower" | "title";
export type TextDecoration = "none" | "underline" | "strikethrough";
export type TextAlign = "left" | "center" | "right" | "justified";
export type TextVerticalAlign = "top" | "center" | "bottom";
export type TextAutoSize = "none" | "height" | "width_and_height";

export interface LineHeight {
  unit: "px" | "auto" | "percent";
  value: number;
}

export interface TextStyle {
  font_family: string;
  font_weight: number;
  italic: boolean;
  font_size: number;
  line_height: LineHeight;
  letter_spacing: number;
  text_case: TextCase;
  decoration: TextDecoration;
  fills: Fill[];
}

export interface TextParagraph {
  align: TextAlign;
  vertical_align: TextVerticalAlign;
  max_lines: number | null;
  paragraph_spacing: number;
}

export interface TextOnPath {
  path_node_id: string;
  start_offset: number;
  side: "left" | "right";
}

export interface TextContent {
  characters: string;
  style: TextStyle;
  paragraph: TextParagraph;
  auto_size: TextAutoSize;
  on_path: TextOnPath | null;
}

// ---------------------------------------------------------------------------
// Layout / constraints / components
// ---------------------------------------------------------------------------

export type LayoutDirection = "horizontal" | "vertical";
export type LayoutAlign = "start" | "center" | "end" | "space_between";
export type LayoutCounterAlign = "start" | "center" | "end" | "stretch" | "baseline";
export type LayoutSizing = "fixed" | "hug" | "fill";

export interface EdgeInsets {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

export interface AutoLayout {
  direction: LayoutDirection;
  wrap: boolean;
  gap: number;
  padding: EdgeInsets;
  primary_align: LayoutAlign;
  counter_align: LayoutCounterAlign;
  sizing: { horizontal: LayoutSizing; vertical: LayoutSizing };
  min_size: Size;
  max_size: Size;
  item_reverse: boolean;
}

export type ConstraintHorizontal = "left" | "right" | "left_right" | "center" | "scale";
export type ConstraintVertical = "top" | "bottom" | "top_bottom" | "center" | "scale";

export interface Constraints {
  horizontal: ConstraintHorizontal;
  vertical: ConstraintVertical;
}

export type ComponentRole =
  | {
      type: "component";
      key: string;
      description: string;
      variant_properties: Record<string, string>;
    }
  | { type: "component_set"; key: string }
  | {
      type: "instance";
      component_id: string;
      overrides: { target: string; prop: string; value: unknown }[];
      swapped_component_id: string | null;
    };

/**
 * Dev Mode resolved style. The shape lands with `dp-devmode`; v1 returns
 * `null`. Treated as opaque data by the UI until then.
 */
export type ResolvedStyle = Record<string, unknown> | null;

// ---------------------------------------------------------------------------
// Document state (as sent to the UI)
// ---------------------------------------------------------------------------

export interface DocMeta {
  id: string;
  name: string;
  path: string | null;
  schema: number;
  revision: number;
}

export interface TreeNode {
  id: string;
  name: string;
  kind: NodeKind;
  depth: number;
  parent: string | null;
  visible: boolean;
  locked: boolean;
  child_count: number;
  is_component: boolean;
  is_instance: boolean;
  /** World-space AABB (axis-aligned), used by the layers panel and tests. */
  frame: Rect;
}

export interface StyleEntry {
  id: string;
  name: string;
}

export interface ComponentEntry {
  node_id: string;
  /** Registry id used by `place_instance`; absent/null for component sets. */
  component_id?: string | null;
  name: string;
  kind: "component" | "component_set";
}

export type VariableTypeName = "color" | "float" | "string" | "boolean";

/**
 * Tagged union matching `dp_model::VariableValue` serde output.
 * `alias` references another variable; the engine resolves it, the UI only
 * shows the reference.
 */
export type VariableValueJson =
  | { type: "color"; color: Color }
  | { type: "float"; value: number }
  | { type: "string"; value: string }
  | { type: "boolean"; value: boolean }
  | { type: "alias"; variable_id: string };

export interface VariableEntry {
  id: string;
  name: string;
  resolved_type: VariableTypeName;
  /** Values per mode id; missing modes fall back to the collection default. */
  values: Record<string, VariableValueJson>;
}

export interface VariableCollectionEntry {
  id: string;
  name: string;
  modes: { id: string; name: string }[];
  /** Active mode from document settings; absent means "collection default". */
  active_mode_id?: string | null;
  /** Full variable list (newer engine snapshots). Older engines omit it. */
  variables?: VariableEntry[];
}

export interface DocSummary {
  pages: { id: string; name: string }[];
  active_page: string;
  /** Flat pre-order of the active page (roots and all descendants). */
  nodes: TreeNode[];
  styles: { paints: StyleEntry[]; texts: StyleEntry[]; effects: StyleEntry[] };
  components: ComponentEntry[];
  variables: VariableCollectionEntry[];
}

export interface NodeDetail {
  id: string;
  name: string;
  kind: NodeKind;
  parent: string | null;
  transform: Transform;
  visible: boolean;
  locked: boolean;
  opacity: number;
  blend: string;
  fills: Fill[];
  strokes: Stroke[];
  effects: Effect[];
  corner_radii: CornerRadii;
  fill_rule: FillRule;
  text: TextContent | null;
  layout: AutoLayout | null;
  constraints: Constraints | null;
  component: ComponentRole | null;
  /** Prototype interactions attached to this node (newer engine snapshots). */
  prototype: PrototypeNodeJson | null;
  is_inside_instance: boolean;
  /** World-space AABB. */
  absolute_bounds: Rect;
  resolved: ResolvedStyle;
}

export interface Viewport {
  /** World position of the viewport's top-left corner. */
  x: number;
  y: number;
  zoom: number;
  /** CSS pixels. */
  width: number;
  height: number;
  dpr: number;
}

export interface SelectionBox {
  id: string;
  /** World-space rotated corners, in order tl, tr, br, bl. */
  corners: [Point, Point, Point, Point];
}

export interface Handle {
  kind: "corner" | "edge" | "rotate";
  /** World-space position. */
  pos: Point;
  /** CSS cursor keyword to show when hovering the handle. */
  cursor: string;
}

export interface SnapLine {
  axis: "x" | "y";
  pos: number;
  from: number;
  to: number;
}

export interface Guide {
  axis: "x" | "y";
  pos: number;
}

export interface PeerCursor {
  actor: string;
  name: string;
  color: string;
  world: Point;
}

export interface Overlay {
  selection: SelectionBox[];
  hover: SelectionBox | null;
  handles: Handle[];
  marquee: Rect | null;
  snap_lines: SnapLine[];
  guides: Guide[];
  cursors: PeerCursor[];
  tool_hint: string | null;
  text_edit: { caret: Point; selection: [number, number] } | null;
}

export type EngineErrorCode =
  | "validation"
  | "io"
  | "permission"
  | "unsupported"
  | "conflict"
  | "internal";

export interface EngineError {
  code: EngineErrorCode;
  message: string;
  severity: "error" | "warning";
}

export interface TextEditState {
  node_id: string;
  /** Screen-space (CSS px) rect where the DOM textarea overlay goes. */
  screen_rect: Rect;
  characters: string;
  /** UTF-16 index into characters (v1 limitation, documented in CONTRACT.md). */
  caret: number;
  selection: [number, number] | null;
  font: {
    family: string;
    weight: number;
    italic: boolean;
    size: number;
    line_height: number;
    letter_spacing: number;
    align: string;
  };
  color: Color;
}

// ---------------------------------------------------------------------------
// Prototype model (mirrors crates/dp-model prototype serde JSON)
// ---------------------------------------------------------------------------

/** Trigger JSON exactly as `dp_model::prototype::Trigger` serializes. */
export type TriggerJson =
  | { type: "click"; delay_ms?: number }
  | { type: "hover" }
  | { type: "press" }
  | { type: "key"; key: string }
  | { type: "after_delay"; delay_ms: number };

/** Action JSON accepted by the engine's `add_interaction` command. */
export type ActionJson =
  | { type: "navigate_to"; frame_id: string }
  | { type: "open_overlay"; frame_id: string }
  | { type: "close_overlay" }
  | { type: "scroll_to"; target: string }
  | { type: "back" }
  | { type: "set_variable"; variable_id: string; value: VariableValueJson };

export interface InteractionJson {
  id: string;
  trigger: TriggerJson;
  actions: ActionJson[];
}

/** `NodeDetail.prototype` / engine `PrototypeNode`. */
export interface PrototypeNodeJson {
  interactions: InteractionJson[];
}

/** Interactive node on the currently presented frame or one of its overlays. */
export interface PrototypeHotspot {
  node_id: string;
  /** World-space AABB. */
  rect: Rect;
  /** Layer name (engine currently supplies the node name). */
  label: string;
}

/** Additive snapshot field present while `prototype_play` is active. */
export interface PrototypeState {
  active: boolean;
  frame_id: string;
  frame_name: string;
  can_back: boolean;
  overlay_count: number;
  /** Empty when not presenting or when the engine build predates hotspots. */
  hotspots?: PrototypeHotspot[];
}

export interface EngineState {
  doc: DocMeta | null;
  /** Bumps on any document mutation (undo included). */
  revision: number;
  /** Bumps when pixels need re-requesting. */
  frame_id: number;
  viewport: Viewport;
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
  /** Prototype presentation session, null when not presenting. */
  prototype: PrototypeState | null;
  errors: EngineError[];
  status: string | null;
}

// ---------------------------------------------------------------------------
// Input payloads
// ---------------------------------------------------------------------------

export interface PointerInput {
  kind: "down" | "move" | "up" | "double_click" | "leave";
  /** 0 left, 1 middle, 2 right. */
  button: 0 | 1 | 2;
  /** CSS px relative to the canvas element. */
  screen_x: number;
  screen_y: number;
  shift: boolean;
  alt: boolean;
  ctrl: boolean;
  meta: boolean;
  pressure?: number;
  timestamp_ms: number;
}

export interface WheelInput {
  delta_x: number;
  delta_y: number;
  screen_x: number;
  screen_y: number;
  shift: boolean;
  ctrl: boolean;
  alt: boolean;
  meta: boolean;
}

export interface KeyInput {
  kind: "down" | "up";
  key: string;
  code: string;
  shift: boolean;
  alt: boolean;
  ctrl: boolean;
  meta: boolean;
  repeat: boolean;
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

export interface NodePatch {
  name?: string;
  visible?: boolean;
  locked?: boolean;
  opacity?: number;
  blend?: string;
  transform?: Transform;
  fills?: Fill[];
  strokes?: Stroke[];
  effects?: Effect[];
  corner_radii?: CornerRadii;
  fill_rule?: FillRule;
  text?: TextContent | null;
  layout?: AutoLayout | null;
  constraints?: Constraints | null;
}

export interface ApplyPatchArgs {
  ids: string[];
  patch: NodePatch;
  label?: string;
  /** Consecutive patches with the same key merge into one undo step. */
  coalesce_key?: string;
}

/**
 * Command argument map for `invoke("command", { name, args })`.
 * Commands that are not implemented by an engine build return a structured
 * `unsupported` EngineError; the UI surfaces it and keeps running.
 */
export interface CommandArgs {
  // History
  undo: undefined;
  redo: undefined;
  delete: undefined;
  duplicate: undefined;
  copy: undefined;
  paste: undefined;
  cut: undefined;
  select_all: undefined;
  select_none: undefined;
  select_inverse: undefined;
  group: undefined;
  ungroup: undefined;
  mask_with_selection: undefined;
  create_component_set: undefined;
  detach_instance: undefined;
  swap_instance: { component_id: string };

  // Selection (Lead-accepted contract extension; unknown ids are ignored)
  select_nodes: { ids: string[]; additive?: boolean };

  // Ordering
  bring_to_front: undefined;
  bring_forward: undefined;
  send_backward: undefined;
  send_to_back: undefined;
  move_to_parent: { parent_id: string | null; index?: number };
  reorder: { node_id: string; new_parent_id: string | null; index: number };

  // View
  zoom_in: undefined;
  zoom_out: undefined;
  zoom_to: { zoom: number };
  zoom_fit: undefined;
  zoom_selection: undefined;
  zoom_100: undefined;
  /** Pan by a delta in world units (Lead-accepted). */
  pan_by: { dx: number; dy: number };
  set_viewport: { width: number; height: number; dpr: number };

  // Edit
  apply_patch: ApplyPatchArgs;
  rename_node: { id: string; name: string };
  toggle_visible: { ids?: string[] } | undefined;
  toggle_locked: { ids?: string[] } | undefined;
  set_opacity: { ids?: string[]; opacity: number };
  nudge: { dx: number; dy: number; ids?: string[] };
  align: { ids?: string[]; align: "left" | "hcenter" | "right" | "top" | "vcenter" | "bottom" };
  /** `spacing` 0 means equal gaps; otherwise a fixed gap in design px. */
  distribute: { ids?: string[]; axis: "horizontal" | "vertical"; spacing: number };
  boolean: { ids?: string[]; op: "union" | "subtract" | "intersect" | "exclude" };
  flatten: undefined;
  outline_stroke: undefined;
  text_edit_begin: { node_id: string };
  text_edit_input: { characters: string; caret: number; selection: [number, number] | null };
  text_edit_set_caret: { caret: number; selection: [number, number] | null };
  text_edit_commit: undefined;
  text_edit_cancel: undefined;

  // Pages (Lead-accepted contract extensions)
  set_active_page: { page_id: string };
  add_page: { name?: string } | undefined;
  rename_page: { page_id: string; name: string };
  /** Returns a validation error when the last remaining page is deleted. */
  delete_page: { page_id: string };

  // Prototype authoring / presentation (v1.5 contract addendum)
  add_interaction: {
    /** Defaults to the first selected node. */
    node_id?: string;
    trigger: TriggerJson;
    actions: ActionJson[] | ActionJson;
  };
  remove_interaction: { node_id?: string; interaction_id: string };
  prototype_play: { frame_id?: string } | undefined;
  prototype_stop: undefined;
  prototype_back: undefined;

  // Design system — sizes / styles / variables
  create_component: { name?: string } | undefined;
  place_instance: {
    component_id: string;
    x: number;
    y: number;
    parent_id?: string | null;
  };
  create_style_from_node: { node_id?: string; name?: string } | undefined;
  apply_style: { style_id: string; ids?: string[] };
  rename_style: { style_id: string; name: string };
  delete_style: { style_id: string };
  create_variable_collection: { name: string };
  create_color_variable: { collection?: string; name: string; color: string };
  create_float_variable: { collection?: string; name: string; value: number };
  set_variable_value: {
    variable_id: string;
    /** Plain number/string or the tagged `VariableValue` JSON. */
    value: number | string | VariableValueJson;
    /** Defaults to the collection's active mode. */
    mode_id?: string;
  };
  set_active_mode: { collection_id: string; mode_id: string };
}

export type CommandName = keyof CommandArgs;

// ---------------------------------------------------------------------------
// Export / save / bootstrap
// ---------------------------------------------------------------------------

export interface ExportRaster {
  node_ids?: string[];
  scale: number;
  transparent: boolean;
  path: string;
}

export interface ExportVector {
  node_ids?: string[];
  path: string;
}

export interface ExportPdf {
  node_ids?: string[];
  path: string;
}

export interface ExportResult {
  path: string;
  bytes: number;
  width?: number;
  height?: number;
  format: string;
}

export interface SaveResult {
  path: string;
  revision: number;
  bytes: number;
}

export interface RecentFile {
  path: string;
  name: string;
}

/**
 * Honest feature flags reported by the engine at bootstrap. Values are booleans,
 * not promises: the UI uses them to disable controls that the running engine does
 * not support yet (rather than letting them fail at call time).
 */
export interface Capabilities {
  ai: boolean;
  plugins: boolean;
  collab: boolean;
  gpu: boolean;
  dev_mode: boolean;
  export_pdf: boolean;
  /** Scoped export (selection bounds) and transparent PNG backgrounds. */
  export_scope: boolean;
  import_svg: boolean;
  import_image: boolean;
  prototype: boolean;
  canvas_text_edit: boolean;
  /** Harness-only flag; absent in the desktop engine. */
  dev_harness?: boolean;
  [capability: string]: unknown;
}

export interface Bootstrap {
  app_version: string;
  recent_files: RecentFile[];
  preferences: Record<string, unknown>;
  capabilities: Capabilities;
}

/** Fallbacks used before/without a bootstrap response; mirrors the current engine. */
export const DEFAULT_CAPABILITIES: Capabilities = {
  ai: false,
  plugins: false,
  collab: false,
  gpu: false,
  dev_mode: true,
  export_pdf: true,
  export_scope: true,
  import_svg: true,
  import_image: false,
  prototype: true,
  canvas_text_edit: true,
};

// ---------------------------------------------------------------------------
// Dev Mode (inspection, code generation, handoff, revision compare)
// Mirrors crates/dp-devmode serde output exactly (snake_case).
// ---------------------------------------------------------------------------

/** Code generator target requested by the UI (Rust parses these aliases). */
export type CodegenTarget = "css" | "react" | "swiftui" | "compose";

/** Code target value echoed back by the engine (`serde(rename_all = "snake_case")`). */
export type GeneratedCodeTarget = "css" | "react_type_script" | "swift_ui" | "compose";

export interface GradientStopInfo {
  /** Normalised position 0..=1. */
  position: number;
  color: Color;
  hex: string;
}

export interface PaintInfo {
  /** Paint kind: solid, linear, radial, angular, image. */
  kind: string;
  color: Color | null;
  hex: string | null;
  stops: GradientStopInfo[];
  opacity: number;
  blend: string;
  style_id: string | null;
  variable_id: string | null;
  variable_name: string | null;
}

export interface DevStrokeInfo {
  paint: PaintInfo;
  weight: number;
  align: string;
  cap: string;
  join: string;
  miter_limit: number;
  dash: number[] | null;
}

export interface DevEffectInfo {
  /** Effect kind: drop_shadow, inner_shadow, layer_blur, background_blur. */
  kind: string;
  color: Color | null;
  offset: [number, number] | null;
  blur: number;
  spread: number;
  blend: string | null;
}

export interface DevTextInfo {
  characters: string;
  font_family: string;
  font_size: number;
  font_weight: number;
  italic: boolean;
  line_height: number;
  letter_spacing: number;
  align: string;
  vertical_align: string;
  color: Color | null;
  overflow: boolean;
}

export interface DevComponentInfo {
  /** Role: component, component_set, instance. */
  role: string;
  component_id: string | null;
  component_name: string | null;
  override_count: number;
  inside_instance: boolean;
}

export interface NodeInspect {
  id: string;
  name: string;
  kind: string;
  page: string;
  parent: string | null;
  children: string[];
  /** Page-space axis-aligned bounds. */
  bounds: [number, number, number, number];
  size: [number, number];
  local_position: [number, number];
  rotation: number;
  flip: [boolean, boolean];
  opacity: number;
  blend: string;
  visible: boolean;
  locked: boolean;
  /** Absolute matrix `[m11, m12, m21, m22, tx, ty]`. */
  matrix: [number, number, number, number, number, number];
  fills: PaintInfo[];
  strokes: DevStrokeInfo[];
  effects: DevEffectInfo[];
  /** Corner radii `[tl, tr, br, bl]`. */
  corner_radii: [number, number, number, number];
  layout_direction: string | null;
  layout_gap: number | null;
  layout_padding: [number, number, number, number] | null;
  layout_sizing: [string, string] | null;
  constraints: [string, string] | null;
  text: DevTextInfo | null;
  component: DevComponentInfo | null;
  variable_refs: string[];
  style_refs: string[];
}

export interface InspectSnapshot {
  nodes: NodeInspect[];
  bounds: [number, number, number, number] | null;
  total_size: [number, number];
  warnings: string[];
}

export interface GeneratedCode {
  target: GeneratedCodeTarget;
  /** Language label: css, typescript, swift, kotlin. */
  language: string;
  extension: string;
  code: string;
  warnings: string[];
  requires_manual_adaptation: boolean;
  node_count: number;
}

export type AnnotationKind = "note" | "implementation" | "measurement";

export interface Annotation {
  id: string;
  node_id: string;
  node_name: string;
  body: string;
  author: string;
  created_at: string;
  kind: AnnotationKind;
  link: string | null;
  resolved: boolean;
}

export type HandoffStatus =
  | "draft"
  | "ready_for_review"
  | "ready_for_development"
  | "implemented";

export interface Handoff {
  revision: number;
  status: HandoffStatus;
  annotations: Annotation[];
  marked_ready_by: string | null;
  marked_ready_at: string | null;
}

export interface AddAnnotationArgs {
  node_id: string;
  body: string;
  author: string;
  created_at: string;
  kind?: AnnotationKind;
  link?: string;
}

export interface MarkReadyArgs {
  status: HandoffStatus;
  by: string;
  at: string;
}

export interface NodeDiff {
  id: string;
  name: string;
  kind: string;
}

export interface NodeChange {
  id: string;
  name: string;
  fields: string[];
}

export interface DiffCounts {
  added: number;
  removed: number;
  changed: number;
}

export interface RevisionDiff {
  added: NodeDiff[];
  removed: NodeDiff[];
  changed: NodeChange[];
  counts: DiffCounts;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

export const ZERO_RECT: Rect = { x: 0, y: 0, w: 0, h: 0 };

export const DEFAULT_VIEWPORT: Viewport = {
  x: 0,
  y: 0,
  zoom: 1,
  width: 0,
  height: 0,
  dpr: 1,
};

export const EMPTY_OVERLAY: Overlay = {
  selection: [],
  hover: null,
  handles: [],
  marquee: null,
  snap_lines: [],
  guides: [],
  cursors: [],
  tool_hint: null,
  text_edit: null,
};

/** Initial engine state used before the first `engine_state` response. */
export function initialEngineState(viewport: Viewport = DEFAULT_VIEWPORT): EngineState {
  return {
    doc: null,
    revision: 0,
    frame_id: 0,
    viewport,
    tool: "move",
    selection: [],
    hover: null,
    overlay: EMPTY_OVERLAY,
    can_undo: false,
    can_redo: false,
    dirty: false,
    summary: null,
    selection_details: [],
    text_edit: null,
    prototype: null,
    errors: [],
    status: null,
  };
}

/** World → screen: `screen = (world - viewport.xy) * zoom`. */
export function worldToScreen(p: Point, viewport: Viewport): Point {
  return {
    x: (p.x - viewport.x) * viewport.zoom,
    y: (p.y - viewport.y) * viewport.zoom,
  };
}

/** Screen → world: `world = viewport.xy + screen / zoom`. */
export function screenToWorld(p: Point, viewport: Viewport): Point {
  return {
    x: viewport.x + p.x / viewport.zoom,
    y: viewport.y + p.y / viewport.zoom,
  };
}

export function rectCorners(rect: Rect): [Point, Point, Point, Point] {
  return [
    { x: rect.x, y: rect.y },
    { x: rect.x + rect.w, y: rect.y },
    { x: rect.x + rect.w, y: rect.y + rect.h },
    { x: rect.x, y: rect.y + rect.h },
  ];
}
