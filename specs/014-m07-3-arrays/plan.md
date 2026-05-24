# Implementation Plan: M07.3 — Arrays (`[T; N]`, stack-allocated sequences)

**Branch**: `014-m07-3-arrays` | **Date**: 2026-05-24 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/014-m07-3-arrays/spec.md`

## Summary

Introduce Rust's fixed-size, stack-allocated array type `[T; N]` end to end. Stack-allocated counterpart to `Vec<T>` — same surface (`t[i]`, `&t[1..]`, `t.len()`), different storage (inline in the slot's value-area, no heap event ever fires). Slicing produces `&[T]` with `Value::Slice { target: Pointee::Slot(_), .. }` — the third Pointee variant, completing the slice trilogy (Slot/Heap/Static all built across M07.3/M07/M07.2 respectively).

**7th invocation of the closed-enum-with-revisions rule**: pure additive `Ty::Array(Box<Ty>, u64)` and `Value::Array { elements, elem_ty }`. AST gains one new `Expr::ArrayLit` and one new `Type::Array`. No new MemEvent variants.

Authority chain: `MILESTONES.md` › M07.3 → `spec.md` (this feature) → this plan.

## Technical Context

**Language/Version**: Rust 2024 edition (same toolchain as M01–M07.2). No new toolchain requirements.
**Primary Dependencies**: existing `indexmap`, `serde`, `serde_json`, `wasm-bindgen`, `js-sys`, `console_error_panic_hook`. **No new Rust deps**. **No JS deps changes**.
**Storage**: in-memory; no new files. Array contents live in the eval-side slot's `LocalSlot.value` as a new `Value::Array { elements: Vec<Value>, elem_ty: Ty }` variant. M01/M02/M03 snapshot tests stay byte-identical (existing L1 samples don't construct arrays).
**Testing**: existing `cargo test --test m01 / m02 / m03` should pass byte-identical. New `cargo test --lib pipeline::tests` covering: array literal + len, array indexing, array indexing OOB, array slicing, array slicing OOB, slot-target slice arrow shape. ≥ 6 new tests. Manual M07.3 QA per the SC-008 procedure.
**Target Platform**: same as M01–M07.2 (host + `wasm32-unknown-unknown`).
**Project Type**: Rust library + companion UI. Touches ~4 source modules + SlotRowView extension + JS inline-cell rendering. Sized M.
**Performance Goals**: same pipeline latency budget. Array operations are O(N) at most (slice creation doesn't copy — borrows source slot's bytes).
**Constraints**: M01/M02/M03 byte-identical; WASM bundle ≤ +15% vs M07.2 baseline (~280 KB raw → ≤ 322 KB raw) per SC-008; zero warnings under `-D warnings` (SC-009); existing M06/M07/M07.1/M07.2 features preserved.
**Scale/Scope**: ~4 source modules + minor JS additions + 3 sample pairs + ≥ 6 new unit tests. **Estimated ~500-700 LOC net change**. Sizing: **M** per the rubric.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is still the unfilled speckit template. Same vacuous PASS as features 001–013.

**Post-design re-check**: still vacuous, still PASS.

## Project Structure

### Documentation (this feature)

```text
specs/014-m07-3-arrays/
├── plan.md                 # This file
├── spec.md                 # Feature spec
├── research.md             # Phase 0: 12 design decisions
├── data-model.md           # Phase 1: 1 new Ty variant, 1 new Value variant, 1 new AST Expr + 1 new AST Type, SlotRowView extension
├── quickstart.md           # Phase 1: dev workflow + manual QA procedure
├── contracts/
│   └── m07-3-protocol-delta.md  # Phase 1: 7th closed-enum invocation; slot-targeted slice usage
└── checklists/
    └── requirements.md     # From /speckit-specify
