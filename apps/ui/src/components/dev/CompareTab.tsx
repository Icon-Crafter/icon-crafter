/**
 * Dev Mode — Compare tab.
 *
 * Captures the current document revision (canonical JSON via `dev_snapshot`)
 * and diffs later revisions against it (`dev_compare`). The engine compares the
 * current document as "before" and the captured snapshot as "after", so the
 * summary re-labels those directions for humans.
 */
import { useEffect, useState } from "react";
import type { ReactNode } from "react";

import type { RevisionCapture } from "../../engine/devMode";
import { formatDiffCounts, summarizeDiffForUi } from "../../engine/devMode";
import { useDevModeStore } from "../../engine/devModeStore";
import { useAppStore } from "../../engine/store";
import { en } from "../../i18n/en";
import { timestampLabel } from "./shared";

export function CompareTab() {
  const captures = useDevModeStore((state) => state.captures);
  const compareResult = useDevModeStore((state) => state.compareResult);
  const compareResultCapture = useDevModeStore((state) => state.compareResultCapture);
  const compareLoading = useDevModeStore((state) => state.compareLoading);
  const compareError = useDevModeStore((state) => state.compareError);
  const captureRevision = useDevModeStore((state) => state.captureRevision);
  const removeCapture = useDevModeStore((state) => state.removeCapture);
  const clearCaptures = useDevModeStore((state) => state.clearCaptures);
  const compareWith = useDevModeStore((state) => state.compareWith);
  const revision = useAppStore((state) => state.engine.revision);
  const [label, setLabel] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (captures.length === 0) {
      setSelectedId(null);
      return;
    }
    if (!selectedId || !captures.some((capture) => capture.id === selectedId)) {
      setSelectedId(captures[0]?.id ?? null);
    }
  }, [captures, selectedId]);

  async function capture() {
    setBusy(true);
    try {
      const created = await captureRevision(label.trim() === "" ? undefined : label.trim());
      setLabel("");
      if (created) setSelectedId(created.id);
    } finally {
      setBusy(false);
    }
  }

  async function compare() {
    const capture = captures.find((item) => item.id === selectedId);
    if (!capture) return;
    setBusy(true);
    try {
      await compareWith(capture);
    } finally {
      setBusy(false);
    }
  }

  const summary = compareResult ? summarizeDiffForUi(compareResult) : null;

  return (
    <div className="dev-tab-body" role="tabpanel" aria-label={en.dev.tabs.compare}>
      <section className="dev-section">
        <h3 className="dev-section-title">{en.dev.compare.captureTitle}</h3>
        <div className="dev-field-row">
          <label className="dev-field">
            <span className="dev-field-label">{en.dev.compare.captureLabel}</span>
            <input
              className="text-input"
              value={label}
              placeholder={en.dev.compare.captureLabelPlaceholder}
              onChange={(event) => setLabel(event.target.value)}
            />
          </label>
        </div>
        <div className="dev-toolbar-actions dev-toolbar-actions--end">
          <span className="dev-muted dev-mono">
            {en.dev.actions.revision.replace("{revision}", String(revision))}
          </span>
          <button type="button" className="dev-copy-button" disabled={busy} onClick={() => void capture()}>
            {en.dev.compare.capture}
          </button>
        </div>
        <p className="dev-muted">{en.dev.compare.captureLimit.replace("{max}", "5")}</p>
      </section>

      <section className="dev-section">
        <h3 className="dev-section-title">
          {en.dev.compare.capturesTitle}
          {captures.length > 0 ? (
            <button
              type="button"
              className="dev-copy-button dev-copy-button--inline"
              title={en.dev.compare.clearAllTitle}
              onClick={() => {
                clearCaptures();
                useDevModeStore
                  .getState()
                  .setCompareState({ compareResult: null, compareResultCapture: null });
              }}
            >
              {en.dev.compare.clearAll}
            </button>
          ) : null}
        </h3>

        {captures.length === 0 ? (
          <div className="dev-empty">
            <p className="dev-empty-title">{en.dev.compare.emptyTitle}</p>
            <p className="dev-empty-body">{en.dev.compare.emptyBody}</p>
          </div>
        ) : (
          <ul className="dev-capture-list" role="listbox" aria-label={en.dev.compare.capturesTitle}>
            {captures.map((capture) => (
              <CaptureRow
                key={capture.id}
                capture={capture}
                selected={capture.id === selectedId}
                onSelect={() => setSelectedId(capture.id)}
                onRemove={() => removeCapture(capture.id)}
              />
            ))}
          </ul>
        )}

        <div className="dev-toolbar-actions dev-toolbar-actions--end">
          <button
            type="button"
            className="dev-copy-button"
            disabled={busy || !selectedId || compareLoading}
            onClick={() => void compare()}
            title={!selectedId ? en.dev.compare.noCaptures : undefined}
          >
            {compareLoading ? en.dev.compare.comparing : en.dev.compare.compare}
          </button>
        </div>
        {compareError ? <p className="form-error">{compareError}</p> : null}
      </section>

      <section className="dev-section">
        <h3 className="dev-section-title">
          {compareResultCapture
            ? en.dev.compare.resultTitle.replace("{label}", compareResultCapture.label)
            : en.dev.compare.resultTitle.replace("{label}", "—")}
        </h3>

        {!compareResult || !summary ? (
          <p className="dev-muted">{en.dev.compare.noResult}</p>
        ) : (
          <>
            <p className="dev-diff-counts">{formatDiffCounts(summary)}</p>
            {summary.addedSinceCapture.length === 0 &&
            summary.removedSinceCapture.length === 0 &&
            summary.changed.length === 0 ? (
              <p className="dev-muted">{en.dev.compare.noChanges}</p>
            ) : (
              <div className="dev-diff-groups">
                <DiffGroup
                  title={en.dev.compare.addedSinceCapture}
                  count={summary.addedSinceCapture.length}
                  rows={summary.addedSinceCapture.map((node) => (
                    <span key={node.id} className="dev-diff-row">
                      <strong>{node.name}</strong>
                      <span className="dev-muted">{node.kind}</span>
                    </span>
                  ))}
                />
                <DiffGroup
                  title={en.dev.compare.removedSinceCapture}
                  count={summary.removedSinceCapture.length}
                  rows={summary.removedSinceCapture.map((node) => (
                    <span key={node.id} className="dev-diff-row">
                      <strong>{node.name}</strong>
                      <span className="dev-muted">{node.kind}</span>
                    </span>
                  ))}
                />
                <DiffGroup
                  title={en.dev.compare.changed}
                  count={summary.changed.length}
                  rows={summary.changed.map((change) => (
                    <span key={change.id} className="dev-diff-row dev-diff-row--changed">
                      <strong>{change.name}</strong>
                      <span className="dev-chip-list">
                        {change.fields.map((field) => (
                          <span key={field} className="dev-chip">
                            {field}
                          </span>
                        ))}
                      </span>
                    </span>
                  ))}
                />
              </div>
            )}
            <p className="dev-muted">{en.dev.compare.directionNote}</p>
          </>
        )}
      </section>
    </div>
  );
}

