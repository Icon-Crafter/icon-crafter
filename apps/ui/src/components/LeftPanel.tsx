import { useAppStore } from "../engine/store";
import type { LeftTab } from "../engine/store";
import { en } from "../i18n/en";
import { format } from "../i18n/en";
import { Component, FileText, Layers, Palette, PanelLeftClose, PanelLeftOpen } from "../icons";
import { LayersPanel } from "./panels/LayersPanel";
import { AssetsPanel } from "./panels/AssetsPanel";
import { ComponentsPanel } from "./panels/ComponentsPanel";
import { PagesPanel } from "./panels/PagesPanel";

const TABS: { id: LeftTab; label: string; icon: React.ReactNode }[] = [
  { id: "layers", label: en.panels.layers, icon: <Layers size={14} /> },
  { id: "assets", label: en.panels.assets, icon: <Palette size={14} /> },
  { id: "components", label: en.panels.components, icon: <Component size={14} /> },
  { id: "pages", label: en.panels.pages, icon: <FileText size={14} /> },
];

/** Left dock: tabbed layers / assets / components / pages with a collapsed rail. */
export function LeftPanel() {
  const leftTab = useAppStore((state) => state.leftTab);
  const collapsed = useAppStore((state) => state.leftCollapsed);
  const setLeftTab = useAppStore((state) => state.setLeftTab);
  const toggleLeftCollapsed = useAppStore((state) => state.toggleLeftCollapsed);
  const summary = useAppStore((state) => state.engine.summary);
  const selection = useAppStore((state) => state.engine.selection);
  const status = useAppStore((state) => state.status);

  const counts: Record<LeftTab, number> = {
    layers: summary?.nodes.length ?? 0,
    assets:
      (summary?.styles.paints.length ?? 0) +
      (summary?.styles.texts.length ?? 0) +
      (summary?.styles.effects.length ?? 0),
    components: summary?.components.length ?? 0,
    pages: summary?.pages.length ?? 0,
  };

  if (collapsed) {
    return (
      <aside className="panel panel--left panel--collapsed" aria-label={en.panels.layers}>
        <div className="panel-rail">
          {TABS.map((tab) => (
            <button
              key={tab.id}
              type="button"
              className={`rail-button${leftTab === tab.id ? " rail-button--active" : ""}`}
              aria-label={`${tab.label} (${en.panels.expand})`}
              title={tab.label}
              onClick={() => {
                setLeftTab(tab.id);
              }}
            >
              {tab.icon}
            </button>
          ))}
        </div>
        <button
          type="button"
          className="panel-collapse"
          aria-label={en.panels.expand}
          title={en.panels.expand}
          onClick={toggleLeftCollapsed}
        >
          <PanelLeftOpen size={14} />
        </button>
      </aside>
    );
  }

  return (
    <aside className="panel panel--left" aria-label={en.panels.layers}>
      <div className="panel-tabs" role="tablist" aria-label={en.panels.layers}>
        {TABS.map((tab) => (
          <button
            key={tab.id}
            type="button"
            role="tab"
            aria-selected={leftTab === tab.id}
            className={`panel-tab${leftTab === tab.id ? " panel-tab--active" : ""}`}
            title={tab.label}
            onClick={() => setLeftTab(tab.id)}
          >
            {tab.icon}
            <span>{tab.label}</span>
            {counts[tab.id] > 0 && <span className="panel-tab-count">{counts[tab.id]}</span>}
          </button>
        ))}
        <div className="panel-tabs-actions">
          <button
            type="button"
            className="panel-collapse"
            aria-label={en.panels.collapse}
            title={en.panels.collapse}
            onClick={toggleLeftCollapsed}
          >
            <PanelLeftClose size={13} />
          </button>
        </div>
      </div>

      <div className="panel-body">
        {leftTab === "layers" && <LayersPanel />}
        {leftTab === "assets" && <AssetsPanel />}
        {leftTab === "components" && <ComponentsPanel />}
        {leftTab === "pages" && <PagesPanel />}
      </div>

      <div className="panel-status">
        <span>
          {(summary?.nodes.length ?? 0) === 1
            ? en.status.objectsOne
            : format(en.status.objects, { count: summary?.nodes.length ?? 0 })}
        </span>
        <span className="panel-status-sep">·</span>
        <span>
          {selection.length === 0
            ? en.status.selectionNone
            : format(en.status.selected, { count: selection.length })}
        </span>
        {status !== "ready" && <span className="status-dot status-dot--pending" aria-hidden="true" />}
      </div>
    </aside>
  );
}
