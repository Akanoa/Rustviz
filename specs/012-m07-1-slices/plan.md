# Implementation Plan: M07.1 — Slices (`&[T]`, range indexing, fat pointers)

**Branch**: `012-m07-1-slices` | **Date**: 2026-05-23 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/012-m07-1-slices/spec.md`

## Summary

Slice infrastructure end to end. Range expressions (`a..b`, `..b`, `a..`, `..`) inside index brackets produce a fat-pointer borrow into a `Vec<T>`'s heap allocation. The slice's length is carried in a new `Value::Slice { borrow_id, target: Pointee::Heap(addr), len, mutable }` variant; rendering shows a blue arrow with a `[len: N]` annotation. `s.len()` is added to the method dispatch table for slice receivers. Out-of-bounds ranges produce `Note { RuntimeError }`. The dangling-borrow detection from M07 fires unchanged when a Vec realloc moves bytes out from under an active slice.

**5th invocation of the closed-enum-with-revisions rule**: `Ty` gains `Slice(Box<Ty>)`; `Value` gains `Slice { borrow_id, target, len, mutable }`. Both purely additive — no restructure. M03 snapshots stay byte-identical.

Authority chain: `MILESTONES.md` › M07.1 → `spec.md` (this feature) → this plan.

## Technical Context

**Language/Version**: Rust 2024 edition (same toolchain as M01–M07). No new toolchain requirements.
**Primary Dependencies**: existing `indexmap`, `serde`, `serde_json`, `wasm-bindgen`, `js-sys`, `console_error_panic_hook`. **No new Rust deps**. **No JS deps changes**.
**Storage**: in-memory; no new files. M01/M02/M03 snapshot tests should stay byte-identical (existing samples don't construct `Value::Slice`, and `Ty::Slice` / `Value::Slice` are additive variants).
**Testing**: existing `cargo test --test m01 / m02 / m03` should pass byte-identical. New `cargo test --lib pipeline::tests` covering: range parsing, slice typing, range-indexed borrow producing slice, `Slice::len()`, out-of-bounds range, slice dangling after realloc, all four range forms. ≥ 7 new tests. Manual M07.1 QA per the SC-008 procedure.
**Target Platform**: same as M01–M07 (host + `wasm32-unknown-unknown`).
**Project Type**: Rust library + companion UI. Touches ~5 source modules + ArrowView extension + SVG length-annotation rendering. Sized L.
**Performance Goals**: same pipeline latency budget. Slice operations are O(1). No new animation paths.
**Constraints**: M01/M02/M03 byte-identical; WASM bundle ≤ +25% vs M07 baseline (905,170 B → ≤ 1,131,463 B uncompressed) per SC-008; zero warnings under `-D warnings` (SC-009); existing M06/M06.1/M07 features preserved.
**Scale/Scope**: ~5 source modules + minor JS additions + 3 sample pairs + ~7 new unit tests. **Estimated ~500-700 LOC net change**. Sizing: **L** per the rubric, on the smaller end (much smaller than M07's 1500 LOC because heap + borrow infrastructure is fully reused).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is still the unfilled speckit template. Same vacuous PASS as features 001–011.

**Post-design re-check**: still vacuous, still PASS.

## Project Structure

### Documentation (this feature)

```text
specs/012-m07-1-slices/
├── plan.md                 # This file
├── spec.md                 # Feature spec
├── research.md             # Phase 0: 14 design decisions
├── data-model.md           # Phase 1: 1 new AST node (Expr::Range), 1 new Ty variant (Slice), 1 new Value variant (Slice), ArrowView extension (annotation)
├── quickstart.md           # Phase 1: dev workflow + manual QA procedure (3 sample walkthroughs)
├── contracts/
│   └── m07-1-protocol-delta.md  # Phase 1: 5th closed-enum invocation; slice value/type additions
└── checklists/
    └── requirements.md     # From /speckit-specify
