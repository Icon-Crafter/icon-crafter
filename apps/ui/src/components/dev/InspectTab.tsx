/**
 * Dev Mode — Inspect tab.
 *
 * Read-only resolved values for the current selection (`dev_inspect`). The
 * engine re-resolves on every call; the panel refreshes when the document
 * revision or the selection changes (debounced to avoid a burst of IPC during
 * a drag).
 */
import { useEffect, useMemo } from "react";

import type { NodeInspect, NodeKind, PaintInfo } from "../../engine/contract";
import { useDevModeStore } from "../../engine/devModeStore";
import { useAppStore } from "../../engine/store";
import { formatColorCss, formatFillCss, formatMeasurements } from "../../engine/devMode";
import { en } from "../../i18n/en";
import { nodeKindLabel } from "../../i18n/labels";
import { CopyButton } from "./shared";

function round(value: number, digits = 2): string {
  return String(Number(value.toFixed(digits)));
}

function paintToCss(paint: PaintInfo): string | null {
  if (paint.kind === "solid") return formatFillCss(paint);
  if (paint.stops.length === 0) return null;
  const stops = paint.stops
    .map((stop) => `${formatColorCss(stop.color)} ${round(stop.position * 100)}%`)
    .join(", ");
  return `linear-gradient(90deg, ${stops})`;
}

function PaintText({ paint }: { paint: PaintInfo }) {
  const css = paintToCss(paint);
  const label =
    paint.kind === "solid"
      ? (paint.hex ?? "—")
      : paint.kind === "image"
        ? "image"
        : `${paint.kind} · ${paint.stops.length} stops`;
  return (
    <span className="dev-paint">
      <span
        className="dev-swatch"
        style={{ background: css ?? "transparent" }}
        aria-hidden="true"
      />
      <span className="dev-paint-label">{label}</span>
      {paint.opacity < 1 ? <span className="dev-muted">{round(paint.opacity * 100)}%</span> : null}
      {paint.blend !== "normal" ? <span className="dev-muted">{paint.blend}</span> : null}
      {paint.style_id ? <span className="dev-chip">{paint.style_id}</span> : null}
      {paint.variable_name ?? paint.variable_id ? (
        <span className="dev-chip dev-chip--variable">
          {paint.variable_name ?? paint.variable_id}
        </span>
      ) : null}
    </span>
  );
}

