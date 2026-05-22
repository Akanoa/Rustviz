# Quickstart — M03.2 development + verification

Audience: maintainer + contributors working on M03.2 or extending the scalar lattice in future revisions.

## Run all tests

```bash
cargo test                        # full suite — m01 (8) + m02 (16) + m03 (8 re-baselined)
                                  # + lib (~30 with new pipeline + typeck + eval tests)

cargo test --lib                  # M03.2's new lib tests
cargo test --test m01             # byte-identical
cargo test --test m02             # byte-identical
cargo test --test m03             # re-baselined snapshots
```

The M03 snapshots are re-baselined because `Value`'s Debug format changes from `Int(5)` to `Int { kind: I32, bits: 5 }`. Spot-check via:

```bash
INSTA_UPDATE=always cargo test --test m03   # accept the diff
git diff tests/snapshots/                   # visually verify pattern is uniform
```

Predicted diff is mechanical: every `Int(N)` → `Int { kind: I32, bits: N }`. If anything else changes, M03.2 has accidentally altered evaluator behavior — investigate.

## Run the page

```bash
cd web && trunk serve --open
```

After M03.2 ships, the dropdown gains 3 entries: `u8 arithmetic (M03.2)`, `u8 overflow (M03.2)`, `f64 NaN (M03.2)`. Existing samples render unchanged except for the new type-tag suffix on values (`5` becomes `5_i32`).

## Manual QA procedure (SC-008)

~7 minutes total. Walk in this order:

1. **Page loads** with default sample (alphabetically first). Verify no console errors.

2. **US1 — integer types**:
   - Select `u8 arithmetic (M03.2)`. Editor shows `let a: u8 = 5; let b: u8 = 3; let c: u8 = a + b;`. Step through; observe `a = 5_u8`, `b = 3_u8`, `c = 8_u8` in the stacks panel.
   - Edit the source — change `5` to `100` and `3` to `200`. Wait ≤ 1s. Re-step; observe `a = 100_u8`, `b = 200_u8`. The addition `100 + 200 = 300` exceeds `u8::MAX = 255` → trace halts on a RuntimeError note. Status bar reads `u8 overflow: …`.

3. **US1 — typeck errors**:
   - Edit to `let x: u8 = 300;` (literal out of range). Within ~1s: red wavy underline on `300`; status bar shows `Typeck error: literal out of range for u8` (or similar).
   - Edit to `let x: u8 = -1;` (negation of unsigned). Red underline on the `-1`; status bar shows typeck error about `Neg` on unsigned.
   - Edit to `let a: u8 = 1; let b: i32 = 2; let c = a + b;`. Red underline on the `b` operand; status bar shows cross-type error.

4. **US2 — float types**:
   - Select `f64 NaN (M03.2)`. Editor shows `let a: f64 = 0.0; let b: f64 = a / a;`. Step through; at the `b = …` step, observe `b = NaN_f64` AND an Info note in the status bar ("produced NaN" or similar). Trace continues to completion (does NOT halt).
   - Edit to `let a: f64 = 0.0; let b: f64 = 1.0 / a;`. Re-step; observe `b = +Inf_f64` + Info note.
   - Type a new program from scratch: `fn main() { let pi: f64 = 3.14; let r: f64 = 2.0; let c: f64 = 2.0 * pi * r; }`. Observe `c = 12.56_f64` (or similar) — no notes.

5. **No regressions**:
   - Cycle through all existing M03/M04/M05 samples. Each renders correctly. The only visible change vs. M05 is the type-tag suffix on values (`5_i32` instead of `5`). If anything else differs, M03.2 has touched code it shouldn't have.

6. **Edge cases** (optional, ~2 min):
   - `let a: i64 = 9223372036854775807; let b: i64 = a + 1;` → overflow halt.
   - `let x: i128 = 0;` → renders `0_i128`. (i128 has the same lattice rules but doesn't usually overflow with small literals.)
   - `let x: usize = 100;` → renders `100_usize`. usize = u64 per FR-011.

## Developer notes

### Adding a new integer type (future)

To extend with e.g. `BigInt` or a `u256`:

1. Add a variant to `IntKind` in `src/typeck.rs`.
2. Implement `min_value`, `max_value` (note: > i128 range requires reconsidering Value's `bits: i128` storage).
3. Match exhaustiveness will fail in `name()`, `is_signed()`, `contains()`, all arithmetic dispatchers in `eval.rs`, and `render_value` in `ui.rs`. Add the new arm to each.
4. Drop a sample in `tests/samples/` + `web/samples/`.
5. Add an entry to `web/index.html`'s dropdown.

### Debugging a "trace halts unexpectedly" symptom

Common causes:
- The pipeline's typeck rejected the source — check the editor for a red wavy underline; status bar should show the message.
- An arithmetic op overflowed without the user expecting it. Step through; the runtime-error note identifies which line.
- For floats: Inf/NaN don't halt — they propagate via Info notes. If the trace halts on a float op, something else is wrong (probably integer arithmetic in a different statement).

### Debugging "Value Debug format wrong in snapshot"

If M03 snapshot tests fail post-M03.2 with unexpected diffs:
- Verify `Value`'s `#[derive(Debug)]` uses the unified shape (`Int { kind, bits }`).
- Compare diff against the R-007 mechanical-rewrite expectation. Anything outside that pattern is a bug.
- Run `INSTA_UPDATE=always cargo test --test m03` to re-accept; then `git diff tests/snapshots/` to verify the diff is consistent.

### NaN equality in tests

```rust
// WRONG: assertion will FAIL even when both are NaN
assert_eq!(value, Value::Float { kind: F64, value: f64::NAN });

// RIGHT: use Debug-format equality
assert_eq!(format!("{:?}", value), "Float { kind: F64, value: NaN }");

// OR: extract and use is_nan()
match value {
    Value::Float { value: v, .. } => assert!(v.is_nan()),
    _ => panic!("expected float"),
}
```

## What this milestone does NOT add

- Literal type suffixes (`5u8`, `2.5_f32`).
- `as` casts.
- Untyped float-literal type defaulting beyond `f64`.
- Bitwise operators on integers (`&`, `|`, `^`, `<<`, `>>`).
- Comparison operators producing pedagogical NaN-comparison notes.
- Per-platform `usize` / `isize` (pinned to 64-bit per FR-011).
- BigInt / arbitrary-precision integers.

## When extending in future levels

- **M06 (references)** doesn't interact with M03.2 directly — borrows are on bindings, irrespective of scalar type. `&u8` and `&i32` work the same way under M06's machinery.
- **M07 (heap)** introduces non-Copy types (`Box<T>`, `Vec<T>`, `String`). `Ty::is_copy()` becomes non-trivial again — it returns `true` for `Int(_)` / `Float(_)` / `Bool` / `Unit` (post-M03.2 scalars are all Copy) and `false` for the new heap-allocated constructors.
- **M08 (threads)** is independent.

The point of M03.2 is: scalar arithmetic is *settled*. Future milestones can assume the lattice doesn't churn.
