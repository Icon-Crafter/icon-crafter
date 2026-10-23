/**
 * DEV-ONLY browser harness.
 *
 * This file is loaded exclusively by `createTransport()` when the app runs
 * OUTSIDE Tauri (`pnpm dev` in a browser) and `import.meta.env.DEV` is true.
 * It simulates the parts of the Rust engine the UI needs so the presentation
 * layer can be developed and visually reviewed without the desktop shell.
 *
 * It is NOT production behaviour: production builds either run inside Tauri
 * (real IPC) or fail loudly (see transport.ts). The status bar labels the
 * active engine so a simulated session can never be mistaken for a real one.
 *
 * Simulated here (deliberately a subset): pages, frames/shapes/text nodes,
 * selection, move/resize/rotate, pan/zoom, marquee select, undo/redo,
 * group/ungroup, copy/paste/duplicate/delete, align/distribute, reorder,
 * visibility/lock/opacity/rename, apply_patch for the inspector, basic text
 * editing, PNG/SVG downloads and localStorage-backed save/open.
 *
 * NOT simulated (returns an honest `unsupported` error, surfaced as a toast):
 * boolean operations, vector/pen editing, instances, prototype/dev-mode,
 * PDF export, image import, on-path text, auto layout.
 */

import type {
  ApplyPatchArgs,
  Bootstrap,
  Color,
  CommandArgs,
  CommandName,
  CornerRadii,
  DocKind,
  DocMeta,
  DocSummary,
  Effect,
  EngineError,
  EngineErrorCode,
  EngineState,
  ExportPdf,
  ExportRaster,
  ExportResult,
  ExportVector,
  Fill,
  Handle,
  KeyInput,
  NodeDetail,
  NodeKind,
  Overlay,
  Paint,
  PointerInput,
  Rect,
  SaveResult,
  Stroke,
  TextContent,
  ToolId,
  TreeNode,
  Viewport,
  WheelInput,
} from "./contract";
import { EMPTY_OVERLAY, screenToWorld } from "./contract";
import type {
  EngineTransport,
  PickOpenOptions,
  PickSaveOptions,
} from "./transport";
import { TransportUnavailableError } from "./transport";

// ---------------------------------------------------------------------------
// Small utilities
// ---------------------------------------------------------------------------

const MIN_ZOOM = 0.01;
const MAX_ZOOM = 64;

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function deepClone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function newId(prefix: string): string {
  const cryptoRef = globalThis.crypto as Crypto | undefined;
  const random =
    cryptoRef && typeof cryptoRef.randomUUID === "function"
      ? cryptoRef.randomUUID()
      : `${Date.now().toString(36)}-${Math.floor(Math.random() * 1e9).toString(36)}`;
  return `${prefix}_${random}`;
}

function rectIntersects(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y;
}

function unionRects(rects: readonly Rect[]): Rect | null {
  if (rects.length === 0) return null;
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const rect of rects) {
    minX = Math.min(minX, rect.x);
    minY = Math.min(minY, rect.y);
    maxX = Math.max(maxX, rect.x + rect.w);
    maxY = Math.max(maxY, rect.y + rect.h);
  }
  return { x: minX, y: minY, w: Math.max(0, maxX - minX), h: Math.max(0, maxY - minY) };
}

function solidPaint(color: Color): Paint {
  return { type: "solid", color };
}

function makeFill(color: Color): Fill {
  return {
    visible: true,
    opacity: 1,
    blend: "normal",
    paint: solidPaint(color),
    style_id: null,
    variable_id: null,
  };
}

function makeStroke(color: Color, weight: number): Stroke {
  return {
    visible: true,
    opacity: 1,
    blend: "normal",
    paint: solidPaint(color),
    style_id: null,
    variable_id: null,
    weight,
    align: "inside",
    cap: "butt",
    join: "miter",
    miter_limit: 4,
    dash: null,
  };
}

const WHITE: Color = { r: 1, g: 1, b: 1, a: 1 };
const GRAPHITE: Color = { r: 0.09, g: 0.1, b: 0.13, a: 1 };
const CORAL: Color = { r: 1, g: 0.42, b: 0.29, a: 1 };
const TEAL: Color = { r: 0.31, g: 0.83, b: 0.75, a: 1 };
const INK: Color = { r: 0.91, g: 0.93, b: 0.95, a: 1 };
const DIM_INK: Color = { r: 0.64, g: 0.67, b: 0.73, a: 1 };

function defaultCornerRadii(radius = 0): CornerRadii {
  return { tl: radius, tr: radius, br: radius, bl: radius };
}

const TOOL_HINTS: Partial<Record<ToolId, string>> = {
  frame: "Drag to draw a frame",
  slice: "Drag to define a slice",
  rectangle: "Drag to draw a rectangle",
  ellipse: "Drag to draw an ellipse",
  line: "Drag to draw a line",
  polygon: "Drag to draw a polygon",
  star: "Drag to draw a star",
  pen: "Click to place points · the pen is not simulated",
  node: "The node tool is not simulated",
  text: "Click to place a text layer",
  image: "Place image is not available in the dev harness",
  comment: "Comments are not available in the dev harness",
  connector: "Connectors are not available in the dev harness",
  hand: "Drag to pan the canvas",
};

// ---------------------------------------------------------------------------
// Dev document model
// ---------------------------------------------------------------------------

interface DevNode {
  id: string;
  name: string;
  kind: NodeKind;
  parent: string | null;
  children: string[];
  page: string;
  transform: {
    x: number;
    y: number;
    w: number;
    h: number;
    rotation: number;
    flip_x: boolean;
    flip_y: boolean;
  };
  visible: boolean;
  locked: boolean;
  opacity: number;
  blend: string;
  fills: Fill[];
  strokes: Stroke[];
  effects: Effect[];
  corner_radii: CornerRadii;
  fill_rule: "nonzero" | "evenodd";
  text: TextContent | null;
  component: NodeDetail["component"];
}

interface DevPage {
  id: string;
  name: string;
  /** Root node ids, bottom-most first (top-most last). */
  roots: string[];
}

interface DevDocument {
  meta: DocMeta;
  pages: DevPage[];
  nodes: Map<string, DevNode>;
  activePage: string;
  styles: DocSummary["styles"];
  components: DocSummary["components"];
  variables: DocSummary["variables"];
}

interface PersistedDocument {
  version: 1;
  name: string;
  path: string | null;
  pages: DevPage[];
  nodes: DevNode[];
  activePage: string;
}

interface DragState {
  mode: "none" | "move" | "marquee" | "draw" | "pan" | "resize" | "rotate";
  startWorld: { x: number; y: number };
  startScreen: { x: number; y: number };
  origin: Map<string, { x: number; y: number }>;
  moved: boolean;
  drawId: string | null;
  handleIndex: number;
  startTransform: DevNode["transform"] | null;
}

interface EditingState {
  nodeId: string;
  original: string;
  characters: string;
  caret: number;
  selection: [number, number] | null;
}

interface HistorySnapshot {
  data: string;
  selection: string[];
}

const FILE_STORAGE_PREFIX = "iconcrafter.dev.files.v1:";

function makeTextContent(characters: string, size = 16, color: Color = INK): TextContent {
  return {
    characters,
    style: {
      font_family: "Inter",
      font_weight: 500,
      italic: false,
      font_size: size,
      line_height: { unit: "auto", value: 0 },
      letter_spacing: 0,
      text_case: "original",
      decoration: "none",
      fills: [makeFill(color)],
    },
    paragraph: {
      align: "left",
      vertical_align: "top",
      max_lines: null,
      paragraph_spacing: 0,
    },
    auto_size: "none",
    on_path: null,
  };
}

// ---------------------------------------------------------------------------
// DevTransport
// ---------------------------------------------------------------------------

export class DevTransport implements EngineTransport {
  readonly kind = "dev" as const;

  private doc: DevDocument | null = null;
  private viewport: Viewport = { x: 0, y: 0, zoom: 1, width: 0, height: 0, dpr: 1 };
  private tool: ToolId = "move";
  private selection: string[] = [];
  private hover: string | null = null;
  private revision = 0;
  private frameId = 0;
  private dirty = false;
  private editing: EditingState | null = null;
  private marquee: Rect | null = null;
  private drag: DragState = this.blankDrag();
  private undoStack: HistorySnapshot[] = [];
  private redoStack: HistorySnapshot[] = [];
  private clipboard: string | null = null;
  private errors: EngineError[] = [];
  private status: string | null = null;
  private preferences: Record<string, unknown> = {};
  private recentFiles: Bootstrap["recent_files"] = [];

  constructor() {
    this.doc = this.buildDemoDocument();
  }

  // -- helpers -------------------------------------------------------------

  private blankDrag(): DragState {
    return {
      mode: "none",
      startWorld: { x: 0, y: 0 },
      startScreen: { x: 0, y: 0 },
      origin: new Map(),
      moved: false,
      drawId: null,
      handleIndex: -1,
      startTransform: null,
    };
  }

  private activePage(): DevPage | null {
    if (!this.doc) return null;
    return this.doc.pages.find((page) => page.id === this.doc?.activePage) ?? null;
  }

  private node(id: string): DevNode | null {
    return this.doc?.nodes.get(id) ?? null;
  }

  private recordError(code: EngineErrorCode, message: string, severity: "error" | "warning" = "error"): void {
    this.errors.push({ code, message, severity });
  }

  private pushHistory(): void {
    if (!this.doc) return;
    this.undoStack.push(this.historySnapshot());
    if (this.undoStack.length > 100) this.undoStack.shift();
    this.redoStack = [];
  }

  private historySnapshot(): HistorySnapshot {
    return {
      data: JSON.stringify(this.serialize()),
      selection: [...this.selection],
    };
  }

  private serialize(): PersistedDocument | null {
    if (!this.doc) return null;
    return {
      version: 1,
      name: this.doc.meta.name,
      path: this.doc.meta.path,
      pages: deepClone(this.doc.pages),
      nodes: [...this.doc.nodes.values()].map((node) => deepClone(node)),
      activePage: this.doc.activePage,
    };
  }

  private restoreSnapshot(data: string, selection: string[]): void {
    const parsed = JSON.parse(data) as PersistedDocument | null;
    if (!parsed) {
      this.doc = null;
      return;
    }
    const nodes = new Map<string, DevNode>();
    for (const node of parsed.nodes) nodes.set(node.id, node);
    this.doc = {
      meta: {
        id: this.doc?.meta.id ?? newId("doc"),
        name: parsed.name,
        path: parsed.path,
        schema: 1,
        revision: this.revision,
      },
      pages: parsed.pages,
      nodes,
      activePage: parsed.activePage,
      styles: this.doc?.styles ?? { paints: [], texts: [], effects: [] },
      components: this.doc?.components ?? [],
      variables: [],
    };
    this.selection = selection.filter((id) => nodes.has(id));
    this.editing = null;
  }

  private markMutated(): void {
    this.revision += 1;
    this.frameId += 1;
    this.dirty = true;
    if (this.doc) this.doc.meta.revision = this.revision;
  }

  private markFrameChanged(): void {
    this.frameId += 1;
  }

  private finish(): EngineState {
    const state = this.snapshot();
    this.errors = [];
    return state;
  }

  private fail(code: EngineErrorCode, message: string): EngineState {
    this.recordError(code, message);
    this.status = message;
    return this.finish();
  }