function NodeBlock({ node, parentName }: { node: NodeInspect; parentName: string | null }) {
  const radii = node.corner_radii;
  const hasRadii = radii.some((radius) => radius !== 0);
  const styleRefs = [...new Set(node.style_refs)];
  const variableRefs = [...new Set(node.variable_refs)];
  const primaryFill = node.fills[0];
  return (
    <div className="dev-node-block">
      <header className="dev-node-header">
        <span className="dev-node-name">{node.name}</span>
        <span className="dev-node-kind">{nodeKindLabel(node.kind as NodeKind)}</span>
      </header>

      <div className="dev-section">
        <h4 className="dev-section-title">{en.dev.inspect.metrics}</h4>
        <div className="dev-rows">
          <div className="dev-row">
            <span className="dev-row-label">{en.dev.inspect.position}</span>
            <span className="dev-row-value dev-mono">
              {round(node.bounds[0])}, {round(node.bounds[1])}
            </span>
          </div>
          <div className="dev-row">
            <span className="dev-row-label">{en.dev.inspect.size}</span>
            <span className="dev-row-value dev-mono">
              {round(node.bounds[2])} × {round(node.bounds[3])}
            </span>
          </div>
          <div className="dev-row">
            <span className="dev-row-label">{en.dev.inspect.rotation}</span>
            <span className="dev-row-value dev-mono">{round(node.rotation)}°</span>
          </div>
          <div className="dev-row">
            <span className="dev-row-label">{en.dev.inspect.opacity}</span>
            <span className="dev-row-value dev-mono">{round(node.opacity * 100)}%</span>
          </div>
          <div className="dev-row">
            <span className="dev-row-label">{en.dev.inspect.blend}</span>
            <span className="dev-row-value">{node.blend}</span>
          </div>
          <div className="dev-row">
            <span className="dev-row-label">{en.dev.inspect.localPosition}</span>
            <span className="dev-row-value dev-mono">
              {round(node.local_position[0])}, {round(node.local_position[1])}
            </span>
          </div>
          <div className="dev-row">
            <span className="dev-row-label">{en.dev.inspect.page}</span>
            <span className="dev-row-value">{node.page}</span>
          </div>
          <div className="dev-row">
            <span className="dev-row-label">{en.dev.inspect.parent}</span>
            <span className="dev-row-value">{parentName ?? en.dev.inspect.root}</span>
          </div>
          {node.parent ? (
            <div className="dev-row">
              <span className="dev-row-label">{en.dev.inspect.spacing}</span>
              <span className="dev-row-value dev-mono">
                {round(node.local_position[0])}, {round(node.local_position[1])}
              </span>
            </div>
          ) : null}
          <div className="dev-row">
            <span className="dev-row-label">{en.dev.inspect.constraints}</span>
            <span className="dev-row-value dev-muted">
              {node.constraints ? `${node.constraints[0]} / ${node.constraints[1]}` : en.dev.inspect.constraintsUnavailable}
            </span>
          </div>
          <div className="dev-row">
            <span className="dev-row-label">{en.dev.inspect.layout}</span>
            <span className="dev-row-value dev-muted">
              {node.layout_direction
                ? [
                    `${en.dev.inspect.direction}: ${node.layout_direction}`,
                    node.layout_gap !== null ? `${en.dev.inspect.gap}: ${round(node.layout_gap)}` : null,
                    node.layout_padding
                      ? `${en.dev.inspect.padding}: ${node.layout_padding.map((value) => round(value)).join(" ")}`
                      : null,
                    node.layout_sizing
                      ? `${en.dev.inspect.sizing}: ${node.layout_sizing[0]} / ${node.layout_sizing[1]}`
                      : null,
                  ]
                    .filter(Boolean)
                    .join(" · ")
                : en.dev.inspect.layoutUnavailable}
            </span>
          </div>
          {hasRadii ? (
            <div className="dev-row">
              <span className="dev-row-label">{en.dev.inspect.radii}</span>
              <span className="dev-row-value dev-mono">{radii.map((radius) => round(radius)).join(" ")}</span>
            </div>
          ) : null}
        </div>
      </div>

      <div className="dev-section">
        <div className="dev-inline-heading">
          <h4 className="dev-section-title">{en.dev.inspect.fills}</h4>
          {primaryFill ? (
            <CopyButton
              text={() => Promise.resolve(formatFillCss(primaryFill) ?? "")}
              title={en.dev.actions.copyColor}
              label={en.dev.actions.copyColor}
            />
          ) : (
            <CopyButton
              text={() => Promise.resolve("")}
              title={en.dev.actions.copyColor}
              label={en.dev.actions.copyColor}
              disabled
              disabledTitle={en.dev.actions.copyColorUnavailable}
            />
          )}
        </div>
        {node.fills.length === 0 ? (
          <p className="dev-muted">{en.dev.inspect.noFills}</p>
        ) : (
          <ul className="dev-paint-list">
            {node.fills.map((fill, index) => (
              <li key={`${node.id}-fill-${index}`}>
                <PaintText paint={fill} />
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="dev-section">
        <h4 className="dev-section-title">{en.dev.inspect.strokes}</h4>
        {node.strokes.length === 0 ? (
          <p className="dev-muted">{en.dev.inspect.noStrokes}</p>
        ) : (
          <ul className="dev-paint-list">
            {node.strokes.map((stroke, index) => (
              <li key={`${node.id}-stroke-${index}`} className="dev-stroke-row">
                <PaintText paint={stroke.paint} />
                <span className="dev-mono dev-muted">
                  {round(stroke.weight)} px {stroke.align}
                </span>
              </li>
            ))}
          </ul>
        )}
        <div className="dev-muted">{en.dev.inspect.effects}: </div>
        {node.effects.length === 0 ? (
          <p className="dev-muted">{en.dev.inspect.noEffects}</p>
        ) : (
          <ul className="dev-effect-list">
            {node.effects.map((effect, index) => (
              <li key={`${node.id}-effect-${index}`} className="dev-mono">
                {effect.kind}
                {effect.color ? ` · ${formatColorCss(effect.color)}` : ""}
                {` · blur ${round(effect.blur)}`}
                {effect.spread ? ` · spread ${round(effect.spread)}` : ""}
              </li>
            ))}
          </ul>
        )}
      </div>

      {node.text ? (
        <div className="dev-section">
          <h4 className="dev-section-title">{en.dev.inspect.typography}</h4>
          <div className="dev-rows">
            <div className="dev-row">
              <span className="dev-row-label">Font</span>
              <span className="dev-row-value">{node.text.font_family}</span>
            </div>
            <div className="dev-row">
              <span className="dev-row-label">Size</span>
              <span className="dev-row-value dev-mono">
                {round(node.text.font_size)} px · {en.dev.inspect.weightLabel.replace("{weight}", String(node.text.font_weight))}
              </span>
            </div>
            <div className="dev-row">
              <span className="dev-row-label">{en.dev.inspect.lineHeightLabel}</span>
              <span className="dev-row-value dev-mono">{round(node.text.line_height)}</span>
            </div>
            <div className="dev-row">
              <span className="dev-row-label">{en.dev.inspect.letterSpacingLabel}</span>
              <span className="dev-row-value dev-mono">{round(node.text.letter_spacing)}</span>
            </div>
            <div className="dev-row">
              <span className="dev-row-label">{en.dev.inspect.alignLabel}</span>
              <span className="dev-row-value">{node.text.align}</span>
            </div>
          </div>
        </div>
      ) : null}

      {node.component ? (
        <div className="dev-section">
          <h4 className="dev-section-title">{en.dev.inspect.component}</h4>
          <div className="dev-rows">
            <div className="dev-row">
              <span className="dev-row-label">Role</span>
              <span className="dev-row-value">{node.component.role}</span>
            </div>
            {node.component.component_name ? (
              <div className="dev-row">
                <span className="dev-row-label">Name</span>
                <span className="dev-row-value">{node.component.component_name}</span>
              </div>
            ) : null}
            <div className="dev-row">
              <span className="dev-row-label">{en.dev.inspect.overridesLabel}</span>
              <span className="dev-row-value dev-mono">{node.component.override_count}</span>
            </div>
            {node.component.inside_instance ? (
              <div className="dev-row">
                <span className="dev-row-label">{en.dev.inspect.insideInstance}</span>
                <span className="dev-row-value">yes</span>
              </div>
            ) : null}
          </div>
        </div>
      ) : null}

      {styleRefs.length > 0 || variableRefs.length > 0 || node.text ? (
        <div className="dev-section">
          <h4 className="dev-section-title">{en.dev.inspect.references}</h4>
          <div className="dev-refs">
            {styleRefs.length > 0 ? (
              <p>
                <span className="dev-row-label">{en.dev.inspect.styleRefs}: </span>
                {styleRefs.map((ref) => (
                  <span key={ref} className="dev-chip">
                    {ref}
                  </span>
                ))}
              </p>
            ) : null}
            {variableRefs.length > 0 ? (
              <p>
                <span className="dev-row-label">{en.dev.inspect.variableRefs}: </span>
                {variableRefs.map((ref) => (
                  <span key={ref} className="dev-chip dev-chip--variable">
                    {ref}
                  </span>
                ))}
              </p>
            ) : null}
            {styleRefs.length === 0 && variableRefs.length === 0 && node.text ? (
              <p className="dev-muted">{en.dev.inspect.none}</p>
            ) : null}
          </div>
        </div>
      ) : null}
    </div>
  );
}

export function InspectTab() {
  const inspect = useDevModeStore((state) => state.inspect);
  const loading = useDevModeStore((state) => state.inspectLoading);
  const error = useDevModeStore((state) => state.inspectError);
  const refreshInspect = useDevModeStore((state) => state.refreshInspect);
  const setInspectState = useDevModeStore((state) => state.setInspectState);
  const engine = useAppStore((state) => state.engine);
  const summary = useAppStore((state) => state.engine.summary);

  const selectionKey = engine.selection.join("|");
  useEffect(() => {
    if (!engine.doc) {
      setInspectState({ inspect: null, inspectLoading: false, inspectError: null });
      return;
    }
    const timer = window.setTimeout(() => {
      void refreshInspect();
    }, 120);
    return () => window.clearTimeout(timer);
  }, [engine.doc, engine.revision, selectionKey, refreshInspect, setInspectState]);

  const parentName = useMemo(() => {
    const node = inspect?.nodes[0];
    if (!node?.parent) return null;
    return summary?.nodes.find((candidate) => candidate.id === node.parent)?.name ?? node.parent;
  }, [inspect, summary]);

  const measurementsText = inspect ? formatMeasurements(inspect) : "";

  return (
    <div className="dev-tab-body" role="tabpanel" aria-label={en.dev.tabs.inspect}>
      <div className="dev-toolbar-row">
        <span className="dev-muted dev-mono">
          {en.dev.actions.revision.replace("{revision}", String(engine.revision))} ·{" "}
          {en.dev.actions.frame.replace("{frame}", String(engine.frame_id))}
        </span>
        <div className="dev-toolbar-actions">
          <CopyButton
            text={() => Promise.resolve(measurementsText)}
            title={en.dev.actions.copyMeasurements}
            label={en.dev.actions.copyMeasurements}
            disabled={!inspect || inspect.nodes.length === 0}
          />
          <button
            type="button"
            className="dev-copy-button"
            onClick={() => void refreshInspect()}
            disabled={loading}
          >
            {loading ? en.common.loading : en.dev.actions.refresh}
          </button>
        </div>
      </div>

      {error ? (
        <p className="form-error" role="status">
          {error}
        </p>
      ) : null}

      {loading && !inspect ? <p className="dev-muted">{en.dev.inspect.loading}</p> : null}

      {!inspect && !loading ? (
        <div className="dev-empty">
          <p className="dev-empty-title">{en.dev.inspect.emptyTitle}</p>
          <p className="dev-empty-body">{en.dev.inspect.emptyBody}</p>
          <CopyButton
            text={() => Promise.resolve(measurementsText)}
            title={en.dev.actions.copyMeasurements}
            disabled
          />
        </div>
      ) : null}

      {inspect && inspect.nodes.length === 0 ? (
        <div className="dev-empty">
          <p className="dev-empty-title">{en.dev.inspect.emptyTitle}</p>
          <p className="dev-empty-body">{en.dev.inspect.emptyBody}</p>
          {inspect.warnings.map((warning) => (
            <p key={warning} className="dev-muted">
              {warning}
            </p>
          ))}
        </div>
      ) : null}

      {inspect && inspect.nodes.length > 0 ? (
        <>
          {inspect.nodes.map((node) => (
            <NodeBlock key={node.id} node={node} parentName={parentName} />
          ))}
          {inspect.warnings.length > 0 ? (
            <div className="dev-warnings">
              {inspect.warnings.map((warning) => (
                <p key={warning} className="dev-warning">
                  {warning}
                </p>
              ))}
            </div>
          ) : null}
        </>
      ) : null}
    </div>
  );
}
