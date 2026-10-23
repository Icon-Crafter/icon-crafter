import { useAppStore } from "../engine/store";
import { AlertTriangle, CheckCircle2, CircleAlert, Info, X } from "../icons";
import type { ToastKind } from "../engine/store";
import type { ReactNode } from "react";
import { en } from "../i18n/en";

const ICONS: Record<ToastKind, ReactNode> = {
  info: <Info size={15} />,
  success: <CheckCircle2 size={15} />,
  warning: <AlertTriangle size={15} />,
  error: <CircleAlert size={15} />,
};

/** Bottom-right toast stack. Rendered above the canvas, below dialogs. */
export function Toasts() {
  const toasts = useAppStore((state) => state.toasts);
  const dismiss = useAppStore((state) => state.dismissToast);

  if (toasts.length === 0) return null;

  return (
    <div className="toasts" role="status" aria-live="polite">
      {toasts.map((toast) => (
        <div key={toast.id} className={`toast toast--${toast.kind}`}>
          <span className="toast-icon">{ICONS[toast.kind]}</span>
          <div className="toast-body">
            <div className="toast-title">{toast.title}</div>
            {toast.message && <div className="toast-message">{toast.message}</div>}
          </div>
          <button
            type="button"
            className="toast-close"
            aria-label={en.toasts.dismiss}
            onClick={() => dismiss(toast.id)}
          >
            <X size={13} />
          </button>
        </div>
      ))}
    </div>
  );
}
