# Implementation Plan: M07 — Level 3: Heap (`Box`, `Vec`, `String`)

**Branch**: `011-m07-heap` | **Date**: 2026-05-23 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/011-m07-heap/spec.md`

## Summary

Level-3 lattice expansion: `Box<T>`, `Vec<T>`, `String`. The heap panel (placeholder since M04) comes alive. Four genuinely-new language features land together — string literals, method calls, path expressions, indexing — because Vec and String require all of them. Three M03-reserved `MemEvent` variants (`HeapAlloc`, `HeapRealloc`, `HeapFree`) start being emitted. Owning arrows (black) point from stack slots into heap boxes. The realloc animation (`Vec` doubling its capacity) + dangling-borrow detection (`&v[0]` invalidated by `v.push(...)`) is the headline pedagogy.

**`Value::Ref` restructure**: extends from `target_slot: SlotId` (M06's slot-only assumption) to `target: Pointee` (the M03-declared enum supporting both `Slot` and `Heap`). 4th invocation of the closed-enum-with-revisions rule.

Authority chain: `MILESTONES.md` › M07 → `spec.md` (this feature) → this plan.

## Technical Context

**Language/Version**: Rust 2024 edition (same toolchain as M01–M06.1). No new toolchain requirements.
**Primary Dependencies**: existing `indexmap`, `serde`, `serde_json`, `wasm-bindgen`, `js-sys`, `console_error_panic_hook`. **No new Rust deps**. **No JS deps changes**.
**Storage**: in-memory; no new files. M01/M02 snapshot tests should stay byte-identical. M03 snapshots: the `Value::Ref` restructure changes Debug format of Value::Ref, but M03's L1 samples don't construct `Value::Ref`, so M03 stays byte-identical too. M06 also has no snapshot tests (only pipeline-level unit tests). The cascade is contained.
**Testing**: existing `cargo test --test m01 / m02 / m03` should pass; new `cargo test --lib pipeline::tests` covering Box alloc/free, Vec push (initial alloc + multiple reallocs), Vec indexing, Vec out-of-bounds, dangling-borrow detection, String alloc, String push_str. ≥ 10 new tests. Manual M07 QA per the SC-008 procedure.
**Target Platform**: same as M01–M06.1 (host + `wasm32-unknown-unknown`).
**Project Type**: Rust library + companion M04/M05/M06 UI; touches ~10 source modules + heap panel rendering (new). Largest milestone since M04.
**Performance Goals**: same pipeline latency budget. Heap state operations are O(1) amortized. Realloc animation should complete in ~300ms; renderer must keep up at 60fps during the transition.
**Constraints**: M01/M02 byte-identical; M03 byte-identical (no L1 samples construct heap values or new-shape Value::Ref); WASM bundle ≤ +60% vs M06.1 baseline (88,841 B → ≤ 142,146 B) per SC-007; zero warnings under `-D warnings` (SC-008); existing M06/M06.1 features preserved.
**Scale/Scope**: ~10 source modules + significant new web-side rendering + 3 sample pairs + ~10 new unit tests. **Estimated ~1200-1500 LOC net change**. Sizing: **L** per the rubric, bordering on XL.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is still the unfilled speckit template. Same vacuous PASS as features 001–010.

**Post-design re-check**: still vacuous, still PASS.

## Project Structure

### Documentation (this feature)

```text
specs/011-m07-heap/
├── plan.md                 # This file
├── spec.md                 # Feature spec
├── research.md             # Phase 0: 28 design decisions
├── data-model.md           # Phase 1: 5 new tokens, 4 new AST nodes + 1 Type, 3 new Ty + 4 Value variants + 1 Value restructure, HeapState model, ArrowView rename
├── quickstart.md           # Phase 1: dev workflow + manual QA procedure (3 sample walkthroughs)
├── contracts/
│   └── m07-protocol-delta.md  # Phase 1: 4th closed-enum invocation; Value::Ref restructure; heap event payload usage
└── checklists/
    └── requirements.md     # From /speckit-specify
