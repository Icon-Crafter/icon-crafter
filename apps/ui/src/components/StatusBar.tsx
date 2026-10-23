import { useAppStore } from "../engine/store";
import { engineEnvironmentLabel, runCommand, shortcutHint } from "../engine/commands";
import { en } from "../i18n/en";
import { format } from "../i18n/en";
import { toolLabel } from "../i18n/labels";
import { formatLength, formatZoom } from "../utils/number";
import { Minus, Plus } from "../icons";

/**
 * Bottom status bar: documents, counts, zoom, tool, frame stats and transport
 * identity. Read-only except the zoom controls, which dispatch real commands.
 */
export function StatusBar() {
  const status = useAppStore((state) => state.status);
  const engine = useAppStore((state) => state.engine);
  const frameStats = useAppStore((state) => state.frameStats);
  const transportKind = useAppStore((state) => state.transportKind);

  const summary = engine.summary;
  const activePage = summary?.pages.find((page) => page.id === summary.active_page) ?? null;
  const nodeCount = summary?.nodes.length ?? 0;
  const single = engine.selection_details.length === 1 ? engine.selection_details[0] : null;
  const zoom = engine.viewport.zoom;
  const ready = status === "ready";

  return (
    <footer className="status-bar" role="contentinfo">
      <span className="status-item">
        <span
          className={`status-dot ${
            status === "ready" ? "status-dot--ok" : status === "connecting" ? "status-dot--pending" : "status-dot--offline"
          }`}
          aria-hidden="true"
        />
        {ready ? en.status.ready : status === "connecting" ? en.app.engineConnecting : en.status.noDocument}
      </span>
      <span className="status-item">{engine.doc ? engine.doc.name : en.app.untitled}</span>
      {activePage && <span className="status-item">{format(en.status.page, { name: activePage.name })}</span>}
      <span className="status-sep" aria-hidden="true" />
      <span className="status-item status-item--mono">{format(en.status.objects, { count: nodeCount })}</span>
      <span className="status-item status-item--mono">
        {engine.selection.length > 0
          ? format(en.status.selected, { count: engine.selection.length })
          : en.status.selectionNone}
      </span>
      {single && (
        <span className="status-item status-item--mono">
          {format(en.status.size, {
            w: formatLength(single.absolute_bounds.w),
            h: formatLength(single.absolute_bounds.h),
          })}
        </span>
      )}
      <span className="status-item">{format(en.status.tool, { tool: toolLabel(engine.tool) })}</span>
      <span className="status-spacer" />
      <div className="status-zoom" role="group" aria-label={en.canvas.zoomControls}>
        <button
          type="button"
          className="status-zoom-button"
          aria-label={en.menu.zoomOut}
          title={`${en.menu.zoomOut} (${shortcutHint("view.zoomOut")})`}
          onClick={() => void runCommand("zoom_out")}
          disabled={!ready}
        >
          <Minus size={12} />
        </button>
        <button
          type="button"
          className="status-zoom-value"
          title={`${en.menu.zoom100} (${shortcutHint("view.zoom100")})`}
          onClick={() => void runCommand("zoom_100")}
          disabled={!ready}
        >
          {formatZoom(zoom)}
        </button>
        <button
          type="button"
          className="status-zoom-button"
          aria-label={en.menu.zoomIn}
          title={`${en.menu.zoomIn} (${shortcutHint("view.zoomIn")})`}
          onClick={() => void runCommand("zoom_in")}
          disabled={!ready}
        >
          <Plus size={12} />
        </button>
      </div>
      <span className="status-item status-item--mono">
        {format(en.status.zoom, { value: formatZoom(zoom) })}
      </span>
      {frameStats.fps > 0 && (
        <span className="status-item status-item--mono" title="Frames drawn per second by the canvas loop">
          {format(en.status.fps, { fps: Math.round(frameStats.fps) })}
        </span>
      )}
      <span className="status-item status-item--mono">{format(en.status.revision, { revision: engine.revision })}</span>
      {engine.dirty && (
        <span className="status-item status-item--warning" title={en.chrome.stateUnsaved}>
          {format(en.status.unsaved, {})}
        </span>
      )}
      <span
        className={`status-badge${transportKind === "dev" ? " status-badge--dev" : ""}`}
        title={transportKind === "dev" ? en.app.devTooltip : en.status.engineOk}
      >
        {transportKind === "dev" ? en.app.devBadge : engineEnvironmentLabel()}
      </span>
    </footer>
  );
}
