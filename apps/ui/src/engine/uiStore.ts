import { create } from "zustand";

/**
 * Transient UI-only state that must be shared across the shell (context menus,
 * focus hints). Everything document-related lives in the engine store.
 */

export interface CanvasMenuState {
  x: number;
  y: number;
}

export interface LayerMenuState {
  x: number;
  y: number;
  nodeId: string;
}

export interface PageMenuState {
  x: number;
  y: number;
  pageId: string;
}

export interface ConfirmRequest {
  title: string;
  body: string;
  confirmLabel: string;
  danger?: boolean;
  onConfirm: () => void | Promise<void>;
}

interface UiStore {
  canvasMenu: CanvasMenuState | null;
  layerMenu: LayerMenuState | null;
  pageMenu: PageMenuState | null;
  /** Node id currently being renamed in the layers panel. */
  renamingNodeId: string | null;
  /** Page id currently being renamed in the pages panel. */
  renamingPageId: string | null;
  shortcutsOpen: boolean;
  confirm: ConfirmRequest | null;
  openCanvasMenu: (x: number, y: number) => void;
  openLayerMenu: (x: number, y: number, nodeId: string) => void;
  openPageMenu: (x: number, y: number, pageId: string) => void;
  closeMenus: () => void;
  setRenamingNodeId: (id: string | null) => void;
  setRenamingPageId: (id: string | null) => void;
  setShortcutsOpen: (open: boolean) => void;
  openConfirm: (request: ConfirmRequest) => void;
  closeConfirm: () => void;
}

export const useUiStore = create<UiStore>((set) => ({
  canvasMenu: null,
  layerMenu: null,
  pageMenu: null,
  renamingNodeId: null,
  renamingPageId: null,
  shortcutsOpen: false,
  confirm: null,
  openCanvasMenu: (x, y) => set({ canvasMenu: { x, y }, layerMenu: null, pageMenu: null }),
  openLayerMenu: (x, y, nodeId) => set({ layerMenu: { x, y, nodeId }, canvasMenu: null, pageMenu: null }),
  openPageMenu: (x, y, pageId) => set({ pageMenu: { x, y, pageId }, canvasMenu: null, layerMenu: null }),
  closeMenus: () => set({ canvasMenu: null, layerMenu: null, pageMenu: null }),
  setRenamingNodeId: (renamingNodeId) => set({ renamingNodeId }),
  setRenamingPageId: (renamingPageId) => set({ renamingPageId }),
  setShortcutsOpen: (shortcutsOpen) => set({ shortcutsOpen }),
  openConfirm: (confirm) => set({ confirm }),
  closeConfirm: () => set({ confirm: null }),
}));
