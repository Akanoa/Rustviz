# rustviz Milestone Roadmap

**Source of truth**: [CLAUDE.md](./CLAUDE.md)
**Last audit**: 2026-05-20

This document partitions the rustviz scope documented in `CLAUDE.md` into ordered, independently-shippable milestones. Each block below carries goal, in/out scope, dependencies, entry/exit criteria, demo artifact, and a citation back to CLAUDE.md. For the read/audit/revise procedures see [`specs/001-milestone-roadmap/quickstart.md`](./specs/001-milestone-roadmap/quickstart.md). The schema each milestone block follows is in [`specs/001-milestone-roadmap/contracts/milestone-schema.md`](./specs/001-milestone-roadmap/contracts/milestone-schema.md). The canonical CLAUDE.md scope-bullet inventory backing the coverage audit lives at [`specs/001-milestone-roadmap/scope-inventory.md`](./specs/001-milestone-roadmap/scope-inventory.md).

## Dependency graph

```
M01 ──► M02 ──► M03 ──► M04 ──► M05 ──► M06 ──► M07 ──► M08
                  │             ▲       ▲       ▲       ▲
                  └─────────────┴───────┴───────┴───────┘
            (M03 also directly feeds M05, M06, M07, M08 —
             each extends MemEvent enum payloads defined in M03)

M03 ──► M03.1     # additive protocol revision; M04+ consume it once shipped
```

Acyclic. The drawn order is one valid topological sort. Edges from M03 to M05–M08 are real direct dependencies (each later milestone fills payloads in the event enum M03 defines), not just transitive. **M03.1** is a *revision* milestone — it patches M03's event protocol after M03 closed, based on pedagogical issues uncovered during M04's manual QA. Revisions don't sit in the main chain; they hang off the milestone they patch and improve downstream milestones' behavior once shipped.

## Milestones

### M01 — Frontend skeleton (lexer + parser)

- **Kind**: foundation
- **Status**: planned
- **Complexity**: L (modules: 4, bullets: 3, boundaries: 0)
- **Depends on**: —
- **Authority**: CLAUDE.md › Architecture › "Interpreter (Rust → WASM)"; CLAUDE.md › Immediate roadmap › "Integrate the parse/ skeleton"; CLAUDE.md › Planned code layout › "src/parse/ … span.rs, lexer.rs, ast.rs, parser.rs"

**Goal.** Deliver the parser front-end (span, lexer, AST, recursive-descent parser) sufficient to consume Level 1 syntax.

**In scope.**
- `src/parse/span.rs`: `Span`, `Spanned<T>`, `SourceMap` (byte offsets + `FileId`).
- `src/parse/lexer.rs`: `&str → Vec<Token>` with whitespace/comment handling and multi-char lookahead (`==`, `!=`, `<=`).
- `src/parse/ast.rs`: AST types with spans at every level for Level 1 syntax (primitives, `let`/`let mut`, `fn`, scopes, blocks-as-expressions, `if` expressions, operators).
- `src/parse/parser.rs`: hand-rolled recursive descent, `Vec<Token> → Program`.
- Reject `&` at the lexer (per CLAUDE.md locked-in decision); reserve `Amp`/`AmpMut` tokens for M06.
- Stop at the first parse error with a span-bearing message.

**Out of scope.**
- Name resolution (M02).
- Type checking (M02).
- Evaluation (M03).
- `&` and `&mut` tokens (M06).
- Error recovery (deferred).

