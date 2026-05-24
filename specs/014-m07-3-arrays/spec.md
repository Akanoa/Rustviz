# Feature Specification: M07.3 — Arrays (`[T; N]`, stack-allocated sequences)

**Feature Branch**: `014-m07-3-arrays`
**Created**: 2026-05-24
**Status**: Draft
**Input**: User description: "M07.3"

**Authoritative scope source**: [`MILESTONES.md` › M07.3 — Arrays](../../MILESTONES.md). The In scope / Out of scope / Entry criteria / Exit criteria / Demo lines in that block are the contract for this feature. This spec elaborates user stories, requirements, and success criteria; it does not redefine scope.

## User Scenarios & Testing *(mandatory)*

M07.3 introduces Rust's fixed-size, stack-allocated array type `[T; N]` — the natural counterpart to `Vec<T>`. The arithmetic surface is the same (indexing `t[i]`, slicing `&t[1..3]`, `t.len()`), but the storage location is fundamentally different: array bytes live inline in the stack slot, never touching the heap. No `HeapAlloc`/`HeapRealloc`/`HeapFree` events fire; the array's contents vanish naturally with the frame.

This milestone also closes a gap left by M07.1: slicing `&t[1..3]` produces an `&[T]` slice whose `target` is `Pointee::Slot(_)` — the third Pointee variant. M07 built the `Heap` case, M07.2 built the `Static` case, and M07.3 finally exercises the `Slot` case. After this, all three Rust memory regions (stack, heap, static) carry the same slice abstraction.

### User Story 1 - Stack-allocated array literal (Priority: P1)

A learner types `fn main() { let t = [10, 20, 30]; let n = t.len(); }`. The stacks panel shows `t : [i32; 3]` with **inline byte-cells** (12 bytes, three 4-byte elements) right in the slot's value area — distinct from the heap panel, which stays empty (zero heap events for the entire trace). `t.len()` evaluates to `3_u64` at the call.

**Why this priority**: this IS the headline pedagogy. Without arrays-as-stack working, M07.3 hasn't shipped. The visible "bytes live in the stack slot, not on the heap" contrast vs `Vec` is the core lesson. P1.

**Independent Test**: load `m07_3_array_basic.rs`, step past `let t = [10, 20, 30]`, observe inline cells in `t`'s slot, no heap activity. Step past `let n = t.len()`, observe `n: u64 = 3_u64`.

**Acceptance Scenarios**:

1. **Given** `fn main() { let t = [10, 20, 30]; }`, **When** the pipeline runs, **Then** typeck succeeds with `t : [i32; 3]`; the trace contains **zero** `HeapAlloc`/`HeapRealloc`/`HeapFree` events; the stack slot for `t` carries inline byte-cell content (12 bytes for three i32 elements).
2. **Given** `let t = [10, 20, 30]; let n = t.len();`, **When** the pipeline runs, **Then** `n`'s SlotWrite carries `Value::Int { kind: U64, bits: 3 }`. (Compile-time-known size; no runtime lookup needed.)
3. **Given** scope exit, **When** the cursor passes `}`, **Then** `t` disappears with the frame card; no `HeapFree` event fires (nothing was ever heap-allocated).

---

### User Story 2 - Indexing an array (Priority: P1)

A learner types `fn main() { let t = [10, 20, 30]; let x = t[1]; }`. Indexing reads element 1 (value 20). Out-of-bounds index (`t[100]`) emits a `Note { RuntimeError }` with span on the indexing expression.

**Why this priority**: indexing is the second-most-common array operation after construction. Without it, arrays look read-only-by-shape — learners need to see the element extraction work. P1 alongside US1.

**Independent Test**: load `m07_3_array_index.rs`, step past `let x = t[1]`, observe `x : i32 = 20_i32`.

**Acceptance Scenarios**:

1. **Given** `let t = [10, 20, 30]; let x = t[1];`, **When** the pipeline runs, **Then** `x`'s SlotWrite has `Value::Int { kind: I32, bits: 20 }`.
2. **Given** `let t = [10, 20]; let x = t[5];`, **When** the pipeline runs, **Then** a `Note { RuntimeError }` fires with a clear "index out of bounds: array len is 2 but index is 5" message; trace halts.
3. **Given** an empty literal `let t: [i32; 0] = []; let x = t[0];`, **When** the pipeline runs, **Then** runtime error at indexing (length 0).

---

### User Story 3 - Slicing an array (Priority: P1)

