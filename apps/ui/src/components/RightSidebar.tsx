/**
 * Right-side surface: the design Inspector normally, the AI assistant or the
 * plugin manager when those panels are open, and the Dev Mode panel whenever
 * Dev Mode is active (Dev Mode takes priority so inspection is never lost).
 */
import { useDevModeStore } from "../engine/devModeStore";
import { useUiStore } from "../engine/uiStore";
import { DevModePanel } from "./dev/DevModePanel";
import { Inspector } from "./Inspector";
import { AiPanel } from "./panels/AiPanel";
import { PluginsPanel } from "./panels/PluginsPanel";

export function RightSidebar() {
  const devActive = useDevModeStore((state) => state.active);
  const rightPanel = useUiStore((state) => state.rightPanel);
  if (devActive) return <DevModePanel />;
  if (rightPanel === "ai") return <AiPanel />;
  if (rightPanel === "plugins") return <PluginsPanel />;
  return <Inspector />;
}
