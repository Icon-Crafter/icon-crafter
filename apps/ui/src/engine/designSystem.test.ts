import { describe, expect, it } from "vitest";
import type {
  Effect,
  Fill,
  NodeDetail,
  Stroke,
  VariableCollectionEntry,
  VariableEntry,
  VariableValueJson,
} from "./contract";
import {
  activeModeId,
  collectionOptions,
  defaultCollectionName,
  describeVariableValue,
  detectStyleKind,
  findVariable,
  isActiveMode,
  modeName,
  parseValueInput,
  styleCounts,
  styleNameFor,
  valueForMode,
  valueToInput,
  variableNameLookup,
  variableSwatch,
  variableTypeLabel,
} from "./designSystem";

function fill(visible = true): Fill {
  return {
    visible,
    opacity: 1,
    blend: "normal",
    paint: { type: "solid", color: { r: 1, g: 0.42, b: 0.29, a: 1 } },
    style_id: null,
    variable_id: null,
  };
}

function stroke(): Stroke {
  return {
    visible: true,
    opacity: 1,
    blend: "normal",
    paint: { type: "solid", color: { r: 0, g: 0, b: 0, a: 1 } },
    style_id: null,
    variable_id: null,
    weight: 1,
    align: "inside",
    cap: "butt",
    join: "miter",
    miter_limit: 4,
    dash: null,
  };
}

function effect(): Effect {
  return {
    type: "drop_shadow",
    color: { r: 0, g: 0, b: 0, a: 0.25 },
    offset: { x: 0, y: 4 },
    blur: 12,
    spread: 0,
    blend: "normal",
  };
}

function node(partial: Partial<NodeDetail> = {}): NodeDetail {
  return {
    id: "node_1",
    name: "Swatch",
    kind: "rectangle",
    parent: null,
    transform: { x: 0, y: 0, w: 10, h: 10, rotation: 0, flip_x: false, flip_y: false },
    visible: true,
    locked: false,
    opacity: 1,
    blend: "normal",
    fills: [],
    strokes: [],
    effects: [],
    corner_radii: { tl: 0, tr: 0, br: 0, bl: 0 },
    fill_rule: "nonzero",
    text: null,
    layout: null,
    constraints: null,
    component: null,
    is_inside_instance: false,
    absolute_bounds: { x: 0, y: 0, w: 10, h: 10 },
    resolved: null,
    prototype: null,
    ...partial,
  };
}

const CORAL: VariableValueJson = { type: "color", color: { r: 1, g: 0.42, b: 0.29, a: 1 } };

function variable(partial: Partial<VariableEntry> & Pick<VariableEntry, "id" | "name" | "resolved_type">): VariableEntry {
  return {
    values: {},
    ...partial,
  };
}

function collection(partial: Partial<VariableCollectionEntry> = {}): VariableCollectionEntry {
  return {
    id: "coll_1",
    name: "Theme",
    modes: [
      { id: "mode_1", name: "Light" },
      { id: "mode_2", name: "" },
    ],
    active_mode_id: null,
    variables: [],
    ...partial,
  };
}

describe("detectStyleKind", () => {
  it("follows the engine order text, fill, effect, stroke", () => {
    expect(detectStyleKind(node({ text: {} as NodeDetail["text"] }))).toBe("text");
    expect(detectStyleKind(node({ fills: [fill()], effects: [effect()] }))).toBe("paint");
    expect(detectStyleKind(node({ effects: [effect()] }))).toBe("effect");
    expect(detectStyleKind(node({ strokes: [stroke()] }))).toBe("paint");
  });

  it("ignores invisible paints and returns null when nothing is styleable", () => {
    expect(detectStyleKind(node({ fills: [fill(false)] }))).toBeNull();
    expect(detectStyleKind(node())).toBeNull();
    expect(detectStyleKind(null)).toBeNull();
  });

  it("derives a readable style name", () => {
    expect(styleNameFor("paint", node({ name: "Swatch" }))).toBe("Swatch fill");
    expect(styleNameFor("text", node({ name: "Caption" }))).toBe("Caption text");
    expect(styleNameFor("effect", node({ name: "Card" }))).toBe("Card effect");
    expect(styleNameFor("paint", null)).toBe("Style fill");
  });

  it("counts styles defensively", () => {
    expect(styleCounts(null)).toEqual({ paints: 0, texts: 0, effects: 0, total: 0 });
    expect(
      styleCounts({
        paints: [{ id: "a", name: "A" }],
        texts: [{ id: "b", name: "B" }],
        effects: [{ id: "c", name: "C" }],
      }),
    ).toEqual({ paints: 1, texts: 1, effects: 1, total: 3 });
  });
});

