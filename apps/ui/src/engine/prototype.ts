/**
 * Pure helpers for the prototype authoring + presentation UI.
 *
 * Everything here is presentation logic only: payload builders mirror the
 * engine's serde shapes, and label helpers translate triggers/actions into
 * short human text. The engine remains the source of truth for navigation and
 * click hit-testing; the UI forwards pointer events and draws hotspots.
 */
import type {
  ActionJson,
  DocSummary,
  EngineState,
  PrototypeHotspot,
  PrototypeNodeJson,
  Rect,
  TriggerJson,
  Viewport,
} from "./contract";
import { screenToWorld, worldToScreen } from "./contract";

export const TRIGGER_TYPES = ["click", "hover", "press", "after_delay"] as const;
export type TriggerType = (typeof TRIGGER_TYPES)[number];

export const ACTION_TYPES = [
  "navigate_to",
  "open_overlay",
  "close_overlay",
  "back",
  "scroll_to",
] as const;
export type ActionType = (typeof ACTION_TYPES)[number];

export interface InteractionForm {
  triggerType: TriggerType;
  /** Milliseconds for `after_delay` triggers. */
  delayMs: number;
  actionType: ActionType;
  frameId: string;
  /** Target node id for `scroll_to`. */
  target: string;
}

export function emptyInteractionForm(trigger: TriggerType = "click"): InteractionForm {
  return {
    triggerType: trigger,
    delayMs: 300,
    actionType: "navigate_to",
    frameId: "",
    target: "",
  };
}

export function buildTrigger(form: InteractionForm): TriggerJson {
  switch (form.triggerType) {
    case "hover":
      return { type: "hover" };
    case "press":
      return { type: "press" };
    case "after_delay":
      return { type: "after_delay", delay_ms: Math.max(0, Math.round(form.delayMs)) };
    case "click":
    default:
      return { type: "click", delay_ms: 0 };
  }
}

/** Returns null when the form has no action payload to send. */
export function buildAction(form: InteractionForm): ActionJson | null {
  switch (form.actionType) {
    case "navigate_to":
      return form.frameId ? { type: "navigate_to", frame_id: form.frameId } : null;
    case "open_overlay":
      return form.frameId ? { type: "open_overlay", frame_id: form.frameId } : null;
    case "close_overlay":
      return { type: "close_overlay" };
    case "back":
      return { type: "back" };
    case "scroll_to":
      return form.target ? { type: "scroll_to", target: form.target } : null;
    default:
      return null;
  }
}

export type FormError = "frameRequired" | "targetRequired" | null;

export function validateInteractionForm(form: InteractionForm): FormError {
  if (form.actionType === "navigate_to" || form.actionType === "open_overlay") {
    return form.frameId ? null : "frameRequired";
  }
  if (form.actionType === "scroll_to") {
    return form.target ? null : "targetRequired";
  }
  return null;
}

export interface FrameOption {
  id: string;
  name: string;
}

/** All frames in summary order (topmost-first), for target pickers. */
export function framesFromSummary(summary: DocSummary | null): FrameOption[] {
  if (!summary) return [];
  return summary.nodes
    .filter((node) => node.kind === "frame")
    .map((node) => ({ id: node.id, name: node.name }));
}

/** Container nodes (frames/groups/components) that can receive a scroll target. */
export function containerOptions(summary: DocSummary | null): FrameOption[] {
  if (!summary) return [];
  return summary.nodes
    .filter(
      (node) =>
        node.kind === "frame" ||
        node.kind === "group" ||
        node.kind === "component" ||
        node.kind === "component_set" ||
        node.kind === "instance",
    )
    .map((node) => ({ id: node.id, name: node.name }));
}

export function frameNameFor(summary: DocSummary | null, frameId: string): string {
  const frame = summary?.nodes.find((node) => node.id === frameId);
  return frame?.name ?? "Frame";
}

export function nodeNameFor(summary: DocSummary | null, nodeId: string): string {
  const node = summary?.nodes.find((candidate) => candidate.id === nodeId);
  return node?.name ?? "Layer";
}

