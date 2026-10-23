/**
 * AI assistant right sidebar.
 *
 * Every action calls the engine's AI commands; the engine validates the
 * proposal and echoes it back through `EngineState.ai_preview`. Nothing is
 * applied until the user presses Apply.
 */
import { useEffect, useMemo, useState } from "react";

import type { AiTextTask, SnapshotAiPreview } from "../../engine/contract";
import { useAppStore, useCapabilities } from "../../engine/store";
import {
  aiApplyLast,
  aiDiscardLast,
  aiGenerateLayout,
  aiRenameLayers,
  aiTextTask,
  toggleAiPanel,
} from "../../engine/commands";
import { en } from "../../i18n/en";
import { Sparkles, X } from "../../icons";
import { Button } from "../primitives/IconButton";

type Busy = "generate" | "apply" | "discard" | "rename" | "text" | null;

const TEXT_TASKS: { value: AiTextTask; label: string }[] = [
  { value: "improve", label: en.ai.actions.taskImprove },
  { value: "explain", label: en.ai.actions.taskExplain },
  { value: "content", label: en.ai.actions.taskContent },
  { value: "style", label: en.ai.actions.taskStyle },
  { value: "comments", label: en.ai.actions.taskComments },
];

function SourceLabel({ preview }: { preview: SnapshotAiPreview }) {
  const label =
    preview.source === "offline" ? en.ai.sourceOffline : preview.source === "remote" ? en.ai.sourceRemote : preview.source;
  return <span className="ai-source">{en.ai.previewSource.replace("{source}", label)}</span>;
}

