/**
 * Dev Mode pure model: revision captures, measurement formatting and diff
 * summaries. Kept free of React and IPC so it can be unit-tested directly.
 *
 * Direction note: the engine's `dev_compare` is called with the CURRENT
 * document as "before" and the CAPTURED revision as "after" (see
 * crates/dp-editor compare_revision). Its `added` list therefore contains
 * nodes that existed at capture time but not now, i.e. nodes REMOVED since
 * the capture. `summarizeDiffForUi` re-labels them for humans without hiding
 * the engine's raw lists.
 */

import type {
  InspectSnapshot,
  NodeChange,
  NodeDiff,
  NodeInspect,
  PaintInfo,
  RevisionDiff,
} from "./contract";

export interface RevisionCapture {
  /** Stable local id (`cap_<timestamp>_<seq>`); used for dedupe/removal. */
  id: string;
  label: string;
  /** Document revision when the capture was taken. */
  revision: number;
  captured_at: string;
  snapshot: string;
}

export const CAPTURE_STORAGE_KEY = "iconcrafter.dev.captures.v1";
export const MAX_CAPTURES = 5;

function isCapture(value: unknown): value is RevisionCapture {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<RevisionCapture>;
  return (
    typeof candidate.id === "string" &&
    typeof candidate.label === "string" &&
    typeof candidate.revision === "number" &&
    typeof candidate.captured_at === "string" &&
    typeof candidate.snapshot === "string"
  );
}

/** Reads captures, tolerating absent/corrupt storage (returns []). */
export function loadCaptures(storage: Storage | undefined): RevisionCapture[] {
  if (!storage) return [];
  try {
    const raw = storage.getItem(CAPTURE_STORAGE_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isCapture).slice(0, MAX_CAPTURES);
  } catch {
    return [];
  }
}

export function saveCaptures(
  captures: readonly RevisionCapture[],
  storage: Storage | undefined,
): void {
  if (!storage) return;
  try {
    storage.setItem(CAPTURE_STORAGE_KEY, JSON.stringify(captures.slice(0, MAX_CAPTURES)));
  } catch {
    // Quota/privacy failures must never break the editor.
  }
}

/**
 * Adds a capture (newest first). A capture with the same document revision and
 * identical snapshot replaces the previous one instead of duplicating it; the
 * list is capped at `max`.
 */
export function addCapture(
  captures: readonly RevisionCapture[],
  capture: RevisionCapture,
  max: number = MAX_CAPTURES,
): RevisionCapture[] {
  const deduped = captures.filter(
    (existing) =>
      existing.id !== capture.id &&
      !(existing.revision === capture.revision && existing.snapshot === capture.snapshot),
  );
  return [capture, ...deduped].slice(0, Math.max(1, max));
}

export function removeCapture(
  captures: readonly RevisionCapture[],
  id: string,
): RevisionCapture[] {
  return captures.filter((capture) => capture.id !== id);
}

export function nextCaptureLabel(captures: readonly RevisionCapture[], revision: number): string {
  const base = `Revision ${revision}`;
  if (!captures.some((capture) => capture.label === base)) return base;
  let index = 2;
  while (captures.some((capture) => capture.label === `${base} (${index})`)) index += 1;
  return `${base} (${index})`;
}

function round(value: number, digits = 2): string {
  const rounded = Number(value.toFixed(digits));
  return String(rounded);
}

export function formatFillCss(paint: PaintInfo | null | undefined): string | null {
  if (!paint) return null;
  if (paint.kind === "solid" && paint.color) {
    return formatColorCss(paint.color);
  }
  if (paint.kind === "image") return null;
  const stop = paint.stops[0];
  if (!stop) return null;
  return formatColorCss(stop.color);
}

/** CSS colour for a resolved sRGB colour (`#RRGGBB` or `rgba(...)`). */
export function formatColorCss(color: { r: number; g: number; b: number; a: number }): string {
  const channel = (value: number): number => Math.round(clampChannel(value) * 255);
  if (color.a >= 1) {
    const hex = `#${[color.r, color.g, color.b]
      .map((value) => channel(value).toString(16).padStart(2, "0"))
      .join("")}`;
    return hex.toUpperCase();
  }
  return `rgba(${channel(color.r)}, ${channel(color.g)}, ${channel(color.b)}, ${round(color.a, 3)})`;
}

