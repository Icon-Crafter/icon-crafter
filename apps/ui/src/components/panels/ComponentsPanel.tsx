import { useAppStore } from "../../engine/store";
import { en } from "../../i18n/en";
import { format } from "../../i18n/en";
import { Component, ComponentGlyph } from "../../icons";
import { runCommand } from "../../engine/commands";
import { IconButton } from "../primitives/IconButton";
import { EmptyState } from "../primitives/Layout";

/** Components in the document: select a main component, create one from selection. */
export function ComponentsPanel() {
  const summary = useAppStore((state) => state.engine.summary);
  const components = summary?.components ?? [];
  const selection = useAppStore((state) => state.engine.selection);
  const pushToast = useAppStore((state) => state.pushToast);
  const status = useAppStore((state) => state.status);
  const ready = status === "ready";

  const createComponent = () => {
    if (selection.length === 0) {
      pushToast({ kind: "info", title: en.components.emptyTitle, message: en.components.emptyBody });
      return;
    }
    void runCommand("create_component", undefined, { quiet: true });
  };

  return (
    <div className="panel-section" style={{ flex: "1 1 auto", minHeight: 0 }}>
      <div className="panel-section-header">
        <span className="panel-section-title">{en.components.title}</span>
        <div className="panel-section-actions">
          <IconButton
            label={en.components.createFromSelection}
            title={en.components.createHint}
            size="sm"
            disabled={!ready || selection.length === 0}
            onClick={createComponent}
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
            <button type="button" className="button" disabled={!ready || selection.length === 0} onClick={createComponent}>
              {en.components.createFromSelection}
            </button>
          }
        />
      ) : (
        <div className="component-list scroll-thin" role="listbox" aria-label={en.components.title}>
          {components.map((component) => (
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
                aria-label={format(en.components.instancesCount, { count: 0 })}
                title={en.components.insertHint}
                onClick={(event) => {
                  event.stopPropagation();
                  pushToast({
                    kind: "warning",
                    title: format(en.toasts.unsupported, { command: en.components.insertHint }),
                    message: en.toasts.unsupportedBody,
                  });
                }}
              >
                +
              </button>
            </div>
          ))}
        </div>
      )}
      <div className="panel-footer panel-footer--hint">{en.components.createHint}</div>
    </div>
  );
}
