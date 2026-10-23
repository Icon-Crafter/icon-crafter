/** Numeric helpers shared by inputs, the inspector and the status bar. */

export function clamp(value: number, min: number, max: number): number {
  if (Number.isNaN(value)) return min;
  return Math.min(max, Math.max(min, value));
}

/**
 * Round away binary floating-point noise (0.1 + 0.2 → 0.3) without changing
 * the design precision users can type (default 3 decimals).
 */
export function roundTo(value: number, precision = 3): number {
  if (!Number.isFinite(value)) return 0;
  const factor = 10 ** precision;
  return Math.round(value * factor) / factor;
}

/** Human, compact number: `12`, `12.5`, `-0`, `1000`. */
export function prettyNumber(value: number, maxDecimals = 2): string {
  if (!Number.isFinite(value)) return "0";
  const rounded = roundTo(value, maxDecimals);
  if (Object.is(rounded, -0)) return "0";
  return String(rounded);
}

/**
 * Parse a numeric field. Accepts a leading sign, decimals and surrounding
 * whitespace. Returns null for anything else (the field keeps its old value).
 */
export function parseNumber(text: string): number | null {
  const trimmed = text.trim();
  if (trimmed === "" || trimmed === "-" || trimmed === "+" || trimmed === ".") return null;
  if (!/^[+-]?(\d+\.?\d*|\.\d+)(e[+-]?\d+)?$/i.test(trimmed)) return null;
  const value = Number(trimmed);
  return Number.isFinite(value) ? value : null;
}

/** Zoom as a percentage label: 1 → "100%", 0.375 → "38%". */
export function formatZoom(zoom: number): string {
  const pct = zoom * 100;
  const rounded = pct >= 100 ? Math.round(pct) : Math.round(pct * 10) / 10;
  return `${prettyNumber(rounded, 1)}%`;
}

/** Design-length label: 24 → "24", 24.5 → "24.5". */
export function formatLength(value: number): string {
  return prettyNumber(value, 2);
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 10 ? Math.round(value) : value.toFixed(1)} ${units[unit]}`;
}

export function formatCount(value: number): string {
  return new Intl.NumberFormat("en-US").format(value);
}

/**
 * Arrow-key stepping for number fields:
 *   step                      → ±1 × step
 *   Shift                     → ±10 × step
 *   Alt                       → ±0.1 × step
 */
export function stepValue(current: number, direction: 1 | -1, step = 1, shift = false, alt = false): number {
  const factor = shift ? 10 : alt ? 0.1 : 1;
  return roundTo(current + direction * step * factor, 3);
}

/** Clamp a rectangle to non-negative width/height. */
export function normalizeRectValues(x: number, y: number, w: number, h: number) {
  return {
    x: w < 0 ? x + w : x,
    y: h < 0 ? y + h : y,
    w: Math.abs(w),
    h: Math.abs(h),
  };
}
