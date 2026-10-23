import { useEffect, useState } from "react";
import { useAppStore } from "../engine/store";
import { en } from "../i18n/en";
import { format } from "../i18n/en";
import { nodeKindLabel } from "../i18n/labels";
import { nodeKindIcon } from "./panels/LayersPanel";
import { runCommand } from "../engine/commands";
import {
  Eye,
  EyeOff,
  Lock,
  LockOpen,
  PanelRightClose,
  PanelRightOpen,
} from "../icons";
import { EmptyState } from "./primitives/Layout";
import { TransformSection } from "./inspector/TransformSection";
import {
  ComponentSection,
  EffectsSection,
  ExportSection,
  FillSection,
  LayoutSection,
  StrokeSection,
  TypographySection,
} from "./inspector/StyleSections";

/**
 * Right inspector: context-sensitive property sections for the current
 * selection. Every edit dispatches an engine command; nothing is faked.
 */
export function Inspector() {
  const details = useAppStore((state) => state.engine.selection_details);
  const selection = useAppStore((state) => state.engine.selection);
  const status = useAppStore((state) => state.status);
  const rightCollapsed = useAppStore((state) => state.rightCollapsed);
  const toggleRightCollapsed = useAppStore((state) => state.toggleRightCollapsed);

  const single = selection.length === 1 ? details[0] ?? null : null;
  const [nameDraft, setNameDraft] = useState(single?.name ?? "");

  useEffect(() => {
    setNameDraft(single?.name ?? "");
  }, [single?.id, single?.name]);

  if (rightCollapsed) {
    return (
      <aside className="panel panel--right panel--collapsed" aria-label={en.panels.inspector}>
        <div className="panel-rail">
          <button
            type="button"
            className="rail-button"
            aria-label={en.panels.expand}
            title={en.panels.expand}
            onClick={toggleRightCollapsed}
          >
            <PanelRightOpen size={15} />
          </button>
        </div>
      </aside>
    );
  }

  const ready = status === "ready";

  return (
    <aside className="panel panel--right" aria-label={en.panels.inspector}>
      <div className="panel-tabs">
        <div className="panel-tab panel-tab--active" role="tab" aria-selected="true">
          {en.panels.inspector}
        </div>
        <div className="panel-tabs-actions">
          <button
            type="button"
            className="panel-collapse"
            aria-label={en.panels.collapse}
            title={en.panels.collapse}
            onClick={toggleRightCollapsed}
          >
            <PanelRightClose size={13} />
          </button>
        </div>
      </div>

      {!ready ? (
        <div className="inspector">
          <EmptyState title={en.app.engineConnecting} body={en.app.engineUnavailableBody} />
        </div>
      ) : single ? (
        <div className="inspector scroll-thin">
          <div className="inspector-header">
            <span className="inspector-node-icon" aria-hidden="true">
              {nodeKindIcon(single.kind, 14)}
            </span>
            <input
              className="inspector-title-text"
              aria-label={en.inspector.name}
              value={nameDraft}
              onChange={(event) => setNameDraft(event.target.value)}
              onBlur={() => {
                const next = nameDraft.trim();
                if (next && next !== single.name) {
                  void runCommand("rename_node", { id: single.id, name: next }, { quiet: true });
                }
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter") event.currentTarget.blur();
                if (event.key === "Escape") {
                  setNameDraft(single.name);
                  event.currentTarget.blur();
                }
              }}
            />
            <div className="inspector-header-actions">
              <button
                type="button"
                className={`layer-toggle${single.visible ? " layer-toggle--on" : ""}`}
                aria-label={single.visible ? en.layers.hide : en.layers.show}
                title={single.visible ? en.layers.hide : en.layers.show}
                onClick={() => void runCommand("toggle_visible", { ids: [single.id] })}
              >
                {single.visible ? <Eye size={13} /> : <EyeOff size={13} />}
              </button>
              <button
                type="button"
                className={`layer-toggle${single.locked ? " layer-toggle--active" : ""}`}
                aria-label={single.locked ? en.layers.unlock : en.layers.lock}
                title={single.locked ? en.layers.unlock : en.layers.lock}
                onClick={() => void runCommand("toggle_locked", { ids: [single.id] })}
              >
                {single.locked ? <Lock size={13} /> : <LockOpen size={13} />}
              </button>
            </div>
          </div>
          <div className="inspector-subtitle">
            {nodeKindLabel(single.kind)}
            {single.is_inside_instance ? ` · ${en.inspector.componentInstance}` : ""}
          </div>

          <TransformSection details={details} node={single} />
          <LayoutSection node={single} />
          <FillSection node={single} />
          <StrokeSection node={single} />
          <EffectsSection node={single} />
          <TypographySection node={single} />
          <ComponentSection node={single} />
          <ExportSection />
        </div>
      ) : selection.length > 1 ? (
        <div className="inspector scroll-thin">
          <div className="inspector-header">
            <span className="inspector-node-icon" aria-hidden="true">
              {nodeKindIcon(details[0]?.kind ?? "group", 14)}
            </span>
            <span className="inspector-title-text">{format(en.inspector.multiSelectionTitle, { count: selection.length })}</span>
          </div>
          <TransformSection details={details} node={null} />
          <ExportSection />
        </div>
      ) : (
        <div className="inspector">
          <EmptyState
            icon={<PanelRightOpen size={18} />}
            title={en.inspector.noSelectionTitle}
            body={en.inspector.noSelectionBody}
          />
        </div>
      )}

      <div className="panel-status">
        <span className="status-dot status-dot--ok" aria-hidden="true" />
        <span>{format(en.status.revision, { revision: useAppStore.getState().engine.revision })}</span>
        <span className="panel-status-sep">·</span>
        <span>frame {useAppStore.getState().engine.frame_id}</span>
      </div>
    </aside>
  );
}
