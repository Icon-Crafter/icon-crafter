import {
  forwardRef,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type ReactNode,
} from "react";
import { cx } from "../../utils/dom";
import type { KeyCombo } from "../../engine/shortcuts";

export interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  label: string;
  tooltip?: string;
  shortcut?: string;
  active?: boolean;
  accent?: boolean;
  size?: "sm" | "md";
  children: ReactNode;
}

/** Square icon button with an accessible label and an optional native tooltip. */
export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  { label, tooltip, shortcut, active, accent, size = "md", className, children, ...rest },
  ref,
) {
  const hint = tooltip ?? label;
  const title = shortcut ? `${hint} (${shortcut})` : hint;
  return (
    <button
      ref={ref}
      type="button"
      aria-label={label}
      title={title}
      {...rest}
      className={cx(
        "icon-button",
        size === "sm" && "icon-button--sm",
        active && "icon-button--active",
        accent && "icon-button--accent",
        className,
      )}
    >
      {children}
    </button>
  );
});

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "default" | "primary" | "ghost" | "danger";
  icon?: ReactNode;
}

export function Button({ variant = "default", icon, className, children, ...rest }: ButtonProps) {
  return (
    <button
      type="button"
      {...rest}
      className={cx(
        "button",
        variant === "primary" && "button--primary",
        variant === "ghost" && "button--ghost",
        variant === "danger" && "button--danger",
        className,
      )}
    >
      {icon}
      {children}
    </button>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return <kbd>{children}</kbd>;
}

/** Render a shortcut binding like "Mod+Shift+Z" as individual keycaps. */
export function ShortcutKeys({ combo, isMac }: { combo: KeyCombo; isMac: boolean }) {
  const keys = useMemo(() => {
    const parts: string[] = [];
    if (combo.ctrl) parts.push(isMac ? "⌃" : "Ctrl");
    if (combo.meta) parts.push(isMac ? "⌘" : "Win");
    if (combo.alt) parts.push(isMac ? "⌥" : "Alt");
    if (combo.shift) parts.push(isMac ? "⇧" : "Shift");
    if (combo.key) parts.push(combo.key);
    return parts;
  }, [combo, isMac]);
  return (
    <span className="palette-item-shortcut" aria-hidden="true">
      {keys.map((key, index) => (
        <Kbd key={`${key}-${index}`}>{key}</Kbd>
      ))}
    </span>
  );
}

/** Spinner used in buttons and loading states. */
export function Spinner({ size = 14 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.4"
      strokeLinecap="round"
      aria-hidden="true"
      style={{ animation: "spin 0.8s linear infinite" }}
    >
      <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>
      <path d="M12 3a9 9 0 1 0 9 9" />
    </svg>
  );
}

export interface MenuItemProps {
  label: string;
  shortcut?: string;
  icon?: ReactNode;
  disabled?: boolean;
  danger?: boolean;
  /** Tooltip explaining why an item is disabled or what it does. */
  title?: string;
  onSelect: () => void;
  /** Optional trailing detail (e.g. current value). */
  detail?: string;
}

export function MenuItem({
  label,
  shortcut,
  icon,
  disabled,
  danger,
  title,
  onSelect,
  detail,
}: MenuItemProps) {
  return (
    <button
      type="button"
      role="menuitem"
      disabled={disabled}
      title={title}
      className={cx("menu-item", danger && "menu-item--danger")}
      onClick={onSelect}
    >
      <span className="menu-item-icon">{icon}</span>
      <span className="menu-item-label">{label}</span>
      {detail !== undefined && <span className="menu-item-shortcut">{detail}</span>}
      {shortcut && (
        <span className="menu-item-shortcut" aria-hidden="true">
          {shortcut.split("+").map((key, index) => (
            <Kbd key={`${key}-${index}`}>{key}</Kbd>
          ))}
        </span>
      )}
    </button>
  );
}

export function MenuSeparator() {
  return <div className="menu-sep" role="separator" />;
}

export function MenuLabel({ children }: { children: ReactNode }) {
  return <div className="menu-label">{children}</div>;
}

interface MenuSurfaceProps {
  /** Viewport anchor point (already clamped by caller where possible). */
  x: number;
  y: number;
  onClose: () => void;
  children: ReactNode;
  align?: "start" | "end";
}

/**
 * Floating menu surface. Closes on Escape, outside pointerdown, scroll and
 * window blur; positions itself inside the viewport.
 */
export function MenuSurface({ x, y, onClose, children, align = "start" }: MenuSurfaceProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: x, top: y });

  useLayoutEffect(() => {
    const node = ref.current;
    if (!node) return;
    const rect = node.getBoundingClientRect();
    const margin = 8;
    let left = align === "end" ? x - rect.width : x;
    let top = y;
    if (left + rect.width > window.innerWidth - margin) left = window.innerWidth - rect.width - margin;
    if (left < margin) left = margin;
    if (top + rect.height > window.innerHeight - margin) top = Math.max(margin, y - rect.height);
    setPos({ left, top });
  }, [x, y, align]);

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) onClose();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("blur", onClose);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("blur", onClose);
    };
  }, [onClose]);

  return (
    <div
      ref={ref}
      className="menu"
      role="menu"
      style={{ left: pos.left, top: pos.top }}
      onContextMenu={(event) => event.preventDefault()}
    >
      {children}
    </div>
  );
}

/** Small hook to open/close anchored popovers. */
export function useAnchoredMenu() {
  const [anchor, setAnchor] = useState<{ x: number; y: number } | null>(null);
  const openAt = (x: number, y: number) => setAnchor({ x, y });
  const close = () => setAnchor(null);
  return { anchor, openAt, close };
}
