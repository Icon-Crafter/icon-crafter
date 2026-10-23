/**
 * Dev Mode panel state (right sidebar) backed by the engine dev_* commands.
 *
 * The panel is intentionally a separate store from the editor store: it holds
 * inspect snapshots, generated code, handoff annotations and revision captures
 * so design-mode state and selections stay untouched when Dev Mode is off.
 */

import { create } from "zustand";

import type {
  AnnotationKind,
  CodegenTarget,
  GeneratedCode,
  Handoff,
  HandoffStatus,
  InspectSnapshot,
  RevisionDiff,
} from "./contract";
import {
  addCapture,
  CAPTURE_STORAGE_KEY,
  loadCaptures,
  nextCaptureLabel,
  removeCapture,
  saveCaptures,
  summarizeDiffForUi,
  type DiffSummary,
  type RevisionCapture,
} from "./devMode";
import { getActivePump, useAppStore } from "./store";
import { normalizeEngineError } from "./transport";

export type DevTab = "inspect" | "code" | "annotate" | "compare";

export interface DevModeStore {
  active: boolean;
  tab: DevTab;
  inspect: InspectSnapshot | null;
  inspectLoading: boolean;
  inspectError: string | null;
  code: GeneratedCode | null;
  codeTarget: CodegenTarget;
  codeLoading: boolean;
  codeError: string | null;
  handoff: Handoff | null;
  handoffLoading: boolean;
  handoffError: string | null;
  captures: RevisionCapture[];
  compareResult: RevisionDiff | null;
  compareResultCapture: RevisionCapture | null;
  compareLoading: boolean;
  compareError: string | null;
  setActive: (enabled: boolean) => Promise<void>;
  setTab: (tab: DevTab) => void;
  setInspectState: (patch: Partial<Pick<DevModeStore, "inspect" | "inspectLoading" | "inspectError">>) => void;
  refreshInspect: () => Promise<void>;
  setCodeTarget: (target: CodegenTarget) => void;
  setCodeState: (patch: Partial<Pick<DevModeStore, "code" | "codeLoading" | "codeError">>) => void;
  generateCode: (target?: CodegenTarget) => Promise<void>;
  setHandoffState: (patch: Partial<Pick<DevModeStore, "handoff" | "handoffLoading" | "handoffError">>) => void;
  reloadHandoff: () => Promise<void>;
  addAnnotation: (input: {
    nodeId: string;
    body: string;
    author: string;
    kind: AnnotationKind;
    link: string;
  }) => Promise<boolean>;
  resolveAnnotation: (annotationId: string) => Promise<boolean>;
  markReady: (status: HandoffStatus, by: string, at: string) => Promise<boolean>;
  setCaptures: (captures: RevisionCapture[]) => void;
  captureRevision: (label?: string) => Promise<RevisionCapture | null>;
  removeCapture: (id: string) => void;
  clearCaptures: () => void;
  setCompareState: (
    patch: Partial<Pick<DevModeStore, "compareResult" | "compareResultCapture" | "compareLoading" | "compareError">>,
  ) => void;
  compareWith: (capture: RevisionCapture) => Promise<void>;
  diffSummary: () => DiffSummary | null;
}

function storage(): Storage | undefined {
  if (typeof window === "undefined") return undefined;
  try {
    return window.localStorage;
  } catch {
    return undefined;
  }
}

function activePumpOrNull() {
  return getActivePump();
}

function reportFailure(context: string, error: unknown): string {
  const normalized = normalizeEngineError(error);
  const message = context ? `${context}: ${normalized.message}` : normalized.message;
  useAppStore.getState().reportEngineError({ ...normalized, message });
  return normalized.message;
}