  private unsupported(message: string): EngineState {
    return this.fail("unsupported", message);
  }

  private pageOf(node: DevNode): DevPage | null {
    return this.doc?.pages.find((page) => page.id === node.page) ?? null;
  }

  private siblings(nodeId: string): string[] {
    const node = this.node(nodeId);
    if (!node || !this.doc) return [];
    if (node.parent) return this.node(node.parent)?.children ?? [];
    return this.pageOf(node)?.roots ?? [];
  }

  private rootRect(node: DevNode): Rect {
    return { x: node.transform.x, y: node.transform.y, w: node.transform.w, h: node.transform.h };
  }

  private nodeBounds(node: DevNode): Rect {
    return this.rootRect(node);
  }

  private subtreeIds(id: string): string[] {
    const out: string[] = [];
    const visit = (nodeId: string): void => {
      out.push(nodeId);
      const node = this.node(nodeId);
      if (!node) return;
      for (const child of node.children) visit(child);
    };
    visit(id);
    return out;
  }

  private translate(id: string, dx: number, dy: number): void {
    const node = this.node(id);
    if (!node) return;
    node.transform.x += dx;
    node.transform.y += dy;
    for (const child of node.children) this.translate(child, dx, dy);
  }

  /** Top-most first pre-order of the active page, for layers + hit testing. */
  private orderedNodes(): DevNode[] {
    const page = this.activePage();
    if (!page || !this.doc) return [];
    const out: DevNode[] = [];
    const visit = (id: string): void => {
      const node = this.node(id);
      if (!node) return;
      out.push(node);
      for (let i = node.children.length - 1; i >= 0; i -= 1) {
        const child = node.children[i];
        if (child) visit(child);
      }
    };
    for (let i = page.roots.length - 1; i >= 0; i -= 1) {
      const root = page.roots[i];
      if (root) visit(root);
    }
    return out;
  }

  private hitTest(world: { x: number; y: number }): string | null {
    for (const node of this.orderedNodes()) {
      if (!node.visible || node.locked) continue;
      const rect = this.rootRect(node);
      if (
        world.x >= rect.x &&
        world.x <= rect.x + rect.w &&
        world.y >= rect.y &&
        world.y <= rect.y + rect.h
      ) {
        return node.id;
      }
    }
    return null;
  }

  private hitTestDeep(world: { x: number; y: number }): string | null {
    for (const node of this.orderedNodes()) {
      if (!node.visible || node.locked) continue;
      const rect = this.rootRect(node);
      if (
        world.x >= rect.x &&
        world.x <= rect.x + rect.w &&
        world.y >= rect.y &&
        world.y <= rect.y + rect.h
      ) {
        // Prefer the deepest visible child that also contains the point.
        for (let i = node.children.length - 1; i >= 0; i -= 1) {
          const childId = node.children[i];
          if (!childId) continue;
          const deeper = this.hitTestDeepIn(childId, world);
          if (deeper) return deeper;
        }
        return node.id;
      }
    }
    return null;
  }

  private hitTestDeepIn(id: string, world: { x: number; y: number }): string | null {
    const node = this.node(id);
    if (!node || !node.visible || node.locked) return null;
    const rect = this.rootRect(node);
    const inside =
      world.x >= rect.x &&
      world.x <= rect.x + rect.w &&
      world.y >= rect.y &&
      world.y <= rect.y + rect.h;
    if (!inside) return null;
    for (let i = node.children.length - 1; i >= 0; i -= 1) {
      const childId = node.children[i];
      if (!childId) continue;
      const deeper = this.hitTestDeepIn(childId, world);
      if (deeper) return deeper;
    }
    return node.id;
  }

  private selectionBounds(): Rect | null {
    const rects = this.selection
      .map((id) => this.node(id))
      .filter((node): node is DevNode => node !== null)
      .map((node) => this.nodeBounds(node));
    return unionRects(rects);
  }

  private pageBounds(): Rect | null {
    const page = this.activePage();
    if (!page) return null;
    const rects = page.roots
      .map((id) => this.node(id))
      .filter((node): node is DevNode => node !== null)
      .map((node) => this.nodeBounds(node));
    return unionRects(rects);
  }

  /**
   * Engine v1 export semantics: explicit node ids, else the current selection,
   * else the whole page (deselect to export the page).
   */
  private exportBounds(nodeIds?: readonly string[]): Rect | null {
    const ids = this.exportNodeIds(nodeIds);
    if (!ids) return this.pageBounds();
    const rects = ids
      .map((id) => this.node(id))
      .filter((node): node is DevNode => node !== null)
      .map((node) => this.nodeBounds(node));
    return unionRects(rects) ?? this.pageBounds();
  }

  private exportNodeIds(nodeIds?: readonly string[]): string[] | null {
    const ids = nodeIds && nodeIds.length > 0 ? [...nodeIds] : [...this.selection];
    return ids.length > 0 ? ids : null;
  }

  private select(ids: string[], additive = false): void {
    const valid = ids.filter((id) => this.node(id) !== null);
    if (additive) {
      const set = new Set(this.selection);
      for (const id of valid) {
        if (set.has(id)) set.delete(id);
        else set.add(id);
      }
      this.selection = [...set];
    } else {
      this.selection = valid;
    }
    if (this.selection.length === 0) this.status = "Nothing selected";
  }

  private createNode(options: {
    name: string;
    kind: NodeKind;
    rect: Rect;
    page: string;
    parent?: string | null;
    fills?: Fill[];
    strokes?: Stroke[];
    text?: TextContent | null;
    cornerRadii?: CornerRadii;
  }): DevNode {
    const node: DevNode = {
      id: newId("node"),
      name: options.name,
      kind: options.kind,
      parent: options.parent ?? null,
      children: [],
      page: options.page,
      transform: {
        x: options.rect.x,
        y: options.rect.y,
        w: options.rect.w,
        h: options.rect.h,
        rotation: 0,
        flip_x: false,
        flip_y: false,
      },
      visible: true,
      locked: false,
      opacity: 1,
      blend: "normal",
      fills: options.fills ?? [],
      strokes: options.strokes ?? [],
      effects: [],
      corner_radii: options.cornerRadii ?? defaultCornerRadii(0),
      fill_rule: "nonzero",
      text: options.text ?? null,
      component: null,
    };
    this.doc?.nodes.set(node.id, node);
    return node;
  }

  private insertNode(node: DevNode, parentId: string | null): void {
    if (!this.doc) return;
    if (parentId) {
      const parent = this.node(parentId);
      if (parent) {
        parent.children.push(node.id);
        node.parent = parentId;
        return;
      }
    }
    const page = this.activePage();
    if (page) page.roots.push(node.id);
    node.parent = null;
  }

  // -- demo document -------------------------------------------------------

  private buildDemoDocument(): DevDocument {
    const pageA: DevPage = { id: newId("page"), name: "Page 1", roots: [] };
    const pageB: DevPage = { id: newId("page"), name: "Explorations", roots: [] };
    const doc: DevDocument = {
      meta: { id: newId("doc"), name: "Untitled", path: null, schema: 1, revision: 0 },
      pages: [pageA, pageB],
      nodes: new Map(),
      activePage: pageA.id,
      styles: { paints: [], texts: [], effects: [] },
      components: [],
      variables: [],
    };
    this.doc = doc;

    const iconFrame = this.createNode({
      name: "App icon",
      kind: "frame",
      rect: { x: 48, y: 64, w: 640, h: 400 },
      page: pageA.id,
      fills: [makeFill(WHITE)],
      cornerRadii: defaultCornerRadii(8),
    });
    pageA.roots.push(iconFrame.id);

    const tile = this.createNode({
      name: "Tile",
      kind: "rectangle",
      rect: { x: 32, y: 32, w: 336, h: 336 },
      page: pageA.id,
      parent: iconFrame.id,
      fills: [makeFill({ r: 0.07, g: 0.08, b: 0.11, a: 1 })],
      cornerRadii: defaultCornerRadii(68),
    });
    iconFrame.children.push(tile.id);

    const spark = this.createNode({
      name: "Spark",
      kind: "star",
      rect: { x: 148, y: 40, w: 104, h: 104 },
      page: pageA.id,
      parent: iconFrame.id,
      fills: [makeFill(CORAL)],
    });
    tile.children.push(spark.id);
    spark.parent = tile.id;

    // Recompute the star inside the tile: keep the demo readable.
    spark.transform = { x: 130, y: 145, w: 108, h: 108, rotation: 0, flip_x: false, flip_y: false };

    const circle = this.createNode({
      name: "Orbit",
      kind: "ellipse",
      rect: { x: 398, y: 96, w: 168, h: 168 },
      page: pageA.id,
      parent: iconFrame.id,
      fills: [makeFill(TEAL)],
    });
    iconFrame.children.push(circle.id);

    const title = this.createNode({
      name: "Wordmark",
      kind: "text",
      rect: { x: 398, y: 286, w: 210, h: 40 },
      page: pageA.id,
      parent: iconFrame.id,
      fills: [],
      text: makeTextContent("Icon Crafter", 28, GRAPHITE),
    });
    iconFrame.children.push(title.id);

    const caption = this.createNode({
      name: "Caption",
      kind: "text",
      rect: { x: 400, y: 326, w: 200, h: 22 },
      page: pageA.id,
      parent: iconFrame.id,
      fills: [],
      text: makeTextContent("Design system in a box", 13, DIM_INK),
    });
    iconFrame.children.push(caption.id);

    const board = this.createNode({
      name: "Concept board",
      kind: "frame",
      rect: { x: 48, y: 520, w: 640, h: 240 },
      page: pageA.id,
      fills: [makeFill({ r: 0.95, g: 0.96, b: 0.97, a: 1 })],
      cornerRadii: defaultCornerRadii(12),
    });
    pageA.roots.push(board.id);
    for (let i = 0; i < 3; i += 1) {
      const swatch = this.createNode({
        name: `Swatch ${i + 1}`,
        kind: "rectangle",
        rect: { x: 40 + i * 190, y: 40, w: 150, h: 160 },
        page: pageA.id,
        parent: board.id,
        fills: [makeFill([CORAL, TEAL, { r: 0.62, g: 0.55, b: 1, a: 1 }][i] ?? CORAL)],
        cornerRadii: defaultCornerRadii(16),
      });
      board.children.push(swatch.id);
    }

    // Second page: an empty exploration canvas with one dark frame.
    const darkFrame = this.createNode({
      name: "Night icon",
      kind: "frame",
      rect: { x: 64, y: 64, w: 256, h: 256 },
      page: pageB.id,
      fills: [makeFill(GRAPHITE)],
      cornerRadii: defaultCornerRadii(32),
    });
    pageB.roots.push(darkFrame.id);
    const nightDot = this.createNode({
      name: "Glow",
      kind: "ellipse",
      rect: { x: 88, y: 88, w: 80, h: 80 },
      page: pageB.id,
      parent: darkFrame.id,
      fills: [makeFill(TEAL)],
    });
    darkFrame.children.push(nightDot.id);

    this.revision = 1;
    this.frameId = 1;
    this.dirty = false;
    if (doc.meta.revision !== this.revision) doc.meta.revision = this.revision;
    return doc;
  }

