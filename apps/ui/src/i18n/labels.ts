/** Display labels for engine enums, centralized with the rest of the UI copy. */
import type { BlendMode, EffectType, NodeKind, ToolId } from "../engine/contract";
import { en } from "./en";

export function nodeKindLabel(kind: NodeKind): string {
  switch (kind) {
    case "frame":
      return en.layers.typeFrame;
    case "group":
      return en.layers.typeGroup;
    case "component":
    case "component_set":
      return en.layers.typeComponent;
    case "instance":
      return en.layers.typeInstance;
    case "text":
      return en.layers.typeText;
    case "rectangle":
      return en.layers.typeRectangle;
    case "ellipse":
      return en.layers.typeEllipse;
    case "vector":
    case "line":
    case "polygon":
    case "star":
    case "boolean":
      return en.layers.typeVector;
    case "image":
      return en.layers.typeImage;
    default:
      return en.layers.typeUnknown;
  }
}

export function toolLabel(tool: ToolId): string {
  return en.tools[tool];
}

export function effectTypeLabel(type: EffectType): string {
  switch (type) {
    case "drop_shadow":
      return en.inspector.effectDropShadow;
    case "inner_shadow":
      return en.inspector.effectInnerShadow;
    case "layer_blur":
      return en.inspector.effectLayerBlur;
    case "background_blur":
      return en.inspector.effectBackgroundBlur;
  }
}

const BLEND_LABELS: Record<BlendMode, string> = {
  normal: "Normal",
  multiply: "Multiply",
  screen: "Screen",
  overlay: "Overlay",
  darken: "Darken",
  lighten: "Lighten",
  "color-dodge": "Color dodge",
  "color-burn": "Color burn",
  "hard-light": "Hard light",
  "soft-light": "Soft light",
  difference: "Difference",
  exclusion: "Exclusion",
  hue: "Hue",
  saturation: "Saturation",
  color: "Color",
  luminosity: "Luminosity",
  "pass-through": "Pass through",
};

export function blendModeLabel(mode: string): string {
  return BLEND_LABELS[mode as BlendMode] ?? mode;
}

export const BLEND_MODES = Object.keys(BLEND_LABELS) as BlendMode[];
