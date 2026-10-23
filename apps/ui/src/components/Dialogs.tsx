import { useEffect, useMemo, useState } from "react";
import { useAppStore, useCapabilities } from "../engine/store";
import {
  chooseExportPath,
  chooseSavePath,
  performExport,
  performIntent,
  resolveUnsavedWithDiscard,
  resolveUnsavedWithSave,
  saveDocumentTo,
  suggestFileName,
  type ExportFormat,
  type ExportScope,
} from "../engine/commands";
import { en } from "../i18n/en";
import { format } from "../i18n/en";
import { Dialog } from "./primitives/Layout";
import { Checkbox, Segmented, TextField } from "./primitives/Fields";
import { Button, Spinner } from "./primitives/IconButton";
import { FolderOpen } from "../icons";

const FORMAT_LABELS: Record<ExportFormat, string> = {
  png: en.dialogs.export.formatPng,
  svg: en.dialogs.export.formatSvg,
  pdf: en.dialogs.export.formatPdf,
};

const EXTENSION: Record<ExportFormat, string> = { png: ".png", svg: ".svg", pdf: ".pdf" };

function PathPicker({
  path,
  onPick,
  busy,
  label,
  hint,
}: {
  path: string;
  onPick: () => void;
  busy: boolean;
  label: string;
  hint?: string;
}) {
  return (
    <div className="form-field">
      <span className="form-field-label">{label}</span>
      <div className="path-picker">
        <div className={`path-display${path ? "" : " path-display--empty"}`} title={path}>
          {path || en.dialogs.export.noPath}
        </div>
        <Button icon={<FolderOpen size={14} />} onClick={onPick} disabled={busy}>
          {en.dialogs.export.choosePath}
        </Button>
      </div>
      {hint ? <span className="form-field-hint">{hint}</span> : null}
    </div>
  );
}

