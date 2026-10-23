import { useState } from "react";
import { useUiStore } from "../engine/uiStore";
import { en } from "../i18n/en";
import { Dialog } from "./primitives/Layout";

/**
 * Confirmations hosted by the UI store. Kept separate from engine dialogs:
 * destructive-but-local actions (delete style, detach component) confirm here
 * and only run their handler after the user accepts.
 */
export function ConfirmDialog() {
  const request = useUiStore((state) => state.confirm);
  const closeConfirm = useUiStore((state) => state.closeConfirm);
  const [busy, setBusy] = useState(false);

  if (!request) return null;

  const run = async () => {
    setBusy(true);
    try {
      await request.onConfirm();
    } finally {
      setBusy(false);
      closeConfirm();
    }
  };

  return (
    <Dialog
      title={request.title}
      onClose={() => {
        if (!busy) closeConfirm();
      }}
      busy={busy}
      footer={
        <>
          <button type="button" className="button" onClick={closeConfirm} disabled={busy}>
            {en.common.cancel}
          </button>
          <button
            type="button"
            className={`button ${request.danger ? "button--danger" : "button--primary"}`}
            disabled={busy}
            onClick={() => void run()}
          >
            {request.confirmLabel}
          </button>
        </>
      }
    >
      <p className="dialog-note">{request.body}</p>
    </Dialog>
  );
}
