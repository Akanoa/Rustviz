# Feature Specification: M07.1 — Slices (`&[T]`, range indexing, fat pointers)

**Feature Branch**: `012-m07-1-slices`
**Created**: 2026-05-23
**Status**: Draft
**Input**: User description: "M07.1"

**Authoritative scope source**: [`MILESTONES.md` › M07.1 — Slices](../../MILESTONES.md). The In scope / Out of scope / Entry criteria / Exit criteria / Demo lines in that block are the contract for this feature. This spec elaborates user stories, requirements, and success criteria; it does not redefine scope.

## User Scenarios & Testing *(mandatory)*

M07.1 introduces the **slice primitive** — Rust's "view into contiguous memory" represented as a fat pointer (data pointer + length). M07 added Vec/Box/String but had no way to express `&v[1..3]` or any `&[T]` type — the borrow system only knew about whole-binding or whole-allocation targets. M07.1 fills that gap with three connected mechanisms: range expressions, range-indexing on Vec, and a slice type that carries length metadata visible on the borrow arrow. The dangling-borrow detection from M07 extends naturally: a slice into a Vec becomes dangling when that Vec reallocates, same RuntimeError pedagogy.

This is the foundational slice infrastructure that M07.2 (`&str` + static memory) will build on — `&'static str` is just "a slice into the binary's read-only data segment", reusing this milestone's slice-type and fat-pointer visual.

### User Story 1 — Take a partial-range slice of a Vec (Priority: P1)

A learner types `fn main() { let v: Vec<i32> = Vec::new(); /* push some */ let s = &v[1..3]; }`. The stacks panel shows `v : Vec<i32>` and `s : &[i32]`. **A blue borrow arrow** connects `s` to the Vec's heap allocation, annotated with **`[len: 2]`** — making the slice's "view of 2 elements" visible alongside the arrow. The slice arrow stays anchored as long as `s` is in scope.

**Why this priority**: this IS the headline pedagogy. Without partial-range slices working, M07.1 hasn't shipped. The length-annotated borrow arrow is the visual centerpiece. P1.

**Independent Test**: load `m07_1_slice_range.rs`, step past the slice-binding step, observe blue arrow with `[len: 2]` annotation pointing at the Vec's heap block.

**Acceptance Scenarios**:

1. **Given** a populated Vec `v` and the source `let s = &v[1..3];`, **When** the pipeline runs, **Then** typeck succeeds, `s` has type `&[i32]` (or equivalent slice-type representation), and the trace contains a `BorrowShared` event whose target is the Vec's heap allocation with a length metadata of 2.
2. **Given** the page renders the trace at the slice-binding step, **When** the user observes the arrow overlay, **Then** the blue borrow arrow from `s` to the heap block displays a visible **length annotation** (e.g. `[len: 2]` or similar).
3. **Given** the slice's scope ends, **When** the cursor passes the closing `}`, **Then** the slice arrow disappears (BorrowEnd fires).
4. **Given** an out-of-bounds range `let s = &v[1..100];`, **When** the pipeline runs, **Then** at the indexing step a `Note { RuntimeError }` fires with a clear "slice end out of bounds" message.

---

### User Story 2 — Full-slice view of a Vec (Priority: P1)

A learner types `let s = &v[..];` (or equivalent full-range form). The slice covers all of `v`'s current elements. The blue arrow shows `[len: N]` where N matches `v.len()`. Calling `s.len()` returns N (typed as `u64` like `Vec::len`).

**Why this priority**: full-vec slices are the most common idiomatic use (`fn takes_slice(s: &[i32])` called with `&v[..]`). Without it, slices feel incomplete. P1 alongside US1.

**Independent Test**: load `m07_1_slice_basic.rs`, observe the full-Vec slice arrow with length = Vec's len.

**Acceptance Scenarios**:

1. **Given** `let v: Vec<i32> = Vec::new(); v.push(1); v.push(2); v.push(3); let s = &v[..];`, **When** the pipeline runs, **Then** `s` is `&[i32]` and its length annotation is 3.
2. **Given** `let s = &v[..]; let n = s.len();`, **When** the pipeline runs, **Then** `n` is `Value::Int { kind: U64, bits: 3 }`.
3. **Given** all four range forms (`a..b`, `..b`, `a..`, `..`), **When** typechecking, **Then** each parses + typechecks without errors when the receiver is a Vec.

---

### User Story 3 — Slice dangles after Vec realloc (Priority: P1)

A learner types:

```rust
fn main() {
    let mut v: Vec<i32> = Vec::new();
    v.push(1);
    v.push(2);
    let s = &v[..];
    v.push(3);  // realloc: moves bytes
}
```

At `let s = &v[..]` the slice arrow appears. At the third push, the Vec's capacity is exceeded → realloc copies bytes to a new heap addr, frees the old. The slice was pointing at the OLD heap addr → **dangling reference**: a `Note { RuntimeError }` fires with "slice still points at the freed heap chunk".

**Why this priority**: extending M07's dangling-borrow pedagogy to slices is essential — it's the same UB story but at a different granularity (`&v[..]` vs `&v[0]`). Without it, slices look "safer" than single-element borrows when they're not. P1.

**Independent Test**: load `m07_1_slice_dangling.rs`, step past the realloc-triggering push, observe RuntimeError note on the slice-binding span.

**Acceptance Scenarios**:

1. **Given** a slice taken before a Vec realloc, **When** the realloc fires, **Then** a `Note { RuntimeError }` fires with span on the slice-binding's source location.
2. **Given** the slice arrow targeted heap addr `X`, **When** the realloc moves bytes to `Y`, **Then** the slice's arrow either (a) becomes visually "stale" (still pointing at the freed `X`, now grayed) or (b) shows a clear distinction from a healthy borrow. Plan-phase decides the exact visual.

---

### Edge Cases