function ExportDialog() {
  const engine = useAppStore((state) => state.engine);
  const busy = useAppStore((state) => state.busy);
  const closeDialog = useAppStore((state) => state.closeDialog);
  const setBusy = useAppStore((state) => state.setBusy);
  const capabilities = useCapabilities();

  const hasSelection = engine.selection.length > 0;
  const canScopeSelection = capabilities.export_scope === true && hasSelection;
  const pageName = engine.summary?.pages.find((page) => page.id === engine.summary?.active_page)?.name;

  const [exportFormat, setExportFormat] = useState<ExportFormat>("png");
  const [scope, setScope] = useState<ExportScope>(
    hasSelection && capabilities.export_scope === true ? "selection" : "page",
  );
  const [scale, setScale] = useState("2");
  const [transparent, setTransparent] = useState(false);
  const [path, setPath] = useState(() => suggestFileName("png"));
  const [error, setError] = useState<string | null>(null);

  const formatOptions = useMemo(
    () => [
      { value: "png" as const, label: FORMAT_LABELS.png },
      { value: "svg" as const, label: FORMAT_LABELS.svg },
      ...(capabilities.export_pdf === true
        ? [{ value: "pdf" as const, label: FORMAT_LABELS.pdf }]
        : []),
    ],
    [capabilities.export_pdf],
  );

  useEffect(() => {
    setPath((current) => {
      const base = current.replace(/\.(png|svg|pdf)$/i, "");
      return `${base}${EXTENSION[exportFormat]}`;
    });
  }, [exportFormat]);

  useEffect(() => {
    if (capabilities.export_pdf !== true && exportFormat === "pdf") setExportFormat("png");
  }, [capabilities.export_pdf, exportFormat]);

  const pick = async () => {
    setError(null);
    const next = await chooseExportPath(exportFormat);
    if (next) setPath(next);
  };

  const submit = async () => {
    if (!path) {
      setError(en.dialogs.export.noPath);
      return;
    }
    if (scope === "selection" && !hasSelection) {
      setError(en.dialogs.export.nothingSelected);
      return;
    }
    setBusy(true);
    try {
      const result = await performExport({
        format: exportFormat,
        path,
        scale: Number(scale) || 1,
        quality: 1,
        scope,
        transparent: exportFormat === "png" && transparent,
      });
      if (result) closeDialog();
    } finally {
      setBusy(false);
    }
  };

  const selectionHint = hasSelection
    ? en.dialogs.export.scopeSelectionHint
    : en.dialogs.export.nothingSelected;
  const selectionUnavailableTitle =
    capabilities.export_scope === true
      ? en.dialogs.export.nothingSelected
      : en.dialogs.export.selectionUnavailable;

  return (
    <Dialog
      title={en.dialogs.export.title}
      subtitle={en.dialogs.export.subtitle}
      wide
      busy={busy}
      onClose={busy ? () => undefined : closeDialog}
      footer={
        <>
          <Button onClick={closeDialog} disabled={busy}>
            {en.common.cancel}
          </Button>
          <Button variant="primary" onClick={() => void submit()} disabled={busy}>
            {busy ? <Spinner size={13} /> : null}
            {busy ? en.dialogs.export.exporting : en.dialogs.export.export}
          </Button>
        </>
      }
    >
      <div className="form-field">
        <span className="form-field-label">{en.dialogs.export.format}</span>
        <Segmented
          ariaLabel={en.dialogs.export.format}
          value={exportFormat}
          onChange={(value) => setExportFormat(value)}
          options={formatOptions}
        />
      </div>

      <div className="form-field">
        <span className="form-field-label">{en.dialogs.export.scope}</span>
        <div className="option-grid">
          <button
            type="button"
            role="radio"
            aria-checked={scope === "page"}
            className={`option-card${scope === "page" ? " option-card--active" : ""}`}
            onClick={() => setScope("page")}
            title={hasSelection ? en.dialogs.export.scopePageHint : undefined}
          >
            <span className="option-card-title">{en.dialogs.export.scopePage}</span>
            {pageName ? <span className="option-card-hint">{pageName}</span> : null}
          </button>
          <button
            type="button"
            role="radio"
            aria-checked={scope === "selection"}
            className={`option-card${scope === "selection" ? " option-card--active" : ""}`}
            disabled={!canScopeSelection}
            onClick={() => setScope("selection")}
            title={selectionUnavailableTitle}
          >
            <span className="option-card-title">{en.dialogs.export.scopeSelection}</span>
            <span className="option-card-hint">
              {capabilities.export_scope === true
                ? selectionHint
                : en.dialogs.export.selectionUnavailable}
            </span>
          </button>
        </div>
        {scope === "page" && hasSelection ? (
          <span className="form-field-hint">{en.dialogs.export.scopePageHint}</span>
        ) : null}
      </div>

      {exportFormat === "png" && (
        <div className="form-field">
          <span className="form-field-label">{en.dialogs.export.scale}</span>
          <Segmented
            ariaLabel={en.dialogs.export.scale}
            value={scale}
            onChange={(value) => setScale(value)}
            options={[
              { value: "1", label: "1×" },
              { value: "2", label: "2×" },
              { value: "3", label: "3×" },
              { value: "4", label: "4×" },
            ]}
          />
        </div>
      )}

      {exportFormat === "png" ? (
        <div className="form-field">
          <span className="form-field-label">{en.dialogs.export.transparent}</span>
          {capabilities.export_scope === true ? (
            <Checkbox
              checked={transparent}
              onChange={setTransparent}
              label={en.dialogs.export.transparent}
            />
          ) : (
            <span className="form-field-hint">{en.dialogs.export.transparentUnavailable}</span>
          )}
          {capabilities.export_scope === true ? (
            <span className="form-field-hint">{en.dialogs.export.transparentHint}</span>
          ) : null}
        </div>
      ) : null}

      <PathPicker path={path} onPick={() => void pick()} busy={busy} label={en.dialogs.export.path} />
      {error && <div className="form-error">{error}</div>}
    </Dialog>
  );
}

