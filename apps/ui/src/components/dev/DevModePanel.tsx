/**
 * Dev Mode right sidebar: Inspect | Code | Annotate | Compare.
 *
 * Shown in place of the design Inspector only while Dev Mode is active; the
 * design inspector is untouched when Dev Mode is off.
 */
import { useEffect } from "react";

import type { DevTab } from "../../engine/devModeStore";
import { useDevModeStore } from "../../engine/devModeStore";
import { toggleDevMode } from "../../engine/commands";
import { useAppStore } from "../../engine/store";
import { en } from "../../i18n/en";
import { PanelRightClose } from "../../icons";
import { AnnotateTab } from "./AnnotateTab";
import { CodeTab } from "./CodeTab";
import { CompareTab } from "./CompareTab";
import { InspectTab } from "./InspectTab";

const TABS: { key: DevTab; label: string }[] = [
  { key: "inspect", label: en.dev.tabs.inspect },
  { key: "code", label: en.dev.tabs.code },
  { key: "annotate", label: en.dev.tabs.annotate },
  { key: "compare", label: en.dev.tabs.compare },
];

export function DevModePanel() {
  const tab = useDevModeStore((state) => state.tab);
  const setTab = useDevModeStore((state) => state.setTab);
  const active = useDevModeStore((state) => state.active);
  const handoff = useDevModeStore((state) => state.handoff);
  const reloadHandoff = useDevModeStore((state) => state.reloadHandoff);
  const doc = useAppStore((state) => state.engine.doc);
  const activePage = useAppStore((state) => state.engine.summary?.active_page ?? "");

  useEffect(() => {
    void reloadHandoff();
  }, [reloadHandoff]);

  if (!active) return null;

  return (
    <section className="panel panel--right dev-panel" aria-label={en.dev.title}>
      <header className="dev-panel-header">
        <div>
          <p className="dev-panel-kicker">{en.dev.label}</p>
          <h2 className="dev-panel-title">{en.dev.title}</h2>
          <p className="dev-panel-subtitle">{en.dev.subtitle}</p>
        </div>
        <button
          type="button"
          className="dev-panel-close"
          aria-label={en.dev.tooltip}
          title={en.dev.tooltip}
          onClick={() => toggleDevMode()}
        >
          <PanelRightClose size={15} />
        </button>
      </header>

      <div className="dev-tabs" role="tablist" aria-label={en.dev.title}>
        {TABS.map((item) => {
          const selected = item.key === tab;
          return (
            <button
              key={item.key}
              type="button"
              role="tab"
              id={`dev-tab-${item.key}`}
              aria-selected={selected}
              aria-controls={`dev-panel-${item.key}`}
              tabIndex={selected ? 0 : -1}
              className={selected ? "dev-tab dev-tab--active" : "dev-tab"}
              onClick={() => setTab(item.key)}
              onKeyDown={(event) => {
                if (event.key !== "ArrowRight" && event.key !== "ArrowLeft") return;
                event.preventDefault();
                const index = TABS.findIndex((candidate) => candidate.key === tab);
                const direction = event.key === "ArrowRight" ? 1 : -1;
                const next = TABS[(index + direction + TABS.length) % TABS.length];
                if (next) {
                  setTab(next.key);
                  document.getElementById(`dev-tab-${next.key}`)?.focus();
                }
              }}
            >
              {item.label}
              {item.key === "annotate" && handoff && handoff.annotations.some((annotation) => !annotation.resolved) ? (
                <span className="dev-tab-dot" aria-hidden="true" />
              ) : null}
            </button>
          );
        })}
      </div>

      <div className="dev-panel-body" id={`dev-panel-${tab}`}>
        {doc ? null : (
          <div className="dev-empty">
            <p className="dev-empty-title">{en.dev.notConnected}</p>
          </div>
        )}
        {doc && activePage ? (
          <>
            {tab === "inspect" ? <InspectTab /> : null}
            {tab === "code" ? <CodeTab /> : null}
            {tab === "annotate" ? <AnnotateTab /> : null}
            {tab === "compare" ? <CompareTab /> : null}
          </>
        ) : null}
      </div>

      <footer className="dev-panel-footer">
        <span className="dev-muted dev-mono">{en.dev.shortcutHint}</span>
        <span className="dev-muted">{en.dev.label}</span>
      </footer>
    </section>
  );
}
