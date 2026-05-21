# Research — M03 Implementation Decisions

Decision / Rationale / Alternatives for the event model and L1 evaluator.

## Module layout

### R-001 — Flat `event.rs` + `eval.rs`

- **Decision**: single `src/event.rs` (enum + all payload types) and single `src/eval.rs` (evaluator). Both at flat top-level under `src/`.
- **Rationale**: estimated 200–400 LOC each fits comfortably in one file. CLAUDE.md sketches them as flat files (`event.rs`, `eval/` — we collapse `eval/` to `eval.rs` for the same reason M01 didn't pre-split: the file isn't large enough yet to warrant a directory).
- **Alternatives considered**:
  - `eval/{mod.rs, frame.rs, value.rs}` split — premature; revisit if `eval.rs` exceeds 600 LOC mid-implementation.

## API shape

### R-002 — Single public entry point

- **Decision**: `pub fn rustviz::evaluate(program: &Program, resolution: &Resolution, types: &TypeMap) -> Result<Vec<MemEvent>, ParseError>`. Plus `pub mod event` so consumers can refer to `rustviz::event::MemEvent` etc.
- **Rationale**: M03 is one pass with one output (the event stream). No split needed. The `event` module re-exposes the type vocabulary M04 will pattern-match on.
- **Alternatives considered**:
  - Builder pattern with `Evaluator::new(...).run()` — adds boilerplate without flexibility. Rejected.
  - Returning a streaming iterator instead of a `Vec` — would be useful for very large programs, but L1 inputs are tiny. `Vec` is simpler. Rejected.

## Runtime error API

### R-003 — Runtime errors as `Note` events, stream returns `Ok`

- **Decision**: when the evaluator detects a runtime error (integer overflow, division by zero, recursion depth exceeded), it emits a `MemEvent::Note { kind: NoteKind::RuntimeError, message, span }`, stops further evaluation, and returns `Ok(events_so_far)`. The stream always includes the partial-execution events leading up to the error. The `ParseError` return path is reserved for static-time / invariant-violation errors that should be unreachable when M02 succeeded.
- **Rationale**:
  - M04 can replay events up to the failure point and show the user *where* it broke — pedagogical win over hard-stopping.
  - Keeps the API symmetric with M01 (parse) and M02 (resolve/typeck) which already use `ParseError` for *static* failures only. Runtime errors are a different category.
  - The `Note` carries `NoteKind` so consumers can distinguish "informational note" from "fatal runtime error" without parsing message strings.
- **Alternatives considered**:
  - Return `Err(ParseError)` on runtime error — discards all the events emitted up to that point; loss of partial trace makes M04's job harder. Rejected.
  - Two return types: `Result<Vec<MemEvent>, (Vec<MemEvent>, RuntimeError)>` — ugly. Rejected.
  - Separate `RuntimeError` type — adds a second error type to the crate; not justified for M03. Rejected.

## Frame lifecycle

### R-004 — `FrameLeave` carries the return value

- **Decision**: `MemEvent::FrameLeave { frame_id, return_value: Value, span }`. The function's computed result is encoded directly in the FrameLeave event.
- **Rationale**:
  - Self-contained: M04 can render "fn returned `5`" by reading FrameLeave alone, no caller coordination needed.
  - Simple to emit: the evaluator computes the body's tail value, then emits FrameLeave with it before unwinding.
  - For functions with implicit `()` return (no `-> T` annotation, no tail expression), `return_value` is `Value::Unit`.
- **Alternatives considered**:
  - A separate `ReturnValue` event between body and `FrameLeave` — extra event, no information gain. Rejected.
  - Caller's next `SlotWrite` implicitly carries the value — works but couples caller and callee; harder for M04 to render "fn returned X" without scanning ahead. Rejected.

## Slot identity

### R-005 — `SlotId` distinct from `BindingId`, allocated per slot instance

- **Decision**: `SlotId(u32)` is a fresh identifier allocated per runtime slot, separate from M02's `BindingId`. Each recursive function call gets new `SlotId`s for its params and locals; each `let`-statement at runtime gets a new `SlotId` (shadowing within a frame still allocates a new `SlotId`, since each shadow is a distinct runtime slot).
- **Rationale**:
  - Recursion requires distinct slots per call frame — `BindingId` is static (one per declaration site), `SlotId` is dynamic (one per runtime instance).
  - L1 has no recursion in the typical sense (the parser accepts it, but tests focus on non-recursive examples), so for most L1 programs `SlotId` ≈ `BindingId` in count. But the distinction is real and must be honored from M03 forward.
- **Alternatives considered**:
  - Reuse `BindingId` as `SlotId` — breaks under recursion (two active slots for the same binding). Rejected.
  - `SlotId = (BindingId, FrameId)` tuple — works but verbose in snapshots. Rejected; fresh `u32` newtype is cleaner.

### R-006 — `FrameId(u32)` allocated per function call

- **Decision**: `FrameId(u32)` fresh per `FrameEnter`. Sequential allocation from 0. Inner calls get higher FrameIds.
- **Rationale**: matches `SlotId` pattern; lets M04 distinguish "this is the 3rd call to `f`" from "this is the 4th call".

## Value type

### R-007 — `Value` enum with L1 variants only

- **Decision**:

  ```rust
  pub enum Value {
      Int(i64),
      Bool(bool),
      Unit,
  }
  ```

  Three variants. M07 will add heap-allocated variants (`Box`, `Vec`, `String`); M03 doesn't pre-declare them since their payloads depend on heap-event design that doesn't ship until M07.