```

### Source Code (repository root) — files M07.3 touches

```text
src/
├── parse/
│   ├── token.rs            # Unchanged — `LBracket`, `RBracket`, `Semi`, `Int` tokens all exist; array syntax reuses them. No new token kinds.
│   ├── ast.rs              # MODIFIED — add `Expr::ArrayLit { elements: Vec<Expr>, span }` AST node; add `Type::Array { inner: Box<Type>, size: u64, span }` AST type. Update `Expr::span()` + `parser::type_span()` to cover both.
│   └── parser.rs           # MODIFIED — `parse_atom`: when seeing `LBracket` at expression-atom position, parse comma-separated expressions, expect `RBracket` → `Expr::ArrayLit`. `parse_type`: when seeing `LBracket`, parse inner type, expect `Semi`, parse integer literal (must be `Int(n, _)` with `n >= 0`), expect `RBracket` → `Type::Array`. Two new entry conditions; both check `LBracket` at the appropriate context.
├── resolve.rs              # MODIFIED — `resolve_expr` adds an `Expr::ArrayLit` arm recursing on each element.
├── typeck.rs               # MODIFIED — add `Ty::Array(Box<Ty>, u64)` variant. Update `Ty::name()` ("[i32; 3]"), `Ty::is_copy()` (Copy iff element is Copy — always true in M07.3 since elements restricted to primitives). Add `ty_from_ast` arm for `Type::Array` → `Ty::Array(inner, size)`. Typecheck `Expr::ArrayLit`: all elements must unify to a common type via existing `try_coerce_to` machinery; result is `Ty::Array(elem_ty, N)` where N = elements.len(). Extend `Expr::Index` typecheck: receiver `Ty::Array(T, _)` → result `T` (parallel to existing Vec case). Extend `typecheck_slice_borrow`: array receiver `&t[range]` returns `Ty::Slice(T)` (size info lost on borrow — matches Rust). Add `(Ty::Array(_, _), "len") → u64` to method dispatch.
├── event.rs                # MODIFIED — add `Value::Array { elements: Vec<Value>, elem_ty: Ty }` variant. Update `Value::type_name()` (returns "[]"). `Pointee::Slot(_)` already exists from M03; no new variant. No new MemEvent.
└── eval.rs                 # MODIFIED — `Expr::ArrayLit` arm in `eval_expr`: eval each element, build `Value::Array { elements, elem_ty }`. Extend `eval_index`: when receiver is `Value::Array { elements, .. }`, bounds-check i against `elements.len()` and return `elements[i].clone()`. Extend `eval_slice_borrow`: when receiver evaluates to `Value::Array { elements, elem_ty, .. }`, derive `target: Pointee::Slot(receiver_slot)` (need to thread receiver's slot through — see research R-006). Skip BorrowShared/BorrowEnd for Slot targets (M07.2 pattern). Extend `eval_method_call` for `(Value::Array, "len")` → `Value::Int { U64, elements.len() as i128 }`. Update `ty_size_bytes` + `value_size_bytes` to handle Array (= N * elem_size).

src/ui.rs                   # MODIFIED — `SlotRowView` gains optional `inline_cells: Option<InlineCellsView>` field for arrays; populated by SlotWrite when value is `Value::Array`. New `InlineCellsView { size, used, elements: Vec<String> }` mirroring the per-byte-cell + per-element rendering used for heap blocks. `render_value` for `Value::Array` returns empty string (inline cells are the visualization). `apply_event` SlotWrite arm for `Value::Slice` with `Pointee::Slot(_)` target: lazy-materialize the borrow with `source_slot` already bound (same pattern as `Pointee::Static` from M07.2). `ArrowTarget::Slot` is already in place from M06.

tests/
├── m01.rs / m02.rs / m03.rs  # Unchanged. Snapshots byte-identical (no L1 sample constructs arrays).
└── samples/
    ├── (existing)            # Unchanged.
    └── m07_3_*.rs            # NEW (3 files): m07_3_array_basic, m07_3_array_index, m07_3_array_slice.

web/
├── samples/                # MODIFIED — add 3 m07_3_*.rs mirrors.
├── index.html              # MODIFIED — dropdown grows 3 entries.
├── index.js                # MODIFIED — `renderStacks()` extends to render inline byte-cells in the slot's value-area when the slot's row carries `inline_cells`. Reuses the per-byte-cell DOM rendering pattern (one `<span class="byte-cell">` per byte, with `byte-used` for filled positions). Uses a distinct CSS hook (`.stack-inline-cells`) so styling can differentiate stack-vs-heap byte-cells. `renderArrows` for slot-target slice arrows: routes from `s`'s slot to `t`'s slot via existing M06 slot-to-slot routing, with the same `[len: N]` annotation + hover-highlight machinery from M07.1/M07.2. Byte-cell highlight extends to query `.stack-inline-cells .byte-cell` (in addition to the existing `.heap-cells` and `.static-cells` queries).
├── style.css               # MODIFIED — `.stack-inline-cells` styling (per-byte cells inside a stack slot's value area; gray-tinted background to distinguish from heap's blue). `.stack-inline-cells .byte-cell.byte-used` filled state, etc.
└── Trunk.toml              # Unchanged.

