/**
 * Dev Mode — Annotate tab.
 *
 * Creates review annotations against the selected layer and tracks the
 * handoff status. Annotations can be marked resolved once the review is
 * addressed.
 */
import { useEffect, useState } from "react";

import type { AnnotationKind, HandoffStatus } from "../../engine/contract";
import { useDevModeStore } from "../../engine/devModeStore";
import { useAppStore } from "../../engine/store";
import { en } from "../../i18n/en";
import { timestampLabel } from "./shared";

const ANNOTATION_KINDS: AnnotationKind[] = ["note", "implementation", "measurement"];

const HANDOFF_STATUSES: { value: HandoffStatus; label: string }[] = [
  { value: "draft", label: en.dev.annotate.statuses.draft },
  { value: "ready_for_review", label: en.dev.annotate.statuses.readyForReview },
  { value: "ready_for_development", label: en.dev.annotate.statuses.readyForDevelopment },
  { value: "implemented", label: en.dev.annotate.statuses.implemented },
];

function statusLabel(status: HandoffStatus): string {
  return HANDOFF_STATUSES.find((entry) => entry.value === status)?.label ?? status;
}

export function AnnotateTab() {
  const handoff = useDevModeStore((state) => state.handoff);
  const loading = useDevModeStore((state) => state.handoffLoading);
  const error = useDevModeStore((state) => state.handoffError);
  const addAnnotation = useDevModeStore((state) => state.addAnnotation);
  const resolveAnnotation = useDevModeStore((state) => state.resolveAnnotation);
  const markReady = useDevModeStore((state) => state.markReady);
  const reloadHandoff = useDevModeStore((state) => state.reloadHandoff);
  const engine = useAppStore((state) => state.engine);
  const bootstrap = useAppStore((state) => state.bootstrap);
  const pushToast = useAppStore((state) => state.pushToast);

  const node = engine.selection_details[0] ?? null;
  const [body, setBody] = useState("");
  const [kind, setKind] = useState<AnnotationKind>("note");
  const [link, setLink] = useState("");
  const [author, setAuthor] = useState("");
  const [status, setStatus] = useState<HandoffStatus>("draft");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    void reloadHandoff();
  }, [reloadHandoff, engine.revision]);

  useEffect(() => {
    if (handoff) setStatus(handoff.status);
  }, [handoff]);

  useEffect(() => {
    if (author !== "") return;
    const preference = bootstrap?.preferences["user.name"];
    setAuthor(typeof preference === "string" && preference.trim() !== "" ? preference : "You");
  }, [author, bootstrap]);

  const openCount = handoff ? handoff.annotations.filter((annotation) => !annotation.resolved).length : 0;

  async function submit() {
    if (!node) return;
    if (body.trim() === "") {
      pushToast({ kind: "warning", title: en.dev.annotate.bodyRequired });
      return;
    }
    setBusy(true);
    try {
      await addAnnotation({
        nodeId: node.id,
        body: body.trim(),
        author: author.trim() === "" ? "You" : author.trim(),
        kind,
        link: link.trim(),
      });
      setBody("");
      setLink("");
      pushToast({ kind: "success", title: en.dev.annotate.added });
    } catch (caught) {
      pushToast({
        kind: "error",
        title: en.dev.actions.copyFailed,
        message: caught instanceof Error ? caught.message : String(caught),
      });
    } finally {
      setBusy(false);
    }
  }

  async function resolve(annotationId: string) {
    setBusy(true);
    try {
      const resolved = await resolveAnnotation(annotationId);
      if (resolved) {
        pushToast({ kind: "success", title: en.toasts.annotationResolved });
      }
    } finally {
      setBusy(false);
    }
  }

  async function updateStatus() {
    setBusy(true);
    try {
      await markReady(status, author.trim() === "" ? "You" : author.trim(), new Date().toISOString());
      pushToast({ kind: "success", title: en.dev.annotate.marked });
    } catch (caught) {
      pushToast({
        kind: "error",
        title: en.dev.annotate.handoffTitle,
        message: caught instanceof Error ? caught.message : String(caught),
      });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="dev-tab-body" role="tabpanel" aria-label={en.dev.tabs.annotate}>
      {error ? <p className="form-error">{error}</p> : null}

      <section className="dev-section">
        <h3 className="dev-section-title">{en.dev.annotate.formTitle}</h3>
        {node ? (
          <>
            <p className="dev-muted">
              {en.dev.annotate.selectedLayer}: <strong>{node.name}</strong>
            </p>
            <label className="dev-field">
              <span className="dev-field-label">{en.dev.annotate.body}</span>
              <textarea
                className="dev-textarea"
                value={body}
                rows={3}
                placeholder={en.dev.annotate.bodyPlaceholder}
                onChange={(event) => setBody(event.target.value)}
              />
            </label>
            <div className="dev-field-row">
              <label className="dev-field">
                <span className="dev-field-label">{en.dev.annotate.kind}</span>
                <select
                  className="select-field"
                  value={kind}
                  onChange={(event) => setKind(event.target.value as AnnotationKind)}
                >
                  {ANNOTATION_KINDS.map((option) => (
                    <option key={option} value={option}>
                      {en.dev.annotate.kinds[option]}
                    </option>
                  ))}
                </select>
              </label>
              <label className="dev-field">
                <span className="dev-field-label">{en.dev.annotate.link}</span>
                <input
                  className="text-input"
                  type="url"
                  value={link}
                  placeholder={en.dev.annotate.linkPlaceholder}
                  onChange={(event) => setLink(event.target.value)}
                />
              </label>
            </div>
            <div className="dev-toolbar-actions dev-toolbar-actions--end">
              <button type="button" className="dev-copy-button" disabled={busy} onClick={() => void submit()}>
                {en.dev.annotate.add}
              </button>
            </div>
          </>
        ) : (
          <div className="dev-empty">
            <p className="dev-empty-title">{en.dev.annotate.noSelection}</p>
            <p className="dev-empty-body">{en.dev.annotate.emptyBody}</p>
          </div>
        )}
      </section>

      <section className="dev-section">
        <h3 className="dev-section-title">
          {en.dev.annotate.listTitle}
          {openCount > 0 ? <span className="dev-count-badge">{en.dev.annotate.openCount.replace("{count}", String(openCount))}</span> : null}
        </h3>
        {loading && !handoff ? <p className="dev-muted">{en.dev.code.loading}</p> : null}
        {handoff && handoff.annotations.length === 0 ? (
          <div className="dev-empty">
            <p className="dev-empty-title">{en.dev.annotate.emptyTitle}</p>
            <p className="dev-empty-body">{en.dev.annotate.emptyBody}</p>
          </div>
        ) : null}
        {handoff && handoff.annotations.length > 0 ? (
          <ul className="dev-annotation-list">
            {handoff.annotations.map((annotation) => (
              <li key={annotation.id} className="dev-annotation">
                <div className="dev-annotation-head">
                  <span className="dev-chip">{en.dev.annotate.kinds[annotation.kind]}</span>
                  <strong className="dev-annotation-node">{annotation.node_name}</strong>
                  <span className={annotation.resolved ? "dev-status dev-status--ok" : "dev-status"}>
                    {annotation.resolved ? en.dev.annotate.resolved : en.dev.annotate.open}
                  </span>
                </div>
                <p className="dev-annotation-body">{annotation.body}</p>
                <div className="dev-annotation-meta">
                  <span>{en.dev.annotate.author.replace("{author}", annotation.author)}</span>
                  <span>{timestampLabel(annotation.created_at)}</span>
                </div>
                {annotation.link ? (
                  <a className="dev-annotation-link" href={annotation.link} target="_blank" rel="noreferrer">
                    {annotation.link}
                  </a>
                ) : null}
                <button
                  type="button"
                  className="dev-copy-button dev-copy-button--inline"
                  disabled={annotation.resolved || busy}
                  onClick={() => void resolve(annotation.id)}
                >
                  {annotation.resolved ? en.dev.annotate.resolved : en.dev.annotate.resolve}
                </button>
              </li>
            ))}
          </ul>
        ) : null}
      </section>

      <section className="dev-section">
        <h3 className="dev-section-title">{en.dev.annotate.handoffTitle}</h3>
        {handoff ? (
          <p className="dev-muted">
            {en.dev.actions.revision.replace("{revision}", String(handoff.revision))} ·{" "}
            <strong>{statusLabel(handoff.status)}</strong>
            {handoff.marked_ready_by
              ? ` · ${en.dev.annotate.markedBy}: ${handoff.marked_ready_by}${
                  handoff.marked_ready_at ? ` (${timestampLabel(handoff.marked_ready_at)})` : ""
                }`
              : null}
          </p>
        ) : null}
        {openCount > 0 ? (
          <p className="dev-warning-note">{en.dev.annotate.openWarning.replace("{count}", String(openCount))}</p>
        ) : (
          <p className="dev-muted">{en.dev.annotate.noOpenAnnotations}</p>
        )}
        <div className="dev-field-row">
          <label className="dev-field">
            <span className="dev-field-label">{en.dev.annotate.status}</span>
            <select
              className="select-field"
              value={status}
              onChange={(event) => setStatus(event.target.value as HandoffStatus)}
            >
              {HANDOFF_STATUSES.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <label className="dev-field">
            <span className="dev-field-label">{en.dev.annotate.markedBy}</span>
            <input
              className="text-input"
              value={author}
              placeholder={en.dev.annotate.markedByPlaceholder}
              onChange={(event) => setAuthor(event.target.value)}
            />
          </label>
        </div>
        <div className="dev-toolbar-actions dev-toolbar-actions--end">
          <button type="button" className="dev-copy-button" disabled={busy} onClick={() => void updateStatus()}>
            {en.dev.annotate.markReady}
          </button>
        </div>
      </section>
    </div>
  );
}