function clampChannel(value: number): number {
  return Math.min(1, Math.max(0, value));
}

/** Multi-line plain-text measurement block, used by the Copy button. */
export function formatMeasurements(snapshot: InspectSnapshot): string {
  const lines: string[] = [];
  if (snapshot.nodes.length === 0) {
    lines.push("No layers selected.");
    if (snapshot.warnings.length > 0) {
      lines.push(...snapshot.warnings.map((warning) => `Note: ${warning}`));
    }
    return lines.join("\n");
  }
  for (const node of snapshot.nodes) {
    lines.push(`${node.name} (${node.kind})`);
    lines.push(
      `  Position  ${round(node.bounds[0])}, ${round(node.bounds[1])}  ·  Size  ${round(node.bounds[2])} × ${round(node.bounds[3])}`,
    );
    if (node.rotation !== 0) lines.push(`  Rotation  ${round(node.rotation)}°`);
    if (node.opacity < 1) lines.push(`  Opacity  ${round(node.opacity * 100)} %`);
    lines.push(`  Blend  ${node.blend}`);
    if (node.fills.length > 0) {
      lines.push(
        `  Fill  ${node.fills
          .map((fill) => `${formatFillCss(fill) ?? fill.kind}${fill.style_id ? ` · style ${fill.style_id}` : ""}${fill.variable_name ? ` · ${fill.variable_name}` : ""}`)
          .join("  |  ")}`,
      );
    }
    if (node.strokes.length > 0) {
      lines.push(
        `  Stroke  ${node.strokes
          .map((stroke) => `${formatFillCss(stroke.paint) ?? stroke.paint.kind} ${round(stroke.weight)} px ${stroke.align}`)
          .join("  |  ")}`,
      );
    }
    if (node.effects.length > 0) {
      lines.push(`  Effects  ${node.effects.map((effect) => effect.kind).join(", ")}`);
    }
    if (node.text) {
      lines.push(
        `  Text  ${node.text.font_family} ${round(node.text.font_size)} px / ${node.text.font_weight}${node.text.italic ? " italic" : ""}`,
      );
    }
    if (node.component) {
      lines.push(
        `  Component  ${node.component.role}${node.component.component_name ? ` · ${node.component.component_name}` : ""}${node.component.override_count > 0 ? ` · ${node.component.override_count} overrides` : ""}`,
      );
    }
  }
  if (snapshot.warnings.length > 0) {
    lines.push(...snapshot.warnings.map((warning) => `Note: ${warning}`));
  }
  return lines.join("\n");
}

export interface DiffSummary {
  /** Nodes that are only in the current document (diff.removed from engine). */
  addedSinceCapture: NodeDiff[];
  /** Nodes that existed at capture time but are gone now (diff.added). */
  removedSinceCapture: NodeDiff[];
  changed: NodeChange[];
  counts: { added: number; removed: number; changed: number };
  hasChanges: boolean;
}

/**
 * Re-labels a RevisionDiff for humans: the engine compares `current` (before)
 * against `captured` (after), so its added/removed are inverted for the UI.
 */
export function summarizeDiffForUi(diff: RevisionDiff): DiffSummary {
  const addedSinceCapture = diff.removed;
  const removedSinceCapture = diff.added;
  return {
    addedSinceCapture,
    removedSinceCapture,
    changed: diff.changed,
    counts: {
      added: addedSinceCapture.length,
      removed: removedSinceCapture.length,
      changed: diff.changed.length,
    },
    hasChanges:
      addedSinceCapture.length > 0 || removedSinceCapture.length > 0 || diff.changed.length > 0,
  };
}

export function formatDiffCounts(summary: DiffSummary): string {
  if (!summary.hasChanges) return "No structural changes since this capture.";
  const parts = [
    `${summary.counts.added} added`,
    `${summary.counts.removed} removed`,
    `${summary.counts.changed} changed`,
  ];
  return parts.join(" · ");
}

export function describeNode(node: NodeInspect): string {
  return `${node.name} (${node.kind})`;
}
