/**
 * Application store — the single source of truth the React tree renders from.
 *
 * Invariants:
 *  - `engine` is replaced wholesale by every engine snapshot; nothing in the UI
 *    mutates document geometry locally.
 *  - The transport is created once (`connect`) and reused; the pump serialises
 *    input so snapshots arrive in order.
 *  - UI-only state (panels, dialogs, toasts, shortcuts) lives next to the engine
 *    snapshot but is never sent to the engine.
 */
import { create } from "zustand";
import type { Bootstrap, Capabilities, EngineError, EngineState } from "./contract";
import { DEFAULT_CAPABILITIES, initialEngineState } from "./contract";
import { EnginePump } from "./input";
import {
  createTransport,
  isTauriRuntime,
  normalizeEngineError,
  type EngineTransport,
} from "./transport";
import { loadBindings, saveBindings, type BindingMap } from "./shortcuts";

export type ConnectionStatus = "connecting" | "ready" | "unavailable";
export type LeftTab = "layers" | "assets" | "components" | "pages";
export type DialogKind = "export" | "saveAs" | "unsaved";
export type UnsavedIntent = "new" | "open" | "close";
export type ToastKind = "info" | "success" | "warning" | "error";

export interface Toast {
  id: number;
  kind: ToastKind;
  title: string;
  message?: string;
}

export interface FrameStats {
  fps: number;
  paintMs: number;
}

/**
 * Honest engine capability flags reported at bootstrap. Controls use these to
 * disable actions the running engine cannot perform instead of failing later.
 */
export function useCapabilities(): Capabilities {
  return useAppStore((state) => state.bootstrap?.capabilities ?? DEFAULT_CAPABILITIES);
}

export interface AppStore {
  status: ConnectionStatus;
  connectionError: string | null;
  transportKind: "tauri" | "dev" | null;
  pump: EnginePump | null;
  bootstrap: Bootstrap | null;
  engine: EngineState;
  toasts: Toast[];
  leftTab: LeftTab;
  leftCollapsed: boolean;
  rightCollapsed: boolean;
  paletteOpen: boolean;
  dialog: DialogKind | null;
  unsavedIntent: UnsavedIntent | null;
  busy: boolean;
  shortcuts: BindingMap;
  recentCommands: string[];
  frameStats: FrameStats;
  hoveredNodeId: string | null;

  connect: () => Promise<void>;
  ingest: (state: EngineState) => void;
  refresh: () => Promise<void>;
  setLeftTab: (tab: LeftTab) => void;
  toggleLeftCollapsed: () => void;
  toggleRightCollapsed: () => void;
  setPaletteOpen: (open: boolean) => void;
  openDialog: (dialog: DialogKind, intent?: UnsavedIntent) => void;
  closeDialog: () => void;
  setBusy: (busy: boolean) => void;
  setShortcuts: (bindings: BindingMap) => void;
  resetShortcuts: () => BindingMap;
  recordCommand: (id: string) => void;
  pushToast: (toast: Omit<Toast, "id">) => void;
  dismissToast: (id: number) => void;
  reportEngineError: (error: EngineError | unknown) => void;
  setFrameStats: (stats: FrameStats) => void;
  setHoveredNodeId: (id: string | null) => void;
}

const TOAST_TIMEOUT_MS = 5600;
const HARNESS_STORAGE_KEY = "iconcrafter.recentCommands.v1";

let toastCounter = 0;
let lastErrorToast: { signature: string; at: number } | null = null;
let connecting: Promise<void> | null = null;

function readRecentCommands(): string[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(HARNESS_STORAGE_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter((v): v is string => typeof v === "string").slice(0, 8) : [];
  } catch {
    return [];
  }
}

function persistRecentCommands(ids: string[]): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(HARNESS_STORAGE_KEY, JSON.stringify(ids));
  } catch {
    // Recent commands are a nicety; storage failures are not user-facing.
  }
}

function freshEngine(): EngineState {
  return initialEngineState();
}