# M03's contract amended for the 7th closed-enum invocation:
specs/004-m03-event-eval/contracts/m03-api.md   # MODIFIED — note M07.3 as the 7th invocation. Adds `Ty::Array(Box<Ty>, u64)`, `Value::Array { elements, elem_ty }`. Pure additive. No event-variant changes; the Pointee::Slot path (declared in M03) finally exercised by Value::Slice targets.
```

**Structure Decision**: smaller than M07.2. The slice abstraction is fully reused; M07.3's headline is the new value-storage location (inline in stack slot vs Vec's heap) and the slot-targeted slice path that closes the Pointee trilogy. Existing M06 slot-to-slot arrow routing handles the slice arrow rendering with no new SVG work. CSS gets a new variant for stack-side byte-cells.

## Complexity Tracking

> No constitutional violations. Table omitted.

### Notable non-trivial complexity

- **Inline byte-cell rendering in stack slots**: the existing `LiveSlot.value: Option<String>` rendered a textual value. For arrays we need a per-byte visualization equivalent to heap blocks but inside the stack slot. Plan: add a parallel `inline_cells: Option<InlineCellsView>` field with `size`, `used`, `elements: Vec<String>` for per-element display segmentation (so the hover-highlight elem-span path works without parsing display text).
- **Slot-targeted slice borrow lifecycle**: per M07.2's established pattern, `Pointee::Slot` slices skip BorrowShared/BorrowEnd events. The UI materializes the arrow lazily at SlotWrite time when it sees `Value::Slice { target: Pointee::Slot(_), .. }` with no matching world.borrows entry. This generalizes M07.2's static-target lazy-materialization to the slot-target case — same code path, just no Pointee restriction.
- **Array receiver in `eval_slice_borrow`**: the existing helper handles Vec (`Value::Vec`) and Slice (`Value::Slice`) receivers. M07.3 adds an Array arm: `Value::Array { elements, elem_ty }`. The source slot for `Pointee::Slot(_)` target needs to come from the receiver expression's slot — for a direct `&t[range]` where `t` is an `Expr::Ident`, the slot is `lookup_local_slot(binding_id)`. Need to thread this through (look at receiver's AST shape before evaluating).
- **Array element-type unification**: `[1, 2, 3]` all I32 → `Ty::Array(Int(I32), 3)`. `[1, true]` → typeck error (no common type). `[1u8, 2]` → coerces the untyped `2` to `u8`. Reuses the existing `try_coerce_to` machinery; first element's type is the unification target, subsequent elements coerce to it.
- **Array as Copy**: with the M07.3 restriction to primitive elements, every `Ty::Array` is Copy. `let t1 = [1, 2, 3]; let t2 = t1; let x = t1[0];` works (t1 still usable after the copy). Eval clones the `Vec<Value>` for the assignment; no SlotMove fires.
- **`t.len()` evaluation**: returns `N` from the type or `elements.len()` from the value — either works. Use `elements.len()` for symmetry with `Vec::len`; both match.
- **Heap panel stays empty for array-only programs**: a strong pedagogical signal. Verified by SC-001's "zero `HeapAlloc`/`HeapRealloc`/`HeapFree` events" assertion.
- **No new MemEvent variants**: the slice/borrow machinery from M07.1+M07.2 carries all needed semantics. The `Value::Array` variant flows through existing `SlotWrite` events.
- **Parser ambiguity check**: `let t = [1, 2, 3];` and `let s = &v[1..3];` both start with `[`. Disambiguation: `[` at expression-atom position → ArrayLit; `[` at postfix position (after another expression) → Index. The Pratt parser already distinguishes these contexts; no change needed.
