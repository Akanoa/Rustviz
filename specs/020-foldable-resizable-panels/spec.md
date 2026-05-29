# Feature Specification: Foldable & resizable panels

**Feature Branch**: `020-foldable-resizable-panels`
**Created**: 2026-05-29
**Status**: Draft
**Input**: User description: "allow panel to be foldable and resizable"

**Authority**: ergonomics polish on top of M08 v1 — no protocol changes; pure UI-shell enhancement. No MILESTONES.md entry (UX iteration, not a Rust-mechanism milestone).

## User Scenarios & Testing *(mandatory)*

The page currently lays out five panels horizontally — **Editor**, **Stacks** (one column per live thread), **Heap**, **VTABLES**, **Static memory** — in a fixed flex ratio. Two existing pain points motivate this feature:

1. **Wasted space when a panel is unused**: M01-M06 samples don't use vtables or static memory; multi-thread M08 samples need more horizontal room for stacks; learners focused on the heap want to ignore the editor for a while. A previous polish (commit `1d8a9e4`) auto-collapses STATIC + VTABLES when empty to a 28px sliver, but the user has no way to override that decision or apply it to OTHER panels (editor, heap).
2. **Important panels feel cramped**: when stacks grows to 2-3 columns, frame cards compete with the editor for screen real estate. Today the user can't trade editor width for stacks width — the flex ratios are hard-coded.

The fix lets the user (a) **fold any panel manually** to recover space, and (b) **drag the dividers between adjacent panels to resize them**. State persists across reloads so a learner who prefers stacks+heap with a hidden editor keeps that layout next session.

### User Story 1 — Fold any panel to reclaim space (Priority: P1)

A learner is studying an M07.7 trait-object program and wants to focus on the VTABLES + heap. They click a fold button on the editor panel; the editor collapses to a thin sliver showing only a label and a "show" affordance; the saved horizontal space redistributes to the remaining panels. Clicking the sliver (or its affordance) restores the panel to its previous width.

**Why this priority**: this IS the foundational pedagogy of the feature — manual user control over panel visibility. Without it, the only way to hide a panel is the auto-collapse heuristic, which only fires for STATIC/VTABLES when empty. P1.

**Independent Test**: load any sample, click the fold button on the Editor panel, observe it collapses to a ~30px sliver showing only "EDITOR" vertical-text label + a chevron. Click the sliver, observe the panel restores to its prior width.

**Acceptance Scenarios**:

