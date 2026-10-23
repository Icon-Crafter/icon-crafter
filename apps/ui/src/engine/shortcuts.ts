import { en } from "../i18n/en";
import type { ToolId } from "./contract";

/**
 * Keyboard shortcut system.
 *
 * Bindings are persisted as portable strings ("Mod+Shift+Z") where `Mod` is
 * Cmd on macOS and Ctrl everywhere else. The resolver is a pure function of
 * (event, bindings, platform), which keeps it unit-testable without a DOM.
 */

export type ShortcutGroup = "tools" | "edit" | "view" | "file" | "app";

interface ShortcutActionDefinition {
  readonly id: string;
  readonly group: ShortcutGroup;
  /** Portable binding, or null when the action is not bound by default. */
  readonly defaultBinding: string | null;
}

export const SHORTCUT_ACTIONS = [
  // Tools
  { id: "tool.move", group: "tools", defaultBinding: "V" },
  { id: "tool.hand", group: "tools", defaultBinding: "H" },
  { id: "tool.frame", group: "tools", defaultBinding: "F" },
  { id: "tool.slice", group: "tools", defaultBinding: null },
  { id: "tool.rectangle", group: "tools", defaultBinding: "R" },
  { id: "tool.ellipse", group: "tools", defaultBinding: "O" },
  { id: "tool.line", group: "tools", defaultBinding: "L" },
  { id: "tool.polygon", group: "tools", defaultBinding: null },
  { id: "tool.star", group: "tools", defaultBinding: null },
  { id: "tool.pen", group: "tools", defaultBinding: "P" },
  { id: "tool.node", group: "tools", defaultBinding: null },
  { id: "tool.text", group: "tools", defaultBinding: "T" },
  { id: "tool.image", group: "tools", defaultBinding: null },
  { id: "tool.comment", group: "tools", defaultBinding: null },
  { id: "tool.connector", group: "tools", defaultBinding: null },
  // Edit
  { id: "edit.undo", group: "edit", defaultBinding: "Mod+Z" },
  { id: "edit.redo", group: "edit", defaultBinding: "Mod+Shift+Z" },
  { id: "edit.cut", group: "edit", defaultBinding: "Mod+X" },
  { id: "edit.copy", group: "edit", defaultBinding: "Mod+C" },
  { id: "edit.paste", group: "edit", defaultBinding: "Mod+V" },
  { id: "edit.duplicate", group: "edit", defaultBinding: "Mod+D" },
  { id: "edit.delete", group: "edit", defaultBinding: "Delete" },
  { id: "edit.selectAll", group: "edit", defaultBinding: "Mod+A" },
  { id: "edit.deselect", group: "edit", defaultBinding: "Escape" },
  { id: "edit.group", group: "edit", defaultBinding: "Mod+G" },
  { id: "edit.ungroup", group: "edit", defaultBinding: "Mod+Shift+G" },
  { id: "edit.createComponent", group: "edit", defaultBinding: "Mod+Alt+K" },
  { id: "edit.bringToFront", group: "edit", defaultBinding: "Mod+Shift+]" },
  { id: "edit.bringForward", group: "edit", defaultBinding: "Mod+]" },
  { id: "edit.sendBackward", group: "edit", defaultBinding: "Mod+[" },
  { id: "edit.sendToBack", group: "edit", defaultBinding: "Mod+Shift+[" },
  // View
  { id: "view.zoomIn", group: "view", defaultBinding: "Mod+=" },
  { id: "view.zoomOut", group: "view", defaultBinding: "Mod+-" },
  { id: "view.zoom100", group: "view", defaultBinding: "Shift+0" },
  { id: "view.zoomFit", group: "view", defaultBinding: "Shift+1" },
  { id: "view.zoomSelection", group: "view", defaultBinding: "Shift+2" },
  // File
  { id: "file.new", group: "file", defaultBinding: "Mod+N" },
  { id: "file.open", group: "file", defaultBinding: "Mod+O" },
  { id: "file.save", group: "file", defaultBinding: "Mod+S" },
  { id: "file.saveAs", group: "file", defaultBinding: "Mod+Shift+S" },
  { id: "file.export", group: "file", defaultBinding: "Mod+Shift+E" },
  { id: "file.close", group: "file", defaultBinding: "Mod+W" },
  // App
  { id: "app.commandPalette", group: "app", defaultBinding: "Mod+K" },
  { id: "app.devMode", group: "app", defaultBinding: "Shift+D" },
  { id: "app.present", group: "app", defaultBinding: "Mod+Enter" },
] as const satisfies readonly ShortcutActionDefinition[];

export type ShortcutActionId = (typeof SHORTCUT_ACTIONS)[number]["id"];

/** Every action id with its current portable binding, or null when unbound. */
export type BindingMap = Record<ShortcutActionId, string | null>;

export const SHORTCUT_GROUP_ORDER: readonly ShortcutGroup[] = ["tools", "edit", "view", "file", "app"];

export function actionLabel(id: ShortcutActionId): string {
  return en.shortcuts.actions[id] ?? id;
}

export function groupLabel(group: ShortcutGroup): string {
  return en.shortcuts.groups[group];
}

