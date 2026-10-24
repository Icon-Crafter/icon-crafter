/**
 * Pure helpers for the AI assistant and plugin manager surfaces.
 *
 * Kept separate from the components so panel availability and preview labels
 * can be unit tested without a running engine.
 */
import type { Capabilities, SnapshotAiPreview } from "./contract";
import { en } from "../i18n/en";

export type PanelAvailability = "ready" | "disabled" | "offline";

/** Whether the assistant panel can talk to a live document. */
export function assistantAvailability(
  capabilities: Capabilities,
  status: "connecting" | "ready" | "unavailable",
  hasDocument: boolean,
): PanelAvailability {
  if (capabilities.ai !== true) return "disabled";
  if (status !== "ready" || !hasDocument) return "offline";
  return "ready";
}

/** Whether the plugin manager can talk to a live document. */
export function pluginAvailability(
  capabilities: Capabilities,
  status: "connecting" | "ready" | "unavailable",
  hasDocument: boolean,
): PanelAvailability {
  if (capabilities.plugins !== true) return "disabled";
  if (status !== "ready" || !hasDocument) return "offline";
  return "ready";
}

/** Human label for the proposal source reported by the engine. */
export function aiSourceLabel(source: string): string {
  if (source === "offline") return en.ai.sourceOffline;
  if (source === "remote") return en.ai.sourceRemote;
  return source;
}

/** "3 steps" / "1 step" label for a pending proposal. */
export function aiStepsLabel(preview: Pick<SnapshotAiPreview, "steps">): string {
  return preview.steps === 1 ? en.ai.stepsOne : en.ai.steps.replace("{count}", String(preview.steps));
}

/**
 * Parses user-entered text into the type of the current setting value.
 * Falls back to the raw string when the target type is not a number/boolean.
 */
export function parseSettingInput(raw: string, current: unknown): unknown {
  if (typeof current === "number") {
    const value = Number(raw);
    return Number.isFinite(value) ? value : current;
  }
  if (typeof current === "boolean") {
    const normalized = raw.trim().toLowerCase();
    if (["true", "1", "yes", "on"].includes(normalized)) return true;
    if (["false", "0", "no", "off"].includes(normalized)) return false;
    return current;
  }
  return raw;
}