1. **Given** any panel is visible at its default width, **When** the user clicks the panel's fold button, **Then** the panel collapses to a thin (≤ 30px) sliver showing only its label + a "show" affordance; the freed horizontal space redistributes to non-folded sibling panels.
2. **Given** a panel is folded, **When** the user clicks the sliver (or its affordance), **Then** the panel restores to the width it had immediately before being folded.
3. **Given** multiple panels are folded, **When** the page is reloaded, **Then** each panel's folded/unfolded state is restored from the previous session.
4. **Given** an auto-collapsed panel (STATIC / VTABLES with no content), **When** the user explicitly unfolds it, **Then** the user's choice OVERRIDES the auto-collapse heuristic for the remainder of the session (the panel stays unfolded even if its content becomes empty).
5. **Given** the Editor is folded, **When** the user opens a sample with a parse error, **Then** the Editor auto-unfolds so the error span and message are visible (errors trump the user's fold preference for visibility).

---

### User Story 2 — Resize adjacent panels by dragging the divider (Priority: P1)

A learner running an M08 multi-thread sample wants more room for the stacks panel. They hover the divider between Editor and Stacks; the cursor changes to a horizontal-resize indicator; they drag the divider rightward. The editor shrinks; stacks grows; both panels' internal contents (code lines, frame cards) re-layout in real time during the drag. State persists across reloads.

**Why this priority**: the resize lets a learner allocate space to whatever they're focused on. Without it, the fold feature alone is a binary choice (full vs. sliver). Resize gives continuous control. P1.

**Independent Test**: load any sample, hover the divider between two panels, observe the cursor changes to `col-resize`. Drag rightward, observe the left panel shrinks and the right panel grows in real time. Release, reload the page, observe the widths are preserved.

**Acceptance Scenarios**:

1. **Given** two adjacent visible panels, **When** the user hovers the divider between them, **Then** the divider visually highlights AND the mouse cursor changes to a horizontal-resize indicator.
2. **Given** the user drags a divider, **When** the drag is in progress, **Then** both adjacent panels resize live (no rubber-banding to the release point) — the user can see content reflow during the drag.
3. **Given** the user releases a drag, **When** the page is later reloaded, **Then** the panel widths are restored from the previous session.
4. **Given** the user drags a divider past a minimum-width threshold (~120px), **When** they release, **Then** the panel that would have gone below the minimum is automatically folded instead of allowed to shrink below readable width.
5. **Given** a panel is folded, **When** the user attempts to resize the divider adjacent to it, **Then** the divider is disabled (no-op or unfolds the panel first — implementation choice deferred to plan).

---

### User Story 3 — Reset to defaults (Priority: P2)

A learner has experimented with fold + resize and wants to return to the default layout. A single "reset layout" affordance (e.g. a small button in the page header, or a right-click context menu on any divider) restores all panels to unfolded + their default widths.

**Why this priority**: escape hatch for a user who has messed up the layout. Not load-bearing — the user could manually unfold + resize, but it's annoying. P2.

**Independent Test**: fold a few panels, resize a divider, then click "reset layout", observe all panels return to defaults.

**Acceptance Scenarios**:

1. **Given** the user has folded panels and/or resized dividers, **When** they trigger the reset affordance, **Then** all panels return to unfolded + their default widths AND the saved state is cleared.

---

### Edge Cases

- **All panels folded at once** — out of scope to prevent. At minimum the central content area shows a row of slivers; the user can unfold any panel to recover.
- **Browser window too narrow to fit even the slivers** — the divider drag is constrained by the window width; below a critical threshold the slivers can horizontally scroll inside the main row.
- **First visit (no saved state)** — defaults: Editor 25%, Stacks 30%, Heap 25%, VTABLES 10%, Static 10%. All panels unfolded.
- **State saved by previous version (before fold)** — gracefully fall back to defaults if the saved structure can't be parsed.
- **Touch / pinch resize on a tablet** — out of scope for this iteration (mouse only).
- **Keyboard accessibility for folding** — the fold button is a real `<button>` reachable by Tab.
- **Keyboard accessibility for resizing** — dividers expose arrow-key resize when focused (left/right arrows nudge by 16px). Stretch goal — implementation-dependent.
- **Resizing during playback (Play mode)** — playback continues during the drag; the divider drag doesn't pause stepping.
- **Resizing while an animation is mid-flight** (e.g. M08 thread-column slide-in) — the animation completes uninterrupted; subsequent frames use the new width.
- **The stacks panel's per-thread column layout** — when stacks panel is resized narrower, the existing `flex: 1 1 auto; min-width: 280px; max-width: 520px` rules continue to govern column sizing. If multiple columns can no longer fit, the existing horizontal scroll kicks in.
- **Vertical resize** — out of scope. Only horizontal panel widths are adjustable.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST display a fold button on each panel's header that, when clicked, collapses the panel to a thin sliver showing only a label + an "unfold" affordance.
- **FR-002**: System MUST restore a folded panel to its previous (pre-fold) width when the user clicks the sliver or its unfold affordance.
- **FR-003**: System MUST display a draggable divider between each pair of adjacent visible panels.
- **FR-004**: System MUST give visual feedback (highlight + cursor change) when the user hovers a divider.
- **FR-005**: System MUST resize both adjacent panels live during a divider drag (no rubber-banding).
- **FR-006**: System MUST persist fold/unfold state per-panel across page reloads (per browser).
- **FR-007**: System MUST persist resized panel widths across page reloads (per browser).
- **FR-008**: System MUST provide a "reset layout" affordance that clears all saved state and restores default widths + all panels unfolded.
- **FR-009**: System MUST keep a panel that would shrink below the minimum readable width (~120px) folded rather than allowing arbitrarily narrow widths.
- **FR-010**: System MUST honor the user's explicit unfold of a panel even when the auto-collapse heuristic would otherwise fold it (user choice overrides auto-collapse for the session).
- **FR-011**: System MUST auto-unfold the Editor panel when a parse error is shown (errors trump the user's fold preference for visibility of the error message + span).
- **FR-012**: System MUST not interfere with existing UI interactions (step-forward/back buttons, sample selector, slot hover, arrow rendering) — all existing flows continue to work regardless of panel widths or fold state.

### Key Entities

- **Panel state**: per panel (Editor, Stacks, Heap, VTABLES, Static), a tuple `(folded: bool, restore_width_pct: number)`. `folded` reflects current fold state; `restore_width_pct` is the width to use when unfolding (captured at last fold).
- **Layout state** (overall): an ordered list of panel states + a schema version (for graceful fallback on schema mismatch).
- **Saved state**: persisted via the browser's local storage. One key, JSON-serialized.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After this feature ships, the user can fold any of the 5 main panels with a single click. The panel collapses within 100ms (no perceptible delay).
- **SC-002**: After this feature ships, the user can resize any pair of adjacent panels by dragging the divider between them. The resize is live (no rubber-banding).
- **SC-003**: After folding panels and resizing dividers, reloading the page restores the same layout (within 1px tolerance for divider positions).
- **SC-004**: A single "reset layout" action restores defaults in under 100ms.
- **SC-005**: All M01-M08 samples remain usable at the default layout (no regression in slot/heap/vtable rendering).
- **SC-006**: A panel folded by the user STAYS folded across cursor steps (the auto-collapse heuristic for STATIC/VTABLES doesn't unfold the user's intentional fold; conversely, the user's explicit unfold survives content becoming empty).
- **SC-007**: Editor auto-unfolds when a parse error fires (errors are always visible).
- **SC-008**: WASM bundle growth ≤ +3% vs the post-M08 polish baseline (~440 KB → ≤ ~453 KB). The fold + resize machinery is JS+CSS-only with no Rust-side changes expected; the budget is generous to allow plan-phase flexibility.
- **SC-009**: Zero new Rust warnings under `RUSTFLAGS="-D warnings" cargo build --release`. All 181 existing tests continue to pass byte-identical (UI-shell changes don't touch the event protocol).

## Assumptions

- **Pure UI-shell feature**: no protocol changes, no eval-side changes, no new `MemEvent` variants. JS + CSS + index.html only. Bundle budget reflects this (≤ +3%).
- **Per-browser persistence via local storage**: standard web pattern; no cookies, no server-side state, no sync across devices. Resets when the user clears site data.
- **Default layout numbers (Editor 25% / Stacks 30% / Heap 25% / VTABLES 10% / Static 10%)** are a recommendation — the plan may tune them.
- **Minimum panel width**: 120px before auto-fold kicks in. This protects against panels becoming unreadably narrow.
- **Fold transitions are instant** (snap), not animated, to keep the interaction snappy. A 100-150ms ease-out transition is acceptable if it doesn't feel sluggish — plan-phase decision.
- **Unfold restores the LAST-USED width, not the default**: if the user resized Stacks to 40% then folded it, unfolding restores to 40%, not 30%. Matches user intuition of "fold = hide temporarily".
- **No drag-to-reorder**: panels stay in their fixed order (Editor → Stacks → Heap → VTABLES → Static). Reordering is out of scope.
- **The existing auto-collapse heuristic (STATIC/VTABLES empty → sliver)** continues to work but is now SUBORDINATE to the user's explicit fold choice. If the user explicitly unfolded an empty STATIC panel, it stays unfolded.
- **Mouse-only**: touch and keyboard arrow-key resize are out of scope for this iteration (keyboard tab-to-fold-button still works; that's standard `<button>` behavior).
- **No vertical resize**: only horizontal panel widths are adjustable. The stacks/heap/vtables panel heights are governed by the existing flex layout and don't need user control.
- **Reset affordance form**: a small button in the page header is the recommendation. Right-click context menu on a divider is an alternative; plan-phase decision.
- **Saved-state schema versioning**: include a version field in the saved JSON so future iterations can detect old shapes and fall back gracefully. Standard web practice.
- **Storage key namespace**: `rustviz.panel-layout.v1` (or similar). Single key for the whole feature.
- **Accessibility**: fold button is a real `<button>` with `aria-expanded` reflecting fold state. Divider is `<div role="separator" aria-orientation="vertical">`. Keyboard arrow-key resize on focused divider is a stretch goal.
- **Mobile responsive behavior**: out of scope. The visualizer targets desktop browsers (≥ 1024px wide per the existing viewport meta tag).
- **No undo/redo of layout changes**: a single "reset to defaults" is the only recovery mechanism. Multi-level undo is overkill for an experimental UI feature.
- **Sized S** by the project rubric — UI-shell only, no protocol changes, ~200-300 LOC across JS+CSS+HTML. Comparable to the auto-collapse polish in commit `1d8a9e4` but with more interaction surface (drag handling).
- **No UX checkpoint expected**: the design is straightforward (fold button + divider drag are standard web idioms). If implementation reveals visual ambiguity (e.g. how the sliver should look, where the fold button sits in the header) a checkpoint may be added at plan time, but it's not expected.