- **Empty slice** `let s = &v[1..1];` — valid; length 0. Arrow with `[len: 0]` annotation. No special handling needed; integrates with existing borrow infrastructure.
- **Whole-vec slice on empty Vec** `let v: Vec<i32> = Vec::new(); let s = &v[..];` — valid; length 0.
- **Inverted range** `let s = &v[3..1];` — Rust would panic at runtime (`slice end < start`). Emit `Note { RuntimeError }`.
- **Mutable slice `&mut v[1..3]`** — out of scope. Plan-phase rejects at typeck with a clear "mutable slices are out of scope in M07.1" message.
- **Iterator on slice** (`for x in s`, `s.iter()`) — out of scope.
- **Slice methods beyond `len()`** (`first()`, `last()`, `is_empty()`, `contains()`, etc.) — out of scope.
- **Slicing a slice** `let t = &s[0..1];` — out of scope; plan-phase decides whether to typeck-reject or quietly support if the implementation falls out naturally. Default: reject for now.
- **Range expression standalone** `let r = 1..3;` — out of scope. Range expressions only valid inside `[]` brackets for indexing in M07.1.
- **Slice as function argument** `fn takes(s: &[i32]) { ... }` — partial scope; plan-phase decides whether function signatures can declare `&[T]` parameters. Default: yes (slice typecheck only; no fancy lifetime work).
- **Slicing across freed-but-reused memory** — same model as M06's dangling-borrow detection extended to heap addrs. If the slice's target heap addr was freed and the addr's slot got reused (free-list reuse), the slice points at the new allocation. Documented as a known sharp edge — pedagogically interesting ("dangling pointers can sometimes hit valid memory after reuse, hiding bugs").
- **Length annotation visual**: must be small enough not to clutter, large enough to read. Plan-phase confirms. Initial proposal: `[len: N]` as a tiny label near the arrowhead or attached to the arrow's mid-point.
- **Range bounds with non-Int types** (e.g. `&v[1.5..3]`) — typeck rejects with "range bounds must be integer".

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST parse range expressions in all four forms when used inside index brackets: `a..b`, `..b`, `a..`, `..`. AST shape is plan-phase's call (single `Expr::Range` variant vs. four AST cases vs. only-inside-Index).
- **FR-002**: System MUST extend the type lattice with a slice type representation that distinguishes `&[T]` (slice borrow) from `&T` (single-element borrow). The exact Ty shape is plan-phase's call (e.g. `Ty::Slice(Box<Ty>)` distinct from `Ty::Ref { inner, .. }`, or a unified shape with a "is-slice" flag).
- **FR-003**: System MUST extend `Expr::Index` typecheck: when the index expression is a range, return a slice type (`&[T]`) rather than a single element (`T`); when scalar integer, keep existing behavior (return `T`).
- **FR-004**: System MUST extend the borrow expression `&expr[range]` form: the result is a slice borrow of the underlying allocation, not a single-element borrow.
- **FR-005**: System MUST register slice borrows in the existing borrow-tracking infrastructure (M06's eval-side world.borrows). The slice's target is the heap allocation's `HeapAddr`; the slice's length metadata is carried in the borrow record.
- **FR-006**: System MUST detect dangling slices after Vec realloc: the existing M07 dangling-detection scans for `Value::Ref { target: Pointee::Heap(from), .. }` — slices extend the same model. On realloc, all slices into the old addr are flagged dangling.
- **FR-007**: System MUST extend the rendered borrow arrow to display a length annotation when the borrow's type is a slice. Exact visual is plan-phase's call; minimum requirement is the length is visible alongside the arrow.
- **FR-008**: System MUST extend `Vec::len` typecheck so it also accepts slice receivers — `s.len()` returns `u64` for `s: &[T]`. Method dispatch table updated.
- **FR-009**: System MUST detect out-of-bounds slice ranges at the indexing step and emit `Note { RuntimeError }` with a clear message ("slice end out of bounds: vec len is N but slice end is M", or "slice start out of bounds", or "slice start > end"). Editor highlights the range expression.
- **FR-010**: System MUST ship at least 3 new reference programs (`tests/samples/m07_1_*.rs` + `web/samples/`) covering: full-vec slice, partial-range slice, and slice-dangling-after-realloc.

### Key Entities

- **Range expression**: an AST node `Expr::Range { start: Option<Box<Expr>>, end: Option<Box<Expr>>, span }` (or similar). Only valid inside index brackets in M07.1; standalone range expressions are out of scope.
- **Slice type**: a new `Ty` variant representing `&[T]` or `&mut [T]`. Distinct from `Ty::Ref { inner: T, .. }` because the inner is "an unsized sequence of T" not a single T. Plan-phase confirms the shape.
- **Slice borrow value**: a new `Value::Slice { borrow_id, target: Pointee::Heap(addr), len: usize, mutable: bool }` (or extension to `Value::Ref` carrying length metadata).
- **Length annotation (UI)**: an additional rendering element on the SVG arrow overlay when the borrow is a slice. Format and position decided by plan-phase. Minimum: visible in the arrow's vicinity, readable.
- **Slice dangling**: an active slice's target heap addr was freed. Existing M07 dangling-detection extends — the slice's stored `target` (`Pointee::Heap(old_addr)`) no longer matches any live heap object.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After M07.1 ships, `let s = &v[1..3];` typechecks; the trace contains a borrow event with slice metadata; the page renders a slice arrow with a visible length annotation.
- **SC-002**: All four range forms (`a..b`, `..b`, `a..`, `..`) parse and typecheck correctly when used inside index brackets on a Vec receiver.
- **SC-003**: A slice taken before a Vec realloc produces a `Note { RuntimeError }` ("dangling reference: slice ...") at the realloc step. Verified by automated test.
- **SC-004**: Out-of-bounds slice ranges produce a `Note { RuntimeError }` ("slice end out of bounds: ...") at the indexing step. Verified by automated test.
- **SC-005**: `s.len()` on a slice returns the slice's length (as `u64`), distinct from the underlying Vec's length.
- **SC-006**: ≥ 3 new `m07_1_*.rs` reference programs ship.
- **SC-007**: Existing M01–M07 tests pass. M03 snapshots stay byte-identical (existing samples don't construct slice values). The `Value::Ref` shape changes that may be needed for slice metadata are additive or use a new variant; M03's existing samples remain unaffected.
- **SC-008**: WASM bundle growth ≤ +25% vs M07 baseline (~100 KB gzipped → ≤ 125 KB). The slice infrastructure is real new code (parser, typeck, eval) but reuses much of M06/M07's borrow + heap machinery; +25% is generous.
- **SC-009**: Zero warnings under `RUSTFLAGS="-D warnings" cargo build --release` AND `cargo test`. Both host and WASM targets clean.

## Assumptions

- **Slice metadata in Value**: a slice's length is stored in the Value variant (`Value::Slice { len, .. }` or extension to `Value::Ref`). Plan-phase decides between (a) extending `Value::Ref` with optional `len: Option<usize>`, (b) adding a sibling `Value::Slice { ... }`. Either way the borrow tracker treats slices and single-element borrows the same way for aliasing-rule purposes; only the rendering and `len()` method differ.
- **Range expressions only valid in `[]`**: M07.1 doesn't introduce standalone range bindings (`let r = 1..3;`). The Range AST node lives ONLY inside `Expr::Index { index: Range(...), .. }`. Standalone use is a typeck error.
- **Slice type bounds**: the slice's referent type `&[T]` is restricted to `T = primitive scalar` for M07.1, matching M07's "Vec<T> only for primitive T" rule. Mutable slices `&mut [T]` are explicitly out of scope (deferred).
- **Method dispatch extension**: `Vec::len` already exists in M07's method table. M07.1 adds `Slice::len` (same signature, different receiver type). No new methods on slices beyond `len()`.
- **Dangling-borrow detection model**: M07's existing dangling-detection scans all Value::Ref-shaped values with `target = Pointee::Heap(freed_addr)`. M07.1 ensures slice values also live in slots where this scan finds them — the same RuntimeError fires.
- **Length annotation visual**: rendered as a small text label on the arrow overlay, positioned mid-arrow or near the arrowhead. Plan-phase tunes the styling. Critically, slice arrows must be visually distinguishable from single-element borrow arrows so learners see "this is a many-element view, not a one-element pointer".
- **Range indexing for non-Vec types**: M07.1 supports range indexing only on `Vec<T>` and `&[T]` receivers. Strings, arrays, references-to-arrays — out of scope.
- **Function signatures with slice params**: `fn takes(s: &[i32]) { ... }` is supported at the signature level (typeck parses + recognizes the type). Calling such a function with a slice argument works through normal type matching. Implementation detail in the slice-type's typeck.
- **No length-metadata in borrow events**: the M03 `BorrowShared` event payload does NOT need a `len` field — the slice's Value carries it. The event captures the target (Pointee::Heap(addr)), and the receiver-side Value provides length context.
- **Bundle target ≤ +25%**: real new code for parsing + typeck + visual but no new MemEvent variants and no Ty/Value cascade restructure. Should fit comfortably.
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION because reasonable defaults exist):
  1. **Range AST shape** — single `Expr::Range` variant vs four variants vs only-inside-Index sugar.
  2. **Slice type representation** — `Ty::Slice(T)` distinct from `Ty::Ref` vs unified with `Ty::Ref { inner, is_slice }`.
  3. **Length annotation visual** — text label vs second arrow vs tooltip on hover.
- **Sized L** per the rubric: 4 source modules touched (parse/{ast,parser}, typeck, eval) + UI arrow extension + 3 sample pairs + ~6 unit tests. Estimated ~600 LOC net change. Smaller than M07 because much of the heap infrastructure is reused.
- **Foundational for M07.2**: `&str` is just "a slice into static memory" — M07.2 will reuse this milestone's slice type, length-annotation visual, and borrow infrastructure. The `Ty::Slice` (or equivalent) shape M07.1 picks will be the same shape M07.2 uses for `&str`.
