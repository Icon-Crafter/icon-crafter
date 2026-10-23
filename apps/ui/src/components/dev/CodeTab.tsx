/**
 * Dev Mode — Code tab.
 *
 * Generates read-only starting code for the current selection on demand
 * (`dev_codegen`). Generation is always labelled as requiring manual
 * adaptation; nothing here implies production readiness.
 */
import { useEffect } from "react";

import type { CodegenTarget } from "../../engine/contract";
import { useDevModeStore } from "../../engine/devModeStore";
import { getActivePump, useAppStore } from "../../engine/store";
import { en } from "../../i18n/en";
import { saveTextFile } from "../../utils/fileSave";
import { CopyButton } from "./shared";

const TARGETS: { key: CodegenTarget; label: string }[] = [
  { key: "css", label: en.dev.code.targets.css },
  { key: "react", label: en.dev.code.targets.react },
  { key: "swiftui", label: en.dev.code.targets.swiftui },
  { key: "compose", label: en.dev.code.targets.compose },
];

export function CodeTab() {
  const code = useDevModeStore((state) => state.code);
  const target = useDevModeStore((state) => state.codeTarget);
  const loading = useDevModeStore((state) => state.codeLoading);
  const error = useDevModeStore((state) => state.codeError);
  const generateCode = useDevModeStore((state) => state.generateCode);
  const selectionKey = useAppStore((state) => state.engine.selection.join("|"));
  const pushToast = useAppStore((state) => state.pushToast);

  useEffect(() => {
    void generateCode();
    // Only re-generate when the tab mounts; manual target changes call generate.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    void generateCode(target);
    // Regenerate when the selection changes while this tab is open.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectionKey]);

  const lineCount = code ? code.code.split("\n").length : 0;

  async function download() {
    if (!code) return;
    const pump = getActivePump();
    if (!pump) return;
    try {
      const path = await pump.transport.pickSavePath({
        title: en.dev.code.downloadTitle,
        defaultPath: `generated.${code.extension}`,
        filters: [{ name: TARGETS.find((item) => item.key === target)?.label ?? "Code", extensions: [code.extension] }],
      });
      if (!path) return;
      const saved = await saveTextFile(path, code.code);
      if (saved) {
        pushToast({ kind: "success", title: en.dev.code.download, message: path });
      }
    } catch (caught) {
      pushToast({
        kind: "error",
        title: en.dev.code.loadError,
        message: caught instanceof Error ? caught.message : String(caught),
      });
    }
  }

  return (
    <div className="dev-tab-body" role="tabpanel" aria-label={en.dev.tabs.code}>
      <div className="dev-language-tabs" role="tablist" aria-label={en.dev.tabs.code}>
        {TARGETS.map((item) => {
          const active = item.key === target;
          return (
            <button
              key={item.key}
              type="button"
              role="tab"
              id={`dev-code-tab-${item.key}`}
              aria-selected={active}
              aria-controls="dev-code-panel"
              tabIndex={active ? 0 : -1}
              className={active ? "dev-language-tab dev-language-tab--active" : "dev-language-tab"}
              onClick={() => {
                if (item.key !== target) {
                  useDevModeStore.getState().setCodeTarget(item.key);
                  void generateCode(item.key);
                }
              }}
              onKeyDown={(event) => {
                if (event.key !== "ArrowRight" && event.key !== "ArrowLeft") return;
                event.preventDefault();
                const index = TARGETS.findIndex((candidate) => candidate.key === target);
                const direction = event.key === "ArrowRight" ? 1 : -1;
                const next = TARGETS[(index + direction + TARGETS.length) % TARGETS.length];
                if (next) {
                  useDevModeStore.getState().setCodeTarget(next.key);
                  void generateCode(next.key);
                  document.getElementById(`dev-code-tab-${next.key}`)?.focus();
                }
              }}
            >
              {item.label}
            </button>
          );
        })}
      </div>

      {error ? (
        <div className="dev-error" role="status">
          <p className="form-error">{error}</p>
          <button type="button" className="dev-copy-button" onClick={() => void generateCode(target)}>
            {en.common.retry}
          </button>
        </div>
      ) : null}

      {loading ? <p className="dev-muted">{en.dev.code.loading}</p> : null}

      {!loading && !error && !code ? (
        <div className="dev-empty">
          <p className="dev-empty-title">{en.dev.code.emptyTitle}</p>
          <p className="dev-empty-body">{en.dev.code.emptyBody}</p>
        </div>
      ) : null}

      {code ? (
        <>
          <div className="dev-code-banner dev-code-banner--warning" role="note">
            <strong>{en.dev.code.requiresAdaptation}</strong>
            <p>{en.dev.code.requiresAdaptationBody}</p>
          </div>

          {code.warnings.length > 0 ? (
            <div className="dev-warnings">
              <p className="dev-section-title">{en.dev.code.warnings}</p>
              <ul className="dev-warning-list">
                {code.warnings.map((warning) => (
                  <li key={warning}>{warning}</li>
                ))}
              </ul>
            </div>
          ) : null}

          <div className="dev-toolbar-row">
            <span className="dev-muted dev-mono">
              {code.language} · {en.dev.code.lines.replace("{count}", String(lineCount))} ·{" "}
              {en.dev.code.nodes.replace("{count}", String(code.node_count))}
            </span>
            <div className="dev-toolbar-actions">
              <CopyButton text={() => Promise.resolve(code.code)} title={en.dev.code.copy} label={en.dev.code.copy} />
              <button type="button" className="dev-copy-button" onClick={() => void download()}>
                {en.dev.code.download}
              </button>
            </div>
          </div>

          <div id="dev-code-panel" role="tabpanel" aria-labelledby={`dev-code-tab-${target}`}>
            <pre className="dev-code">
              <code>{code.code}</code>
            </pre>
            <p className="dev-muted">{code.warnings.length > 0 ? en.dev.code.requiresAdaptation : null}</p>
          </div>
        </>
      ) : null}
    </div>
  );
}
