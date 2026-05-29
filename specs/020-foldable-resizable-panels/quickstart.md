# Quickstart — Foldable & resizable panels: dev + QA

Audience: maintainer + contributors working on this feature or extending it.

## Run the page

```bash
cd web && trunk serve --open
```

After this feature ships, the five panels (Editor / Stacks / Heap / VTABLES / Static) each have a fold button in their header. Dividers between panels are draggable. A "⟲ Reset" button in the page header restores defaults.

## Run all tests

```bash
cargo test
```

181 tests should continue to pass byte-identical — this feature touches only `web/*` files, no Rust code, no event protocol changes.

## Manual QA procedure

~10 minutes. Verifies the feature end-to-end across the three user stories + edge cases.

### 1. First-visit defaults

- Open dev tools → Application → Local Storage → confirm `rustviz.panel-layout.v1` is absent.
- Load the page. All five panels visible at default widths (Editor 25%, Stacks 30%, Heap 25%, VTABLES 10%, Static 10%).
- No fold-button icon should be highlighted; all panels are unfolded.

### 2. US1 — Fold a panel

- Click the fold button on the Editor panel header.
- Editor collapses to a ~28px sliver showing "EDITOR" as vertical text + a `›` unfold chevron.
- The freed horizontal space redistributes to the other four panels.
- Open dev tools → Local Storage. Confirm `rustviz.panel-layout.v1` now exists with `panels.editor.folded === true`.
- Click the sliver. Editor restores to its prior width.

### 3. US1 — User override of auto-collapse

- Load a sample with no static-memory content (e.g. `Box (M07)`). STATIC panel auto-collapses to its sliver.
- Click the STATIC sliver. Panel unfolds to default width.
- Load a different sample with no static memory. STATIC panel STAYS unfolded (user override sticks).
- Check `rustviz.panel-layout.v1`: `panels.static.user_override === true`.
- Click the fold button on STATIC. Panel folds; `user_override` clears.

### 4. US1 — Auto-unfold on parse error

- Fold the Editor panel.
- Edit the source to introduce a parse error (e.g. add `let x = ;`).
- Status bar shows the parse error. Editor panel AUTO-UNFOLDS so the error span + message are visible.
- Fix the error. Source compiles. Editor RE-FOLDS per the user's preference (last persisted state).

### 5. US2 — Drag-resize between panels

- Hover the divider between Editor and Stacks. Cursor changes to `col-resize`; divider highlights.
- Drag right by ~100px. Editor shrinks; Stacks grows. Live (no rubber-banding).
- Release. Check `rustviz.panel-layout.v1`: both `editor.width_pct` and `stacks.width_pct` have updated.
- Reload the page. Widths preserved.

### 6. US2 — Minimum-width clamp

- Drag the Editor/Stacks divider hard left until Editor reaches 120px. Continue dragging left — Editor snaps to 120px (doesn't go below).
- Release. `editor.width_pct` reflects the 120px floor as a pct of `<main>` width.

### 7. US3 — Reset layout

- Fold the Editor; resize Editor/Stacks divider; unfold STATIC (override flag set).
- Click the `⟲ Reset` button in the page header.
- All panels return to defaults (unfolded, default widths). `localStorage` key is removed.

### 8. Storage failure path

- In dev tools, disable localStorage (or open the page in incognito mode with restricted storage).
- Fold a panel, drag a divider. Layout works for the session.
- Reload. Layout resets to defaults (no persistence). No console error other than the documented one-time `console.warn`.

### 9. Existing samples regression check

- Cycle through ~5 samples spanning M01 → M08 at default layout.
- Each renders correctly (no slot/heap/vtable layout regression). Arrow overlay still aligns.

### 10. Schema-mismatch fallback

- In dev tools, set `localStorage.setItem("rustviz.panel-layout.v1", '{"version": 2}')`.
- Reload. Layout uses defaults. Console shows the documented `console.warn`.

## Developer notes

### Why localStorage and not sessionStorage / IndexedDB?

`localStorage` persists across sessions (matches the user's "set it once and forget" expectation). `sessionStorage` would reset on tab close. IndexedDB is overkill for a single small JSON blob (~150 bytes).

### Why a single key, not per-panel keys?

Atomic writes. Reading 5 keys on every load is slower and less robust (partial-update races between fold+drag are possible). A single JSON blob is atomic via `setItem`.

### Why `flex-basis` and not CSS grid?

Minimum-change. The current layout is flex; switching to grid would require re-checking the `<section>`-internal CSS (slot grids, heap-cell strips, etc.). Flex with `flex-basis` is sufficient for horizontal panel widths.

### Why pointer events (not mouse + touch)?

`pointerdown` + `setPointerCapture` handles mouse, pen, and touch uniformly. `setPointerCapture` keeps the drag alive even if the cursor leaves the divider element — robust against the common "drag escapes the handle" bug.

### Why the user-override flag, not just disabling auto-collapse?

Auto-collapse for empty STATIC/VTABLES is valuable as a DEFAULT — first-time users on M01 samples shouldn't see two empty panels. The user override is a per-panel opt-out that sticks across reloads but is local to that panel. A global "disable auto-collapse" would over-correct and remove the feature for users who never opted in.

### How is the editor auto-unfold reverted?

The auto-unfold doesn't touch the persisted `folded` state. It only clears the runtime CSS class. On the next successful parse, `renderUi()` re-applies the persisted state — the editor re-folds if it was folded before the error.

### Reset button placement

Top of the page header, BEFORE the "Sample:" label. Small button with "⟲ Reset" label. Reduces visual weight while staying discoverable.

## When extending in future iterations

- **Drag-to-reorder**: would add a `panel_order: string[]` field to the schema. Bump to v2.
- **Vertical resize**: would add a `height_pct` field per panel. Bump to v2 if needed.
- **Touch / pinch**: the existing pointer-event handlers should cover most cases; tablet-specific tweaks (long-press to fold? swipe?) would need design exploration.
- **Per-sample layouts**: a `layouts: { [sample_id]: LayoutState }` field would enable per-sample preferences. Bump to v2.

## What this iteration does NOT add

- Drag-to-reorder panels — out of scope.
- Vertical (height) resize — out of scope.
- Touch / pinch gestures — out of scope (mouse + pen + touch via pointer events; no gesture handling).
- Mobile responsive (< 1024px viewport) — out of scope.
- Per-sample layouts — out of scope.
- Cloud sync / cross-device — out of scope.
- Undo/redo of layout changes — out of scope (single Reset escape hatch).
- Keyboard arrow-key resize on focused divider — stretch goal, plan-phase decides.