export const useAppStore = create<AppStore>()((set, get) => ({
  status: "connecting",
  connectionError: null,
  transportKind: null,
  pump: null,
  bootstrap: null,
  engine: freshEngine(),
  toasts: [],
  leftTab: "layers",
  leftCollapsed: false,
  rightCollapsed: false,
  paletteOpen: false,
  dialog: null,
  unsavedIntent: null,
  busy: false,
  shortcuts: loadBindings(),
  recentCommands: readRecentCommands(),
  frameStats: { fps: 0, paintMs: 0 },
  hoveredNodeId: null,

  async connect() {
    if (connecting) return connecting;
    set({ status: "connecting", connectionError: null });
    connecting = (async () => {
      try {
        const transport: EngineTransport = await createTransport();
        const pump = new EnginePump(transport, (state) => get().ingest(state), (error) =>
          get().reportEngineError(error),
        );
        set({ pump, transportKind: transport.kind });
        const bootstrap = await transport.bootstrap();
        let engine = await transport.engineState();
        if (!engine.doc) {
          // First launch inside Tauri: open a blank design so the canvas is live.
          engine = await pump.newDocument("design");
        }
        set({
          bootstrap,
          engine,
          status: "ready",
          connectionError: null,
        });
      } catch (error) {
        const normalized = normalizeEngineError(error);
        set({
          status: "unavailable",
          connectionError: normalized.message,
          transportKind: isTauriRuntime() ? "tauri" : null,
        });
        get().pushToast({ kind: "error", title: normalized.message });
      } finally {
        connecting = null;
      }
    })();
    return connecting;
  },

  ingest(state) {
    set({ engine: state, status: "ready" });
    for (const error of state.errors ?? []) {
      const signature = `${error.code}:${error.message}`;
      const now = Date.now();
      if (lastErrorToast && lastErrorToast.signature === signature && now - lastErrorToast.at < 1500) {
        continue;
      }
      lastErrorToast = { signature, at: now };
      if (error.code === "unsupported") {
        get().pushToast({ kind: "warning", title: "Not available yet", message: error.message });
      } else if (error.severity === "warning") {
        get().pushToast({ kind: "warning", title: error.message });
      } else {
        get().pushToast({ kind: "error", title: error.message });
      }
    }
  },

  async refresh() {
    const pump = get().pump;
    if (!pump) return;
    const state = await pump.refresh();
    if (state) get().ingest(state);
  },

  setLeftTab(tab) {
    set({ leftTab: tab, leftCollapsed: false });
  },
  toggleLeftCollapsed() {
    set((s) => ({ leftCollapsed: !s.leftCollapsed }));
  },
  toggleRightCollapsed() {
    set((s) => ({ rightCollapsed: !s.rightCollapsed }));
  },
  setPaletteOpen(open) {
    set({ paletteOpen: open });
  },
  openDialog(dialog, intent) {
    set({ dialog, unsavedIntent: intent ?? null });
  },
  closeDialog() {
    set({ dialog: null, unsavedIntent: null, busy: false });
  },
  setBusy(busy) {
    set({ busy });
  },
  setShortcuts(bindings) {
    saveBindings(bindings);
    set({ shortcuts: bindings });
  },
  resetShortcuts() {
    const bindings = { ...get().shortcuts };
    for (const key of Object.keys(bindings)) delete bindings[key as keyof BindingMap];
    const defaults = loadBindings();
    saveBindings(defaults);
    set({ shortcuts: defaults });
    return defaults;
  },
  recordCommand(id) {
    const next = [id, ...get().recentCommands.filter((existing) => existing !== id)].slice(0, 8);
    persistRecentCommands(next);
    set({ recentCommands: next });
  },
  pushToast(toast) {
    const id = ++toastCounter;
    set((s) => ({ toasts: [...s.toasts.slice(-4), { ...toast, id }] }));
    if (typeof window !== "undefined") {
      window.setTimeout(() => get().dismissToast(id), TOAST_TIMEOUT_MS);
    }
  },
  dismissToast(id) {
    set((s) => ({ toasts: s.toasts.filter((toast) => toast.id !== id) }));
  },
  reportEngineError(error) {
    const normalized = normalizeEngineError(error);
    if (normalized.code === "unsupported") {
      get().pushToast({ kind: "warning", title: "Not available yet", message: normalized.message });
      return;
    }
    get().pushToast({
      kind: normalized.severity === "warning" ? "warning" : "error",
      title: normalized.message,
    });
  },
  setFrameStats(stats) {
    set({ frameStats: stats });
  },
  setHoveredNodeId(id) {
    if (get().hoveredNodeId !== id) set({ hoveredNodeId: id });
  },
}));

/** Convenience accessor for non-React modules (commands, browser listeners). */
export const appStore = {
  getState: () => useAppStore.getState(),
  subscribe: useAppStore.subscribe,
  setState: useAppStore.setState,
};

export function getActivePump(): EnginePump | null {
  return useAppStore.getState().pump;
}