export function actionsInGroup(group: ShortcutGroup): readonly ShortcutActionId[] {
  return SHORTCUT_ACTIONS.filter((action) => action.group === group).map((action) => action.id);
}

// ---------------------------------------------------------------------------
// Platform detection and binding parsing
// ---------------------------------------------------------------------------

export function isMacPlatform(): boolean {
  if (typeof navigator === "undefined") return false;
  const nav = navigator as Navigator & { userAgentData?: { platform?: string } };
  const platform = nav.userAgentData?.platform ?? nav.platform ?? "";
  return /mac|iphone|ipad|ipod/i.test(platform);
}

export interface KeyCombo {
  /** Canonical key name for non-code keys; "" when matched by code. */
  key: string;
  /** event.code value, e.g. "Digit1"; "" when matched by key. */
  code: string;
  ctrl: boolean;
  meta: boolean;
  alt: boolean;
  shift: boolean;
}

export interface KeyboardEventLike {
  key: string;
  code: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

const NAMED_KEYS: Record<string, string> = {
  esc: "Escape",
  escape: "Escape",
  del: "Delete",
  delete: "Delete",
  backspace: "Backspace",
  enter: "Enter",
  return: "Enter",
  tab: "Tab",
  space: "Space",
  spacebar: "Space",
  arrowup: "ArrowUp",
  up: "ArrowUp",
  arrowdown: "ArrowDown",
  down: "ArrowDown",
  arrowleft: "ArrowLeft",
  left: "ArrowLeft",
  arrowright: "ArrowRight",
  right: "ArrowRight",
  home: "Home",
  end: "End",
  pageup: "PageUp",
  pagedown: "PageDown",
  insert: "Insert",
  plus: "Equal",
};

const PUNCTUATION_CODES: Record<string, string> = {
  "-": "Minus",
  "=": "Equal",
  "/": "Slash",
  "\\": "Backslash",
  ",": "Comma",
  ".": "Period",
  ";": "Semicolon",
  "'": "Quote",
  "`": "Backquote",
  "[": "BracketLeft",
  "]": "BracketRight",
};

const FUNCTION_KEY = /^f([1-9]|1[0-2])$/i;

export function parseBinding(binding: string, isMac = isMacPlatform()): KeyCombo | null {
  const tokens = binding
    .split("+")
    .map((token) => token.trim())
    .filter(Boolean);
  if (tokens.length === 0) return null;
  const keyToken = tokens[tokens.length - 1];
  if (!keyToken) return null;

  const combo: KeyCombo = {
    key: "",
    code: "",
    ctrl: false,
    meta: false,
    alt: false,
    shift: false,
  };

  for (const token of tokens.slice(0, -1)) {
    switch (token.toLowerCase()) {
      case "mod":
        if (isMac) combo.meta = true;
        else combo.ctrl = true;
        break;
      case "ctrl":
      case "control":
        combo.ctrl = true;
        break;
      case "cmd":
      case "command":
      case "meta":
        combo.meta = true;
        break;
      case "alt":
      case "option":
        combo.alt = true;
        break;
      case "shift":
        combo.shift = true;
        break;
      default:
        // Unknown modifier token: the binding is invalid rather than silently broad.
        return null;
    }
  }

  const lower = keyToken.toLowerCase();
  if (/^[a-z]$/.test(lower)) {
    combo.key = lower.toUpperCase();
  } else if (/^\d$/.test(lower)) {
    combo.code = `Digit${lower}`;
  } else if (FUNCTION_KEY.test(lower)) {
    combo.key = lower.toUpperCase();
  } else if (NAMED_KEYS[lower]) {
    combo.key = NAMED_KEYS[lower] ?? "";
  } else if (PUNCTUATION_CODES[keyToken]) {
    combo.code = PUNCTUATION_CODES[keyToken] ?? "";
  } else if (keyToken.length === 1) {
    combo.key = keyToken;
  } else {
    combo.key = keyToken;
  }
  if (!combo.key && !combo.code) return null;
  return combo;
}

export function normalizeEventKey(key: string): string {
  if (key === " ") return "Space";
  if (key.length === 1) return key.toUpperCase();
  return key;
}

export function comboMatchesEvent(combo: KeyCombo, event: KeyboardEventLike): boolean {
  if (
    event.ctrlKey !== combo.ctrl ||
    event.metaKey !== combo.meta ||
    event.altKey !== combo.alt ||
    event.shiftKey !== combo.shift
  ) {
    return false;
  }
  if (combo.code) return event.code === combo.code;
  return normalizeEventKey(event.key) === combo.key;
}

const parsedCache = new Map<string, KeyCombo | null>();

function parseCached(binding: string, isMac: boolean): KeyCombo | null {
  const cacheKey = `${isMac ? "mac" : "pc"}:${binding}`;
  if (!parsedCache.has(cacheKey)) parsedCache.set(cacheKey, parseBinding(binding, isMac));
  return parsedCache.get(cacheKey) ?? null;
}

/** True when the event target accepts text (so shortcuts must not fire). */
export function isEditableTarget(target: EventTarget | null): boolean {
  if (!target || !(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

/** Resolve a keyboard event to an action id using the current binding map. */
export function resolveShortcut(
  event: KeyboardEventLike,
  bindings: BindingMap,
  isMac = isMacPlatform(),
): ShortcutActionId | null {
  for (const action of SHORTCUT_ACTIONS) {
    const binding = bindings[action.id];
    if (!binding) continue;
    const combo = parseCached(binding, isMac);
    if (combo && comboMatchesEvent(combo, event)) return action.id;
  }
  return null;
}

/** "Mod+Shift+Z" → "Ctrl+Shift+Z" (Windows/Linux) or "⇧⌘Z" (macOS). */
export function formatBindingForDisplay(binding: string | null, isMac = isMacPlatform()): string {
  if (!binding) return "";
  const parts = binding.split("+").map((token) => token.trim()).filter(Boolean);
  const rendered: string[] = [];
  for (const token of parts.slice(0, -1)) {
    const lower = token.toLowerCase();
    if (lower === "mod") rendered.push(isMac ? "⌘" : "Ctrl");
    else if (lower === "ctrl" || lower === "control") rendered.push(isMac ? "⌃" : "Ctrl");
    else if (lower === "shift") rendered.push(isMac ? "⇧" : "Shift");
    else if (lower === "alt" || lower === "option") rendered.push(isMac ? "⌥" : "Alt");
    else if (lower === "cmd" || lower === "command" || lower === "meta") rendered.push("⌘");
    else rendered.push(token);
  }
  rendered.push(parts[parts.length - 1] ?? "");
  return rendered.join(isMac ? "" : "+");
}

/** Canonical signature for conflict detection, platform-independent. */
export function bindingSignature(binding: string): string {
  // Parse as Windows so `Mod` becomes Ctrl; the shape is platform-independent.
  const combo = parseBinding(binding, false);
  if (!combo) return binding;
  return `${combo.ctrl ? "C" : ""}${combo.meta ? "M" : ""}${combo.alt ? "A" : ""}${combo.shift ? "S" : ""}:${
    combo.code || combo.key
  }`;
}

/** Returns signatures assigned to more than one action. */
export function bindingConflicts(bindings: BindingMap): Map<string, ShortcutActionId[]> {
  const bySignature = new Map<string, ShortcutActionId[]>();
  for (const action of SHORTCUT_ACTIONS) {
    const binding = bindings[action.id];
    if (!binding) continue;
    const signature = bindingSignature(binding);
    const list = bySignature.get(signature) ?? [];
    list.push(action.id);
    bySignature.set(signature, list);
  }
  for (const [signature, ids] of bySignature) {
    if (ids.length <= 1) bySignature.delete(signature);
  }
  return bySignature;
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

export const SHORTCUT_STORAGE_KEY = "iconcrafter.shortcuts.v1";

export function defaultBindings(): BindingMap {
  const result = {} as BindingMap;
  for (const action of SHORTCUT_ACTIONS) {
    result[action.id] = action.defaultBinding;
  }
  return result;
}

const ACTION_IDS = new Set<string>(SHORTCUT_ACTIONS.map((action) => action.id));

function readStorage(storage?: Storage): Storage | null {
  if (storage) return storage;
  if (typeof window === "undefined") return null;
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

/** Load persisted bindings, falling back to defaults for missing/invalid values. */
export function loadBindings(storage?: Storage): BindingMap {
  const result = defaultBindings();
  const store = readStorage(storage);
  if (!store) return result;
  try {
    const raw = store.getItem(SHORTCUT_STORAGE_KEY);
    if (!raw) return result;
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") return result;
    for (const [id, value] of Object.entries(parsed as Record<string, unknown>)) {
      if (!ACTION_IDS.has(id)) continue;
      const key = id as ShortcutActionId;
      if (value === null) {
        result[key] = null;
      } else if (typeof value === "string" && value.trim() !== "" && parseBinding(value)) {
        result[key] = value;
      }
    }
    return result;
  } catch {
    return result;
  }
}

export function saveBindings(bindings: BindingMap, storage?: Storage): void {
  const store = readStorage(storage);
  if (!store) return;
  try {
    store.setItem(SHORTCUT_STORAGE_KEY, JSON.stringify(bindings));
  } catch {
    // Storage can be unavailable (private mode/quota); shortcuts still work for the session.
  }
}

export function resetBindings(storage?: Storage): BindingMap {
  const defaults = defaultBindings();
  const store = readStorage(storage);
  try {
    store?.removeItem(SHORTCUT_STORAGE_KEY);
  } catch {
    // Ignore unavailable storage.
  }
  return defaults;
}

/** Map a tool action id back to the engine ToolId, or null. */
export function toolForShortcut(id: ShortcutActionId): ToolId | null {
  if (!id.startsWith("tool.")) return null;
  const tool = id.slice("tool.".length) as ToolId;
  const known: readonly ToolId[] = [
    "move",
    "hand",
    "frame",
    "slice",
    "rectangle",
    "ellipse",
    "line",
    "polygon",
    "star",
    "pen",
    "node",
    "text",
    "image",
    "comment",
    "connector",
  ];
  return known.includes(tool) ? tool : null;
}
