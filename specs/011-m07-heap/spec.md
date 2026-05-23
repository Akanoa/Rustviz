# Feature Specification: M07 — Level 3: Heap (`Box`, `Vec`, `String`)

**Feature Branch**: `011-m07-heap`
**Created**: 2026-05-23
**Status**: Draft
**Input**: User description: "M07"

**Authoritative scope source**: [`MILESTONES.md` › M07 — Level 3: heap (Box, Vec, String)](../../MILESTONES.md). The In scope / Out of scope / Entry criteria / Exit criteria / Demo lines in that block are the contract for this feature. This spec elaborates user stories, requirements, and success criteria; it does not redefine scope.

## User Scenarios & Testing *(mandatory)*

M07 is **the project's first Level-3 milestone** and the largest single visual addition since M04. The third panel (heap) — a placeholder since M04 — finally comes alive. The lattice grows from "scalars + references between slots" to "scalars + references + heap-allocated owning types (`Box`, `Vec`, `String`)". Three new event variants (`HeapAlloc`, `HeapRealloc`, `HeapFree`) declared with empty payloads in M03 are now emitted by the evaluator. Owning arrows (black, distinct from M06's blue `&` and red `&mut`) point from stack slots into heap boxes. The pedagogical centerpiece is the **realloc animation** that makes `&v[0]`-after-`v.push(...)` viscerally obvious — the box moves in space, the borrow's arrow now points at *invalidated* memory, and a `Note { RuntimeError }` underlines `&v[0]` as a dangling reference.

Three core L3 mechanisms must land together: heap allocation tracking, method-call syntax (`v.push(x)`), and indexing (`v[0]`). String literals (`"hi"`) become a token for the first time. This is genuinely large scope — **plan-phase may decide to split into M07a (Box only, sized M) + M07b (Vec realloc + dangling-borrow + String, sized L)** if mid-implementation sizing exceeds L on any axis. The spec keeps the unified scope for now since MILESTONES.md frames M07 as one block and `do not ship without realloc animation` is the explicit gate.

### User Story 1 — `Box` owning arrow visible (Priority: P1)

A learner types `fn main() { let b = Box::new(5); }`. The stacks panel shows `b : Box<i32>` as a slot. The **heap panel** (previously a placeholder) shows a single box labeled `i32 = 5_i32`. A **black owning arrow** points from `b`'s slot card to the heap box. At end-of-scope, the heap box disappears (HeapFree fires; the owning arrow disappears with it).

**Why this priority**: foundational heap mechanism with the simplest semantics. Without Box working, Vec and String are unreachable (both use Box-like ownership). Establishes the heap panel + owning arrow infrastructure. **MVP candidate**: shippable as a smaller increment if Vec/String defer.

**Independent Test**: load `m07_box.rs`, step through, observe heap box appear with owning arrow from `b`, then disappear at scope close.

**Acceptance Scenarios**:

1. **Given** `fn main() { let b = Box::new(5); }`, **When** the pipeline runs, **Then** typeck succeeds, the evaluator emits a `HeapAlloc` event followed by a `SlotWrite` event landing a `Value::Box { heap_addr, .. }` into `b`'s slot.
2. **Given** the cursor is positioned after the `Box::new(5)` step, **When** the page renders, **Then** the heap panel displays one box labeled `i32 = 5_i32`, AND a black arrow connects `b`'s stack slot to the heap box.
3. **Given** the cursor passes `main`'s closing brace, **When** the page renders, **Then** the heap box is no longer visible (HeapFree fired), AND the owning arrow disappears.
4. **Given** the deref pattern `let v = *b;`, **When** the pipeline runs, **Then** typeck succeeds (Box auto-derefs to its inner type, same convention as M06.1's `&T` deref) and `v` gets `5_i32` (a copy from the heap).

---

### User Story 2 — `Vec` realloc animation + dangling-borrow detection (Priority: P1)

A learner types:

```rust
fn main() {
    let mut v = Vec::new();
    v.push(1);
    v.push(2);
    let r = &v[0];
    v.push(3);
}
```

Stepping through: at `v.push(1)` the heap box appears (size 1 element). At `v.push(2)`, the box may animate if its capacity grew (Vec doubles on growth). At `let r = &v[0]`, a **blue borrow arrow** from `r` into the heap box's first element. At `v.push(3)`, the heap box **animates** to a new position (realloc — the borrow's target memory has been moved) AND a `Note { RuntimeError }` fires: "dangling reference: `r` now points at deallocated memory". The editor underlines `&v[0]` and the cursor halts (or visibly flags the issue without halting; plan-phase decides).