export const useDevModeStore = create<DevModeStore>()((set, get) => ({
  active: false,
  tab: "inspect",
  inspect: null,
  inspectLoading: false,
  inspectError: null,
  code: null,
  codeTarget: "css",
  codeLoading: false,
  codeError: null,
  handoff: null,
  handoffLoading: false,
  handoffError: null,
  captures: loadCaptures(storage()),
  compareResult: null,
  compareResultCapture: null,
  compareLoading: false,
  compareError: null,

  setActive: async (enabled) => {
    set({ active: enabled, tab: enabled ? get().tab : "inspect" });
    const pump = activePumpOrNull();
    if (!pump) {
      set({ inspectError: "The engine is not connected yet." });
      return;
    }
    try {
      await pump.setDevMode(enabled);
    } catch (error) {
      reportFailure("dev mode", error);
    }
  },

  setTab: (tab) => set({ tab }),

  setInspectState: (patch) => set(patch),

  refreshInspect: async () => {
    const pump = activePumpOrNull();
    if (!pump) {
      set({ inspect: null, inspectLoading: false, inspectError: "The engine is not connected yet." });
      return;
    }
    set({ inspectLoading: true, inspectError: null });
    try {
      const inspect = await pump.devInspect();
      set({ inspect, inspectLoading: false, inspectError: null });
    } catch (error) {
      set({ inspectLoading: false, inspectError: reportFailure("dev inspect", error) });
    }
  },

  setCodeTarget: (target) => set({ codeTarget: target }),

  setCodeState: (patch) => set(patch),

  generateCode: async (target) => {
    const pump = activePumpOrNull();
    const nextTarget = target ?? get().codeTarget;
    if (!pump) {
      set({ codeLoading: false, codeError: "The engine is not connected yet." });
      return;
    }
    set({ codeTarget: nextTarget, codeLoading: true, codeError: null });
    try {
      const generated = await pump.devCodegen(nextTarget, 2);
      set({ code: generated, codeLoading: false, codeError: null });
    } catch (error) {
      set({ codeLoading: false, codeError: reportFailure("generate code", error) });
    }
  },

  setHandoffState: (patch) => set(patch),

  reloadHandoff: async () => {
    const pump = activePumpOrNull();
    if (!pump) {
      set({ handoffLoading: false, handoffError: "The engine is not connected yet." });
      return;
    }
    set({ handoffLoading: true, handoffError: null });
    try {
      const handoff = await pump.devHandoff();
      set({ handoff, handoffLoading: false, handoffError: null });
    } catch (error) {
      set({ handoffLoading: false, handoffError: reportFailure("load annotations", error) });
    }
  },

  addAnnotation: async ({ nodeId, body, author, kind, link }) => {
    const pump = activePumpOrNull();
    if (!pump) {
      set({ handoffError: "The engine is not connected yet." });
      return false;
    }
    set({ handoffLoading: true, handoffError: null });
    try {
      const handoff = await pump.devAddAnnotation({
        node_id: nodeId,
        body: body.trim(),
        author: author.trim() || "You",
        created_at: new Date().toISOString(),
        kind,
        link: link.trim() || undefined,
      });
      set({ handoff, handoffLoading: false, handoffError: null });
      return true;
    } catch (error) {
      set({ handoffLoading: false, handoffError: reportFailure("add annotation", error) });
      return false;
    }
  },

  resolveAnnotation: async (annotationId) => {
    const pump = activePumpOrNull();
    if (!pump) {
      set({ handoffError: "The engine is not connected yet." });
      return false;
    }
    set({ handoffLoading: true, handoffError: null });
    try {
      const handoff = await pump.devResolveAnnotation(annotationId);
      set({ handoff, handoffLoading: false, handoffError: null });
      return true;
    } catch (error) {
      set({ handoffLoading: false, handoffError: reportFailure("resolve annotation", error) });
      return false;
    }
  },

  markReady: async (status, by, at) => {
    const pump = activePumpOrNull();
    if (!pump) {
      set({ handoffError: "The engine is not connected yet." });
      return false;
    }
    set({ handoffLoading: true, handoffError: null });
    try {
      const handoff = await pump.devMarkReady({ status, by: by.trim() || "You", at });
      set({ handoff, handoffLoading: false, handoffError: null });
      return true;
    } catch (error) {
      set({ handoffLoading: false, handoffError: reportFailure("mark handoff", error) });
      return false;
    }
  },

  setCaptures: (captures) => {
    const limited = captures.slice(0, 5);
    saveCaptures(limited, storage());
    set({ captures: limited });
  },

  captureRevision: async (label) => {
    const pump = activePumpOrNull();
    if (!pump) {
      set({ compareError: "The engine is not connected yet." });
      return null;
    }
    const engine = useAppStore.getState().engine;
    try {
      const snapshot = await pump.devSnapshot();
      const existing = get().captures;
      const revision = engine.revision;
      const capture: RevisionCapture = {
        id: `cap_${Date.now()}_${existing.length + 1}`,
        label: label?.trim() || nextCaptureLabel(existing, revision),
        revision,
        captured_at: new Date().toISOString(),
        snapshot,
      };
      const captures = addCapture(existing, capture);
      saveCaptures(captures, storage());
      set({ captures });
      return capture;
    } catch (error) {
      set({ compareError: reportFailure("capture revision", error) });
      return null;
    }
  },

  removeCapture: (id) => {
    const captures = removeCapture(get().captures, id);
    saveCaptures(captures, storage());
    set((state) => ({
      captures,
      compareResultCapture:
        state.compareResultCapture?.id === id ? null : state.compareResultCapture,
      compareResult: state.compareResultCapture?.id === id ? null : state.compareResult,
    }));
  },

  clearCaptures: () => {
    const target = storage();
    if (target) {
      try {
        target.removeItem(CAPTURE_STORAGE_KEY);
      } catch {
        // ignore
      }
    }
    set({ captures: [], compareResult: null, compareResultCapture: null, compareError: null });
  },

  setCompareState: (patch) => set(patch),

  compareWith: async (capture) => {
    const pump = activePumpOrNull();
    if (!pump) {
      set({ compareLoading: false, compareError: "The engine is not connected yet." });
      return;
    }
    set({ compareLoading: true, compareError: null, compareResultCapture: capture });
    try {
      const compareResult = await pump.devCompare(capture.snapshot);
      set({ compareResult, compareLoading: false, compareError: null });
    } catch (error) {
      set({
        compareLoading: false,
        compareError: reportFailure("compare revisions", error),
        compareResult: null,
      });
    }
  },

  diffSummary: () => {
    const result = get().compareResult;
    return result ? summarizeDiffForUi(result) : null;
  },
}));

/** Test/support helper: reset persisted captures then reload state. */
export function resetDevCaptures(): void {
  const target = storage();
  if (target) {
    try {
      target.removeItem(CAPTURE_STORAGE_KEY);
    } catch {
      // ignore
    }
  }
  useDevModeStore.setState({ captures: [] });
}
