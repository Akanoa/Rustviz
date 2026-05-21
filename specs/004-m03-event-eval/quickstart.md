# Quickstart — M03 development

Audience: maintainer + contributors working inside M03 or consuming its output from M04+.

## Run the M03 test suite

```bash
cargo test --test m03
```

M01 + M02 tests must still pass (SC-008):

```bash
cargo test --test m01
cargo test --test m02
cargo test    # runs everything
```

Snapshot review with `cargo insta review` (same workflow as M01/M02).

## Use the M03 API

```rust
use rustviz::{parse, resolve, typeck, evaluate, SourceMap};

let mut sm = SourceMap::new();
let file = sm.add("input.rs".into(), src);

let program = parse(file, &sm)?;
let resolution = resolve(&program)?;
let types = typeck(&program, &resolution)?;
let events = evaluate(&program, &resolution, &types)?;

// events: Vec<MemEvent>
for event in &events {
    println!("{event:?}");
}
```

The last event in `events` is normally a `FrameEnter`-matching `FrameLeave`. If runtime evaluation hit an error, the last event is a `MemEvent::Note { kind: NoteKind::RuntimeError, ... }`.

## Pattern-matching on events

Consumers should match all variants — Rust's match-completeness will flag missing arms as the enum grows. For M03 the practical subset is:

```rust
use rustviz::event::*;

match event {
    MemEvent::FrameEnter { fn_name, params, .. } => { /* push frame card */ }
    MemEvent::FrameLeave { return_value, .. } => { /* pop frame card */ }
    MemEvent::SlotAlloc { name, ty, .. } => { /* add slot to frame */ }
    MemEvent::SlotWrite { slot_id, value, .. } => { /* update slot value */ }
    MemEvent::SlotDrop { slot_id, .. } => { /* mark slot dropped */ }
    MemEvent::Note { kind, message, .. } => { /* show notice */ }
    // Variants below not emitted in L1 — handle when M06+ ship.
    MemEvent::SlotMove { .. }
    | MemEvent::HeapAlloc { .. }
    | MemEvent::HeapRealloc { .. }
    | MemEvent::HeapFree { .. }
    | MemEvent::BorrowShared { .. }
    | MemEvent::BorrowMut { .. }
    | MemEvent::BorrowEnd { .. }
    | MemEvent::LockAcquire { .. }
    | MemEvent::LockRelease { .. }
    | MemEvent::ArcClone { .. }
    | MemEvent::ArcDrop { .. }
    | MemEvent::ThreadSpawn { .. }
    | MemEvent::ThreadJoin { .. }
    | MemEvent::ThreadPark { .. } => {
        // Not exercised by L1; reachable from M06+. M04 can panic or no-op.
    }
}
```

## Add a new test

1. Create `tests/samples/m03_<name>.rs` with the input L1 program.
2. Add a `sample_test!(<test_fn_name>, "m03_<name>")` line in `tests/m03.rs`.
3. Run `cargo test --test m03`. First run creates `.snap.new` files.
4. Accept with `cargo insta review` (or `INSTA_UPDATE=always cargo test --test m03`).
5. Visually inspect the snapshot:
   - Events appear in source-execution order.
   - Every event has a non-empty `span`.
   - `FrameEnter` / `FrameLeave` pair correctly.
   - `SlotAlloc` / `SlotDrop` pair correctly (LIFO within a frame).
   - No `SlotMove` events (L1 only has Copy types).

## Debug an evaluation failure

```rust
match evaluate(&program, &resolution, &types) {
    Ok(events) => {
        // Last event might be a runtime-error Note.
        for e in &events {
            if let MemEvent::Note { kind: NoteKind::RuntimeError, message, span } = e {
                let (line, col) = sm.line_col(*span).unwrap_or((0, 0));
                eprintln!("runtime error at {line}:{col}: {message}");
            }
        }
    }
    Err(parse_err) => {
        // Should not happen if M02 succeeded; if it does, M02 → M03 contract drift.
        eprintln!("static error: {}", parse_err.message);
    }
}
```

## What M03 accepts (in 30 seconds)

- Any program that passed M02 (parse → resolve → typeck all green).
- L1 syntactic forms only: primitives, let/let mut, fn, scopes, blocks-as-expr, if-as-expr, operators with precedence, calls of top-level fns.
- Functions can recurse, up to a depth of 100 frames.

## What M03 explicitly rejects

- Static failures from M02 propagate as `ParseError` (M03 doesn't catch them — the caller should chain `?`).
- Runtime errors emit a `Note { kind: RuntimeError }` and stop the stream:
  - Integer overflow on `+ - * / %` between i32 bounds.
  - Division by zero (`x / 0`, `x % 0`).
  - Recursion depth exceeded (> 100 frames).

## Implementer notes (internal)

When extending in M06/M07/M08:

- **Adding new value variants** (e.g. `Value::Box(HeapAddr)` for M07): exhaustive matches in `src/eval.rs` will flag every site needing an update. Add the variant, update the matches.
- **Filling event variant payloads** (e.g. `BorrowShared { ... }` becoming a real M06 event): the variant already exists in `MemEvent` with M03 payload types; M06 may need to refine those payloads if they turn out wrong. Refining payload field types IS a breaking change — coordinate with M04.
- **`SlotMove` payload**: M07 will emit it for heap-allocated moves. The payload (`from: SlotId, to: SlotId, value: Value`) is already designed; just hook the eval path.

## LOC and warnings checks (M03 equivalents)

```bash
# Stay under the soft cap (1500 LOC for event.rs + eval.rs combined):
find src/event.rs src/eval.rs -name '*.rs' -print0 | xargs -0 wc -l

# Zero warnings:
RUSTFLAGS="-D warnings" cargo build --release
RUSTFLAGS="-D warnings" cargo test --test m03
```

## Insta basics

Same as M01/M02. `INSTA_UPDATE=always cargo test --test m03` for non-interactive first-run accept.