  private emptyDocument(kind: DocKind): void {
    const page: DevPage = { id: newId("page"), name: "Page 1", roots: [] };
    this.doc = {
      meta: {
        id: newId("doc"),
        name: "Untitled",
        path: null,
        schema: 1,
        revision: this.revision + 1,
      },
      pages: [page],
      nodes: new Map(),
      activePage: page.id,
      styles: { paints: [], texts: [], effects: [] },
      components: [],
      variables: [],
      ...(kind === "design" ? {} : {}),
    };
    this.selection = [];
    this.hover = null;
    this.editing = null;
    this.revision += 1;
    this.frameId += 1;
    this.dirty = false;
    this.undoStack = [];
    this.redoStack = [];
    this.viewport = { ...this.viewport, x: 0, y: 0, zoom: 1 };
    this.status = `New ${kind} document`;
  }

  // -- EngineTransport -----------------------------------------------------

  bootstrap(): Promise<Bootstrap> {
    return Promise.resolve({
      app_version: "0.1.0-dev",
      recent_files: [...this.recentFiles],
      preferences: { ...this.preferences },
      capabilities: {
        ai: false,
        plugins: false,
        collab: false,
        gpu: false,
        dev_mode: true,
        dev_harness: true,
        export_pdf: false,
        export_scope: true,
        import_svg: false,
        import_image: false,
        prototype: false,
        canvas_text_edit: true,
      },
    });
  }

  newDocument(kind: DocKind): Promise<EngineState> {
    this.emptyDocument(kind);
    return Promise.resolve(this.finish());
  }

  openDocument(path: string): Promise<EngineState> {
    const stored = this.loadPersisted(path);
    if (stored) {
      this.restoreSnapshot(JSON.stringify(stored), []);
      this.doc = this.doc ?? null;
      if (this.doc) {
        this.doc.meta.path = path;
        this.doc.meta.name = stored.name || this.nameFromPath(path);
      }
      this.dirty = false;
      this.revision += 1;
      this.frameId += 1;
      this.status = `Opened ${path}`;
      if (!this.recentFiles.some((file) => file.path === path)) {
        this.recentFiles = [{ path, name: this.nameFromPath(path) }, ...this.recentFiles].slice(0, 20);
      }
      return Promise.resolve(this.finish());
    }
    this.buildDemoDocument();
    if (this.doc) {
      this.doc.meta.path = path;
      this.doc.meta.name = this.nameFromPath(path);
    }
    this.selection = [];
    this.status = `Opened ${path} (sample content)`;
    return Promise.resolve(this.finish());
  }

  closeDocument(): Promise<EngineState> {
    this.doc = null;
    this.selection = [];
    this.hover = null;
    this.editing = null;
    this.dirty = false;
    this.undoStack = [];
    this.redoStack = [];
    this.status = "Document closed";
    return Promise.resolve(this.finish());
  }

  saveDocument(path?: string): Promise<SaveResult> {
    if (!this.doc) return Promise.reject(new Error("no document is open"));
    const target = path ?? this.doc.meta.path;
    if (!target) {
      return Promise.reject(
        new TransportUnavailableError("Choose a destination before saving (no path set)."),
      );
    }
    const serialized = this.serialize();
    if (!serialized) return Promise.reject(new Error("no document is open"));
    serialized.path = target;
    const data = JSON.stringify(serialized);
    this.persist(target, data);
    this.doc.meta.path = target;
    this.doc.meta.name = this.nameFromPath(target);
    this.dirty = false;
    this.status = `Saved ${target}`;
    if (!this.recentFiles.some((file) => file.path === target)) {
      this.recentFiles = [{ path: target, name: this.doc.meta.name }, ...this.recentFiles].slice(0, 20);
    }
    return Promise.resolve({
      path: target,
      revision: this.revision,
      bytes: new Blob([data]).size,
    });
  }

  engineState(): Promise<EngineState> {
    return Promise.resolve(this.snapshot());
  }

  setTool(tool: ToolId): Promise<EngineState> {
    this.tool = tool;
    this.status = `Tool: ${tool}`;
    return Promise.resolve(this.snapshot());
  }

  pointer(input: PointerInput): Promise<EngineState> {
    if (!this.doc) return Promise.resolve(this.finish());
    const screen = { x: input.screen_x, y: input.screen_y };
    const world = screenToWorld(screen, this.viewport);
    switch (input.kind) {
      case "down":
        this.pointerDown(input, world);
        break;
      case "move":
        this.pointerMove(input, world);
        break;
      case "up":
        this.pointerUp(world);
        break;
      case "double_click":
        this.pointerDoubleClick(world);
        break;
      case "leave":
        this.hover = null;
        break;
      default:
        break;
    }
    return Promise.resolve(this.finish());
  }

  wheel(input: WheelInput): Promise<EngineState> {
    const screen = { x: input.screen_x, y: input.screen_y };
    const zoomModifier = input.ctrl || input.meta;
    if (zoomModifier) {
      const before = screenToWorld(screen, this.viewport);
      const factor = Math.exp(-input.delta_y * 0.0015);
      const zoom = clamp(this.viewport.zoom * factor, MIN_ZOOM, MAX_ZOOM);
      this.viewport.zoom = zoom;
      this.viewport.x = before.x - screen.x / zoom;
      this.viewport.y = before.y - screen.y / zoom;
    } else {
      const dx = input.shift ? input.delta_y : input.delta_x;
      const dy = input.shift ? 0 : input.delta_y;
      this.viewport.x += dx / this.viewport.zoom;
      this.viewport.y += dy / this.viewport.zoom;
    }
    this.markFrameChanged();
    return Promise.resolve(this.finish());
  }

  key(input: KeyInput): Promise<EngineState> {
    if (input.kind === "down" && input.key === "Escape" && this.editing) {
      this.cancelTextEdit();
    }
    return Promise.resolve(this.snapshot());
  }

  command<C extends CommandName>(name: C, args?: CommandArgs[C]): Promise<EngineState> {
    if (!this.doc && !["undo", "redo"].includes(name)) {
      return Promise.resolve(this.fail("validation", "No document is open."));
    }
    const result = this.runCommand(name, args);
    return Promise.resolve(result);
  }

  async renderFrame(_quality = 1): Promise<ArrayBuffer> {
    const width = Math.max(1, Math.round(this.viewport.width * this.viewport.dpr));
    const height = Math.max(1, Math.round(this.viewport.height * this.viewport.dpr));
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("2D canvas context unavailable");
    ctx.clearRect(0, 0, width, height);
    this.renderTo(ctx, width, height);
    const data = ctx.getImageData(0, 0, width, height).data;
    return data.buffer.slice(0) as ArrayBuffer;
  }

  nodeDetails(ids: string[]): Promise<NodeDetail[]> {
    const details: NodeDetail[] = [];
    for (const id of ids) {
      const node = this.node(id);
      if (node) details.push(this.detailOf(node));
    }
    return Promise.resolve(details);
  }

  async exportPng(args: ExportRaster): Promise<ExportResult> {
    const bounds = this.exportBounds(args.node_ids);
    if (!bounds) throw new TransportUnavailableError("Nothing to export on this page.");
    const onlyIds = this.exportNodeIds(args.node_ids);
    const scale = clamp(args.scale || 1, 0.01, 16);
    const width = Math.max(1, Math.round(bounds.w * scale));
    const height = Math.max(1, Math.round(bounds.h * scale));
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("2D canvas context unavailable");
    if (!args.transparent) {
      ctx.fillStyle = "#ffffff";
      ctx.fillRect(0, 0, width, height);
    }
    ctx.save();
    ctx.scale(scale, scale);
    ctx.translate(-bounds.x, -bounds.y);
    this.drawPage(ctx, onlyIds);
    ctx.restore();
    const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
    if (!blob) throw new Error("PNG encoding failed");
    this.downloadFile(blob, args.path || "icon-crafter-export.png");
    return { path: args.path, bytes: blob.size, width, height, format: "png" };
  }

  exportSvg(args: ExportVector): Promise<ExportResult> {
    const bounds = this.exportBounds(args.node_ids);
    if (!bounds) throw new TransportUnavailableError("Nothing to export on this page.");
    const onlyIds = this.exportNodeIds(args.node_ids);
    const svg = this.buildSvg(bounds, onlyIds);
    const blob = new Blob([svg], { type: "image/svg+xml" });
    this.downloadFile(blob, args.path || "icon-crafter-export.svg");
    return Promise.resolve({ path: args.path, bytes: blob.size, width: bounds.w, height: bounds.h, format: "svg" });
  }

  exportPdf(_args: ExportPdf): Promise<ExportResult> {
    // Browsers cannot encode PDF from the harness; say so honestly.
    return Promise.reject(
      Object.assign(new Error("PDF export needs the desktop engine (not available in the dev harness)."), {
        code: "unsupported",
        severity: "error",
      }),
    );
  }

  importSvg(_args: { path: string; page_id?: string; parent_id?: string }): Promise<EngineState> {
    return Promise.resolve(
      this.unsupported("SVG import isn't simulated by the dev harness. Use the desktop app."),
    );
  }

  importImage(_args: { path: string }): Promise<EngineState> {
    return Promise.resolve(
      this.unsupported("Image import isn't simulated by the dev harness. Use the desktop app."),
    );
  }

  setPreference(key: string, value: unknown): Promise<Record<string, unknown>> {
    this.preferences = { ...this.preferences, [key]: value };
    return Promise.resolve({ ...this.preferences });
  }

  pickSavePath(options: PickSaveOptions): Promise<string | null> {
    const name = (options.defaultPath ?? "Icon Crafter document").replace(/[\\/]/g, "-");
    return Promise.resolve(`dev-harness/${name}.iconcrafter`);
  }

  pickOpenPath(_options?: PickOpenOptions): Promise<string | null> {
    return Promise.resolve("dev-harness/sample.iconcrafter");
  }

  // -- state snapshot ------------------------------------------------------

  private snapshot(): EngineState {
    const doc = this.doc;
    const selection = [...this.selection];
    const state: EngineState = {
      doc: doc ? deepClone(doc.meta) : null,
      revision: this.revision,
      frame_id: this.frameId,
      viewport: { ...this.viewport },
      tool: this.tool,
      selection,
      hover: this.hover,
      overlay: this.buildOverlay(selection),
      can_undo: this.undoStack.length > 0,
      can_redo: this.redoStack.length > 0,
      dirty: this.dirty,
      summary: this.buildSummary(),
      selection_details: selection
        .map((id) => this.node(id))
        .filter((node): node is DevNode => node !== null)
        .map((node) => this.detailOf(node)),
      text_edit: this.buildTextEditState(),
      errors: [...this.errors],
      status: this.status,
    };
    this.errors = [];
    return state;
  }

  private detailOf(node: DevNode): NodeDetail {
    return {
      id: node.id,
      name: node.name,
      kind: node.kind,
      parent: node.parent,
      transform: { ...node.transform },
      visible: node.visible,
      locked: node.locked,
      opacity: node.opacity,
      blend: node.blend,
      fills: deepClone(node.fills),
      strokes: deepClone(node.strokes),
      effects: deepClone(node.effects),
      corner_radii: { ...node.corner_radii },
      fill_rule: node.fill_rule,
      text: deepClone(node.text),
      layout: null,
      constraints: null,
      component: node.component ? deepClone(node.component) : null,
      is_inside_instance: false,
      absolute_bounds: this.nodeBounds(node),
      resolved: null,
    };
  }

