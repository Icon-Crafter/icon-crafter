import { useMemo, useState } from "react";
import type { NodeDetail } from "../../engine/contract";
import { useAppStore } from "../../engine/store";
import {
  actionLabel,
  buildAction,
  buildTrigger,
  containerOptions,
  emptyInteractionForm,
  frameNameFor,
  framesFromSummary,
  nodeNameFor,
  presentTargetId,
  triggerLabel,
  validateInteractionForm,
  type ActionLabels,
  type InteractionForm,
  type TriggerLabels,
} from "../../engine/prototype";
import {
  addInteraction,
  presentPrototype,
  removeInteraction,
} from "../../engine/commands";
import { en } from "../../i18n/en";
import { Play, Plus, Trash2 } from "../../icons";
import { Section } from "../primitives/Layout";

const TRIGGER_LABELS: TriggerLabels = {
  click: en.prototype.triggerClick,
  hover: en.prototype.triggerHover,
  press: en.prototype.triggerPress,
  afterDelay: (ms) => `${en.prototype.triggerDelay} ${Math.round(ms)} ms`,
};

/** Prototype authoring for the selected layer: triggers, actions, presentation entry. */
export function PrototypeSection({ node }: { node: NodeDetail }) {
  const engine = useAppStore((state) => state.engine);
  const [form, setForm] = useState<InteractionForm>(() => emptyInteractionForm());
  const [error, setError] = useState<string | null>(null);

  const frames = useMemo(() => framesFromSummary(engine.summary), [engine.summary]);
  const targets = useMemo(() => containerOptions(engine.summary), [engine.summary]);
  const interactions = node.prototype?.interactions ?? [];

  const actionLabels: ActionLabels = {
    navigateTo: (frame) => `${en.prototype.actionNavigate} ${frameNameFor(engine.summary, frame)}`,
    openOverlay: (frame) => `${en.prototype.actionOverlay} ${frameNameFor(engine.summary, frame)}`,
    closeOverlay: en.prototype.actionClose,
    back: en.prototype.actionBack,
    scrollTo: (target) => `${en.prototype.actionScroll} ${nodeNameFor(engine.summary, target)}`,
    setVariable: (variable) => `${en.prototype.actionSetVariable} ${variable}`,
  };

  const canPresent = presentTargetId(engine).ok;

  const submit = () => {
    const validation = validateInteractionForm(form);
    if (validation === "frameRequired") {
      setError(en.prototype.frameRequired);
      return;
    }
    if (validation === "targetRequired") {
      setError(en.prototype.targetRequired);
      return;
    }
    const action = buildAction(form);
    if (!action) {
      setError(en.prototype.frameRequired);
      return;
    }
    setError(null);
    void addInteraction(node.id, buildTrigger(form), [action]);
    setForm((current) => emptyInteractionForm(current.triggerType));
  };

  return (
    <Section id="prototype" title={en.prototype.title}>
      {node.kind === "frame" && (
        <div className="prototype-present-row">
          <button
            type="button"
            className="button button--ghost button--small"
            disabled={!canPresent}
            title={canPresent ? en.prototype.presentHint : en.prototype.noFrames}
            onClick={() => void presentPrototype()}
          >
            <Play size={12} /> {en.prototype.present}
          </button>
          <span className="asset-group-hint">{en.prototype.presentHint}</span>
        </div>
      )}

      {interactions.length === 0 ? (
        <p className="asset-group-hint">{en.prototype.emptyTitle}</p>
      ) : (
        <div className="interaction-list">
          {interactions.map((interaction) => (
            <div key={interaction.id} className="interaction-row">
              <span className="interaction-trigger">{triggerLabel(interaction.trigger, TRIGGER_LABELS)}</span>
              <span className="interaction-actions">
                {interaction.actions.map((action, index) => (
                  <span key={`${interaction.id}-${index}`} className="dev-chip">
                    {actionLabel(action, actionLabels)}
                  </span>
                ))}
              </span>
              <button
                type="button"
                className="icon-button icon-button--danger"
                aria-label={en.prototype.remove}
                title={en.prototype.remove}
                onClick={() => void removeInteraction(node.id, interaction.id)}
              >
                <Trash2 size={12} />
              </button>
            </div>
          ))}
        </div>
      )}

      <div className="interaction-form">
        <label className="form-field-inline">
          <span className="field-label">{en.prototype.trigger}</span>
          <select
            className="select-field"
            value={form.triggerType}
            onChange={(event) =>
              setForm((current) => ({ ...current, triggerType: event.target.value as InteractionForm["triggerType"] }))
            }
          >
            <option value="click">{en.prototype.triggerClick}</option>
            <option value="hover">{en.prototype.triggerHover}</option>
            <option value="press">{en.prototype.triggerPress}</option>
            <option value="after_delay">{en.prototype.triggerDelay}</option>
          </select>
        </label>
        {form.triggerType === "after_delay" && (
          <label className="form-field-inline">
            <span className="field-label">{en.prototype.delayMs}</span>
            <input
              type="number"
              className="text-input"
              min={0}
              step={50}
              value={form.delayMs}
              onChange={(event) => setForm((current) => ({ ...current, delayMs: Number(event.target.value) }))}
            />
          </label>
        )}
        <label className="form-field-inline">
          <span className="field-label">{en.prototype.action}</span>
          <select
            className="select-field"
            value={form.actionType}
            onChange={(event) =>
              setForm((current) => ({ ...current, actionType: event.target.value as InteractionForm["actionType"] }))
            }
          >
            <option value="navigate_to">{en.prototype.actionNavigate}</option>
            <option value="open_overlay">{en.prototype.actionOverlay}</option>
            <option value="close_overlay">{en.prototype.actionClose}</option>
            <option value="back">{en.prototype.actionBack}</option>
            <option value="scroll_to">{en.prototype.actionScroll}</option>
          </select>
        </label>
        {(form.actionType === "navigate_to" || form.actionType === "open_overlay") && (
          <label className="form-field-inline">
            <span className="field-label">{en.prototype.frame}</span>
            <select
              className="select-field"
              value={form.frameId}
              onChange={(event) => setForm((current) => ({ ...current, frameId: event.target.value }))}
            >
              <option value="">—</option>
              {frames.map((frame) => (
                <option key={frame.id} value={frame.id}>
                  {frame.name}
                </option>
              ))}
            </select>
          </label>
        )}
        {form.actionType === "scroll_to" && (
          <label className="form-field-inline">
            <span className="field-label">{en.prototype.target}</span>
            <select
              className="select-field"
              value={form.target}
              onChange={(event) => setForm((current) => ({ ...current, target: event.target.value }))}
            >
              <option value="">—</option>
              {targets.map((target) => (
                <option key={target.id} value={target.id}>
                  {target.name}
                </option>
              ))}
            </select>
          </label>
        )}
        {error && <p className="form-error">{error}</p>}
        <button type="button" className="button button--primary button--small" onClick={submit}>
          <Plus size={12} /> {en.prototype.add}
        </button>
      </div>
      {node.prototype?.interactions?.some((interaction) => interaction.actions.some((action) => action.type === "scroll_to")) && (
        <p className="asset-group-hint">{en.prototype.scrollNotSimulated}</p>
      )}
    </Section>
  );
}