```

### Source Code (repository root) — files M07.1 touches

```text
src/
├── parse/
│   ├── token.rs            # MODIFIED — add `DotDot` token for `..`. Lexer emits it on two consecutive `.` chars (after the float-arm has consumed any `digit.digit`).
│   ├── lexer.rs            # MODIFIED — recognize `..` as `DotDot`. Two-char lookahead.
│   ├── ast.rs              # MODIFIED — add `Expr::Range { start: Option<Box<Expr>>, end: Option<Box<Expr>>, span }`. Update `Expr::span()`. Standalone `Expr::Range` allowed in AST but typeck-rejected outside `Expr::Index.index`.
│   └── parser.rs           # MODIFIED — recognize `..` inside `[ ]`. Three range entry points: `[ ..` (start = None), `[ expr ..` (start = Some), `[ ..` followed by expr then `]` (end = Some). Use existing primary-expression parser for bounds; reject ranges outside `[ ]` contexts.
├── resolve.rs              # MODIFIED — traversal for `Expr::Range { start, end }`. No new BindingIds.
├── typeck.rs               # MODIFIED — `Ty::Slice(Box<Ty>)` variant. `is_copy()` returns false for Slice. `name()` renders as `"&[T]"` (always immutable in M07.1). Typecheck `Expr::Range` (errors if standalone — only inside `Expr::Index.index`; bounds must be `Ty::Int(_)`). Typecheck `Expr::Index` extended: if index is a Range, result type is `Ty::Slice(elem_ty)` instead of `elem_ty`. Typecheck `Expr::Borrow` extended: if inner is an `Expr::Index { index: Range, .. }`, result is `Ty::Slice(elem_ty)` (NOT `Ty::Ref { inner: Ty::Slice }` — slices are inherently reference-like, the leading `&` is consumed by the slice type). Add `Slice::len` to method dispatch table: `(receiver_ty: Ty::Slice(_), "len") → (&self) -> u64`. Annotation parser recognizes `&[T]` syntax (NEW Type::Slice variant in AST type, mapped to `Ty::Slice`).
├── event.rs                # MODIFIED — `Value::Slice { borrow_id, target: Pointee, len: u64, mutable: bool }`. Update `Value::type_name()` for Slice arm.
├── eval.rs                 # MODIFIED — evaluate `Expr::Range { start, end }`: compute concrete start/end indices (defaults: 0 / receiver.len()). Eval `Expr::Index { index: Range, .. }` separately from scalar index: bounds-check the range (start <= end, end <= receiver.len()), if OOB emit `Note { RuntimeError, .. }` and halt; otherwise emit a `BorrowShared` event with `target: Pointee::Heap(receiver.addr)` and produce a `Value::Slice { borrow_id, target, len: end - start, mutable: false }`. The borrow is registered in `world.borrows` so the existing dangling-detection scan catches it on later realloc. Slice's BorrowEnd fires at scope exit (same path as other borrows).
└── ui.rs                   # MODIFIED — `ArrowView` gains optional `len: Option<u64>` field (default None for non-slice arrows; Some(N) for slices). `World.apply_event` for SlotWrite with `Value::Slice { .. }` builds an ArrowView with `kind: Shared`, `target: ArrowTarget::Heap(addr)`, `len: Some(len)`. `render_value` for `Value::Slice` returns empty string (arrow IS the visualization, like Vec/Box/String — text would clutter).

tests/
├── m01.rs / m02.rs / m03.rs  # Unchanged. Snapshots byte-identical (no existing sample constructs slice values; Ty/Value additions don't change existing variants' Debug output).
└── samples/
    ├── (existing)            # Unchanged.
    └── m07_1_*.rs            # NEW (3 files): m07_1_slice_basic, m07_1_slice_range, m07_1_slice_dangling.