describe("variable modes", () => {
  it("falls back to the first mode when active_mode_id is absent", () => {
    expect(activeModeId(collection())).toBe("mode_1");
    expect(activeModeId(collection({ active_mode_id: "mode_2" }))).toBe("mode_2");
    expect(isActiveMode(collection({ active_mode_id: "mode_2" }), "mode_2")).toBe(true);
    expect(isActiveMode(collection(), "mode_2")).toBe(false);
  });

  it("labels unnamed modes by position", () => {
    expect(modeName(collection(), "mode_1")).toBe("Light");
    expect(modeName(collection(), "mode_2")).toBe("Mode 2");
    expect(modeName(collection(), "missing")).toBe("Mode 0");
  });

  it("resolves values per mode with a documented fallback", () => {
    const variableValue = variable({
      id: "var_1",
      name: "Brand",
      resolved_type: "color",
      values: { mode_1: CORAL },
    });
    expect(valueForMode(variableValue, collection())).toEqual(CORAL);
    expect(valueForMode(variableValue, collection({ active_mode_id: "mode_2" }))).toEqual(CORAL);
    expect(valueForMode(variableValue, collection(), "mode_2")).toEqual(CORAL);
    const empty = variable({ id: "var_2", name: "Empty", resolved_type: "color" });
    expect(valueForMode(empty, collection())).toBeNull();
  });
});

describe("variable values", () => {
  it("formats each variable value kind, including aliases", () => {
    const lookup = (id: string) => (id === "var_brand" ? "Brand coral" : null);
    expect(describeVariableValue(CORAL, lookup)).toMatchObject({ text: "#FF6B4A", kind: "color", swatch: "#FF6B4A" });
    expect(describeVariableValue({ type: "float", value: 16 }, lookup)).toMatchObject({ text: "16", kind: "number" });
    expect(describeVariableValue({ type: "boolean", value: false }, lookup)).toMatchObject({ text: "false" });
    expect(describeVariableValue({ type: "alias", variable_id: "var_brand" }, lookup)).toMatchObject({
      text: "→ Brand coral",
      aliasVariableId: "var_brand",
    });
    expect(describeVariableValue({ type: "alias", variable_id: "gone" }, lookup).text).toBe("→ Missing variable");
    expect(describeVariableValue(null, lookup)).toMatchObject({ kind: "empty", text: "—" });
  });

  it("turns values into editable text", () => {
    expect(valueToInput(CORAL)).toBe("#ff6b4a");
    expect(valueToInput({ type: "float", value: 4 })).toBe("4");
    expect(valueToInput({ type: "alias", variable_id: "var_brand" })).toBe("");
    expect(valueToInput(null)).toBe("");
  });

  it("parses user input into contract values", () => {
    expect(parseValueInput("color", " #4fd1c5 ")).toMatchObject({ type: "color" });
    expect(parseValueInput("color", "nope")).toBeNull();
    expect(parseValueInput("float", "12.5")).toEqual({ type: "float", value: 12.5 });
    expect(parseValueInput("float", "abc")).toBeNull();
    expect(parseValueInput("boolean", "yes")).toEqual({ type: "boolean", value: true });
    expect(parseValueInput("boolean", "0")).toEqual({ type: "boolean", value: false });
    expect(parseValueInput("boolean", "maybe")).toBeNull();
    expect(parseValueInput("string", " brand ")).toEqual({ type: "string", value: " brand " });
    expect(parseValueInput("string", "   ")).toBeNull();
  });

  it("labels types and resolves swatches through a caller-supplied resolver", () => {
    expect(variableTypeLabel("color")).toBe("Colour");
    expect(variableTypeLabel("float")).toBe("Number");
    const variableValue = variable({
      id: "var_1",
      name: "Brand",
      resolved_type: "color",
      values: { mode_1: CORAL },
    });
    expect(variableSwatch(variableValue, collection(), () => ({ r: 1, g: 0.42, b: 0.29, a: 1 }))).toBe("#ff6b4a");
    expect(variableSwatch(variableValue, collection(), () => null)).toBeNull();
  });

  it("looks up variables and collections", () => {
    const brand = variable({ id: "var_1", name: "Brand", resolved_type: "color" });
    const collections = [collection({ variables: [brand] })];
    expect(findVariable(collections, "var_1")?.variable.name).toBe("Brand");
    expect(findVariable(collections, "missing")).toBeNull();
    const lookup = variableNameLookup(collections);
    expect(lookup("var_1")).toBe("Brand");
    expect(lookup("missing")).toBeNull();
    expect(collectionOptions(collections)).toEqual([{ id: "coll_1", name: "Theme" }]);
    expect(defaultCollectionName(collections)).toBe("Theme");
    expect(defaultCollectionName([])).toBe("Theme");
  });
});