function SaveAsDialog() {
  const engine = useAppStore((state) => state.engine);
  const intent = useAppStore((state) => state.unsavedIntent);
  const busy = useAppStore((state) => state.busy);
  const closeDialog = useAppStore((state) => state.closeDialog);
  const setBusy = useAppStore((state) => state.setBusy);
  const [path, setPath] = useState(() => engine.doc?.path ?? suggestFileName("iconcrafter"));
  const [error, setError] = useState<string | null>(null);

  const pick = async () => {
    const next = await chooseSavePath();
    if (next) setPath(next);
  };

  const submit = async () => {
    if (!path) {
      setError(en.dialogs.saveAs.noPath);
      return;
    }
    setBusy(true);
    try {
      const saved = await saveDocumentTo(path);
      if (!saved) return;
      closeDialog();
      if (intent) await performIntent(intent);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      title={en.dialogs.saveAs.title}
      subtitle={en.dialogs.saveAs.body}
      busy={busy}
      onClose={busy ? () => undefined : closeDialog}
      footer={
        <>
          <Button onClick={closeDialog} disabled={busy}>
            {en.common.cancel}
          </Button>
          <Button variant="primary" onClick={() => void submit()} disabled={busy}>
            {busy ? <Spinner size={13} /> : null}
            {busy ? en.dialogs.saveAs.saving : en.dialogs.saveAs.save}
          </Button>
        </>
      }
    >
      <div className="form-field">
        <span className="form-field-label">{en.dialogs.saveAs.destination}</span>
        <div className="path-picker">
          <TextField
            ariaLabel={en.dialogs.saveAs.title}
            value={path}
            onChange={(event) => setPath(event.target.value)}
            onCommit={(value) => setPath(value)}
            autoFocus
          />
          <Button icon={<FolderOpen size={14} />} onClick={() => void pick()} disabled={busy}>
            {en.dialogs.saveAs.choosePath}
          </Button>
        </div>
      </div>
      {error && <div className="form-error">{error}</div>}
      {intent && <div className="dialog-note">{en.dialogs.unsaved.detail}</div>}
    </Dialog>
  );
}

function UnsavedDialog() {
  const engine = useAppStore((state) => state.engine);
  const busy = useAppStore((state) => state.busy);
  const closeDialog = useAppStore((state) => state.closeDialog);
  const docName = engine.doc?.name ?? en.chrome.documentTitleFallback;

  return (
    <Dialog
      title={en.dialogs.unsaved.title}
      subtitle={format(en.dialogs.unsaved.body, { name: docName })}
      busy={busy}
      onClose={busy ? () => undefined : closeDialog}
      footer={
        <>
          <Button onClick={closeDialog} disabled={busy}>
            {en.dialogs.unsaved.cancel}
          </Button>
          <Button variant="danger" onClick={() => void resolveUnsavedWithDiscard()} disabled={busy}>
            {en.dialogs.unsaved.discard}
          </Button>
          <Button variant="primary" onClick={() => void resolveUnsavedWithSave()} disabled={busy}>
            {busy ? <Spinner size={13} /> : null}
            {busy ? en.dialogs.unsaved.saving : en.dialogs.unsaved.save}
          </Button>
        </>
      }
    >
      <p className="dialog-note">{en.dialogs.unsaved.detail}</p>
    </Dialog>
  );
}

/** Renders whichever modal the store asks for. */
export function DialogHost() {
  const dialog = useAppStore((state) => state.dialog);
  const nodes = useMemo(() => {
    switch (dialog) {
      case "export":
        return <ExportDialog />;
      case "saveAs":
        return <SaveAsDialog />;
      case "unsaved":
        return <UnsavedDialog />;
      default:
        return null;
    }
  }, [dialog]);
  return nodes;
}
