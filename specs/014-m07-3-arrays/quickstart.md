# Quickstart — M07.3 development + verification

Audience: maintainer + contributors working on M07.3 or extending it.

## Run the page

```bash
cd web && trunk serve --open
```

After M07.3 ships, the dropdown gains 3 entries: `Array basic`, `Array index`, `Array slice`. Selecting any of them shows **zero heap activity** for the entire trace — the heap panel stays empty.

## Run all tests

```bash
cargo test                            # full suite

cargo test --lib pipeline::tests::run_pipeline_array_basic      # array literal + len
cargo test --lib pipeline::tests::run_pipeline_array_index      # array indexing
cargo test --lib pipeline::tests::run_pipeline_array_index_oob  # OOB index → RuntimeError
cargo test --lib pipeline::tests::run_pipeline_array_slice      # slot-target slice
cargo test --lib pipeline::tests::run_pipeline_array_slice_oob  # OOB slice → RuntimeError
cargo test --lib pipeline::tests::run_pipeline_array_no_heap    # zero heap events for arrays
```

M01, M02, M03 should stay byte-identical (no existing sample constructs `Ty::Array` or `Value::Array`).

## Manual QA procedure (SC-008)

~6 minutes. Walk in this order:

1. **Page loads** with the default sample. No console errors. Existing M01–M07.2 samples render unchanged.

2. **US1 — Array basic (the headline)**:
   - Select `Array basic (M07.3)`. Editor shows `let t = [10, 20, 30]; let n = t.len();`.
   - Step. Observe at the `let t = [10, 20, 30]` step:
     - `t` row appears in `main`'s frame with type `[i32; 3]`.
     - **Inline byte-cells** in `t`'s value area: 12 gray-tinted cells (3 i32 elements × 4 bytes), all filled.
     - **Heap panel stays empty** — no allocation event fires.
   - Step to `let n = t.len()`. Observe `n: u64 = 3_u64`.

3. **US2 — Array indexing**:
   - Select `Array index (M07.3)`. Editor shows `let t = [10, 20, 30]; let x = t[1];`.
   - Step past `let x = t[1]`. Observe `x: i32 = 20_i32`. Heap still empty.
   - Type `let t = [1, 2]; let x = t[5];` in the editor live. Observe runtime error: "index out of bounds: array len is 2 but the index is 5".

4. **US3 — Array slicing (the structural payoff)**:
   - Select `Array slice (M07.3)`. Editor shows `let t = [1, 2, 3, 4]; let s = &t[1..3];`.
   - Step. At `let s = &t[1..3]`:
     - **`s` row appears with type `&[i32]`**.
     - **A blue slice arrow** connects `s`'s slot to `t`'s slot (slot-to-slot routing, like M06 borrow arrows — NOT to a heap block).
   - Hover the slice arrow: **`[len: 2]` annotation** appears + byte-cells 4-11 of `t` (the 2nd and 3rd elements) light up yellow + the element labels `2_i32, 3_i32` in t's value-area also light up.

5. **Zero heap events**:
   - Look at the trace counter at the bottom. For `Array basic`: trace length is small (~5-7 events depending on the implementation). No HeapAlloc/HeapRealloc/HeapFree among them — verifiable in dev tools or by simply observing the heap panel never populates.

6. **No regressions**:
   - Cycle through M01–M07.2 samples. Each renders correctly. M07's Vec realloc + heap pedagogy unchanged.

## Developer notes

### Why is `Value::Array` a Value variant instead of a heap object?

Because arrays are stack-allocated. The slot's value IS the array's content. Putting the elements in a heap object would defeat the pedagogy AND require allocating heap state that the learner shouldn't see.

### Why do slot-target slice borrows skip BorrowShared/BorrowEnd?

Same reasoning as M07.2's `Pointee::Static` case: the borrow's lifecycle is invisible unless an arrow renders. For slot-target slices, the arrow exists when bound to a slot (`let s = &t[range]`); the borrow_id-only events would just be silent cursor steps. The UI's `apply_event` SlotWrite arm materializes the borrow with `source_slot` bound when it sees a `Value::Slice` with no matching world.borrows entry.

### How does inline byte-cell rendering work?

`SlotRowView` gains an `inline_cells: Option<InlineCellsView>` field, mutually exclusive with the existing `value: Option<String>`. When the slot holds `Value::Array { elements, elem_ty }`, the JS renders per-byte cells (one `<span class="byte-cell">` per byte of the array's total size). The cells use `.stack-inline-cells` (distinct from `.heap-cells`) for CSS styling — gray-tinted background to convey "stack memory".

### How does the hover-highlight work for array slice arrows?

Same pattern as M07.1/M07.2 slice arrows targeting heap/static. The hover handler queries the target block's byte-cells via `[data-slot-id=X] .stack-inline-cells .byte-cell` (slot target) instead of `[data-heap-addr] .heap-cells .byte-cell` (heap) or `[data-static-addr] .static-cells .byte-cell` (static). The `.elem-cell.elem-slice-highlighted` selector already covers element-span highlights inside the slot.

### Why is `[T; N]` parsed differently from `&[T]`?

In Rust, `&[T]` is "borrow of an unsized slice" (size unknown), while `[T; N]` is "sized array with N elements". The parser treats them as distinct syntactic constructs:
- `Type::Slice { inner, mutable }` (from M07.1 — actually was added in M07.2's spec; same here): parsed as `& [T]`
- `Type::Array { inner, size }` (NEW M07.3): parsed as `[T; N]`

The `[` token at type-context entry triggers `Type::Array`; the `[` after `&` (or `&mut`) triggers `Type::Slice`.

### Why don't array literals trigger heap allocation?

Because Rust's `[T; N]` IS the type — the bytes live in the bytecode/stack slot directly, not on the heap. M07.3 models this faithfully: `Expr::ArrayLit` eval builds a `Value::Array` and stores it directly in the slot's `LocalSlot.value`. No `HeapState::alloc_heap` call.

## When extending in M08

M08 (threads) doesn't depend on M07.3. Both are siblings depending on M07.1. M08 introduces `thread::spawn`, `Arc<T>`, `Mutex<T>` — all heap-allocated. M07.3's array machinery is unaffected.

## What this milestone does NOT add

- Repeat syntax `[v; N]` — out of scope.
- Multi-dimensional arrays `[[T; N]; M]` — out of scope.
- Arrays of non-Copy types — out of scope.
- Mutation through index `t[0] = 5;` — out of scope.
- Iterator methods (`for x in t`, `t.iter()`) — out of scope.
- Slicing temporaries (`&[1,2,3][1..2]`) — out of scope. Receiver must be an `Expr::Ident`.
- Const generics in array size — out of scope.
- Array equality `[1, 2] == [1, 2]` — out of scope.
