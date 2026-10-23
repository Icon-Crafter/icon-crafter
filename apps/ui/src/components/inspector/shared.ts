import { useCallback, useMemo, useRef } from "react";
import type {
  BlendMode,
  Color,
  Effect,
  Fill,
  NodeDetail,
  NodePatch,
  Stroke,
} from "../../engine/contract";
import { applyPatchToSelection } from "../../engine/commands";
import { useAppStore } from "../../engine/store";

/** Value shared by all details, or a mixed marker. */
export interface MixedValue<T> {
  mixed: boolean;
  value: T | undefined;
}

export function commonValue<T>(details: readonly NodeDetail[], read: (detail: NodeDetail) => T): MixedValue<T> {
  if (details.length === 0) return { mixed: false, value: undefined };
  const first = read(details[0] as NodeDetail);
  const mixed = details.some((detail) => JSON.stringify(read(detail)) !== JSON.stringify(first));
  return { mixed, value: mixed ? undefined : first };
}

export function useSelectionDetails(): NodeDetail[] {
  return useAppStore((state) => state.engine.selection_details);
}

/**
 * Patch session: consecutive edits within one gesture share a coalesce key so
 * the engine records a single undo step. Call `end()` on pointer-up/blur.
 */
export function usePatchSession(prefix: string) {
  const generation = useRef(0);
  const ids = useAppStore((state) => state.engine.selection.join(","));

  const key = useCallback(() => `${prefix}:${ids}:${generation.current}`, [ids, prefix]);
  const end = useCallback(() => {
    generation.current += 1;
  }, []);

  const apply = useCallback(
    (patch: NodePatch, options?: { label?: string; coalesce?: boolean }) => {
      void applyPatchToSelection(patch, {
        label: options?.label,
        coalesceKey: options?.coalesce === false ? undefined : key(),
      });
    },
    [key],
  );

  return { apply, end, key };
}

export const SOLID_CORAL: Color = { r: 0.89, g: 0.42, b: 0.29, a: 1 };
export const WHITE: Color = { r: 1, g: 1, b: 1, a: 1 };
export const BLACK: Color = { r: 0, g: 0, b: 0, a: 1 };

export function solidPaint(color: Color) {
  return { type: "solid" as const, color };
}

export function createFill(color: Color = SOLID_CORAL): Fill {
  return {
    visible: true,
    opacity: 1,
    blend: "normal" as BlendMode,
    paint: solidPaint(color),
    style_id: null,
    variable_id: null,
  };
}

export function createStroke(color: Color = { r: 0.16, g: 0.19, b: 0.24, a: 1 }): Stroke {
  return {
    visible: true,
    opacity: 1,
    blend: "normal" as BlendMode,
    paint: solidPaint(color),
    style_id: null,
    variable_id: null,
    weight: 1,
    align: "center",
    cap: "butt",
    join: "miter",
    miter_limit: 4,
    dash: null,
  };
}

export function createEffect(type: "drop_shadow" | "inner_shadow" = "drop_shadow"): Effect {
  return {
    type,
    color: { r: 0, g: 0, b: 0, a: 0.25 },
    offset: { x: 0, y: 4 },
    blur: 12,
    spread: 0,
    blend: "normal" as BlendMode,
  };
}

export function effectColor(effect: Effect): Color {
  if (effect.type === "drop_shadow" || effect.type === "inner_shadow") return effect.color;
  return { r: 1, g: 1, b: 1, a: 0.4 };
}

export function clamp01(value: number): number {
  return Math.min(1, Math.max(0, value));
}

/** Details are already depth-first; this returns the topmost id. */
export function topmostId(details: readonly NodeDetail[]): string | undefined {
  return details[0]?.id;
}

export function useInspectorDetails() {
  const details = useSelectionDetails();
  const available = useMemo(() => details.filter((detail) => detail !== undefined), [details]);
  return available;
}