  private detailOfPagePage(node: DevNode): TreeNode {
    return {
      id: node.id,
      name: node.name,
      kind: node.kind,
      depth: this.depthOf(node),
      parent: node.parent,
      visible: node.visible,
      locked: node.locked,
      child_count: node.children.length,
      is_component: node.kind === "component" || node.kind === "component_set",
      is_instance: node.kind === "instance",
      frame: this.nodeBounds(node),
    };
  }

  private depthOf(node: DevNode): number {
    let depth = 0;
    let current: DevNode | null = node;
    while (current?.parent) {
      depth += 1;
      current = this.node(current.parent);
    }
    return depth;
  }

  private buildSummary(): DocSummary | null {
    if (!this.doc) return null;
    const nodes = this.orderedNodes().map((node) => this.detailOfPagePage(node));
    return {
      pages: this.doc.pages.map((page) => ({ id: page.id, name: page.name })),
      active_page: this.doc.activePage,
      nodes,
      styles: deepClone(this.doc.styles),
      components: deepClone(this.doc.components),
      variables: [],
    };
  }

  private cornersOf(node: DevNode): [ { x: number; y: number }, { x: number; y: number }, { x: number; y: number }, { x: number; y: number } ] {
    const { x, y, w, h, rotation } = node.transform;
    const cx = x + w / 2;
    const cy = y + h / 2;
    const radians = (rotation * Math.PI) / 180;
    const cos = Math.cos(radians);
    const sin = Math.sin(radians);
    const rotate = (px: number, py: number): { x: number; y: number } => {
      const dx = px - cx;
      const dy = py - cy;
      return { x: cx + dx * cos - dy * sin, y: cy + dx * sin + dy * cos };
    };
    return [
      rotate(x, y),
      rotate(x + w, y),
      rotate(x + w, y + h),
      rotate(x, y + h),
    ];
  }

  private handlesFor(selection: string[]): Handle[] {
    if (selection.length !== 1) return [];
    const nodeId = selection[0];
    if (!nodeId) return [];
    const node = this.node(nodeId);
    if (!node) return [];
    const { x, y, w, h } = node.transform;
    const corner = (px: number, py: number, cursor: string): Handle => ({
      kind: "corner",
      pos: { x: px, y: py },
      cursor,
    });
    const edge = (px: number, py: number, cursor: string): Handle => ({
      kind: "edge",
      pos: { x: px, y: py },
      cursor,
    });
    return [
      corner(x, y, "nwse-resize"),
      edge(x + w / 2, y, "ns-resize"),
      corner(x + w, y, "nesw-resize"),
      edge(x + w, y + h / 2, "ew-resize"),
      corner(x + w, y + h, "nwse-resize"),
      edge(x + w / 2, y + h, "ns-resize"),
      corner(x, y + h, "nesw-resize"),
      edge(x, y + h / 2, "ew-resize"),
      { kind: "rotate", pos: { x: x + w / 2, y: y - 22 / this.viewport.zoom }, cursor: "grab" },
    ];
  }

  private selectionBoxes(ids: string[]): EngineState["overlay"]["selection"] {
    return ids
      .map((id) => this.node(id))
      .filter((node): node is DevNode => node !== null)
      .map((node) => ({ id: node.id, corners: this.cornersOf(node) }));
  }

  private buildOverlay(selection: string[]): Overlay {
    const overlay: Overlay = {
      ...deepClone(EMPTY_OVERLAY),
      selection: this.selectionBoxes(selection),
      handles: this.handlesFor(selection),
      marquee: this.marquee ? { ...this.marquee } : null,
      tool_hint: this.tool === "move" || this.tool === "hand" ? null : TOOL_HINTS[this.tool] ?? null,
    };
    if (this.hover && !selection.includes(this.hover)) {
      const node = this.node(this.hover);
      if (node) overlay.hover = { id: node.id, corners: this.cornersOf(node) };
    }
    if (this.editing) {
      const node = this.node(this.editing.nodeId);
      if (node) {
        const screen = this.worldRectToScreen(this.nodeBounds(node));
        const fontSize = node.text?.style.font_size ?? 16;
        const caretX = 3 + this.editing.caret * fontSize * 0.55;
        overlay.text_edit = {
          caret: { x: screen.x + caretX, y: screen.y + 2 },
          selection: this.editing.selection ?? [this.editing.caret, this.editing.caret],
        };
      }
    }
    return overlay;
  }

  private worldRectToScreen(rect: Rect): Rect {
    return {
      x: (rect.x - this.viewport.x) * this.viewport.zoom,
      y: (rect.y - this.viewport.y) * this.viewport.zoom,
      w: rect.w * this.viewport.zoom,
      h: rect.h * this.viewport.zoom,
    };
  }

  private buildTextEditState(): EngineState["text_edit"] {
    if (!this.editing) return null;
    const node = this.node(this.editing.nodeId);
    if (!node || !node.text) return null;
    const screenRect = this.worldRectToScreen(this.nodeBounds(node));
    const style = node.text.style;
    const fill = style.fills[0];
    const color =
      fill && fill.paint.type === "solid" ? fill.paint.color : { r: 1, g: 1, b: 1, a: 1 };
    return {
      node_id: node.id,
      screen_rect: screenRect,
      characters: this.editing.characters,
      caret: this.editing.caret,
      selection: this.editing.selection,
      font: {
        family: style.font_family,
        weight: style.font_weight,
        italic: style.italic,
        size: style.font_size,
        line_height:
          style.line_height.unit === "px"
            ? style.line_height.value
            : style.font_size * 1.3,
        letter_spacing: style.letter_spacing,
        align: node.text.paragraph.align,
      },
      color,
    };
  }

  // -- pointer interaction -------------------------------------------------

  private pointerDown(input: PointerInput, world: { x: number; y: number }): void {
    this.hover = this.hitTest(world);
    if (this.editing) {
      this.setCaretFromWorld(world);
      return;
    }
    if (input.button === 1 || this.tool === "hand") {
      this.drag = {
        ...this.blankDrag(),
        mode: "pan",
        startScreen: { x: input.screen_x, y: input.screen_y },
        startWorld: world,
      };
      return;
    }
    if (this.tool !== "move") {
      if (this.tool === "text") {
        this.createTextAt(world);
        return;
      }
      this.beginDraw(world);
      return;
    }

    if (this.selection.length === 1) {
      const handle = this.hitHandle(world);
      if (handle >= 0) {
        const node = this.selection[0] ? this.node(this.selection[0]) : null;
        if (node) {
          this.pushHistory();
          this.drag = {
            ...this.blankDrag(),
            mode: handle === 8 ? "rotate" : "resize",
            startWorld: world,
            startScreen: { x: input.screen_x, y: input.screen_y },
            handleIndex: handle,
            startTransform: { ...node.transform },
          };
          return;
        }
      }
    }

    const hit = this.hitTestDeep(world);
    if (hit) {
      if (input.shift) this.select([hit], true);
      else if (!this.selection.includes(hit)) this.select([hit]);
      else if (this.selection.length > 1) this.select([hit]);
      this.pushHistory();
      const origin = new Map<string, { x: number; y: number }>();
      for (const id of this.selection) {
        const node = this.node(id);
        if (node) origin.set(id, { x: node.transform.x, y: node.transform.y });
      }
      this.drag = {
        ...this.blankDrag(),
        mode: "move",
        startWorld: world,
        startScreen: { x: input.screen_x, y: input.screen_y },
        origin,
      };
      return;
    }

    if (!input.shift) this.select([]);
    this.drag = {
      ...this.blankDrag(),
      mode: "marquee",
      startWorld: world,
      startScreen: { x: input.screen_x, y: input.screen_y },
    };
    this.marquee = { x: world.x, y: world.y, w: 0, h: 0 };
  }

  private pointerMove(input: PointerInput, world: { x: number; y: number }): void {
    if (this.drag.mode === "none") {
      this.hover = this.hitTest(world);
      return;
    }
    const dx = world.x - this.drag.startWorld.x;
    const dy = world.y - this.drag.startWorld.y;
    if (Math.abs(dx) > 0.01 || Math.abs(dy) > 0.01) this.drag.moved = true;
    switch (this.drag.mode) {
      case "pan": {
        this.viewport.x -= (input.screen_x - this.drag.startScreen.x) / this.viewport.zoom;
        this.viewport.y -= (input.screen_y - this.drag.startScreen.y) / this.viewport.zoom;
        this.drag.startScreen = { x: input.screen_x, y: input.screen_y };
        this.markFrameChanged();
        return;
      }
      case "move": {
        for (const [id, start] of this.drag.origin) {
          const node = this.node(id);
          if (!node) continue;
          const targetX = input.shift ? start.x : Math.round(start.x + dx);
          const targetY = input.shift ? start.y : Math.round(start.y + dy);
          this.translate(id, targetX - node.transform.x, targetY - node.transform.y);
        }
        this.markMutated();
        return;
      }
      case "marquee": {
        this.marquee = this.normalizedRect(this.drag.startWorld, world);
        return;
      }
      case "draw": {
        const node = this.drag.drawId ? this.node(this.drag.drawId) : null;
        if (!node) return;
        const rect = this.normalizedRect(this.drag.startWorld, world);
        node.transform.x = rect.x;
        node.transform.y = rect.y;
        node.transform.w = Math.max(1, rect.w);
        node.transform.h = Math.max(1, rect.h);
        this.markMutated();
        return;
      }
      case "resize": {
        this.resizeSelection(world, input.shift);
        return;
      }
      case "rotate": {
        this.rotateSelection(world, input.shift);
        return;
      }
      default:
        return;
    }
  }

  private pointerUp(world: { x: number; y: number }): void {
    const mode = this.drag.mode;
    if (mode === "marquee") {
      const marquee = this.marquee;
      if (marquee) {
        const hits = this.activePage()?.roots
          .map((id) => this.node(id))
          .filter((node): node is DevNode => node !== null)
          .filter((node) => node.visible && rectIntersects(this.nodeBounds(node), marquee))
          .map((node) => node.id);
        this.select(hits ?? [], false);
      }
      this.marquee = null;
    }
    if (mode === "draw") {
      const node = this.drag.drawId ? this.node(this.drag.drawId) : null;
      if (node) {
        if (node.transform.w < 4 && node.transform.h < 4) {
          node.transform.w = 100;
          node.transform.h = 100;
        }
        this.select([node.id]);
        this.status = `${this.labelForKind(node.kind)} created`;
        this.tool = "move";
      }
    }
    if (mode === "move" && !this.drag.moved) {
      // A click without movement is already applied on pointer down.
    }
    this.drag = this.blankDrag();
    this.hover = this.hitTest(world);
    void world;
  }

  private pointerDoubleClick(world: { x: number; y: number }): void {
    const hit = this.hitTestDeep(world);
    if (!hit) return;
    const node = this.node(hit);
    if (!node) return;
    if (node.kind === "text") {
      this.beginTextEdit(node.id);
      return;
    }
    this.select([hit]);
    this.status = node.children.length > 0 ? "Double-click selected a container" : null;
  }

