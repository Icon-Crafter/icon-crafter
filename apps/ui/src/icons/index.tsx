import type { SVGProps } from "react";
import {
  AlignCenterHorizontal,
  AlignCenterVertical,
  AlignEndHorizontal,
  AlignEndVertical,
  AlignHorizontalDistributeCenter,
  AlignHorizontalJustifyCenter,
  AlignHorizontalJustifyEnd,
  AlignHorizontalJustifyStart,
  AlignStartHorizontal,
  AlignStartVertical,
  AlignVerticalDistributeCenter,
  AlignVerticalJustifyCenter,
  AlignVerticalJustifyEnd,
  AlignVerticalJustifyStart,
  AlertTriangle,
  ArrowUpRight,
  Bold,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  Circle,
  CircleAlert,
  ClipboardPaste,
  Copy,
  CopyPlus,
  Component,
  Contrast,
  Download,
  Eraser,
  Eye,
  EyeOff,
  FileCode2,
  FileDown,
  FileImage,
  FilePlus2,
  FileText,
  FileType2,
  FolderOpen,
  Frame,
  Grid3X3,
  Hand,
  Image as ImageIcon,
  ImagePlus,
  Info,
  Italic,
  Keyboard,
  Layers,
  LayoutGrid,
  Link2,
  Lock,
  LockOpen,
  Maximize2,
  Menu as MenuIcon,
  MessageSquare,
  Minus,
  Moon,
  MoreHorizontal,
  Move,
  MoveHorizontal,
  MoveVertical,
  PanelLeftClose,
  PanelLeftOpen,
  PanelRightClose,
  PanelRightOpen,
  Palette,
  PenTool,
  Pentagon,
  Pipette,
  Play,
  Plus,
  Redo2,
  RotateCcw,
  RotateCw,
  Save,
  Scissors,
  Search,
  Settings2,
  Share2,
  SlidersHorizontal,
  Sparkles,
  Spline,
  Square,
  Star,
  Sun,
  Trash2,
  Type,
  Undo2,
  Unlink2,
  X,
  XCircle,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import type { ComponentType } from "react";

export type IconComponent = ComponentType<SVGProps<SVGSVGElement>>;

export {
  AlignCenterHorizontal,
  AlignCenterVertical,
  AlignEndHorizontal,
  AlignEndVertical,
  AlignHorizontalDistributeCenter,
  AlignHorizontalJustifyCenter,
  AlignHorizontalJustifyEnd,
  AlignHorizontalJustifyStart,
  AlignStartHorizontal,
  AlignStartVertical,
  AlignVerticalDistributeCenter,
  AlignVerticalJustifyCenter,
  AlignVerticalJustifyEnd,
  AlignVerticalJustifyStart,
  AlertTriangle,
  ArrowUpRight,
  Bold,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  Circle,
  CircleAlert,
  ClipboardPaste,
  Copy,
  CopyPlus,
  Component,
  Contrast,
  Download,
  Eraser,
  Eye,
  EyeOff,
  FileCode2,
  FileDown,
  FileImage,
  FilePlus2,
  FileText,
  FileType2,
  FolderOpen,
  Frame,
  Grid3X3,
  Hand,
  ImageIcon,
  ImagePlus,
  Info,
  Italic,
  Keyboard,
  Layers,
  LayoutGrid,
  Link2,
  Lock,
  LockOpen,
  Maximize2,
  MenuIcon,
  MessageSquare,
  Minus,
  Moon,
  MoreHorizontal,
  Move,
  MoveHorizontal,
  MoveVertical,
  PanelLeftClose,
  PanelLeftOpen,
  PanelRightClose,
  PanelRightOpen,
  Palette,
  PenTool,
  Pentagon,
  Pipette,
  Play,
  Plus,
  Redo2,
  RotateCcw,
  RotateCw,
  Save,
  Scissors,
  Search,
  Settings2,
  Share2,
  SlidersHorizontal,
  Sparkles,
  Spline,
  Square,
  Star,
  Sun,
  Trash2,
  Type,
  Undo2,
  Unlink2,
  X,
  XCircle,
  ZoomIn,
  ZoomOut,
};

type Props = SVGProps<SVGSVGElement>;

const strokeDefaults = {
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.5,
  strokeLinecap: "round",
  strokeLinejoin: "round",
} as const;

/** Product brand mark: graphite tile, coral sparkle, teal orbit. */
export function Logo({ size = 28, ...rest }: Props & { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 160 160"
      role="img"
      aria-label="Icon Crafter"
      {...rest}
    >
      <defs>
        <linearGradient id="ic-logo-tile" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#171a21" />
          <stop offset="1" stopColor="#0b0c10" />
        </linearGradient>
        <linearGradient id="ic-logo-spark" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#ff8a66" />
          <stop offset="1" stopColor="#f04a3c" />
        </linearGradient>
      </defs>
      <rect x="0" y="0" width="160" height="160" rx="36" fill="url(#ic-logo-tile)" />
      <rect
        x="1"
        y="1"
        width="158"
        height="158"
        rx="35"
        fill="none"
        stroke="rgba(255,255,255,0.10)"
        strokeWidth="2"
      />
      <g transform="translate(80 72)">
        <path
          d="M0 -40 C 4 -14, 14 -4, 40 0 C 14 4, 4 14, 0 40 C -4 14, -14 4, -40 0 C -14 -4, -4 -14, 0 -40 Z"
          fill="url(#ic-logo-spark)"
        />
      </g>
      <circle cx="112" cy="116" r="15" fill="#2dd4bf" />
    </svg>
  );
}

/** Frame / artboard tool glyph. */
export function FrameGlyph(props: Props) {
  return (
    <svg viewBox="0 0 16 16" width="16" height="16" {...strokeDefaults} {...props}>
      <path d="M4.5 2v12M11.5 2v12M2 4.5h12M2 11.5h12" />
    </svg>
  );
}

/** Slice tool glyph. */
export function SliceGlyph(props: Props) {
  return (
    <svg viewBox="0 0 16 16" width="16" height="16" {...strokeDefaults} {...props}>
      <path d="M2.5 8a5.5 5.5 0 0 0 5.5 5.5H13L2.5 3v5Z" />
      <path d="M8 8 13 3" strokeDasharray="1.6 1.6" />
    </svg>
  );
}

/** Vector node editing glyph. */
export function NodeGlyph(props: Props) {
  return (
    <svg viewBox="0 0 16 16" width="16" height="16" {...strokeDefaults} {...props}>
      <rect x="1.5" y="5" width="3" height="3" rx="0.6" />
      <rect x="11.5" y="5" width="3" height="3" rx="0.6" />
      <path d="M4.5 6.5h7" />
      <circle cx="8" cy="2.5" r="1" />
      <path d="M8 3.5v6" />
    </svg>
  );
}

/** Connector glyph. */
export function ConnectorGlyph(props: Props) {
  return (
    <svg viewBox="0 0 16 16" width="16" height="16" {...strokeDefaults} {...props}>
      <circle cx="3" cy="12.5" r="1.6" />
      <circle cx="13" cy="3.5" r="1.6" />
      <path d="M4.5 11.5 12 4.5" />
      <path d="m9.5 3.5 3.5 0 0 3.5" />
    </svg>
  );
}

/** Group glyph. */
export function GroupGlyph(props: Props) {
  return (
    <svg viewBox="0 0 16 16" width="16" height="16" {...strokeDefaults} {...props}>
      <rect x="2" y="2" width="9" height="9" rx="1" />
      <path d="M5 14h9V5" />
    </svg>
  );
}

/** Ungroup glyph. */
export function UngroupGlyph(props: Props) {
  return (
    <svg viewBox="0 0 16 16" width="16" height="16" {...strokeDefaults} {...props}>
      <rect x="1.5" y="1.5" width="6" height="6" rx="1" />
      <rect x="8.5" y="8.5" width="6" height="6" rx="1" strokeDasharray="2 2" />
    </svg>
  );
}

/** Mask glyph. */
export function MaskGlyph(props: Props) {
  return (
    <svg viewBox="0 0 16 16" width="16" height="16" {...strokeDefaults} {...props}>
      <circle cx="6" cy="8" r="4" />
      <circle cx="10" cy="8" r="4" />
    </svg>
  );
}

export function BooleanUnionGlyph(props: Props) {
  return (
    <svg viewBox="0 0 16 16" width="16" height="16" {...strokeDefaults} {...props}>
      <rect x="1.5" y="1.5" width="9" height="9" rx="1" />
      <rect x="5.5" y="5.5" width="9" height="9" rx="1" fill="currentColor" fillOpacity="0.22" />
    </svg>
  );
}

export function BooleanSubtractGlyph(props: Props) {
  return (
    <svg viewBox="0 0 16 16" width="16" height="16" {...strokeDefaults} {...props}>
      <rect x="1.5" y="1.5" width="9" height="9" rx="1" />
      <rect
        x="5.5"
        y="5.5"
        width="9"
        height="9"
        rx="1"
        strokeDasharray="2 2"
        fill="#101318"
      />
    </svg>
  );
}

export function BooleanIntersectGlyph(props: Props) {
  return (
    <svg viewBox="0 0 16 16" width="16" height="16" {...strokeDefaults} {...props}>
      <rect x="1.5" y="1.5" width="9" height="9" rx="1" />
      <rect x="5.5" y="5.5" width="9" height="9" rx="1" />
      <rect x="5.5" y="5.5" width="5" height="5" fill="currentColor" fillOpacity="0.35" stroke="none" />
    </svg>
  );
}

export function BooleanExcludeGlyph(props: Props) {
  return (
    <svg viewBox="0 0 16 16" width="16" height="16" {...strokeDefaults} {...props}>
      <rect x="1.5" y="1.5" width="9" height="9" rx="1" />
      <rect x="5.5" y="5.5" width="9" height="9" rx="1" />
      <rect x="5.5" y="5.5" width="5" height="5" fill="#101318" stroke="none" />
    </svg>
  );
}

/** Component glyph (diamond-in-square) used in the panel vocabulary. */
export function ComponentGlyph(props: Props) {
  return (
    <svg viewBox="0 0 16 16" width="16" height="16" {...strokeDefaults} {...props}>
      <rect x="1.5" y="1.5" width="13" height="13" rx="1.5" />
      <path d="M8 4.5 11.5 8 8 11.5 4.5 8Z" />
    </svg>
  );
}

export function InstanceGlyph(props: Props) {
  return (
    <svg viewBox="0 0 16 16" width="16" height="16" {...strokeDefaults} {...props}>
      <rect x="1.5" y="1.5" width="13" height="13" rx="1.5" strokeDasharray="2.5 2" />
      <path d="M8 5.5 10.5 8 8 10.5 5.5 8Z" />
    </svg>
  );
}

/** Generic "empty" pictogram for panels and the canvas. */
export function EmptyGlyph(props: Props) {
  return (
    <svg viewBox="0 0 24 24" width="24" height="24" {...strokeDefaults} {...props}>
      <rect x="3.5" y="3.5" width="17" height="17" rx="2.5" strokeDasharray="2.5 2.5" />
      <path d="M8 12h8M12 8v8" />
    </svg>
  );
}

export { Pencil } from "lucide-react";
