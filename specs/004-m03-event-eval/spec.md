# Feature Specification: M03 — Event Model + Level 1 Evaluator

**Feature Branch**: `004-m03-event-eval`
**Created**: 2026-05-21
**Status**: Draft
**Input**: User description: "M03"

**Authoritative scope source**: [`MILESTONES.md` › M03 — Event model + Level 1 evaluator](../../MILESTONES.md). The In scope / Out of scope / Entry criteria / Exit criteria / Demo lines in that block are the contract for this feature. This spec elaborates user stories, requirements, and success criteria; it does not redefine scope.

## User Scenarios & Testing *(mandatory)*

The "users" of this feature are internal: the M04 UI shell milestone (which consumes the `Vec<MemEvent>` to drive panel rendering and the replay cursor) and the contributor writing snapshot tests. M03 is the **architectural pivot** — before it, the pipeline is parse → resolve → typeck (all static analysis); after it, the interpreter emits a typed event stream that downstream milestones replay. The stream is the single source of truth (per CLAUDE.md › Architecture).

### User Story 1 — Emit a complete event stream for a valid Level 1 program (Priority: P1)

A contributor writes a valid L1 program and runs the M03 pipeline on the M02 resolved + typed AST. They get back a `Vec<MemEvent>` containing, in source-execution order, the events needed to fully replay the program's stack-level memory behavior: function call boundaries (`FrameEnter` / `FrameLeave`), per-binding allocation / write / drop (`SlotAlloc` / `SlotWrite` / `SlotDrop`), and block-scope teardown. Snapshot tests pin this output for a small set of representative L1 programs (arithmetic, function call with parameters, if-as-expression branch selection, shadowing, nested blocks).

**Why this priority**: this is the entire point of M03. Without the event stream, M04 has nothing to render. M03 closing without a working evaluator means the project is stuck on the foundation layer with no path to demo.

**Independent Test**: write `tests/samples/m03_arithmetic.rs` (`fn main() { let x = 2 + 3; }`), run `cargo test --test m03`, observe a snapshot showing the events `FrameEnter(main) → SlotAlloc(x: i32) → SlotWrite(x = 5) → SlotDrop(x) → FrameLeave(main)`. Verify by visual inspection.

**Acceptance Scenarios**:

1. **Given** a program `fn main() { let x = 2 + 3; }`, **When** the evaluator runs, **Then** the event stream contains (in this order): `FrameEnter` for main, `SlotAlloc` for x with type `i32`, `SlotWrite` for x with value `5`, `SlotDrop` for x at end of body block, `FrameLeave` for main.
2. **Given** a program with a function call `fn add(a: i32, b: i32) -> i32 { a + b } fn main() { let r = add(2, 3); }`, **When** the evaluator runs, **Then** entering `add` emits `FrameEnter(add)` followed by `SlotAlloc` + `SlotWrite` for each parameter (`a = 2`, `b = 3`); the function returns; the events `SlotDrop(b)`, `SlotDrop(a)`, `FrameLeave(add)` close the call; then back in main `SlotWrite(r = 5)` records the result.
3. **Given** an if-expression `let v = if true { 1 } else { 2 };`, **When** the evaluator runs, **Then** only the taken branch's events are emitted (the `else` branch produces no events); `v` ends up with value `1`.
4. **Given** shadowing `let x = 5; let x = 10;`, **When** the evaluator runs, **Then** two distinct `SlotAlloc` events are emitted with distinct `SlotId`s; the second binding shadows the first; both are dropped at end of scope (LIFO order: inner `x` dropped first, outer `x` dropped second).
5. **Given** a program with a nested block `{ let y = 1; }; let z = 2;`, **When** the evaluator runs, **Then** `SlotDrop(y)` fires at the end of the inner block (before the outer block's later statements), and `SlotDrop(z)` fires at the end of the outer block.

---

### User Story 2 — Every event carries a span pointing into the AST (Priority: P1)

Every `MemEvent` records the `Span` of the AST node that triggered it — the `let` statement for a `SlotAlloc`, the `fn` declaration for a `FrameEnter`, the closing `}` (or equivalent) for the corresponding `SlotDrop` / `FrameLeave`. M04 will use these spans to highlight code positions in the editor as the cursor advances through the stream; without spans, the editor highlighting feature is impossible.

**Why this priority**: same level as US1 because span-bearing events are an inseparable part of the contract M04 relies on. Adding spans later means M04 sits idle. Doing them right at M03 is the cheapest moment.

**Independent Test**: visually inspect any snapshot from US1 — every event has a non-empty `Span` field. No `Span(0, 0)` placeholders on non-empty programs.

**Acceptance Scenarios**:

1. **Given** any successful M03 event stream, **When** the snapshot is inspected, **Then** every event carries a non-empty `Span` (start < end, or zero-length only at deliberate end-of-input / EOF positions).
2. **Given** an event for `let x = 5`, **When** the event is examined, **Then** the `Span` points at the `let` statement (or a sub-position chosen consistently — e.g. the `let` token through the `;`).
3. **Given** a `FrameLeave` event for a function, **When** the event is examined, **Then** the `Span` points at the function's closing `}` (or the body's end position).

