# Quickstart — M03.1 development + verification

Audience: maintainer + contributors working on M03.1 or consuming the revised protocol from M05+.

## Run all tests

```bash
# Full suite after M03.1 lands.
cargo test                       # m01 (8) + m02 (16) + m03 (8, re-baselined) + lib (16+ with new ReturnValue test)

# Targeted:
cargo test --test m03            # re-baselined snapshots
cargo test --test m01            # byte-identical
cargo test --test m02            # byte-identical
cargo test --lib ui::            # Cursor + new ReturnValue-handling tests
```

If `cargo test --test m03` reports snapshot drift on the first run after the implementation changes, that's expected — accept via:

```bash
INSTA_UPDATE=always cargo test --test m03
```

Then visually inspect every `.snap` for the expected diff per `research.md` R-008 table:

- All `SlotDrop` events for Copy-typed slots have been removed.
- Each non-halted function frame has exactly one new `ReturnValue` event between the body's last evaluation step and its `FrameLeave`.
- `FrameEnter.params` field is absent.
- `div_by_zero` snapshot is unchanged (halts before any drops or returns).

## Regenerate traces for M04

```bash
cargo run --release --bin gen_traces
```

This rewrites `web/traces/*.json` with the new event schema. Trunk's pre-build hook also does this automatically when serving.

## Verify the M04 page

Per the project's UI QA-split convention, manual QA falls to the maintainer:

```bash
cd web && trunk serve --open
```

Walk the `specs/005-m04-ui-shell/quickstart.md` SC-008 procedure (10 steps). Look specifically for:

1. **For `m03_fn_call`**: At the step matching what was previously "step 7 = SlotDrop(b)", confirm `b` is **still visible** in the `add()` frame. The whole `add()` frame card stays populated through what was previously the drop sequence.
2. **For any non-error sample**: Just before the `add()` (or `main()`) frame card disappears at `FrameLeave`, observe a **transient return-value annotation** on the frame card (e.g. `→ 5` or similar visual) for one cursor step.
3. **For `m03_div_by_zero`**: Should look identical to before — halting happens before drops or returns.

## Add a new sample after M03.1

Same procedure as in `specs/005-m04-ui-shell/quickstart.md`. Just be aware: snapshots will have the M03.1 schema (no Copy-type drops; ReturnValue events present).

## When extending in M07+

When M07 adds heap-allocated `Ty` variants:

1. Extend `Ty` (in `src/typeck.rs`) with new variants — `Box`, `Vec`, `String`, etc.
2. Add arms to `Ty::is_copy()` returning `false` for the new variants. The compiler will flag `is_copy()` as non-exhaustive if you forget — that's the safety net.
3. The evaluator's `drop_current_scope` will now emit `SlotDrop` events for those types automatically (the gate flips per type).
4. M07's `HeapAlloc`/`HeapRealloc`/`HeapFree` events fire alongside.

No changes to M03.1's contract needed — the protocol revision is forward-compatible.

## Debug a state-at-N mismatch

If a snapshot test fails unexpectedly after M03.1:

1. Run `cargo insta review` to inspect the diff interactively.
2. Compare against the R-008 expected-counts table.
3. Common culprits:
   - `is_copy()` returns `false` for some `Ty` variant it shouldn't (forgot the match arm for Unit, etc.).
   - `ReturnValue` emitted in the wrong position (before instead of after body completion; after instead of before drops).
   - `FrameEnter` JSON still has a `params` field somewhere because deserialization is lenient.

## Trace JSON schema validation (optional)

You can validate any trace file matches the new schema:

```bash
jq '.events | map(keys[0]) | unique' web/traces/m03_fn_call.json
# Should show: ["FrameEnter", "FrameLeave", "ReturnValue", "SlotAlloc", "SlotWrite"]
# Notably: NO "SlotDrop" entries for L1 samples.
```

## Implementer notes

- **`is_copy()` is exhaustive on purpose** — when M07 adds new `Ty` variants, Rust's match-completeness check will flag `is_copy()` as missing arms. Add them deliberately, don't bypass with `_`.
- **`ReturnValue` is emitted EVEN for `Unit`-returning functions** (e.g. `fn main()`). Pedagogically, "this function returned (unit)" is a meaningful tick. Don't gate emission on the value being non-Unit.
- **The `pending_return` field is transient** — `Some` only when the immediately-previous event was a `ReturnValue`. Stepping forward to the `FrameLeave` flips it back to `None`. Don't try to persist it across multiple steps.
- **M01 + M02 snapshots must stay byte-identical**. If they drift, M03.1 has accidentally touched shared code (likely `Ty` or `Span` or `Value` in a way that affects Debug output). Investigate.
- **Trace JSON breaking change**: pre-M03.1 trace files (if any persisted somewhere) are invalid after M03.1 lands because of the `FrameEnter.params` removal. The `gen_traces` binary always regenerates from sources, so this is only a concern for any externally-cached traces — not applicable for our flow.