  private beginDraw(world: { x: number; y: number }): void {
    const page = this.activePage();
    if (!page || !this.doc) return;
    const kind = this.drawKindForTool(this.tool);
    if (!kind) return;
    this.pushHistory();
    const rect = { x: Math.round(world.x), y: Math.round(world.y), w: 1, h: 1 };
    const node = this.createNode({
      name: this.labelForKind(kind),
      kind,
      rect,
      page: page.id,
      fills:
        kind === "frame"
          ? [makeFill(WHITE)]
          : kind === "line"
            ? []
            : [makeFill(kind === "ellipse" ? TEAL : CORAL)],
      strokes: kind === "line" ? [makeStroke(INK, 2)] : [],
      cornerRadii: kind === "frame" ? defaultCornerRadii(8) : defaultCornerRadii(0),
    });
    page.roots.push(node.id);
    this.drag = {
      ...this.blankDrag(),
      mode: "draw",
      startWorld: world,
      startScreen: { x: world.x, y: world.y },
      drawId: node.id,
    };
    this.markMutated();
  }

  private createTextAt(world: { x: number; y: number }): void {
    const page = this.activePage();
    if (!page) return;
    this.pushHistory();
    const node = this.createNode({
      name: "Text",
      kind: "text",
      rect: { x: Math.round(world.x), y: Math.round(world.y - 10), w: 220, h: 28 },
      page: page.id,
      fills: [],
      text: makeTextContent("Type something", 20, INK),
    });
    page.roots.push(node.id);
    this.select([node.id]);
    this.markMutated();
    this.tool = "move";
    this.beginTextEdit(node.id);
  }

  private normalizedRect(a: { x: number; y: number }, b: { x: number; y: number }): Rect {
    return {
      x: Math.min(a.x, b.x),
      y: Math.min(a.y, b.y),
      w: Math.abs(b.x - a.x),
      h: Math.abs(b.y - a.y),
    };
  }

  private hitHandle(world: { x: number; y: number }): number {
    const handles = this.handlesFor(this.selection);
    const threshold = 7 / this.viewport.zoom;
    for (let i = handles.length - 1; i >= 0; i -= 1) {
      const handle = handles[i];
      if (!handle) continue;
      if (Math.abs(handle.pos.x - world.x) <= threshold && Math.abs(handle.pos.y - world.y) <= threshold) {
        return i;
      }
    }
    return -1;
  }

  private resizeSelection(world: { x: number; y: number }, shift: boolean): void {
    const node = this.selection[0] ? this.node(this.selection[0]) : null;
    const start = this.drag.startTransform;
    if (!node || !start) return;
    const dx = world.x - this.drag.startWorld.x;
    const dy = world.y - this.drag.startWorld.y;
    let { x, y, w, h } = start;
    switch (this.drag.handleIndex) {
      case 0:
        x += dx;
        y += dy;
        w -= dx;
        h -= dy;
        break;
      case 1:
        y += dy;
        h -= dy;
        break;
      case 2:
        y += dy;
        w += dx;
        h -= dy;
        break;
      case 3:
        w += dx;
        break;
      case 4:
        w += dx;
        h += dy;
        break;
      case 5:
        h += dy;
        break;
      case 6:
        x += dx;
        w -= dx;
        h += dy;
        break;
      case 7:
        x += dx;
        w -= dx;
        break;
      default:
        break;
    }
    if (shift && w > 0 && h > 0 && start.w > 0 && start.h > 0) {
      const aspect = start.w / start.h;
      if (Math.abs(dx) >= Math.abs(dy)) h = w / aspect;
      else w = h * aspect;
    }
    w = Math.max(1, Math.round(w));
    h = Math.max(1, Math.round(h));
    node.transform = { ...node.transform, x: Math.round(x), y: Math.round(y), w, h };
    this.markMutated();
  }

  private rotateSelection(world: { x: number; y: number }, shift: boolean): void {
    const node = this.selection[0] ? this.node(this.selection[0]) : null;
    if (!node) return;
    const cx = node.transform.x + node.transform.w / 2;
    const cy = node.transform.y + node.transform.h / 2;
    const angle = (Math.atan2(world.y - cy, world.x - cx) * 180) / Math.PI + 90;
    node.transform.rotation = shift ? Math.round(angle / 15) * 15 : Math.round(angle);
    this.markMutated();
  }

  private labelForKind(kind: NodeKind): string {
    switch (kind) {
      case "frame":
        return "Frame";
      case "rectangle":
        return "Rectangle";
      case "ellipse":
        return "Ellipse";
      case "line":
        return "Line";
      case "polygon":
        return "Polygon";
      case "star":
        return "Star";
      case "text":
        return "Text";
      case "image":
        return "Image";
      case "group":
        return "Group";
      case "component":
        return "Component";
      default:
        return "Layer";
    }
  }

  private drawKindForTool(tool: ToolId): NodeKind | null {
    switch (tool) {
      case "frame":
        return "frame";
      case "rectangle":
        return "rectangle";
      case "ellipse":
        return "ellipse";
      case "line":
        return "line";
      case "polygon":
        return "polygon";
      case "star":
        return "star";
      case "slice":
        return "slice";
      default:
        return null;
    }
  }

  // -- text editing --------------------------------------------------------

  private beginTextEdit(nodeId: string): void {
    const node = this.node(nodeId);
    if (!node || !node.text) return;
    this.editing = {
      nodeId,
      original: node.text.characters,
      characters: node.text.characters,
      caret: node.text.characters.length,
      selection: null,
    };
    this.selection = [nodeId];
    this.tool = "text";
    this.status = "Editing text";
  }

  private setCaretFromWorld(world: { x: number; y: number }): void {
    if (!this.editing) return;
    const node = this.node(this.editing.nodeId);
    if (!node) return;
    const fontSize = node.text?.style.font_size ?? 16;
    const caret = clamp(
      Math.round((world.x - node.transform.x) / (fontSize * 0.55)),
      0,
      this.editing.characters.length,
    );
    this.editing.caret = caret;
    this.editing.selection = null;
  }

  private commitTextEdit(): void {
    if (!this.editing) return;
    const node = this.node(this.editing.nodeId);
    if (node && node.text) {
      const next = this.editing.characters;
      if (next !== this.editing.original) {
        this.pushHistory();
        node.text.characters = next;
        node.transform.h = Math.max(node.transform.h, (node.text.style.font_size || 16) * 1.4);
        this.markMutated();
      }
    }
    this.editing = null;
    this.tool = "move";
    this.status = "Text committed";
  }

  private cancelTextEdit(): void {
    if (!this.editing) return;
    const node = this.node(this.editing.nodeId);
    if (node && node.text) node.text.characters = this.editing.original;
    this.editing = null;
    this.tool = "move";
    this.status = "Text editing cancelled";
  }

  // -- commands ------------------------------------------------------------

  private runCommand<C extends CommandName>(name: C, args: CommandArgs[C] | undefined): EngineState {
    switch (name) {
      case "undo": {
        const snapshot = this.undoStack.pop();
        if (!snapshot) return this.fail("validation", "Nothing to undo");
        this.redoStack.push(this.historySnapshot());
        this.restoreSnapshot(snapshot.data, snapshot.selection);
        this.markMutated();
        this.status = "Undo";
        return this.finish();
      }
      case "redo": {
        const snapshot = this.redoStack.pop();
        if (!snapshot) return this.fail("validation", "Nothing to redo");
        this.undoStack.push(this.historySnapshot());
        this.restoreSnapshot(snapshot.data, snapshot.selection);
        this.markMutated();
        this.status = "Redo";
        return this.finish();
      }
      case "select_all":
        this.select(this.orderedNodes().filter((node) => node.visible).map((node) => node.id));
        return this.finish();
      case "select_none":
        this.select([]);
        return this.finish();
      case "select_inverse": {
        const all = this.orderedNodes().filter((node) => node.visible).map((node) => node.id);
        this.select(all.filter((id) => !this.selection.includes(id)));
        return this.finish();
      }
      case "select_nodes": {
        const typed = args as CommandArgs["select_nodes"];
        this.select(typed.ids, typed.additive ?? false);
        return this.finish();
      }
      case "delete": {
        this.deleteSelection();
        return this.finish();
      }
      case "duplicate": {
        this.duplicateSelection(false);
        return this.finish();
      }
      case "copy": {
        this.copySelection();
        return this.finish();
      }
      case "cut": {
        this.copySelection();
        this.deleteSelection();
        return this.finish();
      }
      case "paste": {
        this.pasteClipboard();
        return this.finish();
      }
      case "group": {
        this.groupSelection();
        return this.finish();
      }
      case "ungroup": {
        this.ungroupSelection();
        return this.finish();
      }
      case "create_component": {
        this.wrapSelection("component");
        return this.finish();
      }
      case "create_component_set": {
        return this.unsupported("Component sets are not simulated by the dev harness.");
      }
      case "detach_instance":
      case "swap_instance": {
        return this.unsupported("Instances are not simulated by the dev harness.");
      }
      case "bring_to_front":
      case "bring_forward":
      case "send_backward":
      case "send_to_back": {
        this.reorderSelection(name);
        return this.finish();
      }
      case "move_to_parent":
      case "reorder": {
        return this.unsupported("Drag reordering between layers is not simulated yet.");
      }
      case "zoom_in": {
        this.zoomBy(1.25);
        return this.finish();
      }
      case "zoom_out": {
        this.zoomBy(0.8);
        return this.finish();
      }
      case "zoom_to": {
        const typed = args as CommandArgs["zoom_to"];
        this.zoomTo(clamp(typed.zoom, MIN_ZOOM, MAX_ZOOM));
        return this.finish();
      }
      case "zoom_fit": {
        this.zoomFit();
        return this.finish();
      }
      case "zoom_selection": {
        this.zoomToSelection();
        return this.finish();
      }
      case "zoom_100": {
        this.zoomTo(1);
        return this.finish();
      }
      case "pan_by": {
        const typed = args as CommandArgs["pan_by"];
        this.viewport.x += typed.dx;
        this.viewport.y += typed.dy;
        this.markFrameChanged();
        return this.finish();
      }
      case "set_viewport": {
        const typed = args as CommandArgs["set_viewport"];
        this.viewport = { ...this.viewport, width: typed.width, height: typed.height, dpr: typed.dpr };
        this.markFrameChanged();
        return this.finish();
      }
      case "apply_patch": {
        const typed = args as ApplyPatchArgs;
        this.applyPatch(typed);
        return this.finish();
      }
      case "rename_node": {
        const typed = args as CommandArgs["rename_node"];
        const node = this.node(typed.id);
        if (!node) return this.fail("validation", "Unknown layer");
        this.pushHistory();
        node.name = typed.name;
        this.markMutated();
        this.status = "Layer renamed";
        return this.finish();
      }
      case "toggle_visible": {
        const typed = args as CommandArgs["toggle_visible"];
        const ids = typed?.ids && typed.ids.length > 0 ? typed.ids : this.selection;
        if (ids.length === 0) return this.fail("validation", "Select a layer first");
        this.pushHistory();
        for (const id of ids) {
          const node = this.node(id);
          if (node) node.visible = !node.visible;
        }
        this.markMutated();
        return this.finish();
      }
      case "toggle_locked": {
        const typed = args as CommandArgs["toggle_locked"];
        const ids = typed?.ids && typed.ids.length > 0 ? typed.ids : this.selection;
        if (ids.length === 0) return this.fail("validation", "Select a layer first");
        this.pushHistory();
        for (const id of ids) {
          const node = this.node(id);
          if (node) node.locked = !node.locked;
        }
        this.markMutated();
        return this.finish();
      }
      case "set_opacity": {
        const typed = args as CommandArgs["set_opacity"];
        const ids = typed.ids && typed.ids.length > 0 ? typed.ids : this.selection;
        if (ids.length === 0) return this.fail("validation", "Select a layer first");
        this.pushHistory();
        for (const id of ids) {
          const node = this.node(id);
          if (node) node.opacity = clamp(typed.opacity, 0, 1);
        }
        this.markMutated();
        return this.finish();
      }
      case "nudge": {
        const typed = args as CommandArgs["nudge"];
        const ids = typed.ids && typed.ids.length > 0 ? typed.ids : this.selection;
        if (ids.length === 0) return this.fail("validation", "Select a layer first");
        this.pushHistory();
        for (const id of ids) this.translate(id, typed.dx, typed.dy);
        this.markMutated();
        return this.finish();
      }
      case "align": {
        const typed = args as CommandArgs["align"];
        this.alignSelection(typed.ids && typed.ids.length > 0 ? typed.ids : this.selection, typed.align);
        return this.finish();
      }
      case "distribute": {
        const typed = args as CommandArgs["distribute"];
        this.distributeSelection(typed.ids && typed.ids.length > 0 ? typed.ids : this.selection, typed.axis, typed.spacing);
        return this.finish();
      }
      case "boolean": {
        return this.unsupported("Boolean operations are not simulated by the dev harness.");
      }
      case "flatten": {
        return this.unsupported("Flatten is not simulated by the dev harness.");
      }
      case "outline_stroke": {
        return this.unsupported("Outline stroke is not simulated by the dev harness.");
      }
      case "text_edit_begin": {
        const typed = args as CommandArgs["text_edit_begin"];
        this.beginTextEdit(typed.node_id);
        return this.finish();
      }
      case "text_edit_input": {
        const typed = args as CommandArgs["text_edit_input"];
        if (!this.editing) return this.fail("validation", "No text layer is being edited");
        this.editing.characters = typed.characters;
        this.editing.caret = clamp(typed.caret, 0, typed.characters.length);
        this.editing.selection = typed.selection;
        this.markFrameChanged();
        return this.finish();
      }
      case "text_edit_set_caret": {
        const typed = args as CommandArgs["text_edit_set_caret"];
        if (!this.editing) return this.fail("validation", "No text layer is being edited");
        this.editing.caret = clamp(typed.caret, 0, this.editing.characters.length);
        this.editing.selection = typed.selection;
        return this.finish();
      }
      case "text_edit_commit": {
        this.commitTextEdit();
        return this.finish();
      }
      case "text_edit_cancel": {
        this.cancelTextEdit();
        return this.finish();
      }
      case "set_active_page": {
        const typed = args as CommandArgs["set_active_page"];
        return this.setActivePage(typed.page_id);
      }
      case "add_page": {
        const typed = args as CommandArgs["add_page"];
        return this.addPage(typed?.name);
      }
      case "rename_page": {
        const typed = args as CommandArgs["rename_page"];
        return this.renamePage(typed.page_id, typed.name);
      }
      case "delete_page": {
        const typed = args as CommandArgs["delete_page"];
        return this.deletePage(typed.page_id);
      }
      case "mask_with_selection": {
        return this.unsupported("Masks are not simulated by the dev harness.");
      }
      default: {
        return this.unsupported(`Command '${String(name)}' is not simulated by the dev harness.`);
      }
    }
  }