- **Rationale**:
  - Mirrors the `Ty` enum from M02 — one variant per L1 type.
  - `i64` (not `i32`) for `Int` because Rust integer literals parse to i64 in M01's lexer; storing i64 avoids narrowing concerns. M02 typeck'd as `i32` from the user's POV; the runtime stores i64 internally and the visualization can render as i32. (Future: tighten if values larger than i32::MAX trigger overflow events.)
- **Alternatives considered**:
  - Storing `i32` for value — matches user's mental model but requires overflow detection at literal eval. Rejected for M03; defer to a follow-up if it matters.
  - `Box`/`Vec`/`String` placeholder variants now — premature; M07 will design them properly. Rejected.

## `SlotMove` infrastructure (L1 can't exercise it)

### R-008 — Unit test in `event.rs` verifies `SlotMove` variant construction

- **Decision**: a `#[test]` inside `src/event.rs` constructs a `MemEvent::SlotMove { from: SlotId(0), to: SlotId(1), value: Value::Int(5), span: dummy_span }` and asserts its `Debug` output isn't empty. Satisfies FR-006 ("infrastructure must be present and exercised by at least one test").
- **Rationale**: L1's only types (i32, bool) are `Copy`, so no real L1 program emits `SlotMove`. CLAUDE.md's L1 description includes "moves of non-Copy types" as a forward-looking commitment, exercised in M07 when `Box`/`Vec`/`String` arrive. M03 needs to keep the variant compilable and confirm its shape.
- **Alternatives considered**:
  - Skip the unit test — leaves the variant unverified; might silently drift. Rejected.
  - A "fake" non-Copy sample via test-only API — overengineered. Rejected.

## Evaluation strategy

### R-009 — Depth-first, left-to-right, deterministic walk

- **Decision**: the evaluator walks the AST depth-first, left-to-right, matching standard expression evaluation order. Statements within a block execute in source order. Function calls evaluate arguments left-to-right before entering the callee. `&&` / `||` honor short-circuit semantics (FR-008): the RHS is not evaluated when the LHS already determines the result.
- **Rationale**: matches Rust's actual evaluation order, predictable for the L1 audience (newcomers). Determinism (FR-010) falls out naturally.
- **Alternatives considered**:
  - Right-to-left or non-deterministic — confusing for a pedagogical visualizer. Rejected.

### R-010 — Scope and frame stack in `Evaluator`

- **Decision**: the evaluator owns `frames: Vec<Frame>` (active call stack, innermost last); each `Frame` owns `scopes: Vec<Scope>` (active block scopes within that frame, innermost last); each `Scope` owns `locals: Vec<LocalSlot>` (active bindings in declaration order). Lookups walk inner-to-outer through the current frame's scopes (no cross-frame lookups in L1 — no closures).
- **Rationale**: standard interpreter stack model; supports recursion (push/pop frames), nested blocks (push/pop scopes per block), and LIFO drops (pop locals at scope exit, emitting SlotDrop for each).
- **Alternatives considered**:
  - Flat scope list, manually scoped — fragile. Rejected.
  - Persistent (immutable) scope map — overkill. Rejected.

### R-011 — `BindingId → FnDecl` index built once at evaluator construction

- **Decision**: the evaluator builds a `HashMap<BindingId, &ast::FnDecl>` at construction by walking `program.items` and using `resolution.bindings` to map fn names to ids. Subsequent calls look up the AST node in O(1).
- **Rationale**: cleaner than scanning `program.items` on every call. The map is small (one entry per top-level fn).
- **Alternatives considered**:
  - Linear scan on every call — O(n) per call, fine for L1 sizes. Rejected for clarity.
  - Store FnDecl pointers in `Resolution` itself — couples resolve to eval. Rejected.

## Determinism + iteration order

### R-012 — `Vec<MemEvent>` only; no `HashMap` in public output

- **Decision**: the public output is `Vec<MemEvent>` populated in emission order. Internal state may use `HashMap` (e.g. the BindingId→FnDecl index) since its iteration order doesn't leak into the output.
- **Rationale**: determinism is satisfied by writing to a `Vec` in deterministic walk order.
- **Alternatives considered**:
  - `IndexMap<EventId, MemEvent>` — overengineered for an append-only stream. Rejected.

## Recursion depth limit

### R-013 — Limit at 100 frames, emit a `Note` and stop

- **Decision**: a `recursion_depth` counter increments on each `FrameEnter` and decrements on `FrameLeave`. When the counter would exceed 100 before pushing a new frame, the evaluator emits `Note { kind: RuntimeError, message: "recursion depth exceeded (100 frames)" }` and stops.
- **Rationale**: prevents test-time infinite recursion from hanging the snapshot suite. 100 is enough for any meaningful L1 sample; recursive examples in L1 are rare anyway.
- **Alternatives considered**:
  - Unlimited recursion — risks stack overflow in the host process. Rejected.
  - Higher limit (1000, 10000) — no benefit for L1; lower number catches buggy tests faster. Rejected.

## Constitution

### R-014 — Same vacuous PASS

- **Decision**: `.specify/memory/constitution.md` still unfilled. No gates apply.

## Open question — not blocking

- **Where to point `SlotDrop` spans for block-end drops**: when a binding goes out of scope at block end, the `SlotDrop` event needs a span. Options: (a) the binding's original declaration span (re-points the reader back to where the binding came from), (b) the block's closing `}` span (where the drop happens lexically). Choice doesn't affect correctness; (b) is probably more natural for "this is where the drop fires" but (a) is more pedagogically useful. Default to (a) — declaration span — and revisit if M04's UX wants (b).
