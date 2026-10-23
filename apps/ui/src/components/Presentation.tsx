import { useCallback, useEffect, useRef } from "react";
import type { PointerInput } from "../engine/contract";
import { getActivePump } from "../engine/store";
import { useAppStore } from "../engine/store";
import { prototypeBack, runCommand, stopPresentation } from "../engine/commands";
import { hotspotAtScreenPoint, screenHotspots } from "../engine/prototype";
import { en } from "../i18n/en";
import { ChevronLeft, Play, X } from "../icons";

function buttonFromEvent(event: React.PointerEvent): 0 | 1 | 2 {
  if (event.button === 1) return 1;
  if (event.button === 2) return 2;
  return 0;
}

function modifierPayload(event: React.PointerEvent) {
  return {
    shift: event.shiftKey,
    alt: event.altKey,
    ctrl: event.ctrlKey,
    meta: event.metaKey,
  };
}

/**
 * Full-window prototype presentation.
 *
 * The engine owns navigation: it returns the frame, hotspot geometry and
 * `can_back`. This layer renders the frame, draws hotspot outlines and forwards
 * pointer input; it never invents navigation state.
 */
export function Presentation() {
  const prototype = useAppStore((state) => state.engine.prototype);
  const viewport = useAppStore((state) => state.engine.viewport);
  const engine = useAppStore((state) => state.engine);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const frameCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const overlayCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const renderKeyRef = useRef<string>("");
  const hoverRef = useRef<string | null>(null);

  const paint = useCallback(async () => {
    if (!prototype?.active) return;
    const pump = getActivePump();
    const frameCanvas = frameCanvasRef.current;
    const overlayCanvas = overlayCanvasRef.current;
    if (!pump || !frameCanvas || !overlayCanvas) return;
    const dpr = Math.max(window.devicePixelRatio || 1, 1);
    const width = Math.round(viewport.width * dpr);
    const height = Math.round(viewport.height * dpr);
    if (width <= 0 || height <= 0) return;
    if (frameCanvas.width !== width || frameCanvas.height !== height) {
      frameCanvas.width = width;
      frameCanvas.height = height;
    }
    if (overlayCanvas.width !== width || overlayCanvas.height !== height) {
      overlayCanvas.width = width;
      overlayCanvas.height = height;
    }
    const key = `${prototype.frame_id}:${engine.revision}:${width}x${height}`;
    if (renderKeyRef.current === key) return;
    try {
      const buffer = await pump.renderFrame(1);
      if (buffer.byteLength !== width * height * 4) return;
      const context = frameCanvas.getContext("2d");
      if (!context) return;
      context.putImageData(new ImageData(new Uint8ClampedArray(buffer, 0, width * height * 4), width, height), 0, 0);
      renderKeyRef.current = key;
    } catch {
      // A failed frame simply keeps the previous one; engine errors surface as toasts.
    }

    const overlayContext = overlayCanvas.getContext("2d");
    if (!overlayContext) return;
    overlayContext.clearRect(0, 0, overlayCanvas.width, overlayCanvas.height);
    overlayContext.save();
    overlayContext.scale(dpr, dpr);
    const hotspots = screenHotspots(prototype.hotspots, viewport);
    overlayContext.lineWidth = 1;
    overlayContext.font = "11px Inter, system-ui, sans-serif";
    overlayContext.textBaseline = "middle";
    for (const hotspot of hotspots) {
      const hovered = hoverRef.current === hotspot.nodeId;
      overlayContext.strokeStyle = hovered ? "rgba(79,209,197,0.95)" : "rgba(79,209,197,0.55)";
      overlayContext.setLineDash([4, 3]);
      overlayContext.strokeRect(hotspot.rect.x, hotspot.rect.y, hotspot.rect.w, hotspot.rect.h);
      overlayContext.setLineDash([]);
      const label = hotspot.label || "Hotspot";
      const textWidth = overlayContext.measureText(label).width;
      const pillWidth = textWidth + 12;
      const pillHeight = 18;
      const x = hotspot.rect.x;
      const y = hotspot.rect.y - pillHeight - 3;
      overlayContext.fillStyle = hovered ? "rgba(79,209,197,0.28)" : "rgba(79,209,197,0.16)";
      overlayContext.beginPath();
      overlayContext.roundRect(x, y, pillWidth, pillHeight, 4);
      overlayContext.fill();
      overlayContext.fillStyle = "#d8fffb";
      overlayContext.fillText(label, x + 6, y + pillHeight / 2 + 0.5);
    }
    overlayContext.restore();
  }, [engine.revision, prototype, viewport]);

  // Keep the engine viewport in sync with the window; the engine re-fits the frame.
  useEffect(() => {
    if (!prototype?.active) return;
    const apply = () => {
      const dpr = Math.max(window.devicePixelRatio || 1, 1);
      void runCommand("set_viewport", { width: window.innerWidth, height: window.innerHeight, dpr }, { quiet: true });
    };
    apply();
    window.addEventListener("resize", apply);
    return () => window.removeEventListener("resize", apply);
  }, [prototype?.active]);

  // Render on frame/revision/viewport changes.
  useEffect(() => {
    void paint();
  }, [paint]);

  const hotspotUnder = (event: React.PointerEvent) => {
    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect) return null;
    return hotspotAtScreenPoint(prototype?.hotspots, viewport, {
      x: event.clientX - rect.left,
      y: event.clientY - rect.top,
    });
  };

  const sendPointer = (event: React.PointerEvent, kind: PointerInput["kind"]) => {
    const pump = getActivePump();
    const rect = containerRef.current?.getBoundingClientRect();
    if (!pump || !rect) return;
    pump.pointer({
      kind,
      button: buttonFromEvent(event),
      screen_x: event.clientX - rect.left,
      screen_y: event.clientY - rect.top,
      ...modifierPayload(event),
      pressure: 0.5,
      timestamp_ms: performance.now(),
    });
  };

  if (!prototype?.active || !engine.doc) {
    return (
      <div className="presentation-overlay presentation-overlay--empty" role="dialog" aria-label={en.presentation.errorTitle}>
        <div className="presentation-error">
          <p className="empty-state-title">{en.presentation.errorTitle}</p>
          <p className="empty-state-body">{en.presentation.errorBody}</p>
          <button type="button" className="button button--primary" onClick={() => void stopPresentation()}>
            {en.presentation.exit}
          </button>
        </div>
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      className="presentation-overlay"
      role="dialog"
      aria-modal="true"
      aria-label={en.presentation.frame.replace("{name}", prototype.frame_name)}
      onPointerDown={(event) => sendPointer(event, "down")}
      onPointerMove={(event) => {
        const hotspot = hotspotUnder(event);
        hoverRef.current = hotspot?.node_id ?? null;
        sendPointer(event, "move");
        void paint();
      }}
      onPointerUp={(event) => sendPointer(event, "up")}
      onPointerLeave={() => {
        hoverRef.current = null;
        getActivePump()?.pointer(eventlessPointer());
      }}
      onContextMenu={(event) => event.preventDefault()}
    >
      <canvas ref={frameCanvasRef} className="presentation-canvas" aria-hidden="true" />
      <canvas ref={overlayCanvasRef} className="presentation-overlay-canvas" aria-hidden="true" />

      <header className="presentation-header">
        <span className="presentation-title">
          <Play size={13} aria-hidden="true" />
          {en.presentation.frame.replace("{name}", prototype.frame_name)}
          {prototype.overlay_count > 0 && (
            <span className="presentation-overlay-count">
              {en.presentation.overlays.replace("{count}", String(prototype.overlay_count))}
            </span>
          )}
        </span>
        <div className="presentation-actions">
          {prototype.can_back && (
            <button
              type="button"
              className="button button--ghost button--small"
              onClick={() => void prototypeBack()}
              title={en.presentation.back}
            >
              <ChevronLeft size={13} /> {en.presentation.back}
            </button>
          )}
          <button
            type="button"
            className="button button--ghost button--small"
            onClick={() => void stopPresentation()}
            title={en.presentation.exit}
          >
            <X size={13} /> {en.presentation.exit}
          </button>
        </div>
      </header>

      <footer className="presentation-footer">
        <span>{en.presentation.hotspotHint}</span>
        <span className="status-badge status-badge--dev">Esc</span>
      </footer>
    </div>
  );
}

/** Empty pointer payload for `leave` (position is required by the contract). */
function eventlessPointer(): PointerInput {
  return {
    kind: "leave",
    button: 0,
    screen_x: 0,
    screen_y: 0,
    shift: false,
    alt: false,
    ctrl: false,
    meta: false,
    pressure: 0,
    timestamp_ms: performance.now(),
  };
}