```

### Source Code (repository root) — files M07 touches

```text
src/
├── parse/
│   ├── token.rs            # MODIFIED — add `Str(String)`, `ColonColon`, `Dot`, `LBracket`, `RBracket`.
│   ├── lexer.rs            # MODIFIED — lex string literals, `::`, `.`, `[`, `]`.
│   ├── ast.rs              # MODIFIED — `Expr::Path`, `Expr::MethodCall`, `Expr::Index`, `Expr::StrLit`; `Type::Generic`. Update `Expr::span()`.
│   └── parser.rs           # MODIFIED — parse string literals; multi-segment paths; postfix `.method(args)` and `[expr]` (both bp ~90); generic-type parsing for `Box<T>`, `Vec<T>`.
├── resolve.rs              # MODIFIED — traversal for new AST nodes (Path/MethodCall/Index/StrLit). No new BindingIds.
├── typeck.rs               # MODIFIED — `Ty::Box/Vec/String` variants. Typecheck Expr::Path (static-fn dispatch table). Typecheck Expr::MethodCall (method dispatch table by receiver Ty). Typecheck Expr::Index (Vec<T> + Int → T). Vec::new() type inference from let-annotation. `is_copy()` returns false for Box/Vec/String.
├── event.rs                # MODIFIED — `Value::Box/Vec/String { addr }`, `Value::Str(String)`. **Restructure** `Value::Ref` (target_slot → target: Pointee).
├── eval.rs                 # MODIFIED — new `heap: HeapState` field. `next_heap_addr` counter. Heap state helpers (alloc, realloc, free, get, set). Evaluate Expr::Path constructors. Evaluate Expr::MethodCall (push triggers HeapRealloc on overflow). Evaluate Expr::Index (bounds-check + element copy). Dangling-borrow Note at HeapRealloc. Scope-exit HeapFree before SlotDrop. update_slot_value / lookup_slot_value adjust for new Value::Ref shape.
├── ui.rs                   # MODIFIED — `HeapView` struct, `StateSnapshot.heap` field. `World.heap` tracking via apply_event for HeapAlloc/Realloc/Free. Rename `BorrowView` → `ArrowView` with `target: ArrowTarget` and `kind: ArrowKind`. `World.borrows` similarly renamed/restructured. Owning arrows (black) registered whenever a slot holds a Box/Vec/String value. render_value cases for Box/Vec/String.
└── lib.rs                  # MODIFIED — re-export HeapView, ArrowView, ArrowKind.

tests/
├── m01.rs / m02.rs / m03.rs  # Unchanged. Snapshots byte-identical (L1 samples don't construct heap values or new-shape Value::Ref).
└── samples/
    ├── (existing)            # Unchanged.
    └── m07_*.rs              # NEW (3 files): m07_box, m07_vec_realloc, m07_string.

web/
├── samples/                # MODIFIED — add 3 m07_*.rs mirrors.
├── index.html              # MODIFIED — remove `<p class="placeholder">` from `#heap`. Add a black-arrow `<marker>` in the SVG `<defs>` for owning arrows. Dropdown grows 3 entries.
├── index.js                # MODIFIED — `renderHeap(state.heap)`: maintains a `Map<heapAddr, HTMLElement>`, creates/removes DOM elements on HeapAlloc/HeapFree. `renderArrows` extends to look up target by `data-slot-id` OR `data-heap-addr`, color based on `arrow.kind`. `state.borrows` reference renamed to `state.arrows`.
├── style.css               # MODIFIED — `.heap-box` styling; `.arrow-owning` class (black); `#heap` flexbox layout; CSS transition on `transform` for realloc animation.
└── Trunk.toml              # Unchanged.

# M03's contract amended for the 4th closed-enum invocation:
specs/004-m03-event-eval/contracts/m03-api.md   # MODIFIED — note M07 as the 4th invocation. M07 adds Ty + Value variants AND restructures Value::Ref. The HeapAlloc/HeapRealloc/HeapFree variants (declared with payloads in M03) start being emitted.
```

**Structure Decision**: largest milestone since M04. The heap panel — a placeholder since M04 — gains real rendering. The borrow-arrow SVG renderer extends to handle heap targets. ~10 source files modified + significant JS additions.

## Complexity Tracking

> No constitutional violations. Table omitted.

### Notable non-trivial complexity

- **Four genuinely-new language features** in one milestone (string literals, method calls, path expressions, indexing). Each is small alone; together they make Phase 2 (Foundational) bigger than usual.
- **`Value::Ref` restructure** (`target_slot: SlotId` → `target: Pointee`) cascades through M06 + M06.1 code paths: eval's borrow construction, ui's apply_event SlotWrite arm, ui's render_value. Estimated 10-15 sites; mechanical.
- **Heap state machine** in eval: `IndexMap<HeapAddr, HeapObject>`. Vec growth doubles capacity (0 → 1 → 2 → 4 → 8 → ...). Dangling-borrow detection at HeapRealloc scans the active-borrow registry.
- **Heap panel DOM model**: maintain a stable `<div>` per `heap_addr` across renders so CSS transitions animate realloc moves. `Map<heap_addr, HTMLElement>` in JS; creates on HeapAlloc, removes on HeapFree, repositions implicitly via flex reflow on capacity changes.
- **Realloc animation**: CSS `transition: transform 300ms ease-out` on `.heap-box`. Works automatically via flexbox reflow when box sizes/positions change. For single-allocation demos there's no visible move; plan-phase R-024 leaves a border-flash polish as a fallback.
- **`ArrowView` rename + restructure** (was `BorrowView`): the JSON wire format change affects `web/index.js` which queries `state.borrows[].target_slot/.mutable` → `state.arrows[].target/.kind`. Documented in m07-protocol-delta.md.
