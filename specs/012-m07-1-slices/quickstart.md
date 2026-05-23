# Quickstart — M07.1 development + verification

Audience: maintainer + contributors working on M07.1 or extending it.

## Run the page

```bash
cd web && trunk serve --open
```

After M07.1 ships, the dropdown gains 3 entries: `Slice basic`, `Slice range`, `Slice dangling`. Existing samples render unchanged. The arrow overlay gains a small `[len: N]` text annotation on slice arrows.

## Run all tests

```bash
cargo test                            # full suite: m01 + m02 + m03 + lib

cargo test --lib pipeline::tests::run_pipeline_slice_basic    # full-vec slice
cargo test --lib pipeline::tests::run_pipeline_slice_range    # partial-range slice
cargo test --lib pipeline::tests::run_pipeline_slice_dangling # dangling after realloc
cargo test --lib pipeline::tests::run_pipeline_slice_oob      # out-of-bounds range
cargo test --lib pipeline::tests::run_pipeline_slice_all_forms # all four range forms
cargo test --lib pipeline::tests::run_pipeline_slice_len      # s.len() on slice
cargo test --lib pipeline::tests::run_pipeline_mut_slice_rejected # mutable slice typeck error
```

M01, M02, M03 should stay byte-identical (no existing sample constructs `Ty::Slice` or `Value::Slice`).

## Manual QA procedure (SC-008)

~8 minutes. Walk in this order:

1. **Page loads** with the default sample. No console errors. Existing M01–M07 samples render unchanged.

