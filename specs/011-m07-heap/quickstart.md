# Quickstart — M07 development + verification

Audience: maintainer + contributors working on M07 or extending it.

## Run the page

```bash
cd web && trunk serve --open
```

After M07 ships, the dropdown gains 3 entries: `Box`, `Vec realloc`, `String`. Existing samples render unchanged. The heap panel — previously a `<p class="placeholder">Heap (Level 3+)</p>` — now displays real heap boxes.

## Run all tests

```bash
cargo test                            # full suite: m01 (8) + m02 (16) + m03 (8)
                                      # + lib (~61 with new heap tests)

cargo test --lib pipeline::tests::run_pipeline_box   # Box tests
cargo test --lib pipeline::tests::run_pipeline_vec   # Vec tests (incl. realloc + dangling)
cargo test --lib pipeline::tests::run_pipeline_string # String tests
```

M01, M02, M03 should stay byte-identical (no existing sample constructs heap values or `Value::Ref` shapes that change with the restructure).

## Manual QA procedure (SC-008)

~12 minutes. Walk in this order:

1. **Page loads** with the default sample. No console errors. Heap panel shows empty (no `<p class="placeholder">` anymore).

2. **US1 — Box**:
   - Select `Box (M07)`. Editor shows `let b = Box::new(5);`.
   - Step. Observe at the `Box::new(5)` step:
     - **A heap box appears** in the heap panel labeled `i32 = 5_i32` (or similar — confirm with the chosen display format).
     - **A black owning arrow** connects `b`'s stack slot to the heap box.
   - Step to end-of-`main`. Observe HeapFree fires; heap box disappears, owning arrow disappears.

3. **US2 — Vec realloc + dangling borrow (THE headline)**:
   - Select `Vec realloc (M07)`. Editor shows the canonical demo:
     ```rust
     fn main() {
         let mut v: Vec<i32> = Vec::new();
         v.push(1);
         v.push(2);
         let r = &v[0];
         v.push(3);
     }
     ```
   - Step through. Observe sequence:
     1. `Vec::new()` — slot `v : Vec<i32>` allocated. NO heap box yet (empty Vec doesn't allocate).
     2. `v.push(1)` — heap box appears labeled `[1_i32]` (capacity 1). Black owning arrow from `v` to the box.
     3. `v.push(2)` — heap box reallocates (capacity 1 → 2). HeapRealloc event fires. Box content updates to `[1_i32, 2_i32]`.
     4. `let r = &v[0]` — blue borrow arrow from `r` slot to the heap box. Status bar shows "borrow of heap element".
     5. `v.push(3)` — **heap box reallocates AGAIN** (capacity 2 → 4). The borrow becomes dangling. **A `Note { RuntimeError }` fires**: "dangling reference: `r` was borrowed here but the underlying memory has been reallocated". Editor highlights the `&v[0]` span with the existing M05 error-underline style.
   - At step 5, observe: heap box's content updates AND a position shift (if other boxes are visible — for this single-allocation demo, may shift slightly due to capacity-driven width change). The blue arrow's target updates to the NEW heap box (so the arrow appears to "stretch" briefly during realloc). The pedagogy: "your borrow now points at memory that's been freed; the arrow is invalid."

4. **US3 — String**:
   - Select `String (M07)`. Editor shows `let mut s = String::from("hi"); s.push_str("!");`.
   - Step. At `String::from("hi")`, heap box appears labeled `"hi"` (or `String[2]` depending on display format).
   - At `s.push_str("!")`, the box updates to `"hi!"`. If capacity grew, a HeapRealloc fired (capacity was 2; pushing 1 byte fits if capacity stayed 2, otherwise realloc to 4).

5. **Vec out-of-bounds**:
   - Type `fn main() { let v: Vec<i32> = Vec::new(); let x = v[0]; }`. Observe runtime error at the `v[0]` step: "index out of bounds: the len is 0 but the index is 0".

6. **No regressions**:
   - Cycle through M01–M06.1 samples. Each renders correctly. Borrows still show arrows (blue/red). Assignment + deref still work.

## Developer notes

### Why a new `ArrowView` instead of extending `BorrowView`?

Because in M07, the arrow types diversify (Shared / Mut / Owning) and the targets diversify (Slot / Heap). The old `BorrowView { source_slot, target_slot, mutable }` was specifically about borrows. Renaming to `ArrowView` with `kind` and `target` fields makes the new unification explicit. JS-side: `state.borrows` → `state.arrows`.

### Heap state in eval

The Evaluator gains `heap: HeapState` with `objects: IndexMap<HeapAddr, HeapObject>`. Three HeapObject variants: Box, Vec, Str. Plus `next_heap_addr` counter for monotonic addrs.

### Vec growth policy

Doubling. Capacity 0 → 1 → 2 → 4 → 8 → ... The demo's `v.push(1); v.push(2); v.push(3);` produces TWO HeapReallocs (push 2 grows 1 → 2; push 3 grows 2 → 4) and ONE HeapAlloc (push 1 grows 0 → 1). The pedagogy works because the borrow at `&v[0]` lives between pushes 2 and 3 — push 3 is the realloc that invalidates it.

### Dangling-borrow Note timing

The Note fires IMMEDIATELY after the HeapRealloc event, BEFORE the next event in the stream. Multiple dangling borrows → multiple Notes in sequence.

### Why method dispatch is structural

Hardcoded `(Ty, method_name) → signature` table. No traits, no `impl` blocks, no user-defined methods. The recognized methods are exactly: `Vec::push`, `Vec::len`, `Vec::new` (static), `Box::new` (static), `String::from` (static), `String::push_str`. Extending to other built-ins (e.g. `i32::pow`) is out of scope.

### Heap panel layout

Flexbox `display: flex; flex-wrap: wrap; gap: 0.5rem;`. Boxes added at the end on HeapAlloc; removed on HeapFree (CSS opacity fade-out). CSS transition on `transform` for any layout shift.

### Realloc animation

Currently relies on implicit flex reflow (when other heap boxes change, this one's position may shift; CSS transition smooths the move). For single-allocation demos, no visible position shift occurs. Plan-phase R-024 leaves a border-flash effect as a polish option if QA finds the demo isn't visually dramatic enough.

## When extending in M08

M08 adds threads + `Arc<T>` + `Mutex<T>`. Arc is a heap-allocated reference-counted owning type — same heap panel infrastructure handles it. Mutex is heap-allocated with lock state. Both extend M07's HeapObject variants. The owning-arrow rendering (M07's black) reuses for Arc clones (one Arc → multiple owning arrows from different stack slots to the same heap box). M07's heap state machine is the foundation M08 builds on.

## What this milestone does NOT add

- HashMap, Rc, RefCell, or other heap types.
- Threads / Arc / Mutex (M08).
- Vec<T> for non-Copy T (e.g. Vec<Box<i32>>).
- Indexing assignment (`v[0] = 5;`).
- Box re-borrows (`&*b`).
- Slice borrows (`&v[..]`).
- Method chaining other than syntactically.
- `Vec::with_capacity`, `Vec::iter`, `Vec::clear`, `String::new`, etc.
