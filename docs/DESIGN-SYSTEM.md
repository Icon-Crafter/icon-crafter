# Icon Crafter — visual design system

Brand: **Icon Crafter** — a design platform for people who care about details. Identity:
graphite surfaces, coral energy for primary actions, teal for selection/measurement.
Original mark: four-point concave sparkle in coral with a teal core on a rounded graphite
tile (`brand/brand-mark.svg`, app icons in `apps/desktop/icons`).

The editor must read as a premium desktop tool: dense, calm, high-contrast where it
matters, never a dashboard. The canvas is the visual centre; chrome recedes.

## 1. Color tokens (dark theme, default)

| Token | Value | Use |
|---|---|---|
| `--bg-app` | `#0b0d11` | window backdrop, canvas void |
| `--bg-canvas` | `#101318` | infinite canvas plane |
| `--bg-panel` | `#14171d` | sidebars, toolbars |
| `--bg-panel-raised` | `#191d25` | popovers, menus, dialogs |
| `--bg-input` | `#0f1217` | inputs, segmented controls |
| `--bg-hover` | `#1e232c` | hovered rows/buttons |
| `--bg-active` | `#262c37` | pressed/selected rows |
| `--border-subtle` | `#20242e` | panel separators |
| `--border-default` | `#2a303b` | inputs, buttons |
| `--border-strong` | `#3a4250` | focused inputs |
| `--text-primary` | `#e9ecf2` | primary labels |
| `--text-secondary` | `#a3abba` | secondary labels, hints |
| `--text-tertiary` | `#6f7887` | disabled, placeholders |
| `--accent` | `#ff6b4a` | primary buttons, active tool, brand moments |
| `--accent-hover` | `#ff7f62` | |
| `--accent-pressed` | `#e85536` | |
| `--selection` | `#4fd1c5` | selection outlines, measurement badges |
| `--selection-soft` | `rgba(79,209,197,0.16)` | marquee fill, row selection wash |
| `--danger` | `#f05252` | destructive actions, errors |
| `--warning` | `#f2b84b` | warnings |
| `--success` | `#3ecf8e` | saved, ready-for-dev |
| `--focus-ring` | `rgba(255,107,74,0.55)` | keyboard focus ring (2px outer) |
| `--collab-1..6` | `#4fd1c5 #ff6b4a #9d8cff #f2b84b #3ecf8e #7fb2ff` | collaborator cursors |

Light theme tokens exist as a variable set for prototype previews/website previews, not as
an editor theme (editor ships dark-first; a light editor theme is planned).

## 2. Typography

* UI font: **Inter** (bundled variable font, fallback `Segoe UI`, `system-ui`, sans-serif).
* Monospace: **JetBrains Mono** (bundled, fallback `Cascadia Code`, `Consolas`) — code,
  measurement values, Dev Mode snippets.
* Scale: `11px` micro/caps labels (tracking 0.04em), `12px` dense UI, `13px` default UI,
  `15px` panel titles/toolbar, `20px` dialog titles, `28px` empty-state headings.
  Line heights: 16/18/20/22/24/28.
* Numerals in inputs use `font-variant-numeric: tabular-nums`.
* Text selection color: `var(--selection-soft)`; no text shadows.

## 3. Spacing, radii, elevation

* Spacing scale: `2, 4, 6, 8, 10, 12, 16, 20, 24, 32, 48`. Panels use 8/12px padding,
  rows are 28–32px tall, toolbar buttons 28×28, tool buttons 32×32.
* Radii: `2` (chips/color swatches), `4` (inputs/buttons), `6` (cards, popovers),
  `10` (dialogs), `999` (pills/avatars).
* Elevation: panels = 1px border, no shadow. Popovers/dialogs = `0 12px 32px rgba(0,0,0,0.45)`
  plus `0 0 0 1px #2a303b`. Canvas objects cast their document shadows only.