**Why this priority**: this IS the headline pedagogy of M07. Per MILESTONES.md, "do not ship the milestone without it." Vec realloc is the canonical "unsafe in C/C++ would compile; Rust catches at compile time" example. The visualization makes it tangible. P1.

**Independent Test**: load `m07_vec_realloc.rs`, step through, observe (a) heap box appears at `Vec::new` + first push, (b) blue arrow at `&v[0]`, (c) box animates at the realloc push, (d) RuntimeError note underlining `&v[0]`.

**Acceptance Scenarios**:

1. **Given** the source above, **When** the pipeline runs, **Then** the trace contains `HeapAlloc`, multiple `SlotWrite` (element writes), `BorrowShared`, `HeapRealloc`, and `Note { RuntimeError }` events in source-execution order.
2. **Given** the cursor is at the `v.push(3)` step, **When** the page renders, **Then** the heap box's DOM position is observably different from its position at the prior step (animated), AND the borrow's arrow now points at the new heap box (NOT at the freed memory), AND the editor highlights `&v[0]` with a runtime-error underline.
3. **Given** `let v = Vec::new(); let x = v[0];` (indexing an empty Vec), **When** the pipeline runs, **Then** at the `v[0]` step a `Note { RuntimeError }` fires: "index out of bounds: the len is 0 but the index is 0".
4. **Given** `let mut v = Vec::new(); v.push(5); let x = v[0];` (valid indexing), **When** the pipeline runs, **Then** `x` is bound to `5_i32`.

---

### User Story 3 — `String` allocation + push (Priority: P2)

A learner types `fn main() { let mut s = String::from("hi"); s.push_str("!"); }`. The stacks panel shows `s : String`. The heap panel shows a box labeled `"hi"` (or `String[2]` to show length). After `s.push_str("!")`, the heap box either grows in place (if capacity allows) or reallocates with the same animation as Vec, now showing `"hi!"` (or `String[3]`).

**Why this priority**: completes M07's "the three heap types from CLAUDE.md" promise. Pedagogically secondary to Vec realloc (which is more visually dramatic). P2 — could ship M07 with just Box+Vec and defer String to M07.1 if scope pressure demands it.

**Independent Test**: load `m07_string.rs`, step through, observe heap box for the String, and the realloc animation on `push_str` if capacity grows.

**Acceptance Scenarios**:

1. **Given** `let s = String::from("hi");`, **When** the pipeline runs, **Then** the trace contains a `HeapAlloc` event with a payload type indicating String, and `s` is bound to a Value referring to the heap allocation.
2. **Given** the page renders the trace, **When** the cursor is on the `String::from` step, **Then** the heap panel shows a box labeled either `"hi"` or `String[2]` (plan-phase decides display format), with a black owning arrow from `s` to the box.
3. **Given** `let mut s = String::from("hi"); s.push_str("!");`, **When** the pipeline runs and the cursor is at the `push_str` step, **Then** the heap box's content updates to reflect `"hi!"` (or `String[3]`), AND if capacity grew, the box animates to a new heap position.

---

### Edge Cases

