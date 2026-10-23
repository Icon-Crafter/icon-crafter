/**
 * Small shared building blocks for the Dev Mode sidebar tabs.
 */
import { useState, type ReactNode } from "react";

import { en } from "../../i18n/en";
import { getActivePump, useAppStore } from "../../engine/store";

export function DevSection({
  title,
  actions,
  children,
}: {
  title: string;
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="dev-section">
      <header className="dev-section-header">
        <h3 className="dev-section-title">{title}</h3>
        {actions ? <div className="dev-section-actions">{actions}</div> : null}
      </header>
      <div className="dev-section-body">{children}</div>
    </section>
  );
}

export function DevRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="dev-row">
      <span className="dev-row-label">{label}</span>
      <span className="dev-row-value">{children}</span>
    </div>
  );
}

export function DevEmpty({ title, body }: { title: string; body: string }) {
  return (
    <div className="dev-empty">
      <p className="dev-empty-title">{title}</p>
      <p className="dev-empty-body">{body}</p>
    </div>
  );
}

function formatTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function timestampLabel(value: string): string {
  return formatTime(value);
}

/** Copy-to-clipboard button with an honest success/failure toast. */
export function CopyButton({
  text,
  title,
  label,
  disabled,
  disabledTitle,
}: {
  text: string | (() => Promise<string | null>);
  title: string;
  label?: string;
  disabled?: boolean;
  disabledTitle?: string;
}) {
  const [busy, setBusy] = useState(false);
  const pushToast = useAppStore((state) => state.pushToast);

  async function copy() {
    if (disabled || busy) return;
    setBusy(true);
    try {
      const resolved = typeof text === "function" ? await text() : text;
      if (resolved === null) {
        pushToast({ kind: "warning", title, message: en.dev.actions.copyColorUnavailable });
        return;
      }
      await navigator.clipboard.writeText(resolved);
      pushToast({ kind: "success", title: en.dev.actions.copied });
    } catch {
      pushToast({ kind: "error", title: en.dev.actions.copyFailed });
    } finally {
      setBusy(false);
    }
  }

  return (
    <button
      type="button"
      className="dev-copy-button"
      onClick={() => void copy()}
      disabled={disabled || busy}
      title={disabled ? (disabledTitle ?? title) : title}
      aria-label={title}
    >
      {label ?? title}
    </button>
  );
}

/** Current selection ids, for effect dependencies. */
export function useSelectionKey(): string {
  return useAppStore((state) => state.engine.selection.join("|"));
}

export function copySnapshotJson(): Promise<string | null> {
  const pump = getActivePump();
  if (!pump) return Promise.resolve(null);
  return pump.devSnapshot();
}

export function formatRevisionDate(value: string): string {
  return formatTime(value);
}
