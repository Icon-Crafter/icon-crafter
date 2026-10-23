/** Tiny DOM utilities shared by components. */

/** Join truthy class names. */
export function cx(...parts: (string | false | null | undefined)[]): string {
  return parts.filter((part): part is string => Boolean(part)).join(" ");
}

/** CSS px → design px, for readouts. */
export function worldLength(cssLength: number, zoom: number): number {
  return cssLength / Math.max(zoom, 0.0001);
}

/** True when a keyboard event originated in a text-entry surface. */
export function isTextEntryTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
  return target.isContentEditable;
}

/** Clamp a number to a closed range. */
export function clampNumber(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/** Format a numeric readout without trailing noise. */
export function roundForInput(value: number, decimals = 2): number {
  const factor = 10 ** decimals;
  return Math.round(value * factor) / factor;
}