2. **US1 — Partial-range slice (the headline)**:
   - Select `Slice range (M07.1)`. Editor shows:
     ```rust
     fn main() {
         let mut v: Vec<i32> = Vec::new();
         v.push(10);
         v.push(20);
         v.push(30);
         v.push(40);
         let s = &v[1..3];
     }
     ```
   - Step through. Observe sequence:
     1. `Vec::new()` — slot `v: Vec<i32>` allocated. NO heap box yet.
     2. `v.push(10)` — heap box appears (`Vec[1]` capacity 1). Black owning arrow from `v`.
     3. `v.push(20)` — realloc to capacity 2. Contents: `[10, 20]`.
     4. `v.push(30)` — realloc to capacity 4. Contents: `[10, 20, 30, _, _]` (4 cells, 3 used; M07 displays the 4 cells per its byte-cell renderer).
     5. `v.push(40)` — fits in capacity. Contents: `[10, 20, 30, 40]`.
     6. `let s = &v[1..3]` — **blue borrow arrow from `s`'s slot to the Vec's heap allocation**. **The arrow shows `[len: 2]` annotation** (between the arrow's source and its arrowhead, in small blue text).
   - Step to end-of-`main`. `BorrowEnd` fires (arrow disappears), then `HeapFree` (heap box disappears), then `SlotDrop` for `v` and `s`.

3. **US2 — Full-vec slice + `s.len()`**:
   - Select `Slice basic (M07.1)`. Editor shows:
     ```rust
     fn main() {
         let mut v: Vec<i32> = Vec::new();
         v.push(1);
         v.push(2);
         v.push(3);
         let s = &v[..];
         let n = s.len();
     }
     ```
   - Step. At `let s = &v[..]`, observe blue arrow with `[len: 3]` annotation.
   - At `let n = s.len()`, observe `n: u64 = 3_u64` appearing in the stacks panel.

4. **US3 — Slice dangles after realloc**:
   - Select `Slice dangling (M07.1)`. Editor shows:
     ```rust
     fn main() {
         let mut v: Vec<i32> = Vec::new();
         v.push(1);
         v.push(2);
         let s = &v[..];
         v.push(3);
     }
     ```
   - Step through:
     1. `Vec::new()` — slot allocated.
     2. `v.push(1)` — heap box, capacity 1.
     3. `v.push(2)` — realloc to capacity 2.
     4. `let s = &v[..]` — blue arrow with `[len: 2]` annotation.
     5. `v.push(3)` — realloc to capacity 4. **Note { RuntimeError } fires**: "dangling reference: slice still points at the freed heap chunk". Editor highlights the `&v[..]` span with the existing M05 error-underline style.
   - Pedagogy: same UB story as M07's `&v[0]` case, but for slice granularity. Learners see "the slice IS just a borrow under the hood — same rules apply".

5. **Out-of-bounds slice**:
   - Type `fn main() { let mut v: Vec<i32> = Vec::new(); v.push(1); let s = &v[0..5]; }`. Observe runtime error at the `&v[0..5]` step: "slice end out of bounds: end is 5, vec len is 1".

6. **All four range forms**:
   - Type `fn main() { let mut v: Vec<i32> = Vec::new(); v.push(1); v.push(2); v.push(3); let a = &v[..]; let b = &v[1..]; let c = &v[..2]; let d = &v[0..2]; }`. Each binding produces a slice arrow with its correct `[len: N]` annotation (3, 2, 2, 2).

7. **Mutable slice rejected**:
   - Type `fn main() { let mut v: Vec<i32> = Vec::new(); v.push(1); let s = &mut v[..]; }`. Editor shows the M07.1 typeck error: "mutable slices are out of scope in M07.1 — only &[T] (shared) is supported".

8. **Standalone range rejected**:
   - Type `fn main() { let r = 1..3; }`. Editor shows: "range expressions are only valid inside index brackets in M07.1".

9. **No regressions**:
   - Cycle through M01–M07 samples. Each renders correctly. Box owning arrows still work; Vec realloc dangling still fires; String allocates correctly.

## Developer notes

### Why is `Value::Slice` a separate variant from `Value::Ref`?

Slices carry an extra `len` field that single-element borrows don't. Forcing `Value::Ref` to grow `len: Option<u64>` would make every borrow-rendering site need to handle the None case + the Slice case differently — and the discriminator "is this a slice?" would live in the Option, which is fragile. Two variants with parallel structure is clearer: `Value::Ref` → plain arrow; `Value::Slice` → annotated arrow.

### Why does `Ty::Slice(T)` represent `&[T]` (not `[T]` standalone)?

In real Rust, `[T]` is an unsized type that only ever appears behind a reference. M07.1 follows this: `Ty::Slice(T)` IS the slice type — there's no need for a separate `Ty::Ref { inner: Ty::UnsizedArray(T) }` form. The leading `&` in `&[T]` is absorbed into the slice type representation. This matches Rust's "you can't write `[T]` alone, only `&[T]` or `&mut [T]` or `Box<[T]>`" rule.

### Why parse `..` only inside `[ ]` in M07.1?

Standalone range expressions (`let r = 1..3;`, `for i in 1..10`) need precedence decisions (`1 + 2..3` parses as `(1+2)..3` in Rust, with `..` being a fairly low-precedence operator). M07.1 doesn't need standalone ranges and doesn't have `for` loops or pattern matching. Reserving the parse rules for when there's a real consumer keeps M07.1's scope tight.

### Why does `s.len()` return `u64`?

Matches Rust's `usize` (64-bit on the platforms M07/M07.1 targets) and M07's existing `Vec::len() -> u64`. Consistency: any `.len()` method on any container returns `u64`.

### Range-OOB error messages

Three distinct cases:
- `start < 0` or `start > receiver.len()` → "slice start out of bounds: start is {start}, vec len is {len}"
- `end < 0` or `end > receiver.len()` → "slice end out of bounds: end is {end}, vec len is {len}"
- `start > end` → "slice start > end: start is {start}, end is {end}"

Each halts the trace (RuntimeError pedagogy, same as div-by-zero / integer overflow).

### Length annotation positioning

The label sits at the arrow's midpoint, offset perpendicular by ~6-8px to avoid overlapping the arrow line. For arrows routed above the heap row (typical for slice/borrow arrows targeting heap), the label inherits the arrow's lane and floats just above. For arrows routed elsewhere (rare in M07.1 — slices only target heap), the offset direction is chosen based on the arrow's slope.

## When extending in M07.2

M07.2 (`&str` + static memory) reuses M07.1's slice infrastructure:
- `&'static str` is a `Ty::Slice(Box::new(Ty::Int(IntKind::U8)))` (slice of bytes).
- The static-memory region is a new "panel" (or annotation within the heap panel) holding read-only bytes for string literals.
- String literals get a `Value::Slice { borrow_id, target: Pointee::Static(static_id), len, mutable: false }` — note the new `Pointee::Static` variant (or however M07.2 represents the static region).
- The length-annotation visual is shared.
- `String::from(s: &str) -> String` heap-allocates fresh bytes by copying from the static region.

The slice abstraction M07.1 establishes is reused as-is — only the `target` Pointee variant changes (Heap → Static).

## What this milestone does NOT add

- Mutable slices (`&mut [T]`).
- Slicing a slice (`&s[0..1]`).
- Slice methods beyond `len()`.
- Iterator methods (`s.iter()`, `for x in s`).
- Standalone range expressions (`let r = 1..3;`, `for i in 1..10`).
- Range bounds with non-Int types.
- Slicing non-Vec receivers (Strings, arrays).
- Array types `[T; N]` and references to arrays.
- Multi-dimensional slices.
- `&str` and static memory (M07.2).
