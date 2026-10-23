import {
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { ChevronDown, ChevronRight, X } from "../../icons";
import { cx } from "../../utils/dom";

/* -------------------------------------------------------------------------- */
/* Collapsible inspector section                                              */
/* -------------------------------------------------------------------------- */

const SECTION_STATE_KEY = "iconcrafter.inspector.sections.v1";

function loadSectionState(): Record<string, boolean> {
  try {
    const raw = window.localStorage.getItem(SECTION_STATE_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : {};
    return parsed && typeof parsed === "object" ? (parsed as Record<string, boolean>) : {};
  } catch {
    return {};
  }
}

function storeSectionState(state: Record<string, boolean>) {
  try {
    window.localStorage.setItem(SECTION_STATE_KEY, JSON.stringify(state));
  } catch {
    /* non-critical */
  }
}

export interface SectionProps {
  id: string;
  title: string;
  actions?: ReactNode;
  defaultOpen?: boolean;
  children: ReactNode;
}

/** Collapsible property section; open/closed state persists per section id. */
export function Section({ id, title, actions, defaultOpen = true, children }: SectionProps) {
  const [open, setOpen] = useState(() => loadSectionState()[id] ?? defaultOpen);

  const toggle = () => {
    setOpen((value) => {
      const next = !value;
      const all = loadSectionState();
      all[id] = next;
      storeSectionState(all);
      return next;
    });
  };

  return (
    <div className="section">
      <div className="section-header">
        <button
          type="button"
          className="section-collapse"
          aria-expanded={open}
          aria-label={open ? `Collapse ${title}` : `Expand ${title}`}
          onClick={toggle}
        >
          {open ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        </button>
        <button type="button" className="section-title" onClick={toggle} style={{ cursor: "pointer" }}>
          {title}
        </button>
        {actions && <div className="section-actions">{actions}</div>}
      </div>
      {open && <div className="section-body">{children}</div>}
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/* Dialog shell                                                               */
/* -------------------------------------------------------------------------- */

export interface DialogProps {
  title: string;
  subtitle?: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  wide?: boolean;
  /** Prevent closing while a long-running operation is in flight. */
  busy?: boolean;
  /** When true, clicking the backdrop does not dismiss (e.g. unsaved changes). */
  modal?: boolean;
}

/**
 * Accessible modal dialog rendered into a portal with focus trapping,
 * body scroll lock and Escape handling.
 */
export function Dialog({ title, subtitle, onClose, children, footer, wide, busy, modal }: DialogProps) {
  const ref = useRef<HTMLDivElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);

  useEffect(() => {
    previousFocus.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const node = ref.current;
    const target =
      node?.querySelector<HTMLElement>("[data-autofocus]") ??
      node?.querySelector<HTMLElement>("input, button, textarea, select");
    target?.focus();

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) {
        event.stopPropagation();
        onClose();
        return;
      }
      if (event.key !== "Tab" || !node) return;
      const focusables = Array.from(
        node.querySelectorAll<HTMLElement>(
          'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
        ),
      ).filter((element) => element.offsetParent !== null);
      if (focusables.length === 0) return;
      const first = focusables[0] as HTMLElement;
      const last = focusables[focusables.length - 1] as HTMLElement;
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      previousFocus.current?.focus();
    };
  }, [busy, onClose]);

  return createPortal(
    <div
      className="dialog-backdrop"
      onPointerDown={(event) => {
        if (!modal && !busy && event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={ref}
        className={cx("dialog", wide && "dialog--wide")}
        role="dialog"
        aria-modal="true"
        aria-labelledby="dialog-title"
      >
        <div className="dialog-header">
          <div className="dialog-heading">
            <h2 className="dialog-title" id="dialog-title">
              {title}
            </h2>
            {subtitle && <p className="dialog-subtitle">{subtitle}</p>}
          </div>
          <button type="button" className="dialog-close" aria-label="Close dialog" onClick={onClose} disabled={busy}>
            <X size={15} />
          </button>
        </div>
        <div className="dialog-body">{children}</div>
        {footer && <div className="dialog-footer">{footer}</div>}
      </div>
    </div>,
    document.body,
  );
}

/* -------------------------------------------------------------------------- */
/* Empty state                                                                */
/* -------------------------------------------------------------------------- */

export interface EmptyStateProps {
  icon?: ReactNode;
  title: string;
  body?: string;
  actions?: ReactNode;
}

export function EmptyState({ icon, title, body, actions }: EmptyStateProps) {
  return (
    <div className="empty-state">
      {icon && <div className="empty-state-icon">{icon}</div>}
      <div className="empty-state-title">{title}</div>
      {body && <p className="empty-state-body">{body}</p>}
      {actions && <div className="empty-state-actions">{actions}</div>}
    </div>
  );
}
