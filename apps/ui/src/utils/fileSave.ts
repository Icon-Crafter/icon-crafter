/**
 * Saves generated text to a user-chosen path.
 *
 * Inside Tauri this writes through the fs plugin (the path comes from the
 * dialog plugin, which is the only way the user grants access). In the browser
 * dev harness no filesystem exists, so it falls back to a normal download.
 * Returns false when the user cancels the dialog.
 */
import { isTauriRuntime } from "../engine/transport";

export async function saveTextFile(path: string, contents: string): Promise<boolean> {
  if (isTauriRuntime()) {
    const { writeTextFile } = await import("@tauri-apps/plugin-fs");
    await writeTextFile(path, contents);
    return true;
  }
  if (typeof document === "undefined") return false;
  const blob = new Blob([contents], { type: "text/plain;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = path.split(/[\\/]/).pop() ?? "generated.txt";
  anchor.rel = "noopener";
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 2000);
  return true;
}
