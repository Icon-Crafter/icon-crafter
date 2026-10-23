import type { Color } from "../engine/contract";

/** Clamp a number into 0..1. */
export function clamp01(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(1, Math.max(0, value));
}

export function normalizeColor(color: Color): Color {
  return {
    r: clamp01(color.r),
    g: clamp01(color.g),
    b: clamp01(color.b),
    a: clamp01(color.a),
  };
}

function channelTo255(value: number): number {
  return Math.round(clamp01(value) * 255);
}

/** CSS `rgba()` string for canvas/DOM use. */
export function colorToCss(color: Color): string {
  const { r, g, b, a } = normalizeColor(color);
  return `rgba(${channelTo255(r)}, ${channelTo255(g)}, ${channelTo255(b)}, ${Number(a.toFixed(4))})`;
}

/** CSS hex string (`#rrggbb`, plus alpha when `includeAlpha` and a < 1). */
export function colorToHex(color: Color, includeAlpha = false): string {
  const { r, g, b, a } = normalizeColor(color);
  const toHex = (value: number) => channelTo255(value).toString(16).padStart(2, "0");
  const base = `#${toHex(r)}${toHex(g)}${toHex(b)}`;
  if (!includeAlpha || a >= 1) return base;
  return `${base}${toHex(a)}`;
}

/** Parse `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa` (leading # optional). */
export function hexToColor(hex: string): Color | null {
  const raw = hex.trim().replace(/^#/, "");
  if (!/^[0-9a-fA-F]+$/.test(raw)) return null;
  if (raw.length === 3 || raw.length === 4) {
    const chars = raw.split("");
    const expanded = chars.map((ch) => parseInt(ch + ch, 16) / 255);
    const [r = 0, g = 0, b = 0, a = 1] = expanded;
    return { r, g, b, a };
  }
  if (raw.length === 6 || raw.length === 8) {
    const r = parseInt(raw.slice(0, 2), 16) / 255;
    const g = parseInt(raw.slice(2, 4), 16) / 255;
    const b = parseInt(raw.slice(4, 6), 16) / 255;
    const a = raw.length === 8 ? parseInt(raw.slice(6, 8), 16) / 255 : 1;
    return { r, g, b, a };
  }
  return null;
}

export interface HSV {
  /** Hue in degrees, 0..360. */
  h: number;
  /** Saturation 0..1. */
  s: number;
  /** Value 0..1. */
  v: number;
}

export function colorToHsv(color: Color): HSV {
  const { r, g, b } = normalizeColor(color);
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const delta = max - min;
  let h = 0;
  if (delta > 1e-6) {
    if (max === r) h = ((g - b) / delta) % 6;
    else if (max === g) h = (b - r) / delta + 2;
    else h = (r - g) / delta + 4;
    h *= 60;
    if (h < 0) h += 360;
  }
  return { h, s: max === 0 ? 0 : delta / max, v: max };
}

export function hsvToColor(hsv: HSV, alpha = 1): Color {
  const h = ((hsv.h % 360) + 360) % 360;
  const s = clamp01(hsv.s);
  const v = clamp01(hsv.v);
  const c = v * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = v - c;
  let r = 0;
  let g = 0;
  let b = 0;
  if (h < 60) [r, g, b] = [c, x, 0];
  else if (h < 120) [r, g, b] = [x, c, 0];
  else if (h < 180) [r, g, b] = [0, c, x];
  else if (h < 240) [r, g, b] = [0, x, c];
  else if (h < 300) [r, g, b] = [x, 0, c];
  else [r, g, b] = [c, 0, x];
  return { r: r + m, g: g + m, b: b + m, a: clamp01(alpha) };
}

/** Replace alpha, keeping RGB. */
export function withAlpha(color: Color, alpha: number): Color {
  return { ...color, a: clamp01(alpha) };
}

export function colorsEqual(a: Color, b: Color): boolean {
  return (
    Math.abs(a.r - b.r) < 1e-4 &&
    Math.abs(a.g - b.g) < 1e-4 &&
    Math.abs(a.b - b.b) < 1e-4 &&
    Math.abs(a.a - b.a) < 1e-4
  );
}

/** White or near-black, whichever stays legible on `background`. */
export function readableInk(background: Color): string {
  const { r, g, b } = normalizeColor(background);
  const luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  return luminance > 0.55 ? "#12151b" : "#ffffff";
}
