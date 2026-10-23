import { useAppStore } from "../../engine/store";
import { en } from "../../i18n/en";
import { Component, ComponentGlyph, Plus } from "../../icons";
import { createComponentFromSelection, insertComponent, runCommand } from "../../engine/commands";
import { IconButton } from "../primitives/IconButton";
import { EmptyState } from "../primitives/Layout";

/** Components in the document: insert instances, select main components. */
export function ComponentsPanel() {
  const summary = useAppStore((state) => state.engine.summary);
  const components = summary?.components ?? [];
  const selection = useAppStore((state) => state.engine.selection);
  const status = useAppStore((state) => state.status);
  const ready = status === "ready";

  return (
    <div className="components-panel">
      <div className="panel-section-header">
        <span className="panel-section-title">{en.components.title}</span>
        <div className="panel-section-actions">
          <IconButton
            label={en.components.createFromSelection}
            title={en.components.createHint}
            size="sm"
            disabled={!ready || selection.length === 0}
            onClick={() => void createComponentFromSelection()}
          >
            <Component size={14} />
          </IconButton>
        </div>
      </div>

      {components.length === 0 ? (
        <EmptyState
          icon={<ComponentGlyph width={18} height={18} />}
          title={en.components.emptyTitle}
          body={en.components.emptyBody}
          actions={
            <button
              type="button"
              className="button"
              disabled={!ready || selection.length === 0}
              onClick={() => void createComponentFromSelection()}
            >
              {en.components.createFromSelection}
            </button>
          }
        />
      ) : (
        <div className="component-list scroll-thin" role="listbox" aria-label={en.components.title}>
          {components.map((component) => {
            const insertId = component.component_id ?? null;
            return (
              <div
                key={component.node_id}
                className="component-row"
                role="option"
                aria-selected={selection.includes(component.node_id)}
                tabIndex={0}
                onClick={() => void runCommand("select_nodes", { ids: [component.node_id] })}
                onKeyDown={(event) => {
                  if (event.key === "Enter") void runCommand("select_nodes", { ids: [component.node_id] });
                }}
              >
                <span className="layer-icon" aria-hidden="true">
                  <ComponentGlyph width={13} height={13} />
                </span>
                <span className="component-name" title={component.name}>
                  {component.name}
                </span>
                <span className="component-kind">{component.kind === "component_set" ? "Set" : "Main"}</span>
                <button
                  type="button"
                  className="component-add"
                  aria-label={en.components.insertHint}
                  title={insertId ? en.components.insertHint : en.components.componentUnavailable}
                  disabled={insertId === null}
                  onClick={(event) => {
                    event.stopPropagation();
                    if (insertId) void insertComponent(component);
                  }}
                >
                  <Plus size={12} />
                </button>
              </div>
            );
          })}
        </div>
      )}
      <p className="panel-footer-hint">{en.components.createHint}</p>
    </div>
  );
}