---

### User Story 3 — Full event enum exposed for downstream extension (Priority: P2)

The `MemEvent` enum declares variants for **all** event categories listed in `CLAUDE.md` › Event model: `Threads` (ThreadSpawn / Join / Park), `Frames` (Enter / Leave), `Stack slots` (Alloc / Write / Move / Drop), `Heap` (Alloc / Realloc / Free), `Borrows` (Shared / Mut / End with BorrowId), `Synchronization` (LockAcquire / Release / ArcClone / Drop), and `Pedagogy` (`Note { kind, message, span }`). M03 actively emits only the Frames + Stack-slots + Note subset (L1 needs nothing else); the other variants exist with payload types defined but are never emitted from an L1 evaluator. Downstream milestones (M06 borrows, M07 heap, M08 threads) fill in their payloads in place without touching the enum shape.

**Why this priority**: locking the enum shape now means M06–M08 are additive (new payloads) rather than refactoring (new variants). Without this, every later milestone is a coordinated enum change requiring re-snapshotting every test. P2 because if the enum turned out to need a variant later, we could add it (with breakage); but pre-declaring is much cheaper.

**Independent Test**: a smoke test confirms every CLAUDE.md event category appears as a `MemEvent` variant, callable by future code. Compiles cleanly with all variants declared even though most are unused in M03 tests.

**Acceptance Scenarios**:

1. **Given** the `MemEvent` enum, **When** inspected, **Then** it has a variant for every category listed in `CLAUDE.md` › Event model.
2. **Given** the `Note` variant, **When** an L1 evaluator emits one (e.g. as a smoke test or as part of evaluation for a pedagogically-relevant moment), **Then** the message + span + kind appear in the event stream.
3. **Given** an M06+ extension intent, **When** the contributor adds borrow tracking, **Then** they fill in payloads for `BorrowShared` / `BorrowMut` / `BorrowEnd` without adding new variants — additive change only.

---

### Edge Cases