  private deleteSelection(): void {
    if (this.selection.length === 0) {
      this.recordError("validation", "Select a layer to delete.", "warning");
      return;
    }
    this.pushHistory();
    const ids = new Set<string>();
    for (const id of this.selection) for (const child of this.subtreeIds(id)) ids.add(child);
    for (const id of ids) {
      const node = this.doc?.nodes.get(id);
      if (!node || !this.doc) continue;
      if (node.parent) {
        const parent = this.doc.nodes.get(node.parent);
        if (parent) parent.children = parent.children.filter((child) => child !== id);
      } else {
        const page = this.doc.pages.find((candidate) => candidate.id === node.page);
        if (page) page.roots = page.roots.filter((root) => root !== id);
      }
      this.doc.nodes.delete(id);
    }
    this.selection = [];
    this.markMutated();
    this.status = "Deleted";
  }

  private copySelection(): void {
    if (this.doc === null || this.selection.length === 0) {
      this.recordError("validation", "Select a layer to copy.", "warning");
      return;
    }
    const ids = new Set<string>();
    for (const id of this.selection) for (const child of this.subtreeIds(id)) ids.add(child);
    const nodes = [...ids]
      .map((id) => this.node(id))
      .filter((node): node is DevNode => node !== null)
      .map((node) => deepClone(node));
    this.clipboard = JSON.stringify(nodes);
    this.recordError("validation", `${this.selection.length} layer(s) copied`, "warning");
    this.status = "Copied";
  }

  private pasteClipboard(): void {
    if (!this.clipboard || !this.doc) {
      this.recordError("validation", "Nothing to paste.", "warning");
      return;
    }
    const nodes = JSON.parse(this.clipboard) as DevNode[];
    this.pushHistory();
    const idMap = new Map<string, string>();
    const page = this.activePage();
    if (!page) return;
    for (const node of nodes) idMap.set(node.id, newId("node"));
    const newIds: string[] = [];
    for (const source of nodes) {
      const node = deepClone(source);
      const newIdValue = idMap.get(source.id) ?? newId("node");
      node.id = newIdValue;
      node.children = node.children.map((child) => idMap.get(child) ?? child).filter(Boolean);
      node.parent = node.parent ? idMap.get(node.parent) ?? null : null;
      node.transform.x += 16;
      node.transform.y += 16;
      node.page = page.id;
      this.doc.nodes.set(node.id, node);
      if (!node.parent) {
        page.roots.push(node.id);
        newIds.push(node.id);
      }
    }
    this.select(newIds);
    this.markMutated();
    this.status = "Pasted";
  }

  private duplicateSelection(useClipboard: boolean): void {
    if (this.selection.length === 0) {
      this.recordError("validation", "Select a layer to duplicate.", "warning");
      return;
    }
    this.copySelection();
    this.pasteClipboard();
    void useClipboard;
  }

  private groupSelection(): void {
    if (!this.doc) return;
    if (this.selection.length < 2) {
      this.recordError("validation", "Select at least two layers to group.", "warning");
      return;
    }
    const page = this.activePage();
    if (!page) return;
    const selected = [...this.selection]
      .map((id) => this.node(id))
      .filter((node): node is DevNode => node !== null);
    const bounds = unionRects(selected.map((node) => this.nodeBounds(node)));
    if (!bounds) return;
    this.pushHistory();
    const parentId = selected[0]?.parent ?? null;
    const group = this.createNode({
      name: "Group",
      kind: "group",
      rect: bounds,
      page: page.id,
      parent: parentId ?? null,
    });
    this.insertNode(group, parentId);
    for (const node of selected) {
      if (node.parent) {
        const parent = this.node(node.parent);
        if (parent) parent.children = parent.children.filter((child) => child !== node.id);
      } else {
        page.roots = page.roots.filter((root) => root !== node.id);
      }
      node.parent = group.id;
      group.children.push(node.id);
    }
    this.select([group.id]);
    this.markMutated();
    this.status = "Grouped";
  }

  private ungroupSelection(): void {
    if (!this.doc) return;
    const groups = this.selection
      .map((id) => this.node(id))
      .filter((node): node is DevNode => node !== null && (node.kind === "group" || node.kind === "component"));
    if (groups.length === 0) {
      this.recordError("validation", "Select a group to ungroup.", "warning");
      return;
    }
    this.pushHistory();
    const newSelection: string[] = [];
    for (const group of groups) {
      const siblings = this.siblings(group.id);
      const index = siblings.indexOf(group.id);
      const parentId = group.parent;
      for (const childId of group.children) {
        const child = this.node(childId);
        if (!child) continue;
        child.parent = parentId;
        if (parentId) this.node(parentId)?.children.push(childId);
        else this.pageOf(group)?.roots.push(childId);
        newSelection.push(childId);
      }
      if (parentId) {
        const parent = this.node(parentId);
        if (parent) parent.children = parent.children.filter((child) => child !== group.id);
      } else {
        const page = this.pageOf(group);
        if (page) page.roots = page.roots.filter((root) => root !== group.id);
      }
      this.doc.nodes.delete(group.id);
      void index;
    }
    this.select(newSelection);
    this.markMutated();
    this.status = "Ungrouped";
  }

  private wrapSelection(kind: "component"): void {
    if (!this.doc) return;
    if (this.selection.length === 0) {
      this.recordError("validation", "Select a layer first.", "warning");
      return;
    }
    if (this.selection.length === 1) {
      const single = this.selection[0] ? this.node(this.selection[0]) : null;
      if (single && single.kind === "component") {
        this.recordError("validation", "This layer is already a component.", "warning");
        return;
      }
    }
    const page = this.activePage();
    if (!page) return;
    const selected = this.selection
      .map((id) => this.node(id))
      .filter((node): node is DevNode => node !== null);
    const bounds = unionRects(selected.map((node) => this.nodeBounds(node)));
    if (!bounds) return;
    this.pushHistory();
    const parentId = selected[0]?.parent ?? null;
    const wrapper = this.createNode({
      name: "Component",
      kind,
      rect: bounds,
      page: page.id,
      parent: parentId ?? null,
    });
    this.insertNode(wrapper, parentId);
    for (const node of selected) {
      if (node.parent) {
        const parent = this.node(node.parent);
        if (parent) parent.children = parent.children.filter((child) => child !== node.id);
      } else {
        page.roots = page.roots.filter((root) => root !== node.id);
      }
      node.parent = wrapper.id;
      wrapper.children.push(node.id);
    }
    this.doc.components = [...this.doc.components, { node_id: wrapper.id, name: wrapper.name, kind }];
    this.select([wrapper.id]);
    this.markMutated();
    this.status = "Component created";
  }

  private reorderSelection(name: string): void {
    if (this.selection.length === 0) {
      this.recordError("validation", "Select a layer first.", "warning");
      return;
    }
    this.pushHistory();
    for (const id of this.selection) {
      const node = this.node(id);
      if (!node) continue;
      const siblings = this.siblings(id);
      const index = siblings.indexOf(id);
      if (index === -1) continue;
      siblings.splice(index, 1);
      if (name === "bring_to_front") siblings.push(id);
      else if (name === "send_to_back") siblings.unshift(id);
      else if (name === "bring_forward") siblings.splice(Math.min(siblings.length, index + 1), 0, id);
      else siblings.splice(Math.max(0, index - 1), 0, id);
    }
    this.markMutated();
    this.status = "Reordered";
  }

  private alignSelection(ids: string[], align: CommandArgs["align"]["align"]): void {
    if (ids.length < 2) {
      this.recordError("validation", "Select at least two layers to align.", "warning");
      return;
    }
    const nodes = ids.map((id) => this.node(id)).filter((node): node is DevNode => node !== null);
    const bounds = unionRects(nodes.map((node) => this.nodeBounds(node)));
    if (!bounds) return;
    this.pushHistory();
    for (const node of nodes) {
      const rect = this.nodeBounds(node);
      let dx = 0;
      let dy = 0;
      switch (align) {
        case "left":
          dx = bounds.x - rect.x;
          break;
        case "hcenter":
          dx = bounds.x + bounds.w / 2 - (rect.x + rect.w / 2);
          break;
        case "right":
          dx = bounds.x + bounds.w - (rect.x + rect.w);
          break;
        case "top":
          dy = bounds.y - rect.y;
          break;
        case "vcenter":
          dy = bounds.y + bounds.h / 2 - (rect.y + rect.h / 2);
          break;
        case "bottom":
          dy = bounds.y + bounds.h - (rect.y + rect.h);
          break;
        default:
          break;
      }
      if (dx !== 0 || dy !== 0) this.translate(node.id, dx, dy);
    }
    this.markMutated();
    this.status = "Aligned";
  }

