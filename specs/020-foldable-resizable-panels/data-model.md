# Data Model — Foldable & resizable panels: entities

Pure UI-shell feature — no Rust-side entities. All state is JS-side, persisted via `localStorage`.

## In-memory state

```ts
// Maintained in a JS module (web/index.js -> panelLayout module).

interface PanelState {
  /** True when the user has explicitly folded this panel. */
  folded: boolean;
  /** Last-used width as a percentage of <main>'s width (0-100). Captured
   *  on every successful drag-resize; used to restore on unfold. */
  width_pct: number;
  /** True when the user has explicitly UNFOLDED a panel that the
   *  auto-collapse heuristic would otherwise fold (`.panel-empty`). Sticks
   *  until "reset layout". Cleared by user folding the panel again. */
  user_override: boolean;
}

interface LayoutState {
  /** Schema version. v1 in this iteration. */
  version: 1;
  /** Per-panel state, keyed by panel id (matches the <section> id). */
  panels: {
    editor:  PanelState;
    stacks:  PanelState;
    heap:    PanelState;
    vtables: PanelState;
    static:  PanelState;
  };
}
```

### Validation rules

- **VR-1**: `version` must equal `1` to load. Otherwise discard + fall back to defaults.
- **VR-2**: `width_pct` clamped to `[5, 95]` to prevent corruption (a panel taking 100% would hide all others).
- **VR-3**: `folded` defaults to `false` if missing/invalid type.
- **VR-4**: `user_override` defaults to `false` if missing/invalid type.
- **VR-5**: missing panel entry → fall back to that panel's defaults; don't discard the whole blob.
- **VR-6**: load is best-effort — any parse error falls back to defaults + logs one `console.warn`.

### State transitions

| Event | Effect |
|---|---|
| User clicks fold button on panel `p` | `p.folded = true`. The current rendered width is captured as `p.width_pct` for restore. Persist. |
| User clicks sliver / unfold affordance on folded panel `p` | `p.folded = false`. If `p` was empty + auto-collapsed (i.e. would re-fold via `.panel-empty`), set `p.user_override = true`. Persist. |
| User drags divider between panels `p` and `q` | During drag: live update flex-basis (px). On release: convert to pct, write to `p.width_pct` and `q.width_pct`. Persist. |
| User clicks "Reset layout" | Clear localStorage key. Reset all panels to defaults. Animate. |
| Parse error fires (renderError) | If `editor.folded === true`, transiently unfold. `user_override` UNCHANGED (per R-007). Don't persist — the next successful parse re-applies the user's preference. |
| User folds a panel that was `user_override = true` | Clear `user_override`. Set `folded = true`. Persist. (Refolding revokes the override.) |

## Defaults

```ts
const DEFAULTS: LayoutState = {
  version: 1,
  panels: {
    editor:  { folded: false, width_pct: 25, user_override: false },
    stacks:  { folded: false, width_pct: 30, user_override: false },
    heap:    { folded: false, width_pct: 25, user_override: false },
    vtables: { folded: false, width_pct: 10, user_override: false },
    static:  { folded: false, width_pct: 10, user_override: false },
  },
};
```

### Validation rules

- **VR-7**: defaults sum to 100% (25 + 30 + 25 + 10 + 10 = 100). Ensures `<main>` is fully utilized at first visit.
- **VR-8**: defaults are unfolded — first-visit users see all panels at recommended widths.

## Persistence contract

```ts
const STORAGE_KEY = "rustviz.panel-layout.v1";

function loadLayout(): LayoutState { /* try/catch JSON.parse */ }
function saveLayout(state: LayoutState): void { /* try/catch localStorage.setItem */ }
function resetLayout(): void { /* localStorage.removeItem(STORAGE_KEY) */ }
```

### Validation rules

- **VR-9**: storage operations wrapped in try/catch. Failure (private mode, full disk, disabled) falls back to in-memory state.
- **VR-10**: `STORAGE_KEY` includes the schema major version (`v1`) so future incompatible changes (`v2`) read from a different key without polluting old saves.

## DOM contract

### `<main>` becomes a flex row of `.panel` wrappers

Before:
```html
<main>
  <section id="editor">...</section>
  <section id="stacks">...</section>
  ...
</main>
```

After:
```html
<main>
  <div class="panel" data-panel="editor">
    <div class="panel-header">
      <span class="panel-title">EDITOR</span>
      <button class="panel-fold-btn" aria-expanded="true" aria-label="Fold editor panel">−</button>
    </div>
    <section id="editor">...</section>
  </div>
  <div class="panel-divider" role="separator" aria-orientation="vertical" data-divider-between="editor,stacks"></div>
  <div class="panel" data-panel="stacks">
    <div class="panel-header">...</div>
    <section id="stacks">...</section>
  </div>
  <div class="panel-divider" ...></div>
  <div class="panel" data-panel="heap">...</div>
  <div class="panel-divider" ...></div>
  <div class="panel" data-panel="vtables">...</div>
  <div class="panel-divider" ...></div>
  <div class="panel" data-panel="static">...</div>
</main>
```

### Validation rules

- **VR-11**: each `<section>` retains its existing id. All existing JS that queries by id still works.
- **VR-12**: arrow SVG overlay (`#arrow-overlay`) lives inside `<main>`, AFTER all `.panel` wrappers. Its `position: absolute` + full-size rule keeps it spanning all panels.
- **VR-13**: each `.panel` has `data-panel="<id>"` for JS lookup; the `.panel-divider` has `data-divider-between="left,right"` for resize-handler routing.

## State classes (CSS)

| Class on `.panel` | Meaning | Source |
|---|---|---|
| `.panel.is-folded` | User explicitly folded — render as 28px sliver. Highest priority. | User click on fold button. |
| `.panel.is-user-overridden` | User explicitly unfolded an empty panel — render full even if `.panel-empty`. | User click on auto-collapsed sliver. |
| `.panel.panel-empty` | Auto-collapse hint from `renderUi()`. Subordinate to the above. | `renderUi()` based on snapshot content. |
| `.panel-divider.is-dragging` | Active drag in progress. Body cursor lock + suppress `.panel` transition. | `pointerdown` handler. |

### Validation rules

- **VR-14**: state class precedence is purely CSS-driven (`:not()` selectors). No JS-driven priority resolution.
- **VR-15**: `body.is-resizing` class added during active drags; locks `cursor: col-resize` and `user-select: none` across the entire page.

## Failure modes

| Failure | Behavior |
|---|---|
| `localStorage` unavailable | Use in-memory state. No persistence. Feature works for the current page session. |
| Saved blob fails to parse | Discard, use defaults. Log one `console.warn`. |
| Saved blob is v0 / v2 | Same as parse failure. |
| Panel entry missing | Use that panel's default. Don't fail the whole load. |
| `width_pct` out of `[5, 95]` | Clamp into range. Don't fail. |
| `setPointerCapture` not supported | Fall back to global `pointermove` + `pointerup` on `window` (less robust but works). |