A learner types `fn main() { let t = [1, 2, 3, 4]; let s = &t[1..3]; }`. The stacks panel shows `s : &[i32]`. **A blue slice arrow** connects `s`'s slot to `t`'s slot (NOT to a heap block — `t` lives in the stack). The arrow's `[len: 2]` annotation is visible on hover; hovering also lights up cells/bytes 4-11 of `t`'s inline content (elements at indices 1 and 2 = values 20, 30).

**Why this priority**: this is the milestone's structural payoff — `Value::Slice { target: Pointee::Slot(_) }`, the third (and last) Pointee variant the slice abstraction supports. After M07.3 the slice mechanism is complete across all three memory regions. P1.

**Independent Test**: load `m07_3_array_slice.rs`, step past `let s = &t[1..3]`, observe blue slice arrow from `s`'s slot to `t`'s slot (slot-to-slot, NOT slot-to-heap); hover reveals `[len: 2]` + highlights on the covered cells in `t`.

**Acceptance Scenarios**:

1. **Given** `let t = [1, 2, 3, 4]; let s = &t[1..3];`, **When** the pipeline runs, **Then** `s`'s SlotWrite carries `Value::Slice { target: Pointee::Slot(_), len: 2, byte_offset: 4, byte_len: 8, .. }`; the slice arrow renders from `s` to `t`'s slot.
2. **Given** the slice's scope ends, **When** the cursor passes `}`, **Then** the slice arrow disappears alongside `t`'s slot (frame closes).
3. **Given** out-of-bounds slice `let s = &t[1..100];`, **When** the pipeline runs, **Then** a `Note { RuntimeError }` fires with a clear "slice end out of bounds" message.
4. **Given** `s.len()`, **When** the pipeline runs, **Then** returns `2_u64`.

---

### Edge Cases