/**
 * The frame to present when the user has no frame selected: explicit frame
 * selection wins, then the first frame in summary order.
 */
export function presentTargetId(
  engine: EngineState,
): { ok: true; frameId?: string } | { ok: false } {
  const selectedFrame = engine.selection_details.find((node) => node.kind === "frame");
  if (selectedFrame) return { ok: true, frameId: selectedFrame.id };
  const firstFrame = framesFromSummary(engine.summary)[0];
  if (firstFrame) return { ok: true, frameId: firstFrame.id };
  return { ok: false };
}

// ---------------------------------------------------------------------------
// Labels (caller supplies translated text so this module stays i18n-agnostic)
// ---------------------------------------------------------------------------

export interface TriggerLabels {
  click: string;
  hover: string;
  press: string;
  afterDelay: (ms: number) => string;
}

export function triggerLabel(trigger: TriggerJson, labels: TriggerLabels): string {
  switch (trigger.type) {
    case "hover":
      return labels.hover;
    case "press":
      return labels.press;
    case "after_delay":
      return labels.afterDelay(trigger.delay_ms);
    case "key":
      return trigger.key;
    case "click":
    default:
      return labels.click;
  }
}

export interface ActionLabels {
  navigateTo: (frame: string) => string;
  openOverlay: (frame: string) => string;
  closeOverlay: string;
  back: string;
  scrollTo: (target: string) => string;
  setVariable: (variable: string) => string;
}

export function actionLabel(action: ActionJson, labels: ActionLabels): string {
  switch (action.type) {
    case "navigate_to":
      return labels.navigateTo(action.frame_id);
    case "open_overlay":
      return labels.openOverlay(action.frame_id);
    case "close_overlay":
      return labels.closeOverlay;
    case "back":
      return labels.back;
    case "scroll_to":
      return labels.scrollTo(action.target);
    case "set_variable":
      return labels.setVariable(action.variable_id);
    default:
      return "";
  }
}

export function interactionCount(prototype: PrototypeNodeJson | null | undefined): number {
  return prototype?.interactions.length ?? 0;
}

// ---------------------------------------------------------------------------
// Presentation geometry
// ---------------------------------------------------------------------------

export interface ScreenHotspot {
  nodeId: string;
  label: string;
  rect: { x: number; y: number; w: number; h: number };
}

export function hotspotScreenRect(rect: Rect, viewport: Viewport): ScreenHotspot["rect"] {
  const topLeft = worldToScreen({ x: rect.x, y: rect.y }, viewport);
  return {
    x: topLeft.x,
    y: topLeft.y,
    w: rect.w * viewport.zoom,
    h: rect.h * viewport.zoom,
  };
}

export function screenHotspots(
  hotspots: readonly PrototypeHotspot[] | undefined,
  viewport: Viewport,
): ScreenHotspot[] {
  if (!hotspots) return [];
  return hotspots.map((hotspot) => ({
    nodeId: hotspot.node_id,
    label: hotspot.label,
    rect: hotspotScreenRect(hotspot.rect, viewport),
  }));
}

/** Topmost hotspot under a viewport point (engine lists them topmost-last-ish). */
export function hotspotAtScreenPoint(
  hotspots: readonly PrototypeHotspot[] | undefined,
  viewport: Viewport,
  point: { x: number; y: number },
): PrototypeHotspot | null {
  if (!hotspots) return null;
  const world = screenToWorld(point, viewport);
  for (let i = hotspots.length - 1; i >= 0; i -= 1) {
    const hotspot = hotspots[i];
    if (!hotspot) continue;
    const { rect } = hotspot;
    if (
      world.x >= rect.x &&
      world.x <= rect.x + rect.w &&
      world.y >= rect.y &&
      world.y <= rect.y + rect.h
    ) {
      return hotspot;
    }
  }
  return null;
}

export function canPresent(engine: EngineState, capability: boolean | undefined): boolean {
  return capability === true && presentTargetId(engine).ok;
}
