# Quickstart — M06 development + verification

Audience: maintainer + contributors working on M06 or extending it.

## Run the page

```bash
cd web && trunk serve --open
```

After M06 ships, the dropdown gains 4 entries: shared borrow, mut borrow, aliasing error, scoped borrow. Existing samples render unchanged.

## Run all tests

```bash
cargo test                        # full suite — m01 (8) + m02 (16) + m03 (8)
                                  # + lib (~55 with new borrow-tracker + pipeline tests)

cargo test --lib pipeline::       # M06 pipeline tests (borrow scenarios)
cargo test --lib borrow_tracker:: # borrow-tracker unit tests
```

M01 / M02 / M03 should stay byte-identical (existing samples don't construct `Value::Ref` or `Ty::Ref`).

## Manual QA procedure (SC-008)

~10 minutes. Walk in this order:

1. **Page loads** with the default sample (alphabetically first). No console errors.

2. **US1 — shared borrow**:
   - Select `Shared borrow (M06)`. Editor shows the canonical `let x = 5; let r = &x;`.
   - Step through. Observe: at the `let r = &x;` step, `r` appears as a new slot. A **blue arrow** appears originating from `r`'s slot card and pointing at `x`'s slot card.
   - Continue stepping. The blue arrow stays as long as `r` is in scope.
   - At the end of `main`'s scope, the arrow disappears (BorrowEnd fires before `r` and `x` themselves drop with the frame).

3. **US2 — mutable borrow**:
   - Select `Mutable borrow (M06)`. Editor shows `let mut x = 5; let r = &mut x;`.
   - Step. Observe **a red arrow** (visually distinct from blue) from `r` to `x`.
   - Try editing: change `let mut x` to `let x` (without `mut`). Within ~1 second, observe a typeck error in the editor: "cannot borrow `x` as mutable; not declared as mutable" (or similar). Red wavy underline at `&mut x`.

4. **US3 — aliasing violation**:
   - Select `Aliasing error (M06)`. Editor shows `let mut x = 5; let r1 = &x; let r2 = &mut x;`.
   - Observe **a red wavy underline at `&mut x`** with status bar: "cannot borrow `x` as mutable while it is borrowed as immutable" (or similar).
   - Play/Step buttons disabled.
   - Edit the source: change `&mut x` to `&x`. Within ~1s, underline disappears, two blue arrows visible.

5. **US4 — scoped borrow**:
   - Select `Scoped borrow (M06)`. Editor shows nested-block program.
   - Step through. At the inner block's closing `}`, observe the arrow disappear (BorrowEnd) while `x` itself remains.

6. **Free-form editing**:
   - Type `fn main() { let x = 5; let r1 = &x; let r2 = &x; }`. Two blue arrows from `r1` and `r2`, both pointing at `x`. Valid Rust (multiple shared OK).
   - Type a function: `fn f(r: &i32) { } fn main() { let x = 5; f(&x); }`. Trace through; observe the arrow crossing the frame boundary (from `r` in `f`'s active frame back to `x` in the grayed `main`'s slot — or vice versa depending on M03.1 frame semantics).

7. **No regressions**:
   - Cycle through all M03/M04/M05/M03.2 samples. Each renders correctly. No new arrows where there shouldn't be any. Existing functionality (Copy slots persisting, return-value annotation, current-frame red highlight, type suffixes) all still work.

## Developer notes

### Adding a new sample with borrows

1. Drop `tests/samples/m06_<name>.rs` + `web/samples/m06_<name>.rs` with identical content.
2. Add `<option value="m06_<name>">Display name (M06)</option>` to the dropdown.
3. The trunk copy-dir directive auto-picks up the file.

### Borrow tracker debugging

The borrow tracker emits errors via the existing typeck error path (`ParseError { message, span }`). If a borrow check error message looks wrong:

1. Check `BorrowTracker::try_take_shared` / `try_take_mut` for the actual rule application.
2. Spans on `ActiveBorrow.borrow_span` come from the EXISTING borrow that's conflicting (so the error message can say "...because it was already borrowed here at <span>").
3. The NEW (failing) borrow's span is passed to `try_take_*` and used in the typeck error itself.

### Borrow event timing

- `BorrowShared`/`BorrowMut`: emitted from `eval_expr` when an `Expr::Borrow` evaluates. Order: AFTER the inner identifier is resolved, BEFORE the resulting Value::Ref is bound to a slot (via SlotWrite if part of a let).
- `BorrowEnd`: emitted from `drop_current_scope`, BEFORE any SlotDrops, in reverse-allocation order within the scope.

### SVG overlay positioning

The `renderArrows(borrows)` function queries `document.querySelector(`[data-slot-id="${id}"]`)` for each source and target. If a slot isn't visible (e.g. grayed-frame after FrameLeave but before frame removal), the arrow may still need to draw — make sure the data attribute persists on grayed slot cards.

### NaN in borrow tests

Not applicable — borrows don't involve floats. NaN-equality concerns from M03.2 don't propagate here.

## When extending in M07+

M07 adds heap-allocated types. The `Pointee::Heap(HeapAddr)` variant (declared in M03) becomes a real target for borrow events. The existing `BorrowShared`/`BorrowMut` payload accommodates this — no protocol change needed.

For heap borrows, the SVG overlay will draw arrows from a stack slot (or another heap box) into a heap box. The same `renderArrows` function should work: just query the heap-box DOM element by its `data-heap-addr` attribute (introduced in M07).

## What this milestone does NOT add

- Deref operator (`*r` to read; `*r = v` to write).
- Named lifetimes (`<'a>`, `'static`).
- Returning references from functions.
- Re-borrowing (`&*r`).
- Field borrows (no fields yet).
- NLL (non-lexical lifetimes).
- Borrows into heap-allocated values (waits for M07).