web/
├── samples/                # MODIFIED — add 3 m07_1_*.rs mirrors.
├── index.html              # MODIFIED — dropdown grows 3 entries. No new SVG markers (slice arrows reuse the existing blue-Shared marker).
├── index.js                # MODIFIED — `renderArrows()` reads `arrow.len`; when Some, render a small `<text>` near the arrowhead with content `[len: N]`. Position: ~6-8px above arrow's mid-point.
├── style.css               # MODIFIED — `.arrow-len-label` styling (small font, blue color matching arrow, subtle background for legibility against panel content).
└── Trunk.toml              # Unchanged.

# M03's contract amended for the 5th closed-enum invocation:
specs/004-m03-event-eval/contracts/m03-api.md   # MODIFIED — note M07.1 as the 5th invocation. Purely additive: `Ty::Slice(Box<Ty>)`, `Value::Slice { borrow_id, target, len, mutable }`. No event-variant changes. No restructure.
```

**Structure Decision**: smallest milestone since M06.1. Slice infrastructure rides on M06's borrow machinery + M07's heap state; only Ty, Value, AST, and a single arrow-rendering tweak are new. The length-annotation visual is the headline UI change.

## Complexity Tracking

> No constitutional violations. Table omitted.

### Notable non-trivial complexity

- **Range AST design**: `Expr::Range { start: Option<Box<Expr>>, end: Option<Box<Expr>> }` is the singleton AST node for all four forms (`a..b`, `..b`, `a..`, `..`). Standalone Range expressions parse cleanly but typeck rejects them ("range expressions are only valid inside index brackets in M07.1"). This keeps the AST shape forward-compatible with future support for standalone ranges (e.g. `for i in 1..10`) while constraining scope now.
- **Two parse contexts for `..`**: inside `[ ]` it's a range operator; everywhere else it's an error in M07.1. The lexer emits a `DotDot` token unconditionally; the parser only accepts it within `parse_index`. This mirrors how prefix `*` (deref) is only accepted at expression-start positions in M06.1.
- **Slice typing vs `&` consumption**: in real Rust, `&v[1..3]` produces `&[T]`, NOT `&&[T]`. The slice "absorbs" the leading `&`. M07.1's typeck of `Expr::Borrow { inner: Expr::Index { index: Range, .. } }` short-circuits the normal `&T → Ref(T)` rule and returns `Ty::Slice(T)` directly. This is the cleanest representation but introduces an asymmetry — `Expr::Borrow.inner` doesn't always promote through `Ty::Ref`. Documented in R-006.
- **Length annotation rendering**: SVG `<text>` element positioned at the arrow's midpoint with a small offset perpendicular to the arrow direction. For arrows that route over the heap row (heap-targeted shared arrows go above and descend), the label sits just above the arrow at its mid-X. Per-arrow lane stagger from M07 already provides Y separation; the label inherits the arrow's lane.
- **5th closed-enum invocation**: `Ty::Slice` and `Value::Slice` are additive. The existing `Value::Ref` shape is untouched — slices use a parallel variant rather than overloading `Ref`. This is intentional: (a) slices carry an extra `len` field, (b) keeping slices distinct from single-element borrows at the Value layer makes the rendering decision trivial (`Value::Slice` → annotated arrow; `Value::Ref` → plain arrow), (c) avoids the dangerous "is this borrow a fat pointer? check the Option<len>" shape.
- **Method dispatch for `Slice::len`**: adds one row to the M07 method table — `(Ty::Slice(_), "len") → (&self) -> u64`. The existing `Vec::len` row stays. The dispatcher already handles polymorphic receivers via a match; adding a Slice arm is mechanical.
- **OOB detection at indexing step**: M07 already handles scalar OOB (`v[100]`) via a runtime check + RuntimeError note. M07.1 adds range-OOB checks (`start > receiver.len()`, `end > receiver.len()`, `start > end`). Three distinct error messages.
- **Slice dangling reuse from M07**: the existing dangling scan in `realloc_heap` enumerates `world.borrows` and matches on `target: Pointee::Heap(from)`. Slices register the same way → the scan catches them automatically. **Zero new code needed for the dangling pedagogy** — Value::Slice's borrow_id flows through the same registry.
