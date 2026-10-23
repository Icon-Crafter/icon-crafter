/**
 * Right-side surface: the design Inspector normally, or the Dev Mode panel
 * while Dev Mode is active. Both keep their own collapse behaviour.
 */
import { useDevModeStore } from "../engine/devModeStore";
import { DevModePanel } from "./dev/DevModePanel";
import { Inspector } from "./Inspector";

export function RightSidebar() {
  const devActive = useDevModeStore((state) => state.active);
  return devActive ? <DevModePanel /> : <Inspector />;
}