* Focus: 2px offset ring in `--focus-ring`; never remove keyboard focus indicators.
* Motion: 120ms ease-out for hover/press, 180ms for popovers (transform + opacity),
  240ms for panels. Respect `prefers-reduced-motion`.

## 4. Editor layout (design mode)

```
┌────────────────────────────── toolbar 48px ──────────────────────────────┐
│ [logo] [file title ⌄]  [tool cluster]      [zoom cluster] [share] [present]│
├──────────────┬──────────────────────────────────────────┬────────────────┤
│ left 248px   │                 canvas                    │ right 280px    │
│ pages/layers │   (raster frame + DOM overlay, rulers)   │ properties     │
│ assets/comps │                                          │ prototype/dev  │
├──────────────┴──────────────────────────────────────────┴────────────────┤
│ status bar 24px: [hint]              [selection info]   [zoom] [dirty dot]│
└───────────────────────────────────────────────────────────────────────────┘
```

* Left panel: tabs (Layers · Assets · Components · Pages), searchable, virtualized list,
  inline rename, drag reorder with 8px indent per level, disclosure triangles, type icons.
* Right panel: context-sensitive sections (Layout, Fill, Stroke, Effects, Typography,
  Prototype, Variables, Export). Collapsible, keyboard navigable, number fields with
  scrub-to-drag, arrow-key stepping (shift ×10, alt ×0.1).
* Canvas: floating zoom widget bottom-right (`− 100% +` + fit menu), FPS/perf badge only in
  dev builds; rulers top/left; floating selection toolbar (align, boolean, opacity,
  component) above the selection; in-canvas text editor overlay.
* Command palette: centered 560px wide, blurred backdrop, fuzzy search, grouped results
  ("Actions", "Tools", "Layers", "Styles", "Recent"), keyboard-only operation, `⌘/Ctrl+K`.
* Context menus: 8px padding, 28px rows, right-aligned shortcut hints, destructive item in
  `--danger`; submenus for Fill/Stroke type, boolean ops, component actions.
* Empty states: centered, 2px stroke illustration in `--text-tertiary`, one-line action.

## 5. Iconography

* **lucide-react** (24px grid, 1.75 stroke at 16px) for all chrome icons; custom SVGs only
  for editor-specific concepts (boolean ops, constraints, corner radius, auto-layout
  direction) drawn on the same 24px grid with matching stroke weight.
* Tool icons are always paired with tooltips that include the shortcut.
* No emoji in UI chrome (emoji allowed in comment bodies/user content).

## 6. Component inventory (React)

`Toolbar`, `ToolCluster`, `ZoomCluster`, `FileTitle`, `LeftPanel` (tabs, `LayerTree`,
`PageList`, `AssetGrid`, `ComponentGrid`), `RightPanel` + section primitives (`NumberInput`,
`ColorSwatch`+`ColorPicker`, `Select`, `Segmented`, `Slider`, `Switch`, `CornerRadiusInput`,
`FillList`, `StrokeList`, `EffectList`, `LayoutControls`, `TextControls`, `ConstraintsGrid`,
`ExportControls`), `CanvasStage` (raster blit + overlay canvas + rulers + floating toolbar),
`TextEditorOverlay`, `CommandPalette`, `ContextMenu`, `Tooltip`, `Dialog`, `Toast`,
`StatusBar`, `CollaboratorBar`, `DevModePanel`, `PrototypePanel`, `PresentationShell`.

All are keyboard accessible (roles, labels, roving tabindex for clusters, `aria-*`),
localized-ready (strings centralised in `i18n/en.ts`), and use CSS variables — no magic
hex values inside components.

## 7. Writing tone

UI copy: direct, calm, specific. Buttons are verbs ("Export PNG", "Create component").
Errors say what happened, what was preserved, and the next step ("Couldn't export SVG —
the selection contains a plugin node. Saved nothing; try a frame instead."). Empty states
suggest an action, never a slogan. No exclamation marks.
