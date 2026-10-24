import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { EngineState, Overlay, Point, Viewport } from "../engine/contract";
import { en } from "../i18n/en";
import { formatZoom } from "../utils/number";
import { colorToCss } from "../utils/color";
import {
  floatingToolbarPosition,
  rulerTicks,
  screenCorners,
  worldPointToScreen,
  type ScreenRect,
} from "../engine/overlayGeometry";
import { cursorFor, findHandleAtPoint } from "../engine/overlayGeometry";
import { appStore, useAppStore } from "../engine/store";
import { useUiStore } from "../engine/uiStore";
import type { EnginePump } from "../engine/input";
import {
  BooleanExcludeGlyph,
  BooleanIntersectGlyph,
  BooleanSubtractGlyph,
  BooleanUnionGlyph,
  GroupGlyph,
  Logo,
  Maximize2,
  Minus,
  Plus,
  Trash2,
  Undo2,
  UngroupGlyph,
} from "../icons";
import { requestNewDocument, requestOpenDocument, runCommand } from "../engine/commands";
import { IconButton } from "./primitives/IconButton";

const FRAME_QUALITY = 1;
// 0.4 ≈ 6 ms frames at 1200×800; the browser upscales while dragging.
const DRAFT_FRAME_QUALITY = 0.4;

/** Bitmap size for a frame at `quality` (mirrors Rust `device_size_scaled`). */
function frameTargetSize(viewport: { width: number; height: number; dpr: number }, quality: number): {
  width: number;
  height: number;
} {
  const dpr = viewport.dpr || window.devicePixelRatio || 1;
  return {
    width: Math.max(1, Math.round(Math.round(viewport.width * dpr) * quality)),
    height: Math.max(1, Math.round(Math.round(viewport.height * dpr) * quality)),
  };
}

interface CanvasSize {
  width: number;
  height: number;
}

function drawRuler(canvas: HTMLCanvasElement, orientation: "horizontal" | "vertical", viewport: Viewport) {
  const dpr = window.devicePixelRatio || 1;
  const cssWidth = canvas.clientWidth || 1;
  const cssHeight = canvas.clientHeight || 1;
  const scale = Math.max(1, Math.min(3, dpr));
  if (canvas.width !== Math.round(cssWidth * scale) || canvas.height !== Math.round(cssHeight * scale)) {
    canvas.width = Math.round(cssWidth * scale);
    canvas.height = Math.round(cssHeight * scale);
  }
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.setTransform(scale, 0, 0, scale, 0, 0);
  ctx.clearRect(0, 0, cssWidth, cssHeight);
  ctx.fillStyle = "#0d1015";
  ctx.fillRect(0, 0, cssWidth, cssHeight);

  const zoom = viewport.zoom;
  const length = orientation === "horizontal" ? cssWidth : cssHeight;
  // `rulerTicks` centers its tick window on the origin it receives; the ruler
  // canvas is anchored at the viewport's top-left corner, so pass the center.
  const origin = orientation === "horizontal" ? viewport.x : viewport.y;
  const centerWorld = origin + length / 2 / Math.max(zoom, 0.0001);
  const ruler = rulerTicks(centerWorld, length, zoom);
  const step = ruler.step;
  ctx.strokeStyle = "rgba(58, 66, 80, 0.65)";
  ctx.fillStyle = "rgba(163, 171, 186, 0.85)";
  ctx.font = "9px Inter, system-ui, sans-serif";
  ctx.textBaseline = "middle";
  ctx.lineWidth = 1;

  for (const tick of ruler.ticks) {
    const pos = (tick - origin) * zoom;
    if (pos < -1 || pos > length + 1) continue;
    const major = Math.round(tick / step) % 5 === 0;
    if (orientation === "horizontal") {
      ctx.beginPath();
      ctx.moveTo(Math.round(pos) + 0.5, major ? 12 : 15);
      ctx.lineTo(Math.round(pos) + 0.5, 20);
      ctx.stroke();
      if (major) ctx.fillText(String(Math.round(tick)), Math.round(pos) + 3, 7);
    } else {
      ctx.beginPath();
      ctx.moveTo(major ? 12 : 15, Math.round(pos) + 0.5);
      ctx.lineTo(20, Math.round(pos) + 0.5);
      ctx.stroke();
      if (major) {
        ctx.save();
        ctx.translate(7, Math.round(pos) + 3);
        ctx.rotate(-Math.PI / 2);
        ctx.textBaseline = "middle";
        ctx.fillText(String(Math.round(tick)), 0, 0);
        ctx.restore();
      }
    }
  }

  ctx.strokeStyle = "rgba(255, 255, 255, 0.06)";
  ctx.beginPath();
  if (orientation === "horizontal") {
    ctx.moveTo(0, cssHeight - 0.5);
    ctx.lineTo(cssWidth, cssHeight - 0.5);
  } else {
    ctx.moveTo(cssWidth - 0.5, 0);
    ctx.lineTo(cssWidth - 0.5, cssHeight);
  }
  ctx.stroke();
}

