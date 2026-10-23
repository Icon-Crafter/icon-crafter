/**
 * Pure overlay geometry: converts engine overlay data (world space) into
 * screen-space geometry and answers hit-test questions the canvas needs.
 *
 * Kept free of React and DOM so it is unit-testable with plain values.
 */
import type { Handle, Overlay, Point, Rect, SelectionBox, ToolId, Viewport } from "./contract";
import { worldToScreen } from "./contract";

export interface ScreenRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function worldPointToScreen(point: Point, viewport: Viewport): Point {
  return worldToScreen(point, viewport);
}

export function screenCorners(box: SelectionBox, viewport: Viewport): [Point, Point, Point, Point] {
  const [tl, tr, br, bl] = box.corners;
  return [
    worldToScreen(tl, viewport),
    worldToScreen(tr, viewport),
    worldToScreen(br, viewport),
    worldToScreen(bl, viewport),
  ];
}

export function boundsOfPoints(points: readonly Point[]): ScreenRect | null {
  if (points.length === 0) return null;
  let minX = Number.POSITIVE_INFINITY;
  let minY = Number.POSITIVE_INFINITY;
  let maxX = Number.NEGATIVE_INFINITY;
  let maxY = Number.NEGATIVE_INFINITY;
  for (const point of points) {
    minX = Math.min(minX, point.x);
    minY = Math.min(minY, point.y);
    maxX = Math.max(maxX, point.x);
    maxY = Math.max(maxY, point.y);
  }
  return { x: minX, y: minY, w: maxX - minX, h: maxY - minY };
}

export function selectionScreenBounds(
  selection: readonly SelectionBox[],
  viewport: Viewport,
): ScreenRect | null {
  const points: Point[] = [];
  for (const box of selection) points.push(...screenCorners(box, viewport));
  return boundsOfPoints(points);
}

export function marqueeScreenRect(marquee: Rect | null, viewport: Viewport): ScreenRect | null {
  if (!marquee) return null;
  const tl = worldToScreen({ x: marquee.x, y: marquee.y }, viewport);
  return { x: tl.x, y: tl.y, w: marquee.w * viewport.zoom, h: marquee.h * viewport.zoom };
}

export function handleScreenPoint(handle: Handle, viewport: Viewport): Point {
  return worldToScreen(handle.pos, viewport);
}

/** Nearest handle within `radius` CSS px of a screen point, if any. */
export function findHandleAtPoint(
  handles: readonly Handle[],
  screenPoint: Point,
  viewport: Viewport,
  radius = 6,
): Handle | null {
  let best: Handle | null = null;
  let bestDistance = radius;
  for (const handle of handles) {
    const point = handleScreenPoint(handle, viewport);
    const distance = Math.hypot(point.x - screenPoint.x, point.y - screenPoint.y);
    if (distance <= bestDistance) {
      best = handle;
      bestDistance = distance;
    }
  }
  return best;
}

export interface CursorContext {
  tool: ToolId;
  overlay: Overlay;
  screenPoint: Point;
  viewport: Viewport;
  isPanning?: boolean;
  isTextEditing?: boolean;
}

const DRAW_TOOLS = new Set<ToolId>([
  "frame",
  "rectangle",
  "ellipse",
  "line",
  "polygon",
  "star",
  "pen",
  "text",
  "slice",
  "comment",
  "connector",
  "image",
]);

/** CSS cursor for the current tool/hover state. */
export function cursorFor(context: CursorContext): string {
  const { tool, overlay, screenPoint, viewport, isPanning, isTextEditing } = context;
  if (isTextEditing) return "text";
  if (isPanning || tool === "hand") return "grab";
  const handle = findHandleAtPoint(overlay.handles, screenPoint, viewport);
  if (handle) return handle.cursor;
  if (DRAW_TOOLS.has(tool)) return "crosshair";
  if (overlay.hover) return "move";
  return "default";
}

/**
 * Ruler tick spacing in world units: the smallest "nice" step whose on-screen
 * spacing is at least `minScreenGap` CSS pixels.
 */
export function niceRulerStep(zoom: number, minScreenGap = 56): number {
  const nice = [1, 2, 5, 10, 20, 25, 50, 100, 200, 250, 500, 1000, 2000, 5000, 10000];
  const exact = minScreenGap / Math.max(zoom, 0.0001);
  for (const step of nice) {
    if (step >= exact) return step;
  }
  return nice[nice.length - 1] ?? 10000;
}

export interface RulerTicks {
  step: number;
  ticks: number[];
}

/** Tick world-positions covering `length` CSS px starting at viewport world x/y. */
export function rulerTicks(
  originWorld: number,
  lengthCss: number,
  zoom: number,
  minScreenGap = 56,
): RulerTicks {
  const step = niceRulerStep(zoom, minScreenGap);
  const startWorld = originWorld - lengthCss / 2 / Math.max(zoom, 0.0001);
  const endWorld = originWorld + lengthCss / 2 / Math.max(zoom, 0.0001);
  const first = Math.floor(startWorld / step) * step;
  const ticks: number[] = [];
  for (let value = first; value <= endWorld && ticks.length < 400; value += step) {
    if (value >= startWorld) ticks.push(value);
  }
  return { step, ticks };
}

/** Position a floating toolbar centred above the selection (CSS px, clamped). */
export function floatingToolbarPosition(
  bounds: ScreenRect | null,
  viewport: Viewport,
  toolbarSize: { width: number; height: number },
  margin = 12,
): { x: number; y: number; placement: "above" | "below" | "center" } {
  if (!bounds) {
    return {
      x: Math.max(margin, viewport.width / 2 - toolbarSize.width / 2),
      y: Math.max(margin, viewport.height - toolbarSize.height - margin * 2),
      placement: "center",
    };
  }
  const x = Math.min(
    Math.max(margin, bounds.x + bounds.w / 2 - toolbarSize.width / 2),
    Math.max(margin, viewport.width - toolbarSize.width - margin),
  );
  const above = bounds.y - toolbarSize.height - margin;
  if (above > margin) return { x, y: above, placement: "above" };
  return {
    x,
    y: Math.min(bounds.y + bounds.h + margin, viewport.height - toolbarSize.height - margin),
    placement: "below",
  };
}