export function AiPanel() {
  const capabilities = useCapabilities();
  const doc = useAppStore((state) => state.engine.doc);
  const status = useAppStore((state) => state.status);
  const preview = useAppStore((state) => state.engine.ai_preview) ?? null;
  const engineStatus = useAppStore((state) => state.engine.status);
  const selectedText = useAppStore(
    (state) => state.engine.selection_details.find((detail) => detail.text !== null)?.text ?? null,
  );

  const [prompt, setPrompt] = useState("");
  const [busy, setBusy] = useState<Busy>(null);
  const [error, setError] = useState<string | null>(null);
  const [task, setTask] = useState<AiTextTask>("improve");
  const [taskInput, setTaskInput] = useState("");
  const [taskResult, setTaskResult] = useState<string | null>(null);

  const enabled = capabilities.ai === true && status === "ready" && doc !== null;
  const text = selectedText?.characters ?? "";

  useEffect(() => {
    if (text !== "" && taskInput.trim() === "") setTaskInput(text);
  }, [text, taskInput]);

  const stepsLabel = useMemo(() => {
    if (!preview) return "";
    return preview.steps === 1 ? en.ai.stepsOne : en.ai.steps.replace("{count}", String(preview.steps));
  }, [preview]);

  const runGenerate = async () => {
    setBusy("generate");
    setError(null);
    const result = await aiGenerateLayout(prompt);
    setBusy(null);
    setError(result.error);
  };

  const runApply = async () => {
    if (!preview) return;
    setBusy("apply");
    setError(null);
    const result = await aiApplyLast(preview.requires_confirmation);
    setBusy(null);
    setError(result.error);
  };

  const runDiscard = async () => {
    setBusy("discard");
    setError(null);
    const result = await aiDiscardLast();
    setBusy(null);
    setError(result.error);
  };

  const runRename = async () => {
    setBusy("rename");
    setError(null);
    const result = await aiRenameLayers();
    setBusy(null);
    setError(result.error);
  };

  const runTextTask = async () => {
    setBusy("text");
    setError(null);
    const result = await aiTextTask(task, taskInput);
    setBusy(null);
    setError(result.error);
    if (result.state) setTaskResult(result.state.status);
  };

  return (
    <section className="panel panel--right ai-panel" aria-label={en.ai.title}>
      <header className="side-panel-header">
        <div className="side-panel-heading">
          <p className="side-panel-kicker">
            <Sparkles size={12} aria-hidden="true" /> {en.ai.label}
          </p>
          <h2 className="side-panel-title">{en.ai.title}</h2>
          <p className="side-panel-subtitle">{en.ai.subtitle}</p>
        </div>
        <button
          type="button"
          className="side-panel-close"
          aria-label={en.ai.close}
          title={en.ai.close}
          onClick={() => toggleAiPanel()}
        >
          <X size={15} />
        </button>
      </header>

      <div className="side-panel-body">
        {!enabled ? (
          <div className="side-empty">
            <p className="side-empty-title">{en.ai.disabledTitle}</p>
            <p className="side-empty-body">
              {capabilities.ai !== true ? en.ai.disabledHint : en.ai.notConnected}
            </p>
          </div>
        ) : (
          <>
            <section className="side-section" aria-labelledby="ai-generate-title">
              <h3 className="side-section-title" id="ai-generate-title">
                {en.ai.generate}
              </h3>
              <label className="side-field">
                <span className="side-field-label">{en.ai.promptLabel}</span>
                <textarea
                  className="text-area"
                  rows={4}
                  value={prompt}
                  placeholder={en.ai.promptPlaceholder}
                  aria-label={en.ai.promptLabel}
                  onChange={(event) => setPrompt(event.target.value)}
                />
              </label>
              <p className="side-hint">{en.ai.promptHint}</p>
              <div className="side-actions">
                <Button
                  variant="primary"
                  icon={<Sparkles size={14} />}
                  disabled={busy !== null}
                  onClick={() => void runGenerate()}
                  title={en.ai.generateHint}
                >
                  {busy === "generate" ? en.ai.generating : en.ai.generate}
                </Button>
              </div>
              <p className="side-hint side-hint--muted">{en.ai.example}</p>
            </section>

            <section className="side-section" aria-labelledby="ai-preview-title">
              <h3 className="side-section-title" id="ai-preview-title">
                {en.ai.previewTitle}
              </h3>
              {preview ? (
                <div className={preview.requires_confirmation ? "ai-preview ai-preview--destructive" : "ai-preview"}>
                  <div className="ai-preview-meta">
                    <SourceLabel preview={preview} />
                    <span className="ai-preview-steps">{stepsLabel}</span>
                  </div>
                  <ol className="ai-steps">
                    {preview.summary.map((line, index) => (
                      <li key={`${index}-${line}`} className="ai-step">
                        {line}
                      </li>
                    ))}
                  </ol>
                  {preview.warnings.length > 0 ? (
                    <div className="ai-warnings" role="note">
                      <p className="ai-warnings-title">{en.ai.warnings}</p>
                      <ul>
                        {preview.warnings.map((warning) => (
                          <li key={warning}>{warning}</li>
                        ))}
                      </ul>
                    </div>
                  ) : null}
                  {preview.requires_confirmation ? (
                    <div className="ai-confirm" role="alert">
                      <p className="ai-confirm-title">{en.ai.requiresConfirmationTitle}</p>
                      <p className="ai-confirm-body">{en.ai.requiresConfirmationBody}</p>
                    </div>
                  ) : null}
                  <div className="side-actions">
                    <Button
                      variant="primary"
                      disabled={busy !== null}
                      onClick={() => void runApply()}
                      title={en.ai.applyHint}
                    >
                      {preview.requires_confirmation ? en.ai.applyConfirm : en.ai.apply}
                    </Button>
                    <Button variant="ghost" disabled={busy !== null} onClick={() => void runDiscard()}>
                      {en.ai.discard}
                    </Button>
                  </div>
                  <p className="side-hint side-hint--muted">{en.ai.applyHint}</p>
                </div>
              ) : (
                <div className="side-empty side-empty--compact">
                  <p className="side-empty-title">{en.ai.emptyTitle}</p>
                  <p className="side-empty-body">{en.ai.emptyBody}</p>
                </div>
              )}
              {error ? (
                <p className="form-error" role="alert">
                  {error}
                </p>
              ) : null}
            </section>

            <section className="side-section" aria-labelledby="ai-actions-title">
              <h3 className="side-section-title" id="ai-actions-title">
                {en.ai.actions.title}
              </h3>
              <div className="side-actions">
                <Button
                  variant="ghost"
                  disabled={busy !== null || engineStatus === null}
                  title={en.ai.actions.renameHint}
                  onClick={() => void runRename()}
                >
                  {en.ai.actions.renameLayers}
                </Button>
              </div>
              <div className="side-field">
                <label className="side-field-label" htmlFor="ai-task">
                  {en.ai.actions.textTitle}
                </label>
                <select
                  id="ai-task"
                  className="select-field"
                  value={task}
                  onChange={(event) => setTask(event.target.value as AiTextTask)}
                >
                  {TEXT_TASKS.map((item) => (
                    <option key={item.value} value={item.value}>
                      {item.label}
                    </option>
                  ))}
                </select>
              </div>
              <label className="side-field">
                <span className="side-field-label">{en.ai.actions.run}</span>
                <textarea
                  className="text-area"
                  rows={3}
                  value={taskInput}
                  aria-label={en.ai.actions.run}
                  placeholder={selectedText ? text : en.ai.actions.noTextSelection}
                  onChange={(event) => setTaskInput(event.target.value)}
                />
              </label>
              <p className="side-hint">{en.ai.actions.textHint}</p>
              <div className="side-actions">
                <Button
                  variant="ghost"
                  disabled={busy !== null || taskInput.trim() === ""}
                  onClick={() => void runTextTask()}
                >
                  {en.ai.actions.run}
                </Button>
              </div>
              <div className="side-result" role="status" aria-live="polite">
                <p className="side-result-title">{en.ai.resultTitle}</p>
                <p className="side-result-body">{taskResult ?? engineStatus ?? en.ai.emptyBody}</p>
                <p className="side-hint side-hint--muted">{en.ai.actions.runHint}</p>
              </div>
            </section>
          </>
        )}
      </div>
    </section>
  );
}