- **Borrow of a Box's inner value** (`let r = &*b;`): out of scope. Re-borrows through deref were deferred in M06.1. `&b` (borrowing the Box itself) is in scope and produces a `&Box<i32>` typed reference.
- **`Vec<T>` with non-primitive T**: out of scope. M07 supports `Vec<i32>`, `Vec<bool>`, etc. — primitive scalar element types. `Vec<Box<i32>>` (nested heap types) is deferred to a future milestone.
- **`Box<Box<T>>`** (nested heap): in scope syntactically (Ty::Box recursive), but no specific sample. Plan-phase decides whether to test or restrict.
- **String literals beyond the basics**: M07 needs lexer support for `"..."` literals (no escapes beyond `\n`, `\t`, `\\`, `\"`). Multi-line strings and raw strings (`r"..."`) are deferred.
- **Method calls on non-heap types** (e.g. `i32::pow`): M07 introduces method-call syntax. The minimum surface is `Vec::push`, `Vec::new`, `String::from`, `String::push_str`, `Box::new`. Other built-in methods are deferred.
- **`Vec::with_capacity(n)`**: deferred. Vec starts at capacity 0 and grows on the first push. Plan-phase confirms the growth policy (double on each grow vs. fixed-size increments).
- **Indexing assignment** (`v[0] = 5;`): out of scope. M06.1's assignment lhs is `Ident | Deref(Ident)`. Adding `Index(Expr, Expr)` is a place-expression-set extension that M07 won't take on. Reading `v[0]` is in scope; writing isn't.
- **Iterator methods** (`v.iter()`, `v.into_iter()`, `v.len()`): deferred. Plan-phase may include `v.len()` as a sub-bullet for index bounds checking display.
- **Dangling borrow timing**: when does the RuntimeError fire — at the borrow event (which is still valid in Rust at that moment), at the realloc that invalidates it, or at the next access? Plan-phase decides: most likely at the realloc step, with the editor highlight on the original `&v[0]` span.
- **Multiple borrows into the same Vec, then realloc**: all become dangling. One RuntimeError per dangling borrow, or one aggregated note? Plan-phase decides.
- **Heap box DOM position**: free-form area per MILESTONES.md ("free-form area where each HeapAlloc creates a box"). Plan-phase decides the layout strategy (grid, flexbox, manual positioning).
- **Box size proportional to allocation**: per MILESTONES.md ("size ∝ size, label = type"). Plan-phase decides the visual mapping (linear, log, fixed minimum).
- **Heap box LABEL format**: `i32` (type only), `i32 = 5_i32` (type and value), `5_i32` (value only) for primitives; for Vec it's array-like (`[1, 2, 3]` or `Vec[3]`); for String it's the contents (`"hi"`) or length (`String[2]`). Plan-phase decides per type.
- **Realloc animation timing**: how long is the animation, what's the easing curve? Plan-phase decides; should be fast enough to step through but slow enough to perceive (target ~300ms).
- **Box drop with the binding**: `let b = Box::new(5); { }` — at end of main's scope, HeapFree fires. Non-Copy types (Box is non-Copy) emit SlotDrop in addition to HeapFree.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST extend the lexer to recognize string literals (`"..."` with `\n`, `\t`, `\\`, `\"` escapes). String literal evaluates as a Value (plan-phase decides the Value shape — likely `Value::Str(String)` or routed through `String::from` calls).
- **FR-002**: System MUST extend the parser with method-call syntax: `expr.method_name(arg_list)`. Methods bind tighter than binary operators (postfix call form). Recognized methods are limited to the M07 set (`push`, `push_str`, etc. — plan-phase enumerates).
- **FR-003**: System MUST extend the parser with path expressions: `Type::function(arg_list)`. Recognized paths are limited to `Box::new`, `Vec::new`, `String::from`.
- **FR-004**: System MUST extend the parser with indexing: `expr[index_expr]`. Indexing is rvalue-only in M07 (no `v[0] = ...` assignment).
- **FR-005**: System MUST extend `Ty` with `Box(Box<Ty>)`, `Vec(Box<Ty>)`, and `String` variants (or equivalent shapes — plan-phase decides). These are non-Copy.
- **FR-006**: System MUST extend `Value` with shapes representing heap-owning bindings — minimum `Box`, `Vec`, `String` carrying a `HeapAddr` identifying the heap allocation.
- **FR-007**: System MUST emit `MemEvent::HeapAlloc { heap_addr, size, ty_name, span }` when a heap allocation occurs (`Box::new`, `Vec::new` if it allocates, `String::from`).
- **FR-008**: System MUST emit `MemEvent::HeapRealloc { heap_addr, old_addr, old_size, new_size, span }` when a heap-resident object's storage moves (Vec grow, String grow). The `old_addr` lets the renderer animate the move.
- **FR-009**: System MUST emit `MemEvent::HeapFree { heap_addr, span }` when a heap-owning binding is dropped (end of scope).
- **FR-010**: System MUST track active borrows into heap memory. When a `HeapRealloc` event fires, every borrow whose target is the old address is marked dangling, and a `Note { kind: RuntimeError, message: "dangling reference: ..." }` event fires with span on the offending borrow expression.
- **FR-011**: System MUST extend `StateSnapshot` with a heap view: list of currently-live heap allocations, each carrying its `heap_addr`, type label, size, contents (for Vec/String), and an optional position hint (plan-phase decides — could be derived in JS instead).
- **FR-012**: The M04/M05/M06 page MUST render heap allocations in the heap panel as labeled boxes. Box content shows the type/value; Vec shows elements; String shows contents.
- **FR-013**: The page MUST render owning arrows (black) from stack slots holding heap-owning values to their heap boxes. Distinct from M06's blue (`&`) and red (`&mut`) borrow arrows.
- **FR-014**: The page MUST animate `HeapRealloc` events: the heap box's DOM position transitions to the new position over a short duration (target ~300ms). Arrows pointing at the box follow the animation.
- **FR-015**: System MUST ship at least 3 new reference programs (`tests/samples/m07_*.rs` + `web/samples/`): `m07_box`, `m07_vec_realloc`, `m07_string`.
- **FR-016**: System MUST update the M05 sample dropdown with the new M07 entries.