function CaptureRow({
  capture,
  selected,
  onSelect,
  onRemove,
}: {
  capture: RevisionCapture;
  selected: boolean;
  onSelect: () => void;
  onRemove: () => void;
}) {
  return (
    <li
      role="option"
      aria-selected={selected}
      tabIndex={0}
      className={selected ? "dev-capture dev-capture--active" : "dev-capture"}
      onClick={onSelect}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onSelect();
        }
      }}
    >
      <div className="dev-capture-main">
        <strong>{capture.label}</strong>
        <span className="dev-muted dev-mono">
          {en.dev.compare.revisionLabel.replace("{revision}", String(capture.revision))} ·{" "}
          {en.dev.compare.capturedAt.replace("{date}", timestampLabel(capture.captured_at))}
        </span>
      </div>
      <button
        type="button"
        className="dev-copy-button dev-copy-button--inline"
        title={en.dev.compare.removeCapture}
        aria-label={`${en.dev.compare.removeCapture}: ${capture.label}`}
        onClick={(event) => {
          event.stopPropagation();
          onRemove();
        }}
      >
        ×
      </button>
    </li>
  );
}

function DiffGroup({ title, count, rows }: { title: string; count: number; rows: ReactNode }) {
  return (
    <div className="dev-diff-group">
      <p className="dev-section-title">
        {title}
        <span className="dev-count-badge">{count}</span>
      </p>
      {count === 0 ? <p className="dev-muted">—</p> : <div className="dev-diff-rows">{rows}</div>}
    </div>
  );
}