function drawOverlay(
  canvas: HTMLCanvasElement,
  overlay: Overlay,
  viewport: Viewport,
  state: EngineState,
) {
  const dpr = window.devicePixelRatio || 1;
  const cssWidth = canvas.clientWidth || 1;
  const cssHeight = canvas.clientHeight || 1;
  const scale = Math.max(1, Math.min(3, dpr));
  if (canvas.width !== Math.round(cssWidth * scale) || canvas.height !== Math.round(cssHeight * scale)) {
    canvas.width = Math.round(cssWidth * scale);
    canvas.height = Math.round(cssHeight * scale);
  }
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.setTransform(scale, 0, 0, scale, 0, 0);
  ctx.clearRect(0, 0, cssWidth, cssHeight);

  // Snap lines (world) — drawn underneath selection so handles stay crisp.
  ctx.strokeStyle = "rgba(255, 107, 74, 0.85)";
  ctx.lineWidth = 1;
  for (const line of overlay.snap_lines) {
    ctx.beginPath();
    if (line.axis === "x") {
      const start = worldPointToScreen({ x: line.pos, y: line.from }, viewport);
      const end = worldPointToScreen({ x: line.pos, y: line.to }, viewport);
      ctx.moveTo(start.x + 0.5, start.y);
      ctx.lineTo(end.x + 0.5, end.y);
    } else {
      const start = worldPointToScreen({ x: line.from, y: line.pos }, viewport);
      const end = worldPointToScreen({ x: line.to, y: line.pos }, viewport);
      ctx.moveTo(start.x, start.y + 0.5);
      ctx.lineTo(end.x, end.y + 0.5);
    }
    ctx.stroke();
  }

  // Guides.
  ctx.strokeStyle = "rgba(79, 209, 197, 0.8)";
  ctx.setLineDash([4, 4]);
  for (const guide of overlay.guides) {
    ctx.beginPath();
    if (guide.axis === "x") {
      const point = worldPointToScreen({ x: guide.pos, y: 0 }, viewport);
      ctx.moveTo(point.x + 0.5, 0);
      ctx.lineTo(point.x + 0.5, cssHeight);
    } else {
      const point = worldPointToScreen({ x: 0, y: guide.pos }, viewport);
      ctx.moveTo(0, point.y + 0.5);
      ctx.lineTo(cssWidth, point.y + 0.5);
    }
    ctx.stroke();
  }
  ctx.setLineDash([]);

  // Marquee.
  if (overlay.marquee) {
    const box = overlay.marquee;
    const tl = worldPointToScreen({ x: box.x, y: box.y }, viewport);
    const br = worldPointToScreen({ x: box.x + box.w, y: box.y + box.h }, viewport);
    ctx.fillStyle = "rgba(79, 209, 197, 0.08)";
    ctx.strokeStyle = "rgba(79, 209, 197, 0.9)";
    ctx.lineWidth = 1;
    ctx.fillRect(tl.x, tl.y, br.x - tl.x, br.y - tl.y);
    ctx.strokeRect(
      Math.round(tl.x) + 0.5,
      Math.round(tl.y) + 0.5,
      Math.round(br.x - tl.x),
      Math.round(br.y - tl.y),
    );
  }

  // Hover rectangle (only when no selection or hover not selected).
  if (overlay.hover && !state.selection.includes(overlay.hover.id)) {
    const corners = screenCorners(overlay.hover, viewport);
    ctx.strokeStyle = "rgba(163, 171, 186, 0.55)";
    ctx.lineWidth = 1;
    ctx.setLineDash([3, 2]);
    ctx.beginPath();
    ctx.moveTo(corners[0]!.x, corners[0]!.y);
    for (let i = 1; i < corners.length; i += 1) ctx.lineTo(corners[i]!.x, corners[i]!.y);
    ctx.closePath();
    ctx.stroke();
    ctx.setLineDash([]);
  }

  // Selection quads.
  for (const box of overlay.selection) {
    const corners = screenCorners(box, viewport);
    ctx.beginPath();
    ctx.moveTo(corners[0]!.x, corners[0]!.y);
    for (let i = 1; i < corners.length; i += 1) ctx.lineTo(corners[i]!.x, corners[i]!.y);
    ctx.closePath();
    ctx.strokeStyle = "#4fd1c5";
    ctx.lineWidth = 1.25;
    ctx.stroke();
  }

  // Handles.
  for (const handle of overlay.handles) {
    const point = worldPointToScreen(handle.pos, viewport);
    const isRotate = handle.kind === "rotate";
    ctx.save();
    ctx.translate(point.x, point.y);
    if (isRotate) {
      ctx.beginPath();
      ctx.arc(0, 0, 4, 0, Math.PI * 2);
      ctx.fillStyle = "#101318";
      ctx.fill();
      ctx.strokeStyle = "#4fd1c5";
      ctx.lineWidth = 1.25;
      ctx.stroke();
    } else {
      ctx.fillStyle = "#0b0d11";
      ctx.fillRect(-3.5, -3.5, 7, 7);
      ctx.strokeStyle = "#4fd1c5";
      ctx.lineWidth = 1.25;
      ctx.strokeRect(-3.5, -3.5, 7, 7);
    }
    ctx.restore();
  }

  // Peer cursors.
  for (const cursor of overlay.cursors) {
    const point = worldPointToScreen(cursor.world, viewport);
    const color = cursor.color;
    ctx.fillStyle = color;
    ctx.beginPath();
    ctx.moveTo(point.x, point.y);
    ctx.lineTo(point.x + 10, point.y + 3.5);
    ctx.lineTo(point.x + 4.2, point.y + 5.2);
    ctx.lineTo(point.x + 3, point.y + 11);
    ctx.closePath();
    ctx.fill();
    if (cursor.name) {
      ctx.font = "10px Inter, system-ui, sans-serif";
      const width = ctx.measureText(cursor.name).width + 8;
      ctx.fillStyle = color;
      ctx.globalAlpha = 0.92;
      ctx.fillRect(point.x + 10, point.y + 9, width, 14);
      ctx.globalAlpha = 1;
      ctx.fillStyle = "#0b0d11";
      ctx.textBaseline = "middle";
      ctx.fillText(cursor.name, point.x + 14, point.y + 16.5);
    }
  }

  // Text-edit caret (screen coordinates from the engine).
  if (overlay.text_edit && state.text_edit) {
    const caret = overlay.text_edit.caret;
    const height = Math.max(12, state.text_edit.font.size * viewport.zoom);
    ctx.fillStyle = "#ff6b4a";
    ctx.fillRect(Math.round(caret.x), Math.round(caret.y), 1.5, height);
  }

  // Tool hint pill handled in DOM (canvas-hint), not here.
}

