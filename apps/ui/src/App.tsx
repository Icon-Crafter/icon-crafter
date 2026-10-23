import { useEffect } from "react";
import { useAppStore } from "./engine/store";
import { handleShortcut, stopPresentation } from "./engine/commands";
import {
  isEditableTarget,
  isMacPlatform,
  resolveShortcut,
} from "./engine/shortcuts";
import { useUiStore } from "./engine/uiStore";
import { en } from "./i18n/en";
import { CommandPalette } from "./components/CommandPalette";
import { ContextMenus } from "./components/ContextMenus";
import { DialogHost } from "./components/Dialogs";
import { RightSidebar } from "./components/RightSidebar";
import { LeftPanel } from "./components/LeftPanel";
import { CanvasStage } from "./components/CanvasStage";
import { StatusBar } from "./components/StatusBar";
import { Toasts } from "./components/Toasts";
import { Toolbar } from "./components/Toolbar";
import { Presentation } from "./components/Presentation";
import { ConfirmDialog } from "./components/ConfirmDialog";

/**
 * Application shell. Mounts the engine connection and maps the window-level
 * keyboard shortcuts; all visual work lives in the child components.
 */
export function App() {
  const connect = useAppStore((state) => state.connect);
  const engine = useAppStore((state) => state.engine);
  const setPaletteOpen = useAppStore((state) => state.setPaletteOpen);

  useEffect(() => {
    void connect();
  }, [connect]);

  // Document title mirrors the engine document (native window manager only).
  useEffect(() => {
    const name = engine.doc?.name ?? en.app.name;
    document.title = engine.dirty ? `${name} — ${en.chrome.stateUnsaved} · ${en.app.name}` : `${name} · ${en.app.name}`;
  }, [engine.doc?.name, engine.dirty]);

  useEffect(() => {
    const isMac = isMacPlatform();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      // In presentation the engine owns navigation; only Escape is handled here.
      if (useAppStore.getState().engine.prototype?.active) {
        if (event.key === "Escape") {
          event.preventDefault();
          void stopPresentation();
        }
        return;
      }
      const action = resolveShortcut(event, useAppStore.getState().shortcuts, isMac);
      if (!action) return;
      if (action === "app.commandPalette") {
        event.preventDefault();
        setPaletteOpen(!useAppStore.getState().paletteOpen);
        return;
      }
      if (isEditableTarget(event.target)) return;
      event.preventDefault();
      handleShortcut(action);
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [setPaletteOpen]);

  // Escape dismisses transient shell state (menus, rename fields).
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (event.defaultPrevented) return;
      const ui = useUiStore.getState();
      ui.closeMenus();
      ui.setRenamingNodeId(null);
      ui.setRenamingPageId(null);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  // Shortcut bindings are user-customizable; keep the CSS-visible map fresh.
  useEffect(() => {
    document.documentElement.dataset.platform = isMacPlatform() ? "mac" : "win";
  }, []);

  // Presentation replaces the editor chrome: the engine owns navigation there.
  if (engine.prototype?.active) {
    return (
      <div className="app-shell app-shell--presenting">
        <Presentation />
        <Toasts />
      </div>
    );
  }

  return (
    <div className="app-shell">
      <Toolbar />
      <div className="workspace">
        <LeftPanel />
        <main className="canvas-area" aria-label={en.canvas.workspaceLabel}>
          <CanvasStage />
        </main>
        <RightSidebar />
      </div>
      <StatusBar />
      <CommandPalette />
      <ContextMenus />
      <DialogHost />
      <ConfirmDialog />
      <Toasts />
    </div>
  );
}