**Entry criteria.**
- Cargo project exists at the repo root (or scaffolded as part of this milestone's first task).
- `src/parse/` directory exists, even if empty.

**Exit criteria.**
- Snapshot tests under `tests/snapshots/m01_*.snap` cover ≥ 3 sample L1 programs and pass with `cargo test --test m01`.
- Lexer rejects a `&` token with a clear span-bearing error (snapshot-tested).
- Parser stops at the first error with a span-bearing message (snapshot-tested).

**Demo.**
- Format: snapshot
- Inputs: `tests/samples/m01_*.rs` (3 files: arithmetic, let-bindings, if-expressions)
- Outputs: `tests/snapshots/m01_*.snap`
- Command: `cargo test --test m01`

**Notes.** Code drafted in `conversation.html` per CLAUDE.md immediate-roadmap step 1.

---

### M02 — Name resolution + lightweight typeck

- **Kind**: foundation
- **Status**: planned
- **Complexity**: M (modules: 2, bullets: 2, boundaries: 1)
- **Depends on**: M01
- **Authority**: CLAUDE.md › Immediate roadmap › "Name resolver: Ident → BindingId"; CLAUDE.md › Immediate roadmap › "Lightweight typeck: validate annotations, propagate obvious types"; CLAUDE.md › Planned code layout › "resolve/ … Ident → BindingId, scope checks"; CLAUDE.md › Planned code layout › "typeck/ … annotation checks, type propagation"

**Goal.** Resolve identifiers to binding IDs and validate annotations / propagate obvious types over the M01 AST.

**In scope.**
- `src/resolve/`: `Ident → BindingId` resolution, scope checks, "use of undeclared variable" errors with spans.
- `src/typeck/`: annotation validation for L1 types, simple type propagation across `let`/`fn`/`if`-expression results.
- Stable `BindingId` for every use site (deterministic across runs).

**Out of scope.**
- Trait resolution.
- Generic inference.
- Lifetime inference (M06 introduces scope-level lifetimes).
- Borrow checking (M06).

**Entry criteria.**
- M01 closed: AST + spans available; parser tests passing.

**Exit criteria.**
- Snapshot tests under `tests/snapshots/m02_*.snap` cover at least one resolution-failure case and one type-mismatch case.
- Resolver returns a stable `BindingId` for every use site (verifiable via snapshot determinism).
- `cargo test --test m02` passes.

**Demo.**
- Format: snapshot
- Inputs: `tests/samples/m02_*.rs` (happy-path + error-cases)
- Outputs: `tests/snapshots/m02_*.snap` (including error snapshots)
- Command: `cargo test --test m02`

---

### M03 — Event model + Level 1 evaluator

- **Kind**: foundation
- **Status**: planned
- **Complexity**: M (modules: 2, bullets: 12, boundaries: 1)
- **Depends on**: M02
- **Authority**: CLAUDE.md › Architecture › "Event stream (MemEvent[])"; CLAUDE.md › Architecture › "the interpreter never writes to the UI directly. It emits a typed event stream"; CLAUDE.md › Event model › "MemEvent is the centerpiece"; CLAUDE.md › Event model › "Frames: FrameEnter, FrameLeave"; CLAUDE.md › Event model › "Stack slots (bindings): SlotAlloc, SlotWrite, SlotMove, SlotDrop"; CLAUDE.md › Event model › "Pedagogy: Note { kind, message, span }"; CLAUDE.md › Event model › "Every event carries a SourceSpan"; CLAUDE.md › Event model › "Pointee is an enum Slot(SlotId) | Heap(HeapAddr)"; CLAUDE.md › Event model › "SlotMove is intentionally distinct from SlotDrop"; CLAUDE.md › Supported Rust subset › "Level 1: primitives, let/let mut, functions, scopes, moves of non-Copy types"; CLAUDE.md › Immediate roadmap › "Define MemEvent and write the level-1 evaluator"; CLAUDE.md › Planned code layout › "eval/ … AST walker, emits MemEvent"; CLAUDE.md › Planned code layout › "event.rs … MemEvent enum"

**Goal.** Define the `MemEvent` enum (full shape — all categories from CLAUDE.md, even those whose payloads come from later milestones) and an L1 evaluator that walks M02's resolved+typed AST and emits a `Vec<MemEvent>`.

**In scope.**
- `src/event.rs`: full `MemEvent` enum with variants for every category in CLAUDE.md (Threads, Frames, Stack slots, Heap, Borrows, Sync, Pedagogy). Variants for non-L1 categories may have stub payloads to be filled in M06–M08.
- `SourceSpan` carried on every event.
- `Pointee` enum: `Slot(SlotId) | Heap(HeapAddr)`.
- `Note { kind, message, span }` infrastructure (used by every later milestone for pedagogical messages).
- `src/eval/`: L1 evaluator emitting `FrameEnter`/`FrameLeave`, `SlotAlloc`/`SlotWrite`/`SlotMove`/`SlotDrop`, and `Note` events. Covers primitives, let/let mut, functions (call + return), scopes, moves of non-Copy types, blocks-as-expressions, `if` expressions, operators with precedence.
- `SlotMove` and `SlotDrop` emitted as distinct events on the appropriate AST positions.

**Out of scope.**
- Heap allocation events (M07 fills `HeapAlloc`/`HeapRealloc`/`HeapFree` payloads).
- Borrow events (M06 fills `BorrowShared`/`BorrowMut`/`BorrowEnd` payloads).
- Threads / sync events (M08).
- UI rendering (M04).

**Entry criteria.**
- M02 closed: resolved + typed AST available.

**Exit criteria.**
- Running the evaluator on at least 3 L1 sample programs produces a deterministic `Vec<MemEvent>` matching a snapshot under `tests/snapshots/m03_*.snap`.
- A sample program that triggers a move of a non-Copy type produces a `SlotMove` event distinct from any `SlotDrop` (snapshot-verified).
- Every emitted event carries a non-empty `SourceSpan`.
- `cargo test --test m03` passes.

**Demo.**
- Format: snapshot
- Inputs: `tests/samples/m03_*.rs` (L1 programs including moves of non-Copy types)
- Outputs: `tests/snapshots/m03_*.snap` (event-stream dumps)
- Command: `cargo test --test m03`

**Notes.** The full event enum lands here even though M03 only emits a subset, so later milestones extend payloads in-place rather than refactoring the enum. The `Note` infrastructure is intentionally introduced here for the same reason.

---

### M03.1 — Protocol revision: Copy-drop + return-value bridge

- **Kind**: foundation
- **Status**: planned
- **Complexity**: M (modules: 2, bullets: 3, boundaries: 2)
- **Depends on**: M03
- **Authority**: CLAUDE.md › Event model › "SlotMove is intentionally distinct from SlotDrop"; CLAUDE.md › Pedagogical goal › "Give a newcomer concrete intuition for Rust's memory mechanics: moves, borrows, lifetimes, drops"

**Goal.** Revise M03's event protocol to fix two pedagogical issues uncovered during M04's manual QA: (1) `SlotDrop` events are emitted for Copy-typed slots, wrongly visualizing physical-memory loss for types whose bytes don't actually go away; (2) function-return values appear out of thin air in the caller's slot — the ABI return-value mechanic is invisible.

**In scope.**
- Gate `SlotDrop` emission in `src/eval.rs` on the slot's binding type. Only emit `SlotDrop` when `typeck.binding_types[id]` resolves to a non-Copy `Ty`. For L1's `i32` / `bool` (both Copy) no `SlotDrop` is emitted — the slot persists visually until the whole frame leaves.
- Add a new `MemEvent::ReturnValue { frame_id, value, span }` variant in `src/event.rs`, emitted between body completion and `FrameLeave`. Carries the function's computed return value so consumers can visualize the "value lives somewhere between caller and callee" moment.
- Relax the "closed enum from M03" rule in the `MemEvent` contract: additive variants are permitted in **revision milestones** with maintainer consent. The relaxed rule is documented in `specs/004-m03-event-eval/contracts/m03-api.md`.
- Optional cleanup: drop the redundant `FrameEnter.params` field (info already covered by the subsequent per-param `SlotAlloc`+`SlotWrite` events).
- Update `src/ui.rs::apply_event` in M04 to handle the new variant — render the return value as a transient annotation on the frame card before it leaves.
- Regenerate M03 snapshot tests (`tests/snapshots/emits_*.snap`) and M04 traces (`web/traces/*.json`).

**Out of scope.**
- M04 layout / CSS changes (the visualization improves "for free" once Copy drops are gone).
- M06+ events (borrows, heap, sync, threads remain on their respective milestones).
- Visual changes to M07's `SlotDrop` (heap-allocated drops keep firing because their destructors do real work).

**Entry criteria.**
- M03 closed (event model + L1 evaluator on `main`).
- M04 closed (UI shell consuming the event stream on `main`).
- Pedagogical issues documented in this milestone's `research.md`.

**Exit criteria.**
- `cargo test --test m03` passes with revised snapshots (fewer `SlotDrop` events for L1, new `ReturnValue` events in fn-call traces).
- `cargo test --test m01` and `cargo test --test m02` still pass unchanged.
- `cargo test --lib ui::` passes (Cursor handles the new `ReturnValue` variant).
- M04 page (manual QA, per `specs/005-m04-ui-shell/quickstart.md` SC-008 procedure) shows: (a) Copy-type slots stay visible until `FrameLeave`, (b) a transient return-value indicator appears between body completion and frame disappearance.

**Demo.**
- Format: browser
- Inputs: existing `web/samples/m03_*.rs` (no new samples needed)
- Outputs (browser-observed steps): for `m03_fn_call` (`fn add(a, b) -> i32 { a + b }`): step through 1 → 13. At steps 7–8, both `a` and `b` stay visible in the `add` frame (previously they disappeared); at the new step between body and `FrameLeave`, a `→ 5` indicator appears on the `add` frame card; after `FrameLeave`, the frame disappears and `r` gets `5` in `main`.
- Command: `cd web && trunk serve --open`

**Notes.** First revision milestone in the project. The `MemEvent` enum's "closed from M03" rule is relaxed for additive protocol changes in revision milestones — variants can be added with maintainer consent. The new variant `ReturnValue` is documented as additive in M03's contract (`specs/004-m03-event-eval/contracts/m03-api.md`).

---

### M03.2 — Scalar lattice expansion (integer types)

- **Kind**: foundation
- **Status**: planned
- **Complexity**: M (modules: 3, bullets: 5, boundaries: 2)
- **Depends on**: M03, M05
- **Authority**: CLAUDE.md › Supported Rust subset › "Level 1: primitives, let/let mut, functions, scopes". M03's typeck lattice (`i32`, `bool`, `()` only) under-implements the word "primitives" — common L1 patterns like `let count: u32 = …` or `let byte: u8 = …` hit a "Typeck error: unknown type" wall. This milestone broadens the lattice to cover Rust's signed/unsigned integer family.

**Goal.** Extend M03's `Ty` lattice to include the full Rust integer family (`i8`/`i16`/`i32`/`i64`/`i128`, `u8`/`u16`/`u32`/`u64`/`u128`, `isize`/`usize`) so learners writing common Rust patterns are not stopped by the type lattice's narrowness. Floats (`f32`/`f64`) deferred to a future revision because of their distinct comparison and overflow rules.

**In scope.**
- Extend `Ty` enum in `src/typeck.rs` with 12 new integer variants. Update `Ty::is_copy()` (all integers are Copy → return `true`) and `Ty::name()` (per-variant string rendering: `"u8"`, `"i64"`, …). Rust's match-exhaustiveness check will flag every site needing an arm.
- Extend `Value` in `src/event.rs` to hold typed integer values. Plan-phase decides between per-type variants (`Value::I8(i8)`, …) versus a unified representation (`Value::Int { kind: IntKind, bits: i128 }`); the unified form is cleaner but adds a small abstraction. Either way the JSON wire shape is documented as part of M03's amended contract.
- Recognize the 12 new type names as valid type annotations in `let` bindings, function parameters, and function return types. Annotation-driven only — **no literal suffix parsing** (`5u8`, `5_i64`) in this milestone (deferred).
- Add overflow detection at the evaluator: arithmetic operations that would overflow the destination type's range emit a `Note { kind: RuntimeError, message: "u8 overflow: …" }` and halt the trace. Pedagogically aligned with the existing div-by-zero pattern.
- Cross-type arithmetic is a typeck error with a clear message (e.g. `u8 + i32` reports "expected u8, found i32" at the right operand's span).
- Update `src/ui.rs::render_value` to suffix the type when rendering values (`5_u8` instead of just `5`). The stacks panel makes the type visible alongside the value.
- M03 snapshot tests grow at least 2 new samples exercising non-i32 integers; M05 dropdown grows at least 1 user-facing example.

**Out of scope.**
- **Literal type suffixes** (`5u8`, `5_i64`). Values get their type from annotations or function signatures, not from suffix syntax. A future revision could add suffix parsing if learner cases demand it.
- **Float types** (`f32`, `f64`). Different equality/comparison/NaN/overflow semantics; needs its own revision milestone.
- **Casts** (`x as u32`). Conversion is implicit through annotation only; explicit casts deferred.
- **BigInt** / arbitrary-precision integers.

**Entry criteria.**
- M03 closed (event model + L1 evaluator on `main`).
- M05 closed (live pipeline + editor — required for browser-side QA of the new types).

**Exit criteria.**
- `cargo test` passes with new typeck + eval unit tests covering each new integer type's basic arithmetic.
- Typing `fn main() { let x: u8 = 5; }` in the M05 editor produces a trace where `x` shows in the stacks panel as `5_u8`.
- Typing `fn main() { let x: u8 = 250; let y = x + 10; }` produces a `Note { kind: RuntimeError }` pointing at the overflow site (`+ 10` exceeds `u8::MAX`).
- Cross-type arithmetic (e.g. annotating two `let`s with different integer types and adding them) is a typeck error with a span on the mismatched operand.
- No regression to existing M01/M02/M03/M04/M05 tests — `i32`/`bool`/`()` semantics unchanged.
- ≥ 2 new `m03_2_*.rs` samples shipped covering: (a) basic `u8` arithmetic, (b) overflow runtime-error.

**Demo.**
- Format: browser (via M05's editor)
- Inputs: `tests/samples/m03_2_*.rs` (≥ 2 samples) + live editing of typical patterns like `let count: u32 = 100;`.
- Outputs (browser-observed steps): for `m03_2_u8_overflow`: step 1 → main opens; 2-3 → `x: u8 = 250` allocated and written; 4-5 → `+ 10` evaluates → status bar shows `"Typeck error: u8 overflow"`. Stacks panel halts at `x = 250_u8`.
- Command: `cd web && trunk serve --open`

**Notes.** Second revision milestone in the project (after M03.1). Reuses the closed-enum-with-revisions precedent established in M03.1 — `Ty` and `Value` grow additively by 12 + N variants respectively. The choice between per-type `Value` variants vs. a unified `Value::Int { kind, bits }` is a plan-phase decision; both work, the unified form is more compact. Float types are deferred to a separate revision milestone (M03.3 if they land).

- **Kind**: foundation
- **Status**: planned
- **Complexity**: L (modules: 4, bullets: 4, boundaries: 2)
- **Depends on**: M03
- **Authority**: CLAUDE.md › Architecture › "UI (web, WASM bindings)"; CLAUDE.md › Architecture › "The UI replays the stream with a cursor (play / pause / step / rewind)"; CLAUDE.md › The three panels › "Editor (Monaco or CodeMirror)"; CLAUDE.md › The three panels › "Stacks: one column per thread"; CLAUDE.md › Immediate roadmap › "First UI prototype: single stack panel, static replay of a pre-recorded trace"

**Goal.** Deliver a browser UI shell that loads a pre-recorded `Vec<MemEvent>` (from M03) and replays it through a play/pause/step/rewind cursor, with the editor panel highlighting the current event's span and a single-column stacks panel showing slot allocations.

**In scope.**
- WASM bindings exposing the M03 event stream to the browser.
- Minimal HTML host with three panel regions (only editor + stacks panels populated in this milestone; heap panel area reserved but empty).
- Editor panel (Monaco or CodeMirror — decide at M04 start, see Notes) with span-decorator highlighting the current event's span.
- Single-column stacks panel rendering `SlotAlloc` / `SlotWrite` / `SlotMove` / `SlotDrop`.
- Replay cursor with play / pause / step / rewind controls over a pre-recorded trace.

**Out of scope.**
- Live interpretation from editor input (M05).
- Heap panel content (M07).
- Multi-thread stack columns (M08).
- Pointer overlay (M06).

**Entry criteria.**
- M03 closed.
- A pre-recorded `.events.json` trace from at least one L1 sample program exists in `tests/samples/`.

**Exit criteria.**
- Opening the browser, loading the pre-recorded trace, and stepping through it visibly highlights matching spans in the editor and updates the stacks panel for each event.
- Cursor responds to play / pause / step / rewind controls.
- Rewinding to step 0 restores initial state (empty stack, no editor highlight).

**Demo.**
- Format: browser
- Inputs: `tests/samples/m04_pre_recorded.events.json` + matching `.rs` source
- Outputs (browser-observed steps): 1. open page, 2. trace loaded, 3. click play and observe slot `x` highlight at step 3, 4. step backward returns to prior state, 5. rewind to step 0 clears the stacks panel.
- Command: `trunk serve --open` (or equivalent — finalize at M04 start)

**Notes.** Editor framework choice (Monaco vs CodeMirror) is open per `specs/001-milestone-roadmap/research.md` open question; resolve at M04 start and record in this Notes field.

---

### M05 — Live Level 1 (edit → run → watch)

- **Kind**: feature
- **Status**: planned
- **Complexity**: S (modules: 1, bullets: 1, boundaries: 1)
- **Depends on**: M04, M03
- **Authority**: CLAUDE.md › Architecture › "the interpreter never writes to the UI directly. It emits a typed event stream"; CLAUDE.md › Supported Rust subset › "Level 1: primitives, let/let mut, functions, scopes, moves of non-Copy types"

**Goal.** Connect M03's evaluator to M04's UI shell so that editing an L1 program in the editor produces a live event stream the stacks panel replays. First publicly demoable artifact of the project.

**In scope.**
- Glue layer that re-runs the M01 → M02 → M03 pipeline on editor input (debounced).
- Pipe the resulting `Vec<MemEvent>` into the M04 replay cursor.
- Display parse / resolve / typeck errors inline in the editor with a span underline.

**Out of scope.**
- Anything beyond L1: references (M06), heap (M07), threads (M08).

**Entry criteria.**
- M04 closed: UI shell replays static traces.
- M03 closed: evaluator emits L1 events.

**Exit criteria.**
- Typing a valid L1 program in the editor and clicking "run" produces a fresh trace that the stacks panel can replay end-to-end.
- A parse / resolve / typeck error in editor input is shown with a span underline; the stacks panel does not advance until the error is fixed.
- The closing commit is tagged and a short screen-recording is captured for the project README.

**Demo.**
- Format: browser
- Inputs: live editor input; reference programs in `tests/samples/m05_*.rs`
- Outputs (browser-observed steps): 1. type `let x = 5;` in editor, 2. click run, 3. observe `SlotAlloc x:i32=5` in stacks panel, 4. step through and observe `x` highlighting move with the cursor.
- Command: `trunk serve --open`

**Notes.** M05 is the project's first publicly demoable artifact. M05 introduces no new CLAUDE.md scope bullets — it integrates work already authorized by M03 (Level 1 evaluator) and M04 (UI shell). The two Authority citations here are secondary references; primary ownership of both bullets remains with M03.

---

### M06 — Level 2: references and borrows

- **Kind**: feature
- **Status**: planned
- **Complexity**: L (modules: 3, bullets: 3, boundaries: 2)
- **Depends on**: M05, M03
- **Authority**: CLAUDE.md › Supported Rust subset › "Level 2: & and &mut, aliasing rules, scope-level lifetimes"; CLAUDE.md › Event model › "Borrows: BorrowShared, BorrowMut, BorrowEnd"; CLAUDE.md › The three panels › "Pointers: SVG overlay across the panels"

**Goal.** Extend the front-end and evaluator to handle `&` and `&mut` and the aliasing rules; introduce the SVG pointer overlay rendering blue arrows for `&` and red arrows for `&mut`.

**In scope.**
- Lexer: accept `&` and `&mut` (replacing the M01 rejection); emit `Amp` / `AmpMut` tokens per the locked-in decision.
- Parser / AST / resolver / typeck extensions for borrow types and scope-level lifetimes.
- Evaluator emits `BorrowShared` / `BorrowMut` / `BorrowEnd` events with `BorrowId` (M03's enum already has the variants — this milestone fills the payloads).
- Scope-level lifetime tracking sufficient to emit `BorrowEnd` at the right step.
- SVG pointer overlay component in the UI: blue arrows for `&`, red arrows for `&mut`.
- Aliasing-rule violation detection emitting pedagogical `Note` events shown in the UI.

**Out of scope.**
- Heap pointers / owning arrows (M07).
- Threads / `Arc` / `Mutex` (M08).
- Generic / named lifetimes `<'a>` (deferred — see `## Deferred`).

**Entry criteria.**
- M05 closed (live L1 works end-to-end).

**Exit criteria.**
- A sample program `let x = 5; let r = &x;` renders with a visible blue arrow from the `r` slot to the `x` slot in the browser.
- A program with overlapping `&mut` references emits a Note that the UI displays inline.
- `tests/samples/m06_*.rs` programs are all replayable and produce the expected arrows / notes (snapshot tests of event streams pass; browser walkthrough documented).

**Demo.**
- Format: browser
- Inputs: `tests/samples/m06_*.rs` (shared-borrow, mut-borrow, aliasing-violation)
- Outputs (browser-observed steps): 1. load shared-borrow sample, observe blue arrow `r → x`, 2. step past borrow scope, observe arrow disappear, 3. load aliasing-violation sample, observe Note message and span underline.
- Command: `trunk serve --open`

**Notes.** Candidate for in-place split into `M06a` (borrow tracking + events) and `M06b` (pointer overlay) if mid-implementation it exceeds L on any sizing axis.

---

### M07 — Level 3: heap (Box, Vec, String)

- **Kind**: feature
- **Status**: planned
- **Complexity**: L (modules: 3, bullets: 3, boundaries: 2)
- **Depends on**: M06, M03
- **Authority**: CLAUDE.md › Supported Rust subset › "Level 3: Box, Vec (with visible realloc), String"; CLAUDE.md › Event model › "Heap: HeapAlloc, HeapRealloc, HeapFree"; CLAUDE.md › The three panels › "Heap: free-form area where each HeapAlloc creates a box"

**Goal.** Extend the evaluator to model heap allocation and reallocation for `Box`, `Vec`, and `String`; deliver the heap panel UI with allocation boxes and the realloc-snap animation that makes `&v[0]`-after-`push` viscerally obvious.

**In scope.**
- Evaluator handling for `Box::new`, `Vec::new` / `push` / indexing, `String::from` / `push_str`.
- `HeapAlloc` / `HeapRealloc` / `HeapFree` event emission with sizes and types (filling M03's enum payloads).
- Heap panel rendering boxes (size proportional to allocation, label = type name).
- `HeapRealloc` animates: the box moves and every arrow pointing to it follows.
- Pointer overlay extension to draw black owning arrows for `Box` / `Vec` / `String`.
- Dangling-borrow detection after realloc emitting `Note` events.

**Out of scope.**
- Threads / `Arc` / `Mutex` (M08).
- Heap-allocating types beyond `Box`, `Vec`, `String` (e.g. `HashMap`, `Rc`).

**Entry criteria.**
- M06 closed (pointer overlay exists).

**Exit criteria.**
- A sample program creating a `Vec`, taking `&v[0]`, then `push`ing visibly animates the heap box moving and emits a UB-Note describing the dangling borrow.
- `Box::new(5)` shows a black owning arrow from the stack slot into a heap box.
- The realloc animation is reproducible across runs (snapshot of event stream identical; browser walkthrough documented).

**Demo.**
- Format: browser
- Inputs: `tests/samples/m07_vec_realloc.rs`, `tests/samples/m07_box.rs`, `tests/samples/m07_string.rs`
- Outputs (browser-observed steps): 1. step to `Vec` creation, observe heap box appear, 2. step to `&v[0]`, observe blue arrow into the heap box, 3. step to `push`, observe the box animate to a new position and a UB-Note appear underlining `&v[0]`.
- Command: `trunk serve --open`

**Notes.** The realloc animation is the pedagogical centerpiece of M07; do not ship the milestone without it (research.md R-009).

---

### M08 — Level 4: threads (thread::spawn, Arc, Mutex)

- **Kind**: feature
- **Status**: planned
- **Complexity**: L (modules: 3, bullets: 3, boundaries: 2)
- **Depends on**: M07, M03
- **Authority**: CLAUDE.md › Supported Rust subset › "Level 4: thread::spawn, Arc, Mutex, Send/Sync"; CLAUDE.md › Event model › "Threads: ThreadSpawn, ThreadJoin, ThreadPark"; CLAUDE.md › Event model › "Synchronization: LockAcquire, LockRelease, ArcClone, ArcDrop"

**Goal.** Extend the evaluator and UI to handle `thread::spawn`, `Arc`, and `Mutex` on the happy path; stacks panel grows to multiple columns; parked threads grey out with a dotted line to the held mutex; pointer overlay adds dashed purple for `Arc` / `Rc`.

**In scope.**
- Evaluator handling for `thread::spawn` / `join`, `Arc::new` / `clone` / drop, `Mutex::new` / `lock` / `unlock`.
- `ThreadSpawn` / `ThreadJoin` / `ThreadPark` event payloads (filling M03's enum stubs).
- `ArcClone` / `ArcDrop` / `LockAcquire` / `LockRelease` event payloads.
- Stacks panel: multi-column rendering with slide-in-from-right animation when a thread spawns.
- Parked-thread visual treatment: thread column greys out, dotted line drawn to the slot holding the mutex.
- Pointer overlay: dashed purple arrows for `Arc` / `Rc` clones.

**Out of scope.**
- Full `Send` / `Sync` auto-trait inference (deferred — see `## Deferred`).
- Poisoned-mutex behavior.
- Channels, `RwLock`, atomics.
- `async` / `await`.

**Entry criteria.**
- M07 closed (heap exists; owning arrows exist).

**Exit criteria.**
- A two-thread `Arc<Mutex<T>>` sample replays with both stack columns visible.
- One thread parked on the mutex visibly greys out and draws a dotted line to the held mutex slot.
- `Arc::clone` shows a dashed purple arrow pointing to the shared allocation.
- Contention is reproducible (event stream is deterministic given a fixed scheduling order; snapshot tests of the event stream pass).

**Demo.**
- Format: browser
- Inputs: `tests/samples/m08_arc_mutex.rs`
- Outputs (browser-observed steps): 1. spawn thread, observe second column slide in from right, 2. one thread locks, observe `LockAcquire` indicator + Arc arrow, 3. other thread attempts lock, observe parked state and dotted line to mutex slot, 4. first thread unlocks, observe parked thread resume.
- Command: `trunk serve --open`

**Notes.** Borderline-XL by per-event counting (9 atomic events across Threads + Sync categories) but L by event-category counting per the rubric (research.md R-007). If sizing-axis tracking reveals XL during implementation, split into `M08a` (threads + multi-column stacks) and `M08b` (Arc / Mutex / sync + dashed purple overlay).

---

## Deferred

- **Detailed `Send` / `Sync` inference** — M08 ships `Arc<Mutex<T>>` happy-path only; full auto-trait inference and error messages are deferred. Reason: full inference is rustc-grade work, out of scope for a pedagogical visualizer. (CLAUDE.md › Supported Rust subset › "Level 4: thread::spawn, Arc, Mutex, Send/Sync")
- **Parser error recovery** — the CLAUDE.md locked-in decision is "stop at first parse error"; recovery is deferred. Reason: enough for a live editor, smaller scope. (CLAUDE.md › Locked-in decisions › "Stop at first parse error")
- **Multi-file support** — spans already carry `FileId`, but the level milestones target single-file programs; multi-file UI deferred. Reason: complicates the Editor panel without proportional pedagogical gain. (CLAUDE.md › Locked-in decisions › "Spans = byte offsets + `FileId`")
- **Lifetime visualization beyond scope-level** — Level 2 covers scope-level lifetimes; generic / named lifetimes (`<'a>`) deferred. Reason: out of L2 scope as documented in CLAUDE.md.
- **Levels beyond Level 4** — closures, trait objects, `unsafe`, `async`. None are currently in CLAUDE.md; no scope claim, so nothing is being deferred — listed here only to make the boundary explicit.