- **Empty array** `let t: [i32; 0] = [];` — valid; length 0; no cells rendered (or zero cells). `t.len()` returns 0.
- **Single-element array** `let t = [42];` — valid; one inline cell. `t[0]` returns 42.
- **Array of bool** `let t = [true, false, true];` — valid; 3 cells (1 byte each for bool).
- **Large arrays** `let t = [0; 1000]` — out of scope (repeat syntax deferred). The literal-only form `[v1, v2, ...]` is supported, so practical sizes are limited by what a learner would type literally (typically 2-10 elements).
- **Non-primitive element types** `let t = [Box::new(1)];` — out of scope (matches M07's Vec-of-primitives restriction).
- **Multi-dimensional arrays** `[[i32; 3]; 4]` — out of scope.
- **Array as function parameter** `fn takes(t: [i32; 3]) { .. }` — partial scope; plan-phase decides whether `[T; N]` is supported in function signatures. Default: yes (additive typeck only).
- **Array slicing across memory regions** — slicing produces `Value::Slice { target: Pointee::Slot(_) }`; the dangling-borrow scan ignores `Pointee::Slot(_)` (frames disappear atomically; no element-level dangling within an array).
- **Mutating array elements** `let mut t = [1, 2, 3]; t[0] = 5;` — out of scope (extending M06.1's place-expression set to include `Expr::Index` is deferred).
- **Array iteration** `for x in t`, `t.iter()` — out of scope (matches M07.1's slice-method deferrals).
- **Index with negative integer** `t[-1]` — typeck error (index type Int, but value-level negativity caught at eval as out-of-bounds; matches M07's Vec indexing).
- **Element type mismatch in literal** `let t = [1, true]` — typeck error: all literal elements must share a common type.
- **Type-annotation length mismatch** `let t: [i32; 3] = [1, 2];` — typeck error: array length 2 doesn't match annotation length 3.
- **Slicing a `[T; N]` returns `&[T]`, NOT `&[T; M]`** — the slice's static-size info is lost in the borrow; matches Rust semantics.
- **Repeat syntax `[v; N]`** — explicitly out of scope. Reserve future M07.x for ergonomic improvements if a learner case demands it.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST parse array literal expressions `[e1, e2, ..., eN]` and array type annotations `[T; N]` (where N is an integer literal). Repeat syntax `[v; N]` is explicitly out of scope.
- **FR-002**: System MUST extend the type lattice with an array type representation that carries both the element type AND the compile-time-known size N. Exact shape (e.g. `Ty::Array(Box<Ty>, u64)` vs sugar) is plan-phase's call.
- **FR-003**: System MUST typecheck array literals: all elements must share a common type; the literal's inferred size becomes the type's N; type-annotation mismatches (length, element type) produce clear errors.
- **FR-004**: System MUST extend `Expr::Index` typecheck to accept `Ty::Array(T, N)` receivers (in addition to `Ty::Vec(T)`); the result is `T` (element copy).
- **FR-005**: System MUST extend the slice-borrow typecheck to accept array receivers `&t[range]` producing `Ty::Slice(T)`; the result's `Value::Slice` has `target: Pointee::Slot(t_slot)`.
- **FR-006**: System MUST evaluate array literals by writing the element bytes inline into the binding's stack slot. **Zero heap events** fire for arrays at any point in the trace (allocation, indexing, slicing, scope exit).
- **FR-007**: System MUST evaluate `t.len()` on an array as a compile-time-known constant return of the size (as `u64`), without any runtime lookup of the slot's contents.
- **FR-008**: System MUST evaluate `t[i]` (scalar indexing) on an array: bounds-check i against N; on success return element i (a copy); on failure emit `Note { RuntimeError }` with a clear "index out of bounds: array len is N but index is M" message and halt.
- **FR-009**: System MUST evaluate `&t[range]` (range-indexing borrow) on an array: bounds-check the range against N; on success emit `BorrowShared` if appropriate (consistent with M07.2's Static treatment — Slot targets may also skip BorrowShared/BorrowEnd given slots disappear with frames, plan-phase confirms) and return `Value::Slice` with `target: Pointee::Slot(t_slot)`.
- **FR-010**: System MUST render array contents inline in the stack slot — visual byte-cells matching the array's total byte size (`N * elem_size`), filled with the current element values. The visual must be distinguishable from heap-block byte-cells (different color or position) so the learner reads it as "stack memory" not "heap memory".
- **FR-011**: System MUST render slice arrows from a `&t[range]` binding to `t`'s stack slot (slot-to-slot routing, M06-style), distinct from slot-to-heap routing used for `&v[range]` on Vec.
- **FR-012**: System MUST extend the slice hover-highlight (M07.1/M07.2) to work on array-targeted slices — covered byte-cells AND element-spans in the source slot light up on slice-arrow hover.
- **FR-013**: System MUST ship at least 3 new reference programs (`tests/samples/m07_3_*.rs` + `web/samples/`) covering: basic array + `len`, indexing, slicing.

### Key Entities

- **Array type**: a new `Ty` variant representing `[T; N]`. Carries the element type T and the compile-time size N (as `u64`). Distinct from `Ty::Vec(T)` (size unknown at compile time, heap-allocated).
- **Array literal expression**: a new `Expr::ArrayLit { elements: Vec<Expr>, span }` AST node. The number of elements determines the inferred type's N.
- **Array type annotation**: a new `Type::Array { inner: Box<Type>, size: u64, span }` AST type node. Parsed from `[T; <integer literal>]`.
- **Inline array bytes**: the stack slot's `LiveSlot` representation extends (or a new sibling representation) to hold per-array byte content for inline rendering. Plan-phase decides exact UI-side shape.
- **Slot-targeted slice**: `Value::Slice { target: Pointee::Slot(slot_id), .. }` — the slice variant pointing into a stack slot's bytes. First scenario where this is constructed (M07.1 declared the Slot case; M07.2 added Static; M07.3 completes the trilogy).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After M07.3 ships, `let t = [1, 2, 3];` typechecks as `[i32; 3]`; the trace contains **zero** `HeapAlloc`/`HeapRealloc`/`HeapFree` events; the page renders `t`'s slot with 12 inline byte-cells (3 elements × 4 bytes).
- **SC-002**: `t.len()` on a `[T; N]` returns `Value::Int { kind: U64, bits: N }` — verified by automated test.
- **SC-003**: `t[i]` with valid `i` returns the element value as `T` (a copy); out-of-bounds `t[N+k]` emits `Note { RuntimeError }` at the indexing step and halts the trace.
- **SC-004**: `let s = &t[1..3];` typechecks as `&[T]`; produces `Value::Slice { target: Pointee::Slot(_), len: 2, .. }`; the slice arrow renders from `s` to `t`'s slot (slot-to-slot routing); `s.len()` returns `2_u64`.
- **SC-005**: Out-of-bounds array slicing emits `Note { RuntimeError }` and halts the trace.
- **SC-006**: ≥ 3 new `m07_3_*.rs` reference programs ship.
- **SC-007**: Existing M01–M07.2 tests pass byte-identical. M03 snapshots should stay unchanged (L1 samples don't construct arrays, so no `Ty::Array` / `Value::Slice { target: Slot, .. }` is produced in existing traces).
- **SC-008**: WASM bundle growth ≤ +15% vs M07.2 baseline. Small additive surface — one Ty variant, one AST literal node, one AST type node, slot-targeted slice path (parallel to existing Heap/Static paths), inline-cells UI rendering.
- **SC-009**: Zero warnings under `RUSTFLAGS="-D warnings" cargo build --release` AND `cargo test`. Both host and WASM targets clean.

## Assumptions

- **Array element type**: restricted to primitive scalar types (Int, Float, Bool — same as M07's Vec restriction). Element types like `Box<i32>`, nested arrays, or `&str` are out of scope.
- **Array size**: parsed as an integer literal in the type annotation (e.g. `[i32; 3]`). No const expressions, no const generics, no inference from literal length in the annotation form. Inferred from the literal's element count when no annotation is given (`let t = [1, 2, 3]` → `[i32; 3]`).
- **Array literal syntax**: only `[e1, e2, ..., eN]` (comma-separated). Repeat syntax `[v; N]` is explicitly deferred.
- **Annotation length must match literal length**: typeck error on mismatch with a clear message.
- **No mutation through index** `t[0] = 5;` — out of scope. Arrays in M07.3 are read-only after construction. (Extending M06.1's place-expression set to include `Expr::Index` is a separate concern.)
- **No `mut` arrays**: even with `let mut t = [..];`, the only mutation paths available are full reassignment of `t` (already supported as a binding assignment). Index-position writes are deferred.
- **Slot-targeted slices skip BorrowShared/BorrowEnd**: M07.2 established this pattern for `Pointee::Static` — the borrow-lifecycle events are silent no-op cursor steps since static memory never goes dangling. The same logic applies to `Pointee::Slot` — stack slots disappear atomically with their frame; no scope-exit "borrow ended" is meaningful. UI materializes the arrow lazily at SlotWrite time. Plan-phase confirms; this assumption keeps cursor-step counts lean and consistent.
- **Inline byte-cell rendering in stack slot**: the slot's value cell area shows array bytes as a horizontal strip of cells, similar to heap blocks but visually distinct (e.g. gray-tinted to convey "stack memory"). Plan-phase tunes exact styling.
- **Array as function parameter and return type**: allowed at the signature level (`fn takes(t: [i32; 3])`); the call-site evaluation copies the array's bytes into the param slot. No fancy borrow semantics. (Function calls are the existing M03 path; arrays just flow through as copyable values.)
- **`is_copy()` for arrays**: an array is Copy iff its element type is Copy. M07.3 restricts to primitive elements (all Copy), so `Ty::Array(_, _)` is always Copy in M07.3. Future M07.x with non-Copy elements would extend this rule.
- **`Ty::is_copy()` consequence**: passing an array to a function copies its bytes (like primitives); the source binding remains usable. Matches Rust's actual semantics for `[T; N] where T: Copy`.
- **Slicing semantics**: slicing a `[T; N]` produces `&[T]` (size-erased). The slice's `Value::Slice { target: Pointee::Slot(_), byte_offset, byte_len, len, .. }` carries the same fields as Heap-targeted slices.
- **OOB error messages match M07/M07.1 wording for consistency** — "index out of bounds: array len is N but index is M", "slice end out of bounds: end is M, array len is N", etc.
- **No first-class array equality**: `[1, 2, 3] == [1, 2, 3]` is out of scope. Comparison ops on arrays not supported.
- **Bundle target ≤ +15%**: small additive surface — no protocol restructure, no new event variants. Reuses M07.1's slice + range parsing infrastructure.
- **Sized M** per the rubric: ~4 source modules touched (parse/{ast,parser}, typeck, eval, ui) + minor JS for inline-slot rendering + 3 sample pairs + ~6 unit tests. Estimated ~500-700 LOC net change. Smaller than M07 because heap infrastructure isn't touched.
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION because reasonable defaults exist):
  1. **Inline byte-cell visual styling** in the stack slot — gray tint, distinct from heap-block blue cells. Plan-phase tunes.
  2. **Slot-targeted borrow lifecycle** — skip BorrowShared/BorrowEnd same as Static (consistent with M07.2's pattern), or emit them for completeness. Default: skip.
  3. **Whether `[T; N]` in function signatures requires plan-phase typeck work** — likely additive only.
- **Foundation for future work**: M07.3 completes the slice abstraction across all three Pointee variants (Slot, Heap, Static). Future milestones (mutation through index, repeat syntax, iterators, multi-dimensional arrays) can layer on top. M08 (threads) is independent of M07.3.