  private distributeSelection(ids: string[], axis: "horizontal" | "vertical", spacing: number): void {
    if (ids.length < 2) {
      this.recordError("validation", "Select at least three layers to distribute.", "warning");
      return;
    }
    const nodes = ids
      .map((id) => this.node(id))
      .filter((node): node is DevNode => node !== null)
      .sort((a, b) =>
        axis === "horizontal"
          ? this.nodeBounds(a).x - this.nodeBounds(b).x
          : this.nodeBounds(a).y - this.nodeBounds(b).y,
      );
    const first = nodes[0];
    const last = nodes[nodes.length - 1];
    if (!first || !last) return;
    const firstRect = this.nodeBounds(first);
    const lastRect = this.nodeBounds(last);
    this.pushHistory();
    if (axis === "horizontal") {
      const start = firstRect.x + firstRect.w;
      const end = lastRect.x;
      const available = end - start;
      const totalSize = nodes.slice(1, -1).reduce((sum, node) => sum + this.nodeBounds(node).w, 0);
      const gap = spacing > 0 ? spacing : (available - totalSize) / Math.max(1, nodes.length - 1);
      let cursor = spacing > 0 ? start + gap : start;
      for (const node of nodes.slice(1, -1)) {
        const rect = this.nodeBounds(node);
        this.translate(node.id, cursor - rect.x, 0);
        cursor += rect.w + gap;
      }
    } else {
      const start = firstRect.y + firstRect.h;
      const end = lastRect.y;
      const available = end - start;
      const totalSize = nodes.slice(1, -1).reduce((sum, node) => sum + this.nodeBounds(node).h, 0);
      const gap = spacing > 0 ? spacing : (available - totalSize) / Math.max(1, nodes.length - 1);
      let cursor = spacing > 0 ? start + gap : start;
      for (const node of nodes.slice(1, -1)) {
        const rect = this.nodeBounds(node);
        this.translate(node.id, 0, cursor - rect.y);
        cursor += rect.h + gap;
      }
    }
    this.markMutated();
    this.status = "Distributed";
  }

  private applyPatch(args: ApplyPatchArgs): void {
    if (!this.doc) return;
    if (args.ids.length === 0) {
      this.recordError("validation", "No layers to update.", "warning");
      return;
    }
    this.pushHistory();
    for (const id of args.ids) {
      const node = this.node(id);
      if (!node) continue;
      const patch = args.patch;
      if (patch.transform) {
        const next = patch.transform;
        this.translate(node.id, next.x - node.transform.x, next.y - node.transform.y);
        node.transform.w = Math.max(1, next.w);
        node.transform.h = Math.max(1, next.h);
        node.transform.rotation = next.rotation;
        node.transform.flip_x = next.flip_x;
        node.transform.flip_y = next.flip_y;
      }
      if (patch.name !== undefined) node.name = patch.name;
      if (patch.visible !== undefined) node.visible = patch.visible;
      if (patch.locked !== undefined) node.locked = patch.locked;
      if (patch.opacity !== undefined) node.opacity = clamp(patch.opacity, 0, 1);
      if (patch.blend !== undefined) node.blend = patch.blend;
      if (patch.fills !== undefined) node.fills = deepClone(patch.fills);
      if (patch.strokes !== undefined) node.strokes = deepClone(patch.strokes);
      if (patch.effects !== undefined) node.effects = deepClone(patch.effects);
      if (patch.corner_radii !== undefined) node.corner_radii = { ...patch.corner_radii };
      if (patch.fill_rule !== undefined) node.fill_rule = patch.fill_rule;
      if (patch.text !== undefined) node.text = deepClone(patch.text);
    }
    this.markMutated();
    this.status = args.label ?? "Updated properties";
  }

  private setActivePage(pageId: string): EngineState {
    if (!this.doc) return this.fail("validation", "No document is open.");
    const page = this.doc.pages.find((candidate) => candidate.id === pageId);
    if (!page) return this.fail("validation", "Unknown page.");
    this.doc.activePage = pageId;
    this.selection = [];
    this.hover = null;
    this.editing = null;
    this.markMutated();
    this.status = `Page: ${page.name}`;
    return this.finish();
  }

  private addPage(name?: string): EngineState {
    if (!this.doc) return this.fail("validation", "No document is open.");
    this.pushHistory();
    const count = this.doc.pages.length + 1;
    const page: DevPage = { id: newId("page"), name: name ?? `Page ${count}`, roots: [] };
    this.doc.pages.push(page);
    this.doc.activePage = page.id;
    this.selection = [];
    this.markMutated();
    this.status = `Page added: ${page.name}`;
    return this.finish();
  }

  private renamePage(pageId: string, name: string): EngineState {
    if (!this.doc) return this.fail("validation", "No document is open.");
    const page = this.doc.pages.find((candidate) => candidate.id === pageId);
    if (!page) return this.fail("validation", "Unknown page.");
    this.pushHistory();
    page.name = name;
    this.markMutated();
    this.status = "Page renamed";
    return this.finish();
  }

  private deletePage(pageId: string): EngineState {
    if (!this.doc) return this.fail("validation", "No document is open.");
    if (this.doc.pages.length <= 1) {
      return this.fail("validation", "A document must keep at least one page.");
    }
    const page = this.doc.pages.find((candidate) => candidate.id === pageId);
    if (!page) return this.fail("validation", "Unknown page.");
    this.pushHistory();
    const remove = (ids: string[]): void => {
      for (const id of ids) {
        const node = this.node(id);
        if (!node) continue;
        remove(node.children);
        this.doc?.nodes.delete(id);
      }
    };
    remove(page.roots);
    this.doc.pages = this.doc.pages.filter((candidate) => candidate.id !== pageId);
    if (this.doc.activePage === pageId) {
      this.doc.activePage = this.doc.pages[0]?.id ?? "";
      this.selection = [];
    }
    this.markMutated();
    this.status = "Page deleted";
    return this.finish();
  }

  // -- viewport ------------------------------------------------------------

  private zoomBy(factor: number): void {
    const zoom = clamp(this.viewport.zoom * factor, MIN_ZOOM, MAX_ZOOM);
    this.zoomTo(zoom);
  }

  private zoomTo(zoom: number): void {
    const center = { x: this.viewport.width / 2, y: this.viewport.height / 2 };
    const before = screenToWorld(center, this.viewport);
    this.viewport.zoom = clamp(zoom, MIN_ZOOM, MAX_ZOOM);
    this.viewport.x = before.x - center.x / this.viewport.zoom;
    this.viewport.y = before.y - center.y / this.viewport.zoom;
    this.markFrameChanged();
    this.status = `Zoom ${Math.round(this.viewport.zoom * 100)}%`;
  }

  private zoomFit(): void {
    const bounds = this.pageBounds();
    if (!bounds || bounds.w <= 0 || bounds.h <= 0) {
      this.zoomTo(1);
      return;
    }
    const padding = 48;
    const zoom = clamp(
      Math.min(
        this.viewport.width / (bounds.w + padding * 2),
        this.viewport.height / (bounds.h + padding * 2),
      ),
      MIN_ZOOM,
      MAX_ZOOM,
    );
    this.viewport.zoom = zoom;
    this.viewport.x = bounds.x + bounds.w / 2 - this.viewport.width / (2 * zoom);
    this.viewport.y = bounds.y + bounds.h / 2 - this.viewport.height / (2 * zoom);
    this.markFrameChanged();
  }

  private zoomToSelection(): void {
    const bounds = this.selectionBounds();
    if (!bounds) {
      this.zoomFit();
      return;
    }
    const zoom = clamp(
      Math.min(this.viewport.width / Math.max(1, bounds.w + 120), this.viewport.height / Math.max(1, bounds.h + 120)),
      MIN_ZOOM,
      MAX_ZOOM,
    );
    this.viewport.zoom = zoom;
    this.viewport.x = bounds.x + bounds.w / 2 - this.viewport.width / (2 * zoom);
    this.viewport.y = bounds.y + bounds.h / 2 - this.viewport.height / (2 * zoom);
    this.markFrameChanged();
  }

  // -- persistence ---------------------------------------------------------

  private loadPersisted(path: string): PersistedDocument | null {
    try {
      const raw = window.localStorage.getItem(`${FILE_STORAGE_PREFIX}${path}`);
      if (!raw) return null;
      const parsed = JSON.parse(raw) as PersistedDocument;
      if (parsed.version !== 1) return null;
      return parsed;
    } catch {
      return null;
    }
  }

  private persist(path: string, data: string): void {
    try {
      window.localStorage.setItem(`${FILE_STORAGE_PREFIX}${path}`, data);
    } catch {
      // The harness still works in-memory if storage is unavailable.
    }
  }

  private nameFromPath(path: string): string {
    const clean = path.replace(/\\/g, "/");
    const base = clean.slice(clean.lastIndexOf("/") + 1) || "Untitled";
    return base.replace(/\.(iconcrafter|json)$/i, "") || "Untitled";
  }

  // -- rendering -----------------------------------------------------------

  private renderTo(ctx: CanvasRenderingContext2D, width: number, height: number): void {
    const { zoom, x, y, dpr } = this.viewport;
    ctx.setTransform(dpr * zoom, 0, 0, dpr * zoom, -x * zoom * dpr, -y * zoom * dpr);
    void width;
    void height;
    this.drawPage(ctx, null);
  }

  private drawPage(ctx: CanvasRenderingContext2D, onlyIds: string[] | null): void {
    const page = this.activePage();
    if (!page) return;
    const roots = onlyIds && onlyIds.length > 0 ? onlyIds : page.roots;
    for (const id of roots) {
      const node = this.node(id);
      if (node) this.drawNode(ctx, node, 1);
    }
  }

