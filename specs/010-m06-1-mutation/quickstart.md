# Quickstart — M06.1 development + verification

Audience: maintainer + contributors working on M06.1 or extending it.

## Run the page

```bash
cd web && trunk serve --open
```

After M06.1 ships, the dropdown gains 3 entries: assign_basic, deref_read, deref_write. Existing samples render unchanged.

## Run all tests

```bash
cargo test                        # full suite — m01 (8) + m02 (16) + m03 (8)
                                  # + lib (~51 with new mutation tests)

cargo test --lib pipeline::tests::run_pipeline_assign  # M06.1 assignment tests
cargo test --lib pipeline::tests::run_pipeline_deref   # M06.1 deref tests
```

M01 / M02 / M03 stay byte-identical (existing samples don't use assignment or deref).

## Manual QA procedure (SC-008)

~7 minutes. Walk in this order:

1. **Page loads** with the default sample. No console errors.

2. **US1 — direct assignment**:
   - Select `Direct assignment (M06.1)`. Editor shows `let mut x = 0; x = 7;`.
   - Step through. Observe: `x` slot allocated with value `0_i32` (around step 2-3). At the `x = 7;` step, the slot's value **animates** from `0_i32` to `7_i32`. No new slot — same slot, different value.
   - Edit the source: change `let mut x` to `let x`. Within ~1 second, observe a typeck error at the assignment line: "cannot assign to immutable variable `x`" (or similar). Red wavy underline.

3. **US2 — deref-read**:
   - Select `Deref read (M06.1)`. Editor shows `let x = 42; let r = &x; let y = *r;`.
   - Step. Observe: at `let r = &x` step, **blue arrow** from `r` to `x`. At `let y = *r;` step, slot `y` allocated with value `42_i32`. The value flowed through the reference. Blue arrow persists.
   - Try `let x = 5; let y = *x;` — observe typeck error: "cannot dereference value of type `i32`" (or similar).

4. **US3 — deref-write (THE headline)**:
   - Select `Deref write (M06.1)`. Editor shows `let mut x = 5; let r = &mut x; *r = 10;`.
   - Step. Observe sequence:
     1. `let mut x = 5;` — slot `x = 5_i32`.
     2. `let r = &mut x;` — slot `r` allocated, **red arrow** from `r` to `x`.
     3. `*r = 10;` — **slot `x`'s value animates from `5_i32` to `10_i32`**, AND the **red arrow stays anchored**.
   - The pedagogy: "the arrow doesn't move; the value at the target slot changes."
   - Edit: change `&mut x` to `&x`. Observe typeck error at `*r = 10`: "cannot assign through `&T`; need `&mut T`".

5. **Borrow-during-assignment rejection**:
   - Type `fn main() { let mut x = 5; let r = &x; x = 7; }`. Observe typeck error at `x = 7`: "cannot assign to `x` because it is borrowed".

6. **Free-form editing**:
   - Type `fn main() { let mut x: u8 = 5; let r = &mut x; *r = 250; }`. Observe: works, x animates 5 → 250 (all u8).
   - Try `*r = 300` (with x: u8). Observe range-check error.

7. **No regressions**:
   - Cycle through M03/M04/M05/M03.2/M06 samples. Each renders correctly. Borrows still show arrows, scoped borrow still ends correctly.

## Developer notes

### Why `Stmt::Assign` and not `Expr::Assign`?

Per R-001 (research.md): keeping assignment as a Stmt avoids the bigger refactor of adding an Expr arm. M06.1's scope is statement-position only. If a future revision needs `let y = (x = 5);` semantics, adding `Expr::Assign` then is additive.

### Why no new `MemEvent` variant?

The existing `SlotWrite` variant carries exactly "write this value into this slot." Whether the write originates from `let x = init;` (M03) or `x = v;` (M06.1) doesn't change its semantics. Reusing the variant is the right answer — it's also what makes the visualization "free" (the stacks panel's existing animation handles slot value changes).

### What if `*r = v` happens after `r`'s scope ended?

Can't happen — typeck rejects use of out-of-scope bindings. The borrow tracker keeps `r` alive only until its scope's `}` (M06's scope-level lifetimes).

### Span on the SlotWrite for assignment

Uses the full `Stmt::Assign.span` (lhs through `;`). Matches the let-init convention. The editor highlight on this step covers the whole assignment statement, helpful pedagogy.

### What about `let mut r = &x; r = &y;` (reassigning the ref itself)?

In scope — this is just direct assignment of `r` (a `mut` binding). lhs = `Expr::Ident(r)`, rhs = `Expr::Borrow(&y)`. Typeck checks r is `let mut`. Both r-the-binding and r-the-Value::Ref are updated. The blue arrow's source stays `r`, target updates from `x` to `y`. Visualization correctness requires the existing World tracking to handle the new `Value::Ref` value flowing into `r`'s slot — should work via the SlotWrite-binds-source_slot path established in M06.

Wait, actually there's a subtle issue: reassigning `r` requires the old borrow of `x` to END (since `r` no longer holds it) and a new borrow of `y` to begin. The M06 BorrowEnd events are scope-tied; they don't fire when a ref-holding binding is reassigned. This is an EDGE CASE that M06.1 doesn't explicitly handle. The trace would have two BorrowShared events but only one BorrowEnd (at scope close). The first arrow would point at x THEN at y after the reassignment — but the underlying Value::Ref structure might still carry the OLD borrow_id. Worth flagging as a known limitation; documented in the audit log if observed.

### When extending in M07+

M07 adds heap-allocated types (Box, Vec, String). Assignment to a Box-typed binding: `let mut b: Box<i32> = Box::new(5); b = Box::new(7);` — same `Stmt::Assign` machinery; the SlotWrite carries a Box value. The old heap allocation needs to be freed (HeapFree event) — that's an M07 concern, not M06.1.

Deref of a Box: `*b` already used by Box's natural deref coercion in Rust. Whether M07 introduces `*b` as Box-deref syntax or relies on coercion is M07's call.

## What this milestone does NOT add

- Compound assignment (`+=`, etc.).
- Multi-level deref (`**r`).
- Re-borrows through deref (`&*r`, `&mut *r`).
- Assignment as expression (`let y = (x = 5);`).
- Field/index lhs (no fields or indexing yet).
- Method calls on deref (`(*r).method()`).
- Reassignment-aware borrow lifetime (the edge case above — old borrow technically should end when r reassigns).