- **`SlotMove` for non-Copy types**: L1 has only `i32` and `bool`, both `Copy`. So pure L1 programs do not in practice produce `SlotMove` events — every `let y = x` for Copy types is conceptually a copy. The `SlotMove` variant exists in the enum (M07+ will emit it for `Box` / `Vec` / `String`), but M03's L1 evaluator simply never emits it. This is consistent with CLAUDE.md's "SlotMove is intentionally distinct from SlotDrop" — the distinction is built in even though L1 doesn't exercise the move path.
- **Empty program**: an empty input has no items; the event stream is `[]` (no events). Tested.
- **Empty function body**: `fn main() {}` emits `FrameEnter(main)` then immediately `FrameLeave(main)` — no slot events in between.
- **Function with no return value** (implicit `()` return): `FrameLeave` carries `Unit` as the return value (or some agreed encoding — implementation detail).
- **If-as-statement** (no else, body with no tail): only the `then` branch's events are emitted if the condition is true; nothing is emitted if the condition is false. The `let` binding that contains the `if` doesn't fire `SlotWrite` until the if completes.
- **Operator short-circuit** for `&&` and `||`: the right-hand side is evaluated only if the left-hand side's value requires it (`false && x` doesn't evaluate `x`; `true || x` doesn't evaluate `x`). M03 must honor this. Slot reads inside the RHS that doesn't get evaluated do not produce events.
- **Integer overflow / division by zero**: M03 either panics (matching Rust's default debug behavior) or emits a `Note` with kind `RuntimeError` and stops. Decision: emit a `Note` and stop. Reason: pedagogical — showing "this is where the program crashed" is more useful than aborting the visualization.
- **Function-pointer-as-value attempted**: caught earlier by typeck (M02 rejects function references in value position). M03 doesn't have to defend against it.
- **Recursion limit**: M03 includes a depth limit on call stack (e.g. 100 frames) to prevent runaway recursion crashing the evaluator. On hit, emit a `Note` and stop.
- **Determinism**: identical input must produce byte-identical event streams across runs. No non-deterministic iteration order, no time-dependent values.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST define a `MemEvent` enum with variants for every category in `CLAUDE.md` › Event model: `ThreadSpawn`, `ThreadJoin`, `ThreadPark`, `FrameEnter`, `FrameLeave`, `SlotAlloc`, `SlotWrite`, `SlotMove`, `SlotDrop`, `HeapAlloc`, `HeapRealloc`, `HeapFree`, `BorrowShared`, `BorrowMut`, `BorrowEnd`, `LockAcquire`, `LockRelease`, `ArcClone`, `ArcDrop`, `Note`. Variants used only by later milestones may carry partial / placeholder payloads — the requirement is variant presence, not full payload semantics.
- **FR-002**: System MUST define a `Pointee` enum with variants `Slot(SlotId)` and `Heap(HeapAddr)`. (Per CLAUDE.md › Event model.)
- **FR-003**: System MUST emit a `FrameEnter` event when a function call begins and a matching `FrameLeave` when it returns. Frames nest correctly across recursive calls; `FrameLeave` events appear in LIFO order relative to their matching `FrameEnter`s.
- **FR-004**: System MUST emit `SlotAlloc` for every introduced binding (let-stmt, function parameter) and `SlotDrop` at the end of every scope (block, function body) in reverse declaration order.
- **FR-005**: System MUST emit `SlotWrite` whenever a slot receives a value — once on initialization for a let-binding, and on every assignment for `let mut` bindings. (L1 doesn't support reassignment as an expression, so for M03 this is one write per binding.)
- **FR-006**: System MUST emit `SlotMove` ONLY when a non-Copy value is moved between slots. For pure L1 (Copy-only types: `i32`, `bool`), `SlotMove` is never emitted. The infrastructure must be present and exercised by at least one snapshot test (e.g. via a contrived test that constructs a non-Copy value through a future test-only API, or by verifying via unit test that the `SlotMove` event variant is constructible).
- **FR-007**: Every `MemEvent` MUST carry a `Span` (reusing M01's `Span` type) pointing into the AST source position that triggered it. Spans are not optional.
- **FR-008**: System MUST honor short-circuit evaluation of `&&` and `||`: the RHS is only evaluated when needed. Events for unevaluated sub-expressions are not emitted.
- **FR-009**: System MUST honor `if` branch selection: only the taken branch's events are emitted. Untaken branches produce no events.
- **FR-010**: System MUST be deterministic: identical input produces a byte-identical event stream across runs. No `HashMap` iteration leakage, no time-dependent values.
- **FR-011**: System MUST detect runtime errors (integer overflow, division by zero, recursion depth exceeded) and either emit a `Note` event with a `RuntimeError` kind and stop, OR return an error result. Choice is plan-phase but the behavior must be consistent and snapshot-tested.
- **FR-012**: System MUST expose a public `evaluate(program, &Resolution, &TypeMap) -> Result<Vec<MemEvent>, ParseError>` entry point. Function reuses M01's `ParseError` type for static-time failures (which shouldn't occur if M02 succeeded). Runtime errors per FR-011 surface as `Note` events in the stream or as a separate result variant — plan-phase.
- **FR-013**: System MUST expose a Cargo test target `m03` (`cargo test --test m03` runs the M03 snapshot suite).

### Key Entities

- **MemEvent**: the central event type. Variants per FR-001. Each variant carries (a) category-specific payload (e.g. `SlotAlloc` has a `SlotId`, a name, a declared type) and (b) a `Span`.
- **SlotId**: a stable, unique identifier for a runtime stack slot. Distinct from `BindingId` (which is static); a single `BindingId` may map to multiple `SlotId`s across recursive function calls (each call gets fresh slots). For L1 with no recursion in tests, `SlotId` ≈ `BindingId` numerically, but they are conceptually different.
- **FrameId**: a stable, unique identifier for a stack frame instance (one per function call). Recursive calls get distinct `FrameId`s.
- **Value**: the runtime value associated with a slot. Variants for `Int(i64)`, `Bool(bool)`, `Unit`. Future milestones add `Box`, `Vec`, etc.
- **Pointee**: per FR-002 — `Slot(SlotId)` or `Heap(HeapAddr)`. `HeapAddr` is defined here as a forward-compatible placeholder; not used in M03.
- **HeapAddr**: a stable identifier for a heap allocation, used only by M07+. Defined here so it doesn't need to be retrofitted.
- **BorrowId**: a stable identifier for a borrow, used only by M06+. Defined here as forward-compatible.
- **NoteKind**: an enum classifying note types (`RuntimeError`, `MoveInvalidatesUse` for M07, `BorrowEndsBefore` for M06, etc.). M03 includes at minimum `RuntimeError`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `cargo test --test m03` exits 0 after running ≥ 6 snapshot tests covering: (a) arithmetic happy path, (b) function call with parameters and return value, (c) if-as-expression branch selection (both branches in separate tests), (d) shadowing across scopes, (e) nested blocks with correct LIFO drop order, (f) a runtime error case (e.g. division by zero) producing a `Note` event.
- **SC-002**: 100% of emitted events carry a non-empty span (`start < end`, or zero-length only at function-exit positions). Verified by visual inspection of snapshots.
- **SC-003**: Function-call events are correctly paired: every `FrameEnter` has a matching `FrameLeave` in LIFO order. Verified by visual inspection and (optionally) by a runtime check inside the test driver.
- **SC-004**: `SlotDrop` events appear in reverse declaration order at every scope exit. Verified by snapshot inspection.
- **SC-005**: Snapshot tests are deterministic — `cargo test --test m03` twice in succession produces no `.snap.new` files.
- **SC-006**: Total source under `src/event.rs` + `src/eval.rs` stays under ~1500 LOC. Soft cap; reconsider before crossing.
- **SC-007**: `cargo build --release` succeeds with zero warnings under `RUSTFLAGS="-D warnings"`.
- **SC-008**: M01 and M02 tests still pass — `cargo test --test m01` and `cargo test --test m02` both exit 0 unchanged.
- **SC-009**: The `MemEvent` enum declares all categories from CLAUDE.md (verified by visual inspection of the enum definition; at least 20 distinct variants accounting for all listed events).

## Assumptions

- M01 and M02 are closed and on `main`. The M03 input is the `(Program, Resolution, TypeMap)` triple from those passes.
- M03 reuses M01's `ParseError` type for static-time failures (which shouldn't occur post-M02). Runtime errors surface as `Note` events with `NoteKind::RuntimeError`. No new error type introduced.
- The L1 grammar from M01 is the closed set M03 evaluates. M03 doesn't extend the grammar.
- Snapshot output format is the same as M01/M02 — `insta::assert_debug_snapshot!` on the `Vec<MemEvent>` (or a wrapping struct). Output reads top-down in event-emission order.
- Determinism is achieved by deterministic data structures and a fixed evaluation strategy: depth-first, left-to-right, source order. No threading in the evaluator itself (M08 threads will use a deterministic scheduler — separate concern).
- The L1 type system from M02 is sound enough that M03 doesn't need to recheck types — it consumes the `TypeMap` and trusts it.
- Recursion limit: 100 frames default, sufficient for L1 sample programs without runaway-recursion test cases.
- Implementation is by AI agents under maintainer direction. Sizing per the S/M/L rubric from `specs/001-milestone-roadmap/research.md` — M03 is rated M.
- The `Note` variant infrastructure ships with M03 but is sparsely emitted (mainly for `RuntimeError`). Later milestones (M06 dangling borrows, M07 realloc invalidation, M08 lock poisoning) will emit notes from their evaluators without changing the enum.