  private drawNode(ctx: CanvasRenderingContext2D, node: DevNode, inheritedOpacity: number): void {
    if (!node.visible) return;
    const opacity = inheritedOpacity * node.opacity;
    const rect = this.rootRect(node);
    ctx.save();
    const cx = rect.x + rect.w / 2;
    const cy = rect.y + rect.h / 2;
    ctx.translate(cx, cy);
    if (node.transform.rotation) ctx.rotate((node.transform.rotation * Math.PI) / 180);
    if (node.transform.flip_x || node.transform.flip_y) {
      ctx.scale(node.transform.flip_x ? -1 : 1, node.transform.flip_y ? -1 : 1);
    }
    ctx.translate(-rect.w / 2, -rect.h / 2);
    ctx.globalAlpha = opacity;

    const radius = this.uniformRadius(node.corner_radii);
    if (node.kind === "frame" || node.kind === "group" || node.kind === "component") {
      const frameFill = node.fills.find((fill) => fill.visible);
      if (frameFill) this.fillShape(ctx, rect.w, rect.h, radius, frameFill);
    } else if (node.kind === "rectangle" || node.kind === "image" || node.kind === "slice") {
      const fill = node.fills.find((candidate) => candidate.visible);
      if (fill) this.fillShape(ctx, rect.w, rect.h, radius, fill);
      else {
        ctx.fillStyle = "#3a4250";
        this.roundRectPath(ctx, 0, 0, rect.w, rect.h, radius);
        ctx.fill();
      }
      if (node.kind === "image") this.drawImagePlaceholder(ctx, rect.w, rect.h);
    } else if (node.kind === "ellipse") {
      const fill = node.fills.find((candidate) => candidate.visible);
      ctx.beginPath();
      ctx.ellipse(rect.w / 2, rect.h / 2, rect.w / 2, rect.h / 2, 0, 0, Math.PI * 2);
      this.paintFill(ctx, fill);
    } else if (node.kind === "star" || node.kind === "polygon") {
      const fill = node.fills.find((candidate) => candidate.visible);
      this.drawPolygon(ctx, rect.w, rect.h, node.kind === "star" ? 5 : 6, fetchColor(fill?.paint), node.kind === "star");
    } else if (node.kind === "line") {
      ctx.strokeStyle = this.strokeColor(node) ?? "#e9ecf2";
      ctx.lineWidth = node.strokes[0]?.weight ?? 2;
      ctx.beginPath();
      ctx.moveTo(0, 0);
      ctx.lineTo(rect.w, rect.h);
      ctx.stroke();
    } else if (node.kind === "text") {
      this.drawText(ctx, node);
    } else {
      const fill = node.fills.find((candidate) => candidate.visible);
      if (fill) this.fillShape(ctx, rect.w, rect.h, radius, fill);
    }

    // Strokes (frames and shapes).
    const stroke = node.strokes.find((candidate) => candidate.visible);
    if (stroke && node.kind !== "text") {
      const color = fetchColor(stroke.paint);
      if (color) {
        ctx.strokeStyle = color;
        ctx.lineWidth = stroke.weight;
        if (stroke.dash && stroke.dash.length > 0) ctx.setLineDash(stroke.dash);
        if (node.kind === "ellipse") {
          ctx.beginPath();
          ctx.ellipse(rect.w / 2, rect.h / 2, rect.w / 2, rect.h / 2, 0, 0, Math.PI * 2);
        } else {
          this.roundRectPath(ctx, 0, 0, rect.w, rect.h, radius);
        }
        ctx.stroke();
        ctx.setLineDash([]);
      }
    }

    ctx.globalAlpha = opacity;
    for (const childId of node.children) {
      const child = this.node(childId);
      if (child) this.drawNode(ctx, child, opacity);
    }
    ctx.restore();
  }

  private drawText(ctx: CanvasRenderingContext2D, node: DevNode): void {
    const text = node.text;
    if (!text) return;
    const style = text.style;
    const lineHeight =
      style.line_height.unit === "px" && style.line_height.value > 0
        ? style.line_height.value
        : style.font_size * 1.3;
    const fontStyle = style.italic ? "italic " : "";
    ctx.font = `${fontStyle}${style.font_weight} ${style.font_size}px ${style.font_family}, sans-serif`;
    ctx.textBaseline = "top";
    ctx.textAlign =
      text.paragraph.align === "center"
        ? "center"
        : text.paragraph.align === "right"
          ? "right"
          : "left";
    const x = text.paragraph.align === "center" ? node.transform.w / 2 : text.paragraph.align === "right" ? node.transform.w : 0;
    const lines = text.characters.split("\n");
    lines.forEach((line, index) => {
      const y = index * lineHeight;
      for (const fill of style.fills) {
        const color = fetchColor(fill.paint);
        if (!color) continue;
        ctx.fillStyle = color;
        ctx.fillText(line, x, y);
        break;
      }
    });
  }

  private fillShape(
    ctx: CanvasRenderingContext2D,
    w: number,
    h: number,
    radius: number,
    fill: Fill,
  ): void {
    const color = fetchColor(fill.paint);
    if (!color) {
      ctx.fillStyle = "#3a4250";
      this.roundRectPath(ctx, 0, 0, w, h, radius);
      ctx.fill();
      return;
    }
    ctx.fillStyle = color;
    ctx.globalAlpha *= fill.opacity;
    this.roundRectPath(ctx, 0, 0, w, h, radius);
    ctx.fill();
    ctx.globalAlpha /= Math.max(fill.opacity, 0.0001);
    void h;
  }

  private paintFill(ctx: CanvasRenderingContext2D, fill: Fill | undefined): void {
    const color = fetchColor(fill?.paint);
    ctx.fillStyle = color ?? "#3a4250";
    ctx.fill();
  }

  private drawPolygon(
    ctx: CanvasRenderingContext2D,
    w: number,
    h: number,
    points: number,
    color: string | null,
    star: boolean,
  ): void {
    const cx = w / 2;
    const cy = h / 2;
    const outer = Math.min(w, h) / 2;
    const inner = outer * 0.45;
    ctx.beginPath();
    const total = star ? points * 2 : points;
    for (let i = 0; i < total; i += 1) {
      const radius = star && i % 2 === 1 ? inner : outer;
      const angle = (Math.PI * 2 * i) / total - Math.PI / 2;
      const px = cx + Math.cos(angle) * radius;
      const py = cy + Math.sin(angle) * radius;
      if (i === 0) ctx.moveTo(px, py);
      else ctx.lineTo(px, py);
    }
    ctx.closePath();
    ctx.fillStyle = color ?? "#ff6b4a";
    ctx.fill();
  }

  private drawImagePlaceholder(ctx: CanvasRenderingContext2D, w: number, h: number): void {
    ctx.strokeStyle = "rgba(233,236,242,0.35)";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(0, 0);
    ctx.lineTo(w, h);
    ctx.moveTo(w, 0);
    ctx.lineTo(0, h);
    ctx.stroke();
  }

  private roundRectPath(
    ctx: CanvasRenderingContext2D,
    x: number,
    y: number,
    w: number,
    h: number,
    radius: number,
  ): void {
    const r = Math.max(0, Math.min(radius, Math.min(w, h) / 2));
    ctx.beginPath();
    if (r === 0) {
      ctx.rect(x, y, w, h);
      return;
    }
    const anyCtx = ctx as CanvasRenderingContext2D & {
      roundRect?: (x: number, y: number, w: number, h: number, r: number) => void;
    };
    if (typeof anyCtx.roundRect === "function") {
      anyCtx.roundRect(x, y, w, h, r);
      return;
    }
    ctx.moveTo(x + r, y);
    ctx.lineTo(x + w - r, y);
    ctx.quadraticCurveTo(x + w, y, x + w, y + r);
    ctx.lineTo(x + w, y + h - r);
    ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
    ctx.lineTo(x + r, y + h);
    ctx.quadraticCurveTo(x, y + h, x, y + h - r);
    ctx.lineTo(x, y + r);
    ctx.quadraticCurveTo(x, y, x + r, y);
    ctx.closePath();
  }

  private uniformRadius(radii: CornerRadii): number {
    const values = [radii.tl, radii.tr, radii.br, radii.bl];
    return values.every((value) => value === values[0]) ? radii.tl : 0;
  }

  private strokeColor(node: DevNode): string | null {
    return fetchColor(node.strokes.find((stroke) => stroke.visible)?.paint);
  }

  // -- export helpers ------------------------------------------------------

  private buildSvg(bounds: Rect, onlyIds: string[] | null): string {
    const page = this.activePage();
    if (!page) return "<svg xmlns=\"http://www.w3.org/2000/svg\"/>";
    const roots = onlyIds && onlyIds.length > 0 ? onlyIds : page.roots;
    const parts: string[] = [
      `<svg xmlns="http://www.w3.org/2000/svg" width="${Math.round(bounds.w)}" height="${Math.round(bounds.h)}" viewBox="${bounds.x} ${bounds.y} ${bounds.w} ${bounds.h}">`,
    ];
    const emit = (node: DevNode): void => {
      if (!node.visible) return;
      const rect = this.rootRect(node);
      const fill = fetchColor(node.fills.find((candidate) => candidate.visible)?.paint);
      const stroke = fetchColor(node.strokes.find((candidate) => candidate.visible)?.paint);
      const strokeAttr = stroke ? ` stroke="${stroke}" stroke-width="${node.strokes[0]?.weight ?? 1}"` : "";
      const fillAttr = fill ? ` fill="${fill}"` : ' fill="none"';
      if (node.kind === "rectangle" || node.kind === "frame" || node.kind === "group" || node.kind === "component") {
        parts.push(
          `<rect x="${rect.x}" y="${rect.y}" width="${rect.w}" height="${rect.h}" rx="${this.uniformRadius(node.corner_radii)}"${fillAttr}${strokeAttr}/>`,
        );
      } else if (node.kind === "ellipse") {
        parts.push(
          `<ellipse cx="${rect.x + rect.w / 2}" cy="${rect.y + rect.h / 2}" rx="${rect.w / 2}" ry="${rect.h / 2}"${fillAttr}${strokeAttr}/>`,
        );
      } else if (node.kind === "text" && node.text) {
        const color = fetchColor(node.text.style.fills[0]?.paint) ?? "#e9ecf2";
        const lines = node.text.characters.split("\n");
        lines.forEach((line, index) => {
          parts.push(
            `<text x="${rect.x}" y="${rect.y + node.text!.style.font_size * (1.2 + index * 1.3)}" font-family="${node.text!.style.font_family}" font-size="${node.text!.style.font_size}" fill="${color}">${escapeXml(line)}</text>`,
          );
        });
      } else if (node.kind === "line") {
        parts.push(
          `<line x1="${rect.x}" y1="${rect.y}" x2="${rect.x + rect.w}" y2="${rect.y + rect.h}"${strokeAttr || ' stroke="#e9ecf2"'}/>`,
        );
      }
      for (const childId of node.children) {
        const child = this.node(childId);
        if (child) emit(child);
      }
    };
    for (const id of roots) {
      const node = this.node(id);
      if (node) emit(node);
    }
    parts.push("</svg>");
    return parts.join("\n");
  }

  private downloadFile(blob: Blob, filename: string): void {
    if (typeof document === "undefined") return;
    const safeName = filename.replace(/[\\/]/g, "-");
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = safeName;
    anchor.rel = "noopener";
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
    window.setTimeout(() => URL.revokeObjectURL(url), 2000);
  }
}

function fetchColor(paint: Paint | undefined): string | null {
  if (!paint) return null;
  if (paint.type === "solid") {
    const { r, g, b, a } = paint.color;
    return `rgba(${Math.round(r * 255)}, ${Math.round(g * 255)}, ${Math.round(b * 255)}, ${a})`;
  }
  if (paint.type === "linear" || paint.type === "radial" || paint.type === "angular") {
    const stop = paint.stops[0];
    if (stop) {
      const { r, g, b, a } = stop.color;
      return `rgba(${Math.round(r * 255)}, ${Math.round(g * 255)}, ${Math.round(b * 255)}, ${a})`;
    }
  }
  return null;
}

function escapeXml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** Used by diagnostics: confirms the dev harness is the one running. */
export const DEV_HARNESS_LABEL = "Icon Crafter dev engine (browser harness) — no Rust engine attached";

void FILE_STORAGE_PREFIX;
