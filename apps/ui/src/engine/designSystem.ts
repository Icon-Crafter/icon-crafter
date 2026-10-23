/**
 * Pure helpers for the design-system panels (styles + variables).
 *
 * The engine is authoritative for applying styles and evaluating variables;
 * these helpers only decide which actions make sense in the UI, format values
 * for display and parse user input into contract payloads.
 */
import type {
  Color,
  NodeDetail,
  StyleEntry,
  VariableCollectionEntry,
  VariableEntry,
  VariableTypeName,
  VariableValueJson,
} from "./contract";
import { colorToHex, hexToColor } from "../utils/color";

export type StyleKind = "paint" | "text" | "effect";

/**
 * What style can be created from this node, mirroring the engine's order:
 * text → text style, visible fill → paint style, effects → effect style,
 * visible stroke → paint style.
 */
export function detectStyleKind(node: NodeDetail | null | undefined): StyleKind | null {
  if (!node) return null;
  if (node.text) return "text";
  if (node.fills.some((fill) => fill.visible)) return "paint";
  if (node.effects.length > 0) return "effect";
  if (node.strokes.some((stroke) => stroke !== null)) return "paint";
  return null;
}

export function styleNameFor(kind: StyleKind, node: NodeDetail | null | undefined): string {
  const base = node?.name?.trim() || "Style";
  const suffix = kind === "text" ? "text" : kind === "effect" ? "effect" : "fill";
  return `${base} ${suffix}`;
}

export function styleKindLabelKey(kind: StyleKind): "paint" | "text" | "effect" {
  return kind;
}

export function styleCounts(styles: {
  paints: StyleEntry[];
  texts: StyleEntry[];
  effects: StyleEntry[];
} | null): { paints: number; texts: number; effects: number; total: number } {
  const paints = styles?.paints.length ?? 0;
  const texts = styles?.texts.length ?? 0;
  const effects = styles?.effects.length ?? 0;
  return { paints, texts, effects, total: paints + texts + effects };
}

// ---------------------------------------------------------------------------
// Variables
// ---------------------------------------------------------------------------

export function activeModeId(collection: VariableCollectionEntry): string {
  return collection.active_mode_id ?? collection.modes[0]?.id ?? "";
}

export function modeName(collection: VariableCollectionEntry, modeId: string): string {
  const index = collection.modes.findIndex((mode) => mode.id === modeId);
  const mode = index >= 0 ? collection.modes[index] : undefined;
  if (mode && mode.name.trim().length > 0) return mode.name;
  return `Mode ${index + 1}`;
}

export function isActiveMode(collection: VariableCollectionEntry, modeId: string): boolean {
  return activeModeId(collection) === modeId;
}

/** The value used for a mode, falling back to the collection default. */
export function valueForMode(
  variable: VariableEntry,
  collection: VariableCollectionEntry,
  modeId?: string,
): VariableValueJson | null {
  const target = modeId ?? activeModeId(collection);
  const exact = variable.values[target];
  if (exact) return exact;
  const fallbackMode = collection.modes.find((mode) => variable.values[mode.id]);
  return fallbackMode ? variable.values[fallbackMode.id] ?? null : null;
}

export type VariableValueDisplay = {
  text: string;
  kind: "color" | "number" | "string" | "boolean" | "alias" | "empty";
  swatch: string | null;
  aliasVariableId: string | null;
};

export function describeVariableValue(
  value: VariableValueJson | null,
  variableName: (id: string) => string | null,
): VariableValueDisplay {
  if (!value) return { text: "—", kind: "empty", swatch: null, aliasVariableId: null };
  switch (value.type) {
    case "color": {
      const hex = colorToHex(value.color).toUpperCase();
      return { text: hex, kind: "color", swatch: hex, aliasVariableId: null };
    }
    case "float":
      return { text: String(value.value), kind: "number", swatch: null, aliasVariableId: null };
    case "string":
      return { text: value.value, kind: "string", swatch: null, aliasVariableId: null };
    case "boolean":
      return {
        text: value.value ? "true" : "false",
        kind: "boolean",
        swatch: null,
        aliasVariableId: null,
      };
    case "alias": {
      const name = variableName(value.variable_id);
      return {
        text: name ? `→ ${name}` : "→ Missing variable",
        kind: "alias",
        swatch: null,
        aliasVariableId: value.variable_id,
      };
    }
    default:
      return { text: "—", kind: "empty", swatch: null, aliasVariableId: null };
  }
}

/** Editable text for a variable value in the active mode (aliases stay read-only). */
export function valueToInput(value: VariableValueJson | null): string {
  if (!value) return "";
  switch (value.type) {
    case "color":
      return colorToHex(value.color);
    case "float":
      return String(value.value);
    case "string":
      return value.value;
    case "boolean":
      return value.value ? "true" : "false";
    case "alias":
      return "";
    default:
      return "";
  }
}

export function parseValueInput(type: VariableTypeName, text: string): VariableValueJson | null {
  const trimmed = text.trim();
  if (trimmed.length === 0) return null;
  switch (type) {
    case "color": {
      const color = hexToColor(trimmed);
      return color ? { type: "color", color } : null;
    }
    case "float": {
      const parsed = Number(trimmed);
      return Number.isFinite(parsed) ? { type: "float", value: parsed } : null;
    }
    case "string":
      return { type: "string", value: text };
    case "boolean": {
      const normalized = trimmed.toLowerCase();
      if (normalized === "true" || normalized === "1" || normalized === "yes") {
        return { type: "boolean", value: true };
      }
      if (normalized === "false" || normalized === "0" || normalized === "no") {
        return { type: "boolean", value: false };
      }
      return null;
    }
    default:
      return null;
  }
}

export function variableTypeLabel(type: VariableTypeName): string {
  switch (type) {
    case "color":
      return "Colour";
    case "float":
      return "Number";
    case "string":
      return "Text";
    case "boolean":
      return "Boolean";
    default:
      return type;
  }
}

/** Swatch colour for a variable across the active mode, if it resolves to one. */
export function variableSwatch(
  variable: VariableEntry,
  collection: VariableCollectionEntry,
  color: (value: VariableValueJson) => Color | null,
): string | null {
  const value = valueForMode(variable, collection);
  if (!value) return null;
  const resolved = color(value);
  return resolved ? colorToHex(resolved) : null;
}

export function findVariable(
  collections: readonly VariableCollectionEntry[],
  variableId: string,
): { collection: VariableCollectionEntry; variable: VariableEntry } | null {
  for (const collection of collections) {
    for (const variable of collection.variables ?? []) {
      if (variable.id === variableId) return { collection, variable };
    }
  }
  return null;
}

export function variableNameLookup(
  collections: readonly VariableCollectionEntry[],
): (id: string) => string | null {
  return (id) => findVariable(collections, id)?.variable.name ?? null;
}

export function collectionOptions(
  collections: readonly VariableCollectionEntry[],
): { id: string; name: string }[] {
  return collections.map((collection) => ({ id: collection.id, name: collection.name }));
}

export function defaultCollectionName(collections: readonly VariableCollectionEntry[]): string {
  return collections.length === 0 ? "Theme" : collections[0]?.name ?? "Theme";
}