/** Small bracket shape used for off-screen selection corners. */
function screenBoundsOfOverlay(overlay: Overlay, viewport: Viewport): ScreenRect | null {
  const points: Point[] = [];
  for (const box of overlay.selection) points.push(...screenCorners(box, viewport));
  if (points.length === 0) return null;
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  const x = Math.min(...xs);
  const y = Math.min(...ys);
  return { x, y, w: Math.max(...xs) - x, h: Math.max(...ys) - y };
}

export function CanvasStage() {
  const containerRef = useRef<HTMLDivElement>(null);
  const frameCanvasRef = useRef<HTMLCanvasElement>(null);
  const overlayCanvasRef = useRef<HTMLCanvasElement>(null);
  const rulerHRef = useRef<HTMLCanvasElement>(null);
  const rulerVRef = useRef<HTMLCanvasElement>(null);
  const surfaceRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const status = useAppStore((state) => state.status);
  const engine = useAppStore((state) => state.engine);
  const setFrameStats = useAppStore((state) => state.setFrameStats);

  const [size, setSize] = useState<CanvasSize>({ width: 0, height: 0 });
  const [cursor, setCursor] = useState("default");
  const [textDraft, setTextDraft] = useState("");
  const panning = useRef(false);
  const interactingRef = useRef(false);
  const interactionTimer = useRef<number | null>(null);
  const [interacting, setInteracting] = useState(false);
  const lastPointer = useRef<{ x: number; y: number } | null>(null);
  const inFlightFrame = useRef(false);
  const lastPainted = useRef<{ frameId: number; revision: number; width: number; height: number; quality: number } | null>(null);
  const textNodeRef = useRef<string | null>(null);
  const fpsRef = useRef<{ frames: number; start: number }>({ frames: 0, start: performance.now() });

  const pump = appStore.getState().pump;
  const hasDocument = engine.doc !== null;

  /* ---------------- viewport measurement ---------------- */

  useEffect(() => {
    const element = containerRef.current;
    if (!element) return;
    const update = () => {
      const rect = element.getBoundingClientRect();
      setSize({ width: Math.max(1, Math.round(rect.width)), height: Math.max(1, Math.round(rect.height)) });
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    window.addEventListener("resize", update);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", update);
    };
  }, []);

  useEffect(() => {
    const activePump = pump ?? appStore.getState().pump;
    if (!activePump || size.width < 2 || size.height < 2) return;
    if (engine.doc === null) return;
    const dpr = window.devicePixelRatio || 1;
    const viewport = engine.viewport;
    const matches =
      Math.abs(viewport.width - size.width) < 1 &&
      Math.abs(viewport.height - size.height) < 1 &&
      Math.abs(viewport.dpr - dpr) < 0.01;
    if (matches) return;
    // The engine's default viewport (1200×800) and a document reset can leave
    // the render size and the CSS canvas size out of sync; that stretches the
    // frame canvas non-uniformly and makes the overlay (drawn in CSS space)
    // disagree with the rendered pixels. Re-send the measured size whenever it
    // drifts, including right after a document is opened.
    void activePump
      .command("set_viewport", { width: size.width, height: size.height, dpr })
      .catch(() => {
        /* engine reports through errors */
      });
  }, [pump, size.width, size.height, engine.viewport, engine.doc, engine]);

  /* ---------------- frame painting ---------------- */

  const paintFrame = useCallback(
    async (activePump: EnginePump, state: EngineState) => {
      const quality = interactingRef.current ? DRAFT_FRAME_QUALITY : FRAME_QUALITY;
      // Never paint while the engine viewport disagrees with the measured CSS
      // size: the canvas would be stretched and the overlay misaligned.
      if (
        size.width >= 2 &&
        size.height >= 2 &&
        (Math.abs(state.viewport.width - size.width) > 1 ||
          Math.abs(state.viewport.height - size.height) > 1)
      ) {
        return;
      }
      const key = {
        frameId: state.frame_id,
        revision: state.revision,
        width: state.viewport.width,
        height: state.viewport.height,
        quality,
      };
      const last = lastPainted.current;
      if (
        last &&
        last.frameId === key.frameId &&
        last.revision === key.revision &&
        last.width === key.width &&
        last.height === key.height &&
        last.quality === key.quality
      ) {
        return;
      }
      if (inFlightFrame.current) return;
      inFlightFrame.current = true;
      const started = performance.now();
      try {
        const buffer = await activePump.renderFrame(quality);
        const viewport = state.viewport;
        const { width, height } = frameTargetSize(viewport, quality);
        if (buffer.byteLength < width * height * 4) {
          return;
        }
        const canvas = frameCanvasRef.current;
        if (!canvas) return;
        if (canvas.width !== width || canvas.height !== height) {
          canvas.width = width;
          canvas.height = height;
        }
        const context = canvas.getContext("2d");
        if (!context) return;
        const imageData = new ImageData(new Uint8ClampedArray(buffer, 0, width * height * 4), width, height);
        context.putImageData(imageData, 0, 0);
        lastPainted.current = key;
        const now = performance.now();
        fpsRef.current.frames += 1;
        if (now - fpsRef.current.start >= 1000) {
          setFrameStats({
            fps: Math.round((fpsRef.current.frames * 1000) / (now - fpsRef.current.start)),
            paintMs: Math.round(now - started),
          });
          fpsRef.current = { frames: 0, start: now };
        }
      } catch (error) {
        // Silent: the transport already surfaces errors via the store.
        void error;
      } finally {
        inFlightFrame.current = false;
      }
    },
    [setFrameStats, size.width, size.height],
  );

  useEffect(() => {
    const activePump = pump ?? appStore.getState().pump;
    if (!activePump || status !== "ready") return;
    if (engine.doc === null) return;
    if (size.width < 2 || size.height < 2) return;
    void paintFrame(activePump, engine);
  }, [pump, status, engine, engine.frame_id, engine.revision, engine.viewport.width, engine.viewport.height, size.width, size.height, interacting, paintFrame]);

  /* ---------------- overlay + rulers ---------------- */

  const screenBounds = useMemo(() => screenBoundsOfOverlay(engine.overlay, engine.viewport), [engine.overlay, engine.viewport]);
  const selectionCount = engine.selection.length;

  useEffect(() => {
    const canvas = overlayCanvasRef.current;
    if (!canvas) return;
    drawOverlay(canvas, engine.overlay, engine.viewport, engine);
  }, [engine, engine.overlay, engine.viewport, engine.revision]);

  useEffect(() => {
    if (rulerHRef.current) drawRuler(rulerHRef.current, "horizontal", engine.viewport);
    if (rulerVRef.current) drawRuler(rulerVRef.current, "vertical", engine.viewport);
  }, [engine.viewport, size.width, size.height]);

  /* ---------------- pointer input ---------------- */

  const sendPointer = useCallback(
    (kind: "down" | "move" | "up" | "double_click" | "leave", event: React.PointerEvent | React.MouseEvent) => {
      const activePump = pump ?? appStore.getState().pump;
      const surface = surfaceRef.current;
      if (!activePump || !surface) return;
      const rect = surface.getBoundingClientRect();
      activePump.pointer({
        kind,
        button: event.button === 1 ? 1 : event.button === 2 ? 2 : 0,
        screen_x: event.clientX - rect.left,
        screen_y: event.clientY - rect.top,
        shift: event.shiftKey,
        alt: event.altKey,
        ctrl: event.ctrlKey,
        meta: event.metaKey,
        pressure: kind === "move" || kind === "down" ? 0.5 : undefined,
        timestamp_ms: performance.now(),
      });
    },
    [pump],
  );

  const beginInteraction = () => {
    if (!interactingRef.current) {
      interactingRef.current = true;
      setInteracting(true);
    }
  };

  const endInteraction = () => {
    if (interactionTimer.current !== null) {
      window.clearTimeout(interactionTimer.current);
      interactionTimer.current = null;
    }
    if (interactingRef.current) {
      interactingRef.current = false;
      setInteracting(false);
    }
  };

  const onPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button === 0) surfaceRef.current?.focus();
    beginInteraction();
    lastPointer.current = { x: event.clientX, y: event.clientY };
    if (event.button === 1) {
      panning.current = true;
      event.preventDefault();
    }
    try {
      (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    } catch {
      // Pointer capture is best-effort: synthetic events and some input devices
      // report pointer ids the browser does not consider active.
    }
    sendPointer("down", event);
  };

  const onPointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const previous = lastPointer.current;
    lastPointer.current = { x: event.clientX, y: event.clientY };
    sendPointer("move", event);
    if (panning.current && previous) {
      const dx = event.clientX - previous.x;
      const dy = event.clientY - previous.y;
      if (dx !== 0 || dy !== 0) {
        const activePump = pump ?? appStore.getState().pump;
        void activePump?.command("pan_by", { dx: dx / engine.viewport.zoom, dy: dy / engine.viewport.zoom });
      }
    }
    // Cursor feedback per tool / hover / handle.
    const activePump = appStore.getState().pump;
    if (!activePump) return;
    const surface = surfaceRef.current;
    if (!surface) return;
    const rect = surface.getBoundingClientRect();
    const point = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    const handle = findHandleAtPoint(engine.overlay.handles, point, engine.viewport, 7);
    setCursor(
      cursorFor({
        tool: engine.tool,
        overlay: engine.overlay,
        screenPoint: point,
        viewport: engine.viewport,
        isPanning: panning.current,
      }) ?? (handle ? handle.cursor : "default"),
    );
  };

  const onPointerUp = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    panning.current = false;
    endInteraction();
    sendPointer("up", event);
  };

  const onPointerLeave = (event: React.PointerEvent<HTMLDivElement>) => {
    endInteraction();
    sendPointer("leave", event);
    setCursor("default");
  };

  const onWheel = (event: React.WheelEvent<HTMLDivElement>) => {
    event.preventDefault();
    beginInteraction();
    if (interactionTimer.current !== null) {
      window.clearTimeout(interactionTimer.current);
    }
    interactionTimer.current = window.setTimeout(() => {
      interactionTimer.current = null;
      endInteraction();
    }, 180);
    const activePump = pump ?? appStore.getState().pump;
    const surface = surfaceRef.current;
    if (!activePump || !surface) return;
    const rect = surface.getBoundingClientRect();
    activePump.wheel({
      delta_x: event.deltaX,
      delta_y: event.deltaY,
      screen_x: event.clientX - rect.left,
      screen_y: event.clientY - rect.top,
      shift: event.shiftKey,
      ctrl: event.ctrlKey,
      alt: event.altKey,
      meta: event.metaKey,
    });
  };

  /* ---------------- text editing ---------------- */

  const textEdit = engine.text_edit;
  useEffect(() => {
    if (!textEdit) {
      textNodeRef.current = null;
      return;
    }
    if (textNodeRef.current !== textEdit.node_id) {
      textNodeRef.current = textEdit.node_id;
      setTextDraft(textEdit.characters);
      const textarea = textareaRef.current;
      if (textarea) {
        textarea.value = textEdit.characters;
        textarea.focus();
        textarea.setSelectionRange(textEdit.caret, textEdit.caret);
      }
    }
  }, [textEdit]);

  const sendTextInput = (textarea: HTMLTextAreaElement) => {
    const activePump = pump ?? appStore.getState().pump;
    if (!activePump) return;
    setTextDraft(textarea.value);
    void activePump.command("text_edit_input", {
      characters: textarea.value,
      caret: textarea.selectionStart,
      selection:
        textarea.selectionStart !== textarea.selectionEnd
          ? [textarea.selectionStart, textarea.selectionEnd]
          : null,
    });
  };

  /* ---------------- derived UI ---------------- */

  const floating = useMemo(() => {
    if (!screenBounds || selectionCount === 0 || textEdit) return null;
    const result = floatingToolbarPosition(screenBounds, engine.viewport, { width: 380, height: 36 }, 12);
    return { left: result.x, top: result.y };
  }, [screenBounds, selectionCount, textEdit, engine.viewport]);

  const zoomPercent = formatZoom(engine.viewport.zoom);
  const showEmpty = status === "ready" && !hasDocument;
  const showConnecting = status === "connecting";
  return (
    <div className="canvas-stage" ref={containerRef}>
      <div className="canvas-stack">
        <canvas ref={frameCanvasRef} className="canvas-frame" aria-hidden="true" />
        <canvas ref={overlayCanvasRef} className="canvas-overlay" aria-hidden="true" />
        <div
          ref={surfaceRef}
          className="canvas-surface"
          role="application"
          aria-label={en.canvas.workspaceLabel}
          tabIndex={0}
          style={{ cursor }}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
          onPointerLeave={onPointerLeave}
          onDoubleClick={(event) => sendPointer("double_click", event)}
          onWheel={onWheel}
          onContextMenu={(event) => {
            event.preventDefault();
            useUiStore.getState().openCanvasMenu(event.clientX, event.clientY);
          }}
        />
      </div>

      <div className="canvas-rulers" aria-hidden="true">
        <div className="ruler ruler--h">
          <canvas ref={rulerHRef} />
        </div>
        <div className="ruler ruler--v">
          <canvas ref={rulerVRef} />
        </div>
        <button
          type="button"
          className="ruler-corner"
          aria-label={en.canvas.zoomToFit}
          title={en.canvas.zoomToFit}
          onClick={() => void runCommand("zoom_fit")}
        >
          <Maximize2 size={11} />
        </button>
      </div>

      {/* Floating zoom widget */}
      <div className="floating-zoom" aria-label={en.canvas.zoomControls}>
        <IconButton label={en.toolbar.zoomOut} size="sm" onClick={() => void runCommand("zoom_out")}>
          <Minus size={13} />
        </IconButton>
        <button
          type="button"
          className="floating-zoom-value"
          title={en.toolbar.zoomTo100}
          aria-label={en.toolbar.zoomTo100}
          onClick={() => void runCommand("zoom_100")}
        >
          {zoomPercent}
        </button>
        <IconButton label={en.toolbar.zoomIn} size="sm" onClick={() => void runCommand("zoom_in")}>
          <Plus size={13} />
        </IconButton>
      </div>

      {/* Floating selection toolbar */}
      {floating && (
        <div
          className="floating-toolbar"
          style={{ left: floating.left, top: floating.top }}
          role="toolbar"
          aria-label={en.canvas.selectionActions}
          onPointerDown={(event) => event.stopPropagation()}
        >
          <div className="floating-toolbar-group">
            <IconButton
              label={en.menu.undo}
              shortcut="Ctrl+Z"
              size="sm"
              onClick={() => void runCommand("undo")}
            >
              <Undo2 size={14} />
            </IconButton>
          </div>
          <div className="floating-toolbar-divider" />
          <div className="floating-toolbar-group" aria-label={en.inspector.align}>
            <IconButton label={en.inspector.alignLeft} size="sm" onClick={() => void runCommand("align", { align: "left" })}>
              <span style={{ fontSize: 9, fontWeight: 700 }}>L</span>
            </IconButton>
            <IconButton label={en.inspector.alignCenterH} size="sm" onClick={() => void runCommand("align", { align: "hcenter" })}>
              <span style={{ fontSize: 9, fontWeight: 700 }}>C</span>
            </IconButton>
            <IconButton label={en.inspector.alignRight} size="sm" onClick={() => void runCommand("align", { align: "right" })}>
              <span style={{ fontSize: 9, fontWeight: 700 }}>R</span>
            </IconButton>
          </div>
          <div className="floating-toolbar-divider" />
          <div className="floating-toolbar-group" aria-label={en.inspector.boolean}>
            <IconButton label={en.inspector.booleanUnion} size="sm" onClick={() => void runCommand("boolean", { op: "union" })}>
              <BooleanUnionGlyph width={14} height={14} />
            </IconButton>
            <IconButton label={en.inspector.booleanSubtract} size="sm" onClick={() => void runCommand("boolean", { op: "subtract" })}>
              <BooleanSubtractGlyph width={14} height={14} />
            </IconButton>
            <IconButton label={en.inspector.booleanIntersect} size="sm" onClick={() => void runCommand("boolean", { op: "intersect" })}>
              <BooleanIntersectGlyph width={14} height={14} />
            </IconButton>
            <IconButton label={en.inspector.booleanExclude} size="sm" onClick={() => void runCommand("boolean", { op: "exclude" })}>
              <BooleanExcludeGlyph width={14} height={14} />
            </IconButton>
          </div>
          <div className="floating-toolbar-divider" />
          <div className="floating-toolbar-group">
            <IconButton label={en.menu.group} size="sm" onClick={() => void runCommand("group")}>
              <GroupGlyph width={14} height={14} />
            </IconButton>
            <IconButton label={en.menu.ungroup} size="sm" onClick={() => void runCommand("ungroup")}>
              <UngroupGlyph width={14} height={14} />
            </IconButton>
          </div>
          <div className="floating-toolbar-divider" />
          <div className="floating-toolbar-group">
            <IconButton label={en.common.delete} size="sm" onClick={() => void runCommand("delete")}>
              <Trash2 size={13} />
            </IconButton>
          </div>
        </div>
      )}

      {/* Text edit overlay */}
      {textEdit && (
        <textarea
          ref={textareaRef}
          className="text-edit-layer"
          aria-label={en.canvas.textEditor}
          spellCheck={false}
          wrap="off"
          style={{
            left: `${textEdit.screen_rect.x}px`,
            top: `${textEdit.screen_rect.y}px`,
            width: `${Math.max(40, textEdit.screen_rect.w)}px`,
            height: `${Math.max(20, textEdit.screen_rect.h)}px`,
            fontSize: `${Math.max(9, textEdit.font.size * engine.viewport.zoom)}px`,
            fontFamily: textEdit.font.family,
            fontWeight: textEdit.font.weight,
            fontStyle: textEdit.font.italic ? "italic" : "normal",
            lineHeight: `${Math.max(10, textEdit.font.size * engine.viewport.zoom * textEdit.font.line_height)}px`,
            letterSpacing: `${textEdit.font.letter_spacing * engine.viewport.zoom}px`,
            textAlign: textEdit.font.align as React.CSSProperties["textAlign"],
            color: colorToCss(textEdit.color),
          }}
          value={textDraft}
          onChange={(event) => sendTextInput(event.currentTarget)}
          onSelect={(event) => {
            const textarea = event.currentTarget;
            const activePump = pump ?? appStore.getState().pump;
            void activePump?.command("text_edit_set_caret", {
              caret: textarea.selectionStart,
              selection:
                textarea.selectionStart !== textarea.selectionEnd
                  ? [textarea.selectionStart, textarea.selectionEnd]
                  : null,
            });
          }}
          onKeyDown={(event) => {
            event.stopPropagation();
            if (event.key === "Escape") {
              event.preventDefault();
              void (pump ?? appStore.getState().pump)?.command("text_edit_cancel");
            } else if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
              event.preventDefault();
              void (pump ?? appStore.getState().pump)?.command("text_edit_commit");
            }
          }}
          onBlur={() => {
            void (pump ?? appStore.getState().pump)?.command("text_edit_commit");
          }}
        />
      )}

      {/* Tool hint / status message */}
      {engine.overlay.tool_hint && <div className="canvas-hint">{engine.overlay.tool_hint}</div>}

      {showConnecting && (
        <div className="canvas-loading" role="status">
          <Logo size={40} />
          <span>{en.canvas.loading}</span>
        </div>
      )}

      {showEmpty && (
        <div className="canvas-empty">
          <div className="canvas-empty-card">
            <Logo size={64} className="canvas-empty-brand" />
            <div className="canvas-empty-title">{en.canvas.noDocumentTitle}</div>
            <p className="canvas-empty-body">{en.canvas.noDocumentBody}</p>
            <div className="canvas-empty-actions">
              <button
                type="button"
                className="button button--primary"
                onClick={() => void requestNewDocument()}
              >
                {en.canvas.newDocument}
              </button>
              <button type="button" className="button" onClick={() => void requestOpenDocument()}>
                {en.canvas.openDocument}
              </button>
            </div>
          </div>
        </div>
      )}

      {status === "ready" &&
        hasDocument &&
        selectionCount === 0 &&
        engine.tool === "move" &&
        (engine.summary?.nodes.length ?? 0) === 0 && (
          <div className="canvas-hint" aria-hidden="true">
            {en.canvas.emptyPageHint}
          </div>
        )}

    </div>
  );
}