### Key Entities

- **Heap allocation**: a chunk of memory identified by a `HeapAddr`. Carries a size, a type label (`i32`, `bool`, `String`, `[i32; N]`, etc.), and contents (Value for Box; Vec<Value> for Vec; String for String).
- **`HeapAddr`**: opaque identifier — already declared in M03's event protocol. M07 fills usage.
- **`Pointee::Heap(HeapAddr)`**: existing M03 enum variant. M07 starts producing borrow events with this target form (in addition to M06's `Pointee::Slot`).
- **`Ty::Box(Box<Ty>)`** / **`Ty::Vec(Box<Ty>)`** / **`Ty::String`**: new type variants (plan-phase confirms shape). All non-Copy.
- **`Value::Box { heap_addr, ty_name }`** / **`Value::Vec { heap_addr, elem_ty }`** / **`Value::String { heap_addr }`**: new Value variants (plan-phase confirms shape). All store an HeapAddr indirection — the actual data lives in a separate heap-state map maintained by the evaluator.
- **Heap state (Evaluator side)**: `IndexMap<HeapAddr, HeapObject>` tracking live allocations. `HeapObject` is an enum (BoxOf(Value), VecOf(Vec<Value>, capacity), StringOf(String, capacity)).
- **Dangling-borrow tracker**: at `HeapRealloc`, every active borrow whose target_pointee is `Heap(old_addr)` becomes dangling. The evaluator emits a `Note { RuntimeError }` for each. Plan-phase decides one Note per dangling borrow vs. aggregated.
- **String literal token**: new `TokenKind::Str(String)` produced by the lexer. Used only as input to `String::from(...)` in M07's restricted method-recognition path.
- **Method-call expression**: new `Expr::MethodCall { receiver, name, args, span }`. Method-name resolution is structural (matched against a hardcoded set per receiver type) for M07; full trait-based resolution is out of scope.
- **Index expression**: new `Expr::Index { receiver, index, span }`. Only valid receiver in M07 is `Vec<T>`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After M07 ships, the M05 page accepts source containing `Box::new(...)`, `Vec::new()`, `v.push(x)`, `v[i]`, `String::from("...")`, `s.push_str("...")` and produces a trace with the appropriate heap events.
- **SC-002**: For `m07_box.rs`, the heap panel displays a single box with a black owning arrow from `b` to the box. At scope close, both disappear.
- **SC-003**: For `m07_vec_realloc.rs`, stepping past `v.push(3)` produces an OBSERVABLE position change of the heap box (animation), AND a runtime-error note with span on `&v[0]`. The Vec/realloc demo is the headline pedagogy and a maintainer can replicate the visual at will.
- **SC-004**: For `m07_string.rs`, the heap panel shows a box with the string's contents (or length indicator), and `push_str` causing a realloc triggers the same animation.
- **SC-005**: Indexing a Vec out of bounds (`let v = Vec::new(); let x = v[0];`) emits a `Note { RuntimeError }` with a clear "index out of bounds" message.
- **SC-006**: Existing M01–M06.1 tests pass. M01/M02 stay byte-identical. M03 may re-baseline if new event variants change the Debug format ordering (additive variants typically don't, but if Ty/Value reorder for shape changes, M03 snapshots drift — predictable mechanical diff).
- **SC-007**: WASM bundle growth ≤ +60% vs M06.1 baseline (88,841 B gzipped → ≤ 142,146 B). M07's variant + event growth is genuinely large; the bundle-size policy memory authorizes a generous budget for variant-growth milestones. Hard ceiling stays M04's 2 MB.
- **SC-008**: Zero warnings under `RUSTFLAGS="-D warnings" cargo build --release` AND `cargo test`. Both host and WASM targets clean.

## Assumptions

- **Method-call resolution is structural, not trait-based**: M07's parser recognizes `expr.method_name(...)` syntax; typeck dispatches based on the receiver's `Ty` against a hardcoded set of `(Ty, method_name) → signature` rules. No traits, no `impl` blocks, no user-defined methods. The recognized methods are: `Vec::push`, `Vec::new` (static), `Vec::len`, `String::from` (static), `String::push_str`, `Box::new` (static). Plan-phase may add/remove from this list.
- **Vec growth policy**: doubles on each push that exceeds capacity. Initial capacity 0; first push grows to 1; second push grows to 2; third push grows to 4; etc. This produces visible reallocations on pushes 1, 2, 3, 5, 9 (in a sequence of growing pushes). String uses the same policy.
- **Strings are byte-counted, not character-counted**: capacity is in bytes. For M07's pedagogy, ASCII-only strings keep this simple.
- **Heap allocations have monotonic addresses**: `HeapAddr(0)`, `HeapAddr(1)`, etc. Realloc gets a fresh address. Free invalidates the address (no reuse in M07; pedagogy doesn't need address reuse).
- **`Box::new(v)` triggers ONE HeapAlloc** with the inner Value moved into the heap object. The stack slot holds `Value::Box { heap_addr }`.
- **`Vec::new()` triggers ZERO HeapAllocs** (empty Vec doesn't allocate in real Rust). First `push` triggers the first HeapAlloc. Subsequent pushes that exceed capacity trigger HeapRealloc.
- **`String::from("hi")` triggers ONE HeapAlloc** (allocates the string's bytes immediately). `push_str` triggers HeapRealloc if capacity exceeded.
- **No `Drop` for primitive heap contents**: when `Box<i32>` drops, only the heap allocation is freed (HeapFree); no inner `SlotDrop` for the i32 (Copy). When `Vec<i32>` drops, same. When `String` drops, same.
- **Indexing produces a COPY for `Vec<T>` where T is Copy**: `let x = v[0];` copies the i32 from the heap to x's slot. Indexing where T is non-Copy (e.g. `Vec<String>`) would require move semantics — out of scope.
- **Borrowing a Vec element**: `let r = &v[0];` borrows the heap allocation's first element. The borrow's target is `Pointee::Heap(addr_of_v's_storage)` with an element offset (plan-phase decides whether to track per-element borrows or whole-allocation borrows; the latter is simpler and sufficient for the dangling-borrow demo).
- **Dangling-borrow detection scope**: M07 detects borrows into Vec elements that become dangling on realloc. Borrows into Box (`&*b`) are out of scope (re-borrows through deref deferred from M06.1). Borrows into String content are also out of scope (no character-level access).
- **Heap panel rendering uses flexbox or grid layout**: free-form per MILESTONES.md but plan-phase confirms. CSS-based positioning so the realloc animation can use CSS transitions on `transform` or `top`/`left`.
- **Realloc animation duration**: target 250-400ms. Plan-phase confirms; should feel snappy yet visible.
- **Bundle-size budget**: +60% from M06.1 baseline. M07 is genuinely large (lexer + parser + typeck + eval + new UI panel + heap state + dangling-borrow logic). Per the bundle-size policy memory, variant-growth + new-functionality milestones warrant generous budgets. Hard ceiling remains M04's 2 MB.
- **Sized L per the rubric**, but bordering on XL. Plan-phase may decide to split into M07a (Box only) + M07b (Vec + String + dangling borrow + realloc) if mid-implementation sizing exceeds L on any axis. Per MILESTONES.md's `do not ship without realloc animation` note, M07a alone wouldn't close the milestone — split would be a delivery-pacing decision, not a milestone-restructure.
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION because reasonable defaults exist):
  1. **Method-call vs static-function path syntax** — plan-phase decides shared AST (`Expr::MethodCall` + `Expr::PathCall`) or unified (`Expr::Call` with a callee that can be a path).
  2. **Vec realloc growth policy** — doubling is the default; could be configurable for visualization purposes.
  3. **Heap panel layout strategy** — flexbox auto-layout vs. JS-computed absolute positions.
