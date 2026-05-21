# Quickstart — M05 development + verification

Audience: maintainer + contributors working on M05 or extending in M06+.

## Run the page

```bash
cd web && trunk serve --open
```

Same as M04. The pre-build `gen_traces` hook is gone (M05 doesn't need pre-recorded traces) so startup is slightly faster.

## Run all tests

```bash
cargo test                       # m01 (8) + m02 (16) + m03 (8) + lib (~18 with new pipeline tests)
cargo test --lib pipeline::      # M05's new pipeline runner unit tests
cargo test --lib ui::            # M04+M03.1 cursor tests; M05 doesn't change these
```

`m01`, `m02`, `m03` snapshots are byte-identical to post-M03.1; M05 doesn't touch them.

## Manual QA procedure (SC-008)

Procedure for the maintainer to verify M05 end-to-end. ~5 minutes.

1. Run `cd web && trunk serve --open`. Page should open with `m05_minimal.rs` (or whatever's set as default) loaded into the editor; the stacks panel shows the parsed trace at position 0.

2. **US1 — Edit a live program**:
   - Select `Function Call` from the dropdown. Editor populates with the M03 fn-call sample.
   - Change `add(2, 3)` to `add(10, 20)` in the editor.
   - Wait ≤ 1 second. The stacks panel should reset and now show `a = 10, b = 20, → 30` when stepped through.
   - Verify Step Forward / Play work; the cursor walks through the **new** trace.

3. **US1 — Type from scratch**:
   - Clear the editor (select-all + delete).
   - Type `fn main() { let x = 7 + 3; }`.
   - After the debounce, the stacks panel should show `main` with `x = 10` once stepped through.

4. **US2 — Parse error UX**:
   - Edit the source to `fn main() { let x = ; }` (delete the `7 + 3`).
   - Within ~1 second, observe a **red wavy underline** under the offending span (likely the `;` after `=`).
   - The status bar should show the error message ("expected expression" or similar) with red styling.
   - Click Play / Step Forward — buttons should be **disabled** (grayed out, no advancement).
   - Click Rewind — should still work (cursor resets to 0; nothing visible changes because the trace is empty).
   - Fix the syntax: change to `fn main() { let x = 1; }`. The underline + status disappear within ~1 second. Step / Play re-enable.

5. **US2 — Typeck error UX**:
   - Edit to `fn main() { let x: i32 = true; }`.
   - Underline appears on `true` (the mismatched expression). Status bar shows the typeck error.
   - Fix and confirm clear.

6. **US3 — M05 samples**:
   - Open the dropdown; verify ≥ 3 M05-prefixed options appear alongside the M03/M04 ones.
   - Select `m05_double`. Editor shows `fn double(n: i32) -> i32 { n + n } fn main() { let r = double(21); }`. Step through; observe `r = 42`.
   - Select `m05_broken_parse`. Editor shows the broken source; underline appears immediately on load (the debounce still fires for the load-triggered editor update).

7. **No regressions**:
   - Select all four original M03/M04 samples in turn. Each should populate the editor with its original source, and the trace should be the same shape as in M04 (just now generated live instead of fetched as JSON).
   - Walk through the existing M04 SC-008 procedure quickly — frame grayed-on-leave, return-value annotation, current-call-site highlight, etc., all still work.

If any of these steps fail, treat as a bug and fix on-branch before commit.

## Developer notes

### Adding a new sample

1. Drop the `.rs` source into both `tests/samples/m05_<name>.rs` and `web/samples/m05_<name>.rs` (identical content).
2. Add an `<option value="m05_<name>">Display name</option>` to the dropdown in `web/index.html`.
3. The trunk `copy-dir` directive picks up the new file automatically; no other code change needed.

### Adding a new pipeline stage (future)

If M06+ adds a new pipeline stage between typeck and evaluate (e.g. borrow-checking):

1. Add a new variant to `CompileStage` (closed-enum-with-revisions rule applies — additive only).
2. Add a `From` impl on the new error type → `CompileError`.
3. Insert the new stage call in `run_pipeline` between typeck and evaluate.

The `Player::set_source` JSON shape is unchanged. JS doesn't need updates beyond optionally surfacing the new `stage` value in error messages.

### Debugging a "trace doesn't update" symptom

Common causes:
- The editor's `updateListener` isn't firing — check the browser console for JS errors during init.
- The debounce timer is racing with a sample-load — check whether `editor.setValue` is firing within ≤ 300 ms of the user's last keystroke (it shouldn't be, but a sloppy sample-load might).
- WASM cache: do a hard reload (Ctrl+Shift+R / Cmd+Shift+R) after any Rust change.

### Debugging a "underline doesn't clear" symptom

The error decoration field's `update` callback dispatches `setError.of(null)` on `set_source` Ok. If the underline persists, check that the JS render path is dispatching the clear effect.

## What this milestone does NOT add

- Live-evaluation without debounce (deliberate).
- A "Run" button (the demo path uses the implicit auto-run; the Play button still exists for stepping).
- IDE-style hover tooltips for error details (just status-bar message).
- Code formatting / auto-indent on enter (default CodeMirror behavior; no `rustfmt`).
- An undo/redo stack of traces (each successful run replaces the previous).

## When extending in M06+

When M06 adds reference / borrow events:

1. The `Cursor` already consumes any `MemEvent` stream. New variants extend the stream additively.
2. The `Player::set_source` JSON contract stays. New events surface via `state.frames` etc.
3. The error pipeline stays as-is: M06 will add a borrow-check stage; add a `CompileStage::Borrowck` variant and a `From` impl.

The point of M05 is that the *wiring* is stable. M06+ just adds events flowing through it.
