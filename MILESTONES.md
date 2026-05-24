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
M07 ──► M07.1 ──► M07.2  # slices, then &str + static memory
            └──► M07.3   # arrays (sibling of M07.2; both depend only on M07.1)
            └──► M07.4 ──► M07.5 ──► M07.6
                            # structs/impl → generics → traits
                            # M07.5 unlocks `T:` payoff; M07.6 layers static dispatch on top
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

### M03.2 — Scalar lattice expansion (integer + float types)

- **Kind**: foundation
- **Status**: planned
- **Complexity**: M (modules: 3, bullets: 6, boundaries: 2)
- **Depends on**: M03, M05
- **Authority**: CLAUDE.md › Supported Rust subset › "Level 1: primitives, let/let mut, functions, scopes". M03's typeck lattice (`i32`, `bool`, `()` only) under-implements the word "primitives" — common L1 patterns like `let count: u32 = …`, `let byte: u8 = …`, or `let ratio: f64 = …` hit a "Typeck error: unknown type" wall. This milestone broadens the lattice to cover Rust's signed/unsigned integer family plus the two IEEE 754 float types.

**Goal.** Extend M03's `Ty` lattice to include (a) the full Rust integer family (`i8`/`i16`/`i32`/`i64`/`i128`, `u8`/`u16`/`u32`/`u64`/`u128`, `isize`/`usize`) and (b) the two float types (`f32`, `f64`), so learners writing common Rust patterns are not stopped by the lattice's narrowness.

**In scope.**
- Extend `Ty` enum in `src/typeck.rs` with 12 new integer variants + 2 float variants (`F32`, `F64`). Update `Ty::is_copy()` (all 14 are Copy → return `true`) and `Ty::name()` (per-variant string rendering). Rust's match-exhaustiveness check will flag every site needing an arm.
- Extend `Value` in `src/event.rs` to hold typed integer + float values. Plan-phase decides between per-type variants vs. a unified representation (e.g. `Value::Int { kind: IntKind, bits: i128 }` plus `Value::Float { kind: FloatKind, bits: u64 }` storing the IEEE 754 bit pattern); the unified form is cleaner but adds a small abstraction. Either way the JSON wire shape is documented as part of M03's amended contract. Note: introducing floats means `Value` can no longer derive `Eq` — `PartialEq` only. Any downstream usage relying on `Eq` (HashMap keys, etc.) gets refactored as part of this milestone.
- Recognize the 14 new type names as valid type annotations in `let` bindings, function parameters, and function return types. Annotation-driven only — **no literal suffix parsing** (`5u8`, `2.5_f32`) in this milestone (deferred).
- **Integer arithmetic**: overflow on `+`/`-`/`*` emits a `Note { kind: RuntimeError, message: "u8 overflow: …" }` and halts the trace. Pedagogically aligned with the existing div-by-zero pattern.
- **Float arithmetic**: follows IEEE 754 — overflow produces `±Inf`, `0.0 / 0.0` produces `NaN`, etc. **No runtime error** for these (they're valid Rust behavior). Instead, the evaluator emits a `Note { kind: Info, message: "produced NaN" }` (or similar) the first time a NaN or Inf appears in a binding, surfacing the special-value pedagogy without halting the trace.
- Cross-type arithmetic is a typeck error with a clear message (e.g. `u8 + i32` reports "expected u8, found i32" at the right operand's span). `i32 + f64` likewise.
- Update `src/ui.rs::render_value` to suffix the type when rendering values (`5_u8`, `2.5_f64`, `NaN_f32`, `-Inf_f64`). The stacks panel makes the type visible alongside the value; NaN/Inf are rendered as the literal Rust strings.
- M03 snapshot tests grow at least 3 new samples exercising non-`i32` types; M05 dropdown grows at least 2 user-facing examples (one integer, one float).

**Out of scope.**
- **Literal type suffixes** (`5u8`, `2.5_f32`). Values get their type from annotations or function signatures, not from suffix syntax. A future revision could add suffix parsing if learner cases demand it.
- **Float-typed literals without annotation**: `let x = 2.5;` requires the parser to recognize `2.5` as a float literal. This milestone supports `let x: f64 = 2;` (integer literal coerced via annotation, plan-phase confirms) and `let x: f64 = 2.5;` (float literal with a `.`). The full untyped-literal inference story is plan-phase work.
- **Casts** (`x as u32`, `i as f64`). Conversion is implicit through annotation only; explicit casts deferred.
- **BigInt** / arbitrary-precision integers.
- **Float comparisons that exercise NaN ordering quirks** (`NaN < 1.0` returns `false`, etc.) get the standard Rust `PartialOrd` semantics; no special pedagogical UX for this in M03.2.

**Entry criteria.**
- M03 closed (event model + L1 evaluator on `main`).
- M05 closed (live pipeline + editor — required for browser-side QA of the new types).

**Exit criteria.**
- `cargo test` passes with new typeck + eval unit tests covering each new type's basic arithmetic.
- Typing `fn main() { let x: u8 = 5; }` in the M05 editor produces a trace where `x` shows in the stacks panel as `5_u8`.
- Typing `fn main() { let x: u8 = 250; let y = x + 10; }` produces a `Note { kind: RuntimeError }` pointing at the overflow site (`+ 10` exceeds `u8::MAX`).
- Typing `fn main() { let x: f64 = 0.0; let y = 1.0 / x; }` produces a `Note { kind: Info }` describing `y` as `+Inf_f64`; the trace does NOT halt (Inf is valid).
- Typing `fn main() { let x: f64 = 0.0; let y = x / x; }` produces a `Note { kind: Info }` describing `y` as `NaN_f64`; trace does NOT halt.
- Cross-type arithmetic (e.g. `let a: u8 = 1; let b: i32 = 2; let c = a + b;`) is a typeck error with a span on the mismatched operand.
- No regression to existing M01/M02/M03/M04/M05 tests — `i32`/`bool`/`()` semantics unchanged.
- ≥ 3 new `m03_2_*.rs` samples shipped covering: (a) basic `u8` arithmetic, (b) integer overflow runtime-error, (c) float arithmetic with NaN or Inf.

**Demo.**
- Format: browser (via M05's editor)
- Inputs: `tests/samples/m03_2_*.rs` + live editing of typical patterns like `let count: u32 = 100;` or `let ratio: f64 = 3.14;`.
- Outputs (browser-observed steps): for `m03_2_u8_overflow`: step 1 → main opens; 2-3 → `x: u8 = 250` allocated and written; 4-5 → `+ 10` evaluates → status bar shows `"u8 overflow"`. Stacks panel halts. For `m03_2_float_nan`: similar but ends with `y = NaN_f64` and an info note.
- Command: `cd web && trunk serve --open`

**Notes.** Second revision milestone in the project (after M03.1). Reuses the closed-enum-with-revisions precedent established in M03.1 — `Ty` and `Value` grow additively. Adding floats forces `Value` to drop the `Eq` derive (floats don't impl `Eq` because `NaN != NaN`); this is intentional and any downstream `Eq`-dependent code gets refactored to `PartialEq` as part of M03.2. Integer overflow halts the trace (consistent with div-by-zero); float `Inf`/`NaN` are valid Rust behavior and surface via `Note { kind: Info }` without halting — the distinction is itself pedagogically meaningful. Plan-phase decides the exact `Value` shape (per-type variants vs. unified `{ kind, bits }`).

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

### M06.1 — Mutation: assignment + deref read/write

- **Kind**: feature (revision-style — extends M06 with the missing-half of borrow pedagogy)
- **Status**: planned
- **Complexity**: M (modules: 3, bullets: 4, boundaries: 2)
- **Depends on**: M06
- **Authority**: CLAUDE.md › Supported Rust subset › "Level 1: primitives, let/let mut, …" (the `let mut` keyword has been cosmetic since M03 — no assignment statement existed); CLAUDE.md › Supported Rust subset › "Level 2: & and &mut" (M06 ships `&mut` as observable arrows but without deref-write the mutability is invisible).

**Goal.** Close M06's pedagogical loop by adding the three connected mutation forms: plain assignment `x = 5` (for `let mut` bindings — M03's `mut` keyword finally gains meaning), deref-as-rvalue `let y = *r;` (read through `&T` or `&mut T`), and deref-as-lvalue `*r = 5;` (write through `&mut T` — the mutation flows visibly into the borrowed slot, with the borrow arrow persisting through the change).

**In scope.**
- AST: `Expr::Deref { inner: Box<Expr>, span }` for both rvalue and lvalue positions. New `Stmt::Assign { lhs: Expr, rhs: Expr, span }` (or `Expr::Assign { ... }` — plan-phase decides). Place expressions on the lhs are: `Expr::Ident` (direct binding assign) and `Expr::Deref(Expr::Ident)` (through-ref assign).
- Parser: prefix `*` (disambiguated from binary `*` by token position — prefix only at expression-start). `=` parsed as assignment at statement level.
- Typeck:
  - `*r` requires `r` to have type `Ty::Ref { inner, .. }`; the deref's type is `*inner`. Both `&T` and `&mut T` deref to `T`.
  - Assignment `lhs = rhs`: lhs must be a place expression. For `Expr::Ident(x)`, `x` must be a `let mut` binding (typeck error otherwise: "cannot assign to immutable variable `x`"). For `Expr::Deref(Expr::Ident(r))`, `r` must have type `Ty::Ref { mutable: true, .. }` (typeck error otherwise: "cannot assign through `&T`; need `&mut T`"). Rhs type must match the lhs's type.
  - Aliasing rules: an assignment through `*r` does NOT take a new borrow — it uses the existing `&mut` borrow. The borrow tracker's state is unchanged. (For direct `x = 5` on a mutable binding, no borrow is taken either.)
- Eval:
  - `Expr::Deref(r_expr)` as rvalue: resolve `r_expr` to a `Value::Ref { target_slot, .. }`; read `target_slot`'s current value; return it.
  - `*r = v` as statement: resolve `r` to `Value::Ref { target_slot, mutable: true, .. }`; emit `MemEvent::SlotWrite { slot_id: target_slot, value: v, span }`. The existing visualization animates the slot value change — and the red arrow stays anchored on the source-slot through the change.
  - `x = v` as statement: find the local for binding `x`; update its value; emit `SlotWrite { slot_id: x's slot, value: v, span }`.
- Ship at least 3 new reference programs in `tests/samples/m06_1_*.rs` + `web/samples/`: (a) direct assignment to a `mut` binding, (b) read through a shared ref, (c) write through a mut ref with the arrow persisting through the mutation.

**Out of scope.**
- Compound assignment (`+=`, `-=`, etc.) — not needed for the mutation pedagogy.
- `Box`/`Vec`/`String` (still M07).
- Multi-level deref `**r` — only single-level deref.
- Assignment to an arbitrary place expression (no fields, no array indexing — neither exists in the AST yet).
- Method calls / `*self` — not in any L1–L2 milestone yet.

**Entry criteria.**
- M06 closed (borrow events + SVG overlay + aliasing rules on `main`).

**Exit criteria.**
- `cargo test --test m01 / m02 / m03` byte-identical (no events change; no new variants).
- `cargo test --lib` passes with new tests covering: direct assignment, deref-read, deref-write, immutable-binding-assignment-rejected, deref-write-on-shared-rejected.
- M06 page (manual QA): typing `let mut x = 5; let r = &mut x; *r = 10;` produces a trace where the cursor steps through the borrow being taken (red arrow), then `*r = 10` emits a SlotWrite for `x`'s slot, observably updating its value to `10_i32` WHILE the red arrow stays anchored.
- ≥ 3 new `m06_1_*.rs` reference programs shipped.
- WASM bundle growth ≤ +20% vs M06 baseline (87,354 B gzipped → ≤ 104,825 B). Adding deref + assignment is small surface change; should easily fit under +20%.

**Demo.**
- Format: browser (via M05's editor)
- Inputs: `tests/samples/m06_1_*.rs` + live editing
- Outputs (browser-observed steps): for `m06_1_mut_through_ref`: load → step → `let mut x = 5` shows `x = 5_i32` → `let r = &mut x` shows red arrow → `*r = 10` step shows `x`'s value animate to `10_i32` with the red arrow still pointing at `x`.
- Command: `cd web && trunk serve --open`

**Notes.** Closes M06's pedagogical gap discovered during QA: "&mut without deref-write is observation-only." M03's `let mut` keyword has been cosmetic since M03; M06.1 makes it actually do something. Reuses the existing `SlotWrite` event variant — no protocol changes, no new variants. The visualization improves "for free" because slot-value updates are already animated.

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

### M07.1 — Slices (`&[T]`, range indexing, fat pointers)

- **Kind**: feature (revision-style — fills a gap in M07 by introducing the slice primitive that M07.2 builds on)
- **Status**: planned
- **Complexity**: L (modules: 4, bullets: 5, boundaries: 2)
- **Depends on**: M07
- **Authority**: Rust language reference for slice types `&[T]`; the existing M06 borrow infrastructure (slices are borrows with extra metadata).

**Goal.** Introduce the slice primitive end to end. Slices are Rust's "view into contiguous memory" — represented at runtime as a fat pointer (data ptr + length). M07 had no slice support; the existing `&[T]` shape from heap allocations couldn't be expressed. M07.1 adds the slice type, range expressions, range indexing on Vec/array, and the fat-pointer visual.

**In scope.**
- New AST: range expression `a..b`, `..b`, `a..`, `..` (parser + Expr::Range or four explicit forms).
- New Ty: slice type — `Ty::Slice(Box<Ty>)` for `&[T]` / `&mut [T]` representation. The unsized `[T]` itself need not be a first-class Ty if all uses go through references.
- Range indexing: `&v[1..3]` produces `&[i32]`. Typeck enforces receiver = `Vec<T>` or already-a-slice. Borrow-tracker integration.
- Vec/array `len()` returns `usize`; same for slice.
- Fat-pointer visual: the existing blue/red borrow arrow gains a small `[len: N]` annotation when the borrow's type is a slice (vs a single-element ref). Plan-phase decides whether to render two arrows (ptr + len) or a single arrow with metadata.
- New reference samples: ≥ 3 covering vector slicing, range bounds, len() on a slice.

**Out of scope.**
- `&str` (M07.2 — built on this slice infrastructure).
- Iterator methods (`v.iter()`, `for x in slice`) — keep this milestone focused.
- Mutable slices `&mut [T]` with element mutation — single-element borrows already deferred index-write; same here. Slices borrowed immutably only.
- Multi-dimensional slices.

**Entry criteria.**
- M07 closed.

**Exit criteria.**
- `let v = vec![…]; let s = &v[1..3];` typechecks; `s.len()` returns the correct length.
- The blue borrow arrow from `s` to the heap shows a length annotation.
- A slice taken before a Vec realloc becomes dangling (reuses M07's dangling-borrow detection on the underlying Vec's HeapAddr).
- ≥ 3 new `m07_1_*.rs` samples ship.

**Demo.** Browser. `m07_1_slice_basic.rs`: take `&v[..]`, observe slice arrow with length annotation. `m07_1_slice_range.rs`: `&v[1..3]`, observe partial-slice arrow. `m07_1_slice_dangling.rs`: slice survives a Vec push that reallocates — RuntimeError note.

**Notes.** Foundational for M07.2 — `&str` is "a slice into static memory" and reuses this milestone's fat-pointer + slice-type infrastructure.

---

### M07.2 — `&str` + static memory

- **Kind**: feature (revision-style — fixes M07's incorrect-but-pragmatic shortcut where string literals became `String` instead of `&'static str`)
- **Status**: planned
- **Complexity**: M (modules: 3, bullets: 4, boundaries: 2)
- **Depends on**: M07.1
- **Authority**: Rust language reference for `&'static str`; the slice infrastructure from M07.1.

**Goal.** Make string literals typecheck correctly as `&'static str` and visualize the static read-only memory region they live in. Currently M07 treats `let s = "toto"` as creating a `String` (heap-allocated, owned) — wrong type, wrong allocation behavior. M07.2 introduces a static-memory panel/region; literals get a slice into it; `String::from("toto")` is what actually allocates on the heap and copies the bytes.

**In scope.**
- New visual region: static-memory panel (or annotation within the existing heap panel) showing read-only bytes for each unique string literal.
- New Ty: `Ty::Ref { inner: Ty::Slice(Box::new(Ty::Int(U8))), mutable: false }` or a sugar `Ty::Str` (plan-phase decides). Borrowed-from-rodata pedagogy.
- String literal typing: `"toto"` is `&'static str` (a slice pointing into the static region), NOT `String`.
- `String::from(s: &str) -> String` heap-allocates a fresh chunk and copies the bytes from the static region.
- `String::push_str(s: &str)` takes &str.
- New reference samples: ≥ 2 covering the static-memory + heap distinction.

**Out of scope.**
- `&str` slicing (`&"hello"[1..3]`) — M07.1 already covers the range-indexing mechanics; could be tested but not the focus.
- `format!`, `print!`, owned string concatenation operators — these are separate features.
- Unicode handling beyond ASCII bytes — same as M07.

**Entry criteria.**
- M07.1 closed.

**Exit criteria.**
- `let s = "hi";` produces `s: &'static str`, NOT `s: String`. The visualization shows `s`'s borrow arrow pointing into the static-memory region.
- `String::from("hi")` produces a heap allocation; the trace contains a `HeapAlloc` event and the bytes are copied (visible as the static `"hi"` and the heap `"hi"` both showing the same content).
- The static-memory region persists across the whole trace; no allocations / no frees within it.
- ≥ 2 new `m07_2_*.rs` samples ship.

**Demo.** Browser. `m07_2_str_literal.rs`: `let s = "toto";`, observe the static-memory region + the &str arrow. `m07_2_string_from.rs`: `String::from("hi")` shows the static "hi" PLUS the new heap allocation with a copy.

**Notes.** Closes M07's known type-incorrectness (string literals shouldn't be `String`). The static-memory region is a new visual concept worth its own pedagogy — distinguishes "this binding's bytes are baked into the binary" from "this binding owns a heap allocation."

---

### M07.3 — Arrays (`[T; N]`, stack-allocated sequences)

- **Kind**: feature (revision-style — fills a Level-3 gap by introducing the stack-allocated counterpart to `Vec<T>`)
- **Status**: planned
- **Complexity**: M (modules: 3, bullets: 4, boundaries: 2)
- **Depends on**: M07.1
- **Authority**: Rust language reference for arrays `[T; N]`; M07.1's slice infrastructure (slicing an array produces `&[T]`).

**Goal.** Introduce fixed-size, stack-allocated arrays `[T; N]` end to end. Arrays are Rust's compile-time-sized sequence stored INLINE in the stack slot — no heap allocation, no realloc, no destructor. This is the natural counterpart to `Vec<T>`: same indexing/slicing/`len()` surface, different storage location. Slicing an array (`&t[1..3]`) produces an `&[T]` whose `target` is `Pointee::Slot(_)` — the first scenario in the project where `Value::Slice` carries a Slot target (M07.1 declared the variant but only ever constructed Heap-targeted slices).

**In scope.**
- Array literal expression `[e1, e2, e3]` — new AST node `Expr::ArrayLit { elements: Vec<Expr>, span }`.
- Array type annotation `[T; N]` — new AST `Type::Array { inner, size, span }` and `Ty::Array(Box<Ty>, u64)`. Size is a literal integer (no const-eval beyond `LitInt`).
- Array indexing `t[i]` — extends M07's Vec-only `Expr::Index` typeck to accept `Ty::Array(_, _)` receivers; same runtime bounds-check + RuntimeError pedagogy.
- Array slicing `&t[range]` produces `Ty::Slice(T)` with `Value::Slice { target: Pointee::Slot(t_slot), .. }` — extends M07.1's `typecheck_slice_borrow` to accept array receivers, and `eval_slice_borrow` to emit `BorrowShared { target: Pointee::Slot(_), .. }`.
- `t.len()` — extend the method dispatch with `(Ty::Array(_, N), "len") -> u64`; eval returns the array's size (no runtime lookup needed, it's compile-time-known).
- Visualization: array binding renders its bytes inline in its stack slot — multi-cell display (similar to M07's heap byte-cells but inside the slot value-column). Slice arrows into arrays route from slot to slot (M06-style routing) instead of slot to heap.
- ≥ 3 new reference samples (`m07_3_*.rs`): basic array, array indexing, slice-into-array.

**Out of scope.**
- Array repeat syntax `[v; N]` (e.g. `[0; 100]`) — deferred.
- Multi-dimensional arrays `[[T; N]; M]` — deferred.
- Arrays of non-Copy types (`[Box<i32>; 3]`) — deferred (matches M07's Vec-of-primitives restriction).
- Mutation through index `t[0] = 5;` — would require extending M06.1's place-expression set to include `Expr::Index`; deferred.
- `t.iter()`, `for x in t`, slice methods beyond `len()` — out of scope (matches M07.1's deferrals).

**Entry criteria.**
- M07.1 closed (slice infrastructure: `Ty::Slice`, `Value::Slice`, range parsing).

**Exit criteria.**
- `let t = [1, 2, 3];` typechecks as `Ty::Array(Ty::Int(I32), 3)`; the t slot in the stacks panel renders 3 inline i32 cells (no heap event fires).
- `let s = &t[1..]` typechecks as `Ty::Slice(Ty::Int(I32))`; produces a `Value::Slice { target: Pointee::Slot(t_slot), len: 2, .. }`; slice arrow points from s's slot to t's slot (NOT a heap block); `[len: 2]` annotation visible on the arrow; hover highlights cover the t-slot's 2nd and 3rd cells.
- `t.len()` returns `Value::Int { kind: U64, bits: 3 }`.
- Out-of-bounds index `t[100]` and out-of-bounds slice `&t[0..100]` fire the same `Note { RuntimeError }` messages as their Vec counterparts (reuses M07/M07.1 bounds-check infrastructure).
- ≥ 3 new `m07_3_*.rs` reference programs ship.
- All M01-M07.2 tests pass byte-identical (additive variant only; existing samples don't construct `Ty::Array`).
- WASM bundle growth ≤ +15% vs M07.2 baseline (small surface — one Ty variant, one AST expr, slice-target Pointee::Slot path that already exists in the renderer).

**Demo.**
- Format: browser
- Inputs: `tests/samples/m07_3_array_basic.rs` (`let t = [1, 2, 3]; let n = t.len();`), `m07_3_array_index.rs` (`let t = [10, 20, 30]; let x = t[1];`), `m07_3_array_slice.rs` (`let t = [1, 2, 3, 4]; let s = &t[1..3];`).
- Outputs (browser-observed steps): for array_basic — t's slot displays `[1_i32, 2_i32, 3_i32]` inline, no heap activity, n = 3_u64. For array_slice — load → step → at `let s` step, observe blue slice arrow from s to t (slot-to-slot, NOT slot-to-heap), `[len: 2]` annotation visible, hover lights up t's 2nd and 3rd element cells.

**Notes.** First milestone where `Value::Slice.target = Pointee::Slot(_)` is actually constructed — exercises a code path M07.1 declared but didn't reach. The hover-highlight infrastructure from M07.1 (byte-cells + element-spans) should generalize: instead of `[data-heap-addr]` lookups, slot-target slices look up `[data-slot-id]` then find the slot's inline byte-cells. Plan-phase decides whether to render array bytes as a Vec-style horizontal cell strip or a more compact form. Pedagogically the headline contrast is **storage**: arrays live in the stack frame (bytes vanish when the frame leaves; no Drop, no free, no realloc); Vec lives on the heap (owning arrow + HeapAlloc + HeapFree). Same `len()`, same indexing, same slicing — different memory location.

---

### M07.4 — Structs + impl blocks (named-field composite types with methods)

- **Kind**: feature (revision-style — fills a Level-3 gap by introducing user-defined composite types, completing the in-the-language tools learners need to model their own data)
- **Status**: shipped (commit `eabacb5`)
- **Complexity**: XL (modules: 5, bullets: 9, boundaries: 3)
- **Depends on**: M07.3
- **Authority**: CLAUDE.md › Pedagogical goal › "Give a newcomer concrete intuition for Rust's memory mechanics: moves, borrows, lifetimes, drops, heap allocations" — structs are the primary tool a learner uses to MODEL data once they've understood primitives + sequences. Without structs, every example is a synthetic toy. CLAUDE.md › Supported Rust subset doesn't explicitly enumerate `struct`, but the broader pedagogical goal of "make Rust's memory mechanics tangible" requires the type system's core composite-type primitive.

**Goal.** Introduce user-defined `struct` types with named fields, plus `impl` blocks providing associated functions and methods (with `&self`, `&mut self`, and `self` receivers). Inline rendering shows the struct's byte layout in the stack slot — one cell strip + per-field label per field — with field-borrow arrows pointing at the slot with a field-name annotation (analogous to slice arrows' `[len: N]` label). Method dispatch resolves via a per-type table populated from `impl` blocks.

**In scope.**
- **Struct declaration**: `struct Point { x: i32, y: i32 }` as a new `Item::Struct` AST node; named fields with types; declaration order significant for byte layout AND drop order.
- **Struct literal**: `Point { x: 1, y: 2 }` as a new `Expr::StructLit { path, fields, span }` AST node. Field-shorthand `Point { x, y }` (when local has same name) included — small parser concession with big ergonomic payoff.
- **Field access**: `p.x` as a new `Expr::FieldAccess { receiver, name, span }`. Rvalue — returns a copy of the field's value (assuming Copy element type).
- **Field borrow**: `&p.x` produces `Value::Ref { target: Pointee::Slot(slot_id), .. }` with a new `field_offset` / `field_name` annotation that drives the per-field hover-highlight on the source slot. Plan-phase confirms the Value::Ref extension shape (extending Ref with optional field metadata vs. a new Value::FieldRef variant).
- **`impl` blocks**: `impl Point { ... }` as a new `Item::Impl { ty_path, items, span }` AST node. Inside: associated functions (`fn new(x: i32, y: i32) -> Point`) and methods with self-receivers (`fn x(&self) -> i32 { self.x }`).
- **Self-receivers**: `&self`, `&mut self`, `self` (owned — moves the receiver, becomes inaccessible after). Mirrors Rust's standard receiver semantics.
- **Method call dispatch**: extends the M07 method dispatch table — first looks for matching `(receiver_ty, method_name)` rows in the M07 hardcoded built-ins (Vec/String/Slice/Array/Str), then in the user-defined impl-block registry built during typeck's collect-impls pre-pass.
- **Associated function call**: `Point::new(1, 2)` extends the path-fn dispatch table — first the hardcoded built-ins (Box::new, Vec::new, String::from), then user-defined impl-block associated functions.
- **Drop semantics**: per-field destructor in source declaration order at scope exit. For M07.4's primitive-only restriction (all fields are Copy), this is observable only as the slot's cells clearing in declaration order — but the pedagogy mechanic is established for future non-Copy field types.
- **Inline rendering**: stack slot's value area shows byte-cell strips per field with field-name labels above each strip (similar to M07.3 array inline cells but per-field rather than per-element).
- **Restrictions** preserving manageable scope: primitive field types only (Int/Float/Bool/Unit, matching M07's Vec/Array restriction); single-segment paths only (no `mod::Point`); one impl block per struct (multiple impl blocks deferred); no generic structs / generic methods; no derive macros.

**Out of scope.**
- **Generic structs** `Point<T>`, generic methods — deferred.
- **Traits, trait impls, trait objects** — deferred.
- **Derive macros** (`#[derive(Debug, Clone)]`) — deferred.
- **Struct update syntax** `Point { x: 10, ..p }` — deferred.
- **Tuple structs** `struct Pair(i32, i32)` — deferred.
- **Unit structs** `struct Marker;` — deferred.
- **Pattern matching on struct fields** `let Point { x, y } = p;` — deferred (no pattern matching in any milestone yet).
- **Multiple `impl` blocks per struct** — only one impl block recognized per struct in M07.4.
- **Non-Copy field types** (`struct Wrapper { v: Vec<i32> }`) — deferred. M07.4 restricts fields to primitives.
- **Recursive structs** (`struct Node { next: Option<Box<Node>> }`) — deferred (would require Option / enum support).
- **Associated constants** in impl blocks — deferred.
- **`self` field shorthand** in impl methods (e.g. `self.x = 5;` requires extending M06.1's place-expression set; same restriction as array index-write) — partial; read-only field access works; field assignment deferred.

**Entry criteria.**
- M07.3 closed (inline byte-cell + per-element rendering pattern in stack slots).
- M07.4 doesn't depend on M08 (threads) — sibling milestone.

**Exit criteria.**
- `struct Point { x: i32, y: i32 } fn main() { let p = Point { x: 1, y: 2 }; }` typechecks; the page renders p's slot with two byte-cell strips (one per field, labeled `x` and `y`) totaling 8 bytes.
- `let a = p.x` evaluates to `1_i32`; the field-access expression doesn't move `p` (Copy semantics).
- `let r = &p.x` produces a `Value::Ref { target: Pointee::Slot(_), .. }` with field metadata; the borrow arrow points from `r` to `p`'s slot with a `.x` annotation; hover highlights the `x` field's bytes specifically (not the whole struct).
- `impl Point { fn x(&self) -> i32 { self.x } }`; `let v = p.x();` typechecks; method dispatch resolves to the impl block's method; `v == 1_i32`.
- `let p = Point::new(1, 2);` typechecks; associated function call dispatches to the impl block; constructs the struct value.
- At scope exit, the slot's cells clear in field declaration order (drop pedagogy).
- ≥ 4 new `m07_4_*.rs` reference programs ship covering: struct literal + field access, field borrow, associated function (`Point::new`), method call (`p.x()`).
- All M01–M07.3 tests pass byte-identical.
- WASM bundle growth ≤ +25% vs M07.3 baseline (substantial new surface: AST nodes for struct/impl, typeck registry for user-defined types/methods, eval method dispatch).

**Demo.** Browser. `m07_4_struct_basic.rs` (`struct Point { x: i32, y: i32 } let p = Point { x: 1, y: 2 }; let a = p.x;`): stack slot shows `p : Point` with two byte-cell strips. `m07_4_field_borrow.rs`: `&p.x` produces a slot-target arrow with `.x` field annotation. `m07_4_method.rs`: defines `impl Point { fn x(&self) -> i32 { self.x } }` and calls `p.x()` — dispatches via the impl registry. `m07_4_associated_fn.rs`: `Point::new(1, 2)` constructs the struct via associated function call.

**Notes.** Sized XL by design — adds two new top-level Items (`Struct` and `Impl`), one new `Ty::Struct` variant carrying field schema, one new `Value::Struct` variant carrying field values, and the impl-block dispatch registry. Plan-phase will likely identify a clean US split (struct decl + literal + field access as US1/P1; field borrow as US2/P1; impl/methods as US3/P1; associated functions as US4/P2). 8th invocation of the closed-enum-with-revisions rule (additive Ty::Struct + Value::Struct + possibly Value::Ref field extension). Considered splitting into M07.4 (basic structs) + M07.5 (impl/methods) — maintainer chose to bundle for cohesive pedagogy, accepting larger scope. If implementation slips XL → XXL during execution, plan-phase splits in place per the M06/M06a/M06b precedent.

---

### M07.5 — Generics (`fn foo<T>(...)`, `struct Wrapper<T>`, monomorphization viz)

- **Kind**: feature (revision-style — adds type parameters to the function/struct surface introduced in M07.4, the foundation that M07.6 trait bounds build on)
- **Status**: shipped (commit `983995a`)
- **Complexity**: XL (modules: 5+, bullets: 8, boundaries: 2)
- **Depends on**: M07.4
- **Authority**: Rust language reference for generic parameters; CLAUDE.md › Pedagogical goal extension — without generics, structs/fns can't express "container that holds any T" (`Vec<T>` is hardcoded as a built-in; `Wrapper<T>` would be the first user-defined generic).

**Goal.** Introduce type parameters end to end: `fn id<T>(x: T) -> T { x }`, `struct Wrapper<T> { v: T }`, and `let w = Wrapper::<i32> { v: 5 };` (turbofish for explicit param). Pedagogical headline: **monomorphization** — each concrete `T` substitution creates a distinct instantiation visible in the trace. `id::<i32>(5)` and `id::<bool>(true)` produce two distinct FrameEnter events with the substituted type names (`id::<i32>` and `id::<bool>`), making "generics are zero-cost via duplication at compile time" tangible.

**In scope.**
- **Generic-parameter syntax** on fn decls (`fn id<T>(x: T) -> T`) and struct decls (`struct Wrapper<T> { v: T }`) — single-letter type params, no bounds yet (those land in M07.6).
- **Type parameter AST node**: extend `FnDecl` and `StructDecl` with `type_params: Vec<TypeParam { name: String, span: Span }>`.
- **Type-variable representation in typeck**: new `Ty::Var(TyVarId)` (or `Ty::Param(String)`) for substitution during instantiation.
- **Generic-fn typecheck**: when a fn has type params, body typecheck uses fresh type variables; call site infers + substitutes (simple unification — no full HM, just direct-match).
- **Turbofish call** `id::<i32>(5)` for explicit type annotation; bare `id(5)` infers from arg types.
- **Generic struct literal**: `Wrapper::<i32> { v: 5 }` or `Wrapper { v: 5 }` (inferred).
- **Monomorphization trace**: each call-site's frame name reflects the substituted types — `id::<i32>` vs `id::<bool>` — drives the pedagogy "the same source fn produces distinct frames per substitution".
- **Restrictions** preserving manageable scope: single-letter type params (T, U, V…); one type param per fn/struct (no `<T, U>`); no bounds (`T: Foo` deferred to M07.6); no higher-kinded; no lifetimes-as-generics (lifetimes elided as before); no const generics.

**Out of scope.**
- **Trait bounds** (`T: Foo`) — deferred to M07.6.
- **Multiple type params** (`fn pair<T, U>(...)`) — deferred. Single-T is enough for the headline pedagogy.
- **Where clauses** — deferred.
- **Lifetime parameters** as explicit generics — deferred (existing scope-level lifetime handling stays).
- **Const generics** (`Wrapper<T, const N: usize>`) — deferred.
- **GATs / higher-kinded types** — never.
- **Default type params** — deferred.

**Entry criteria.**
- M07.4 closed (struct + impl + method dispatch infrastructure that generic impls layer onto).

**Exit criteria.**
- `fn id<T>(x: T) -> T { x }` typechecks; `let a = id(5); let b = id(true);` both work; the trace shows two distinct frames `id::<i32>` and `id::<bool>`.
- `struct Wrapper<T> { v: T } let w = Wrapper { v: 5 };` typechecks; w's slot shows `Wrapper<i32>` with the inner `v: i32` field rendering identically to a non-generic struct.
- Turbofish `id::<bool>(false)` works.
- Mismatched type arg (`id::<bool>(5)`) → typeck error with both types named.
- ≥ 3 new `m07_5_*.rs` reference programs ship: identity fn, generic wrapper struct, monomorphization-shows-distinct-frames.
- All M01–M07.4 tests pass byte-identical.
- WASM bundle growth ≤ +20% vs M07.4 baseline.

**Demo.** Browser. `m07_5_id_fn.rs`: `id(5); id(true);` — observe two distinct `id::<i32>` and `id::<bool>` frames. `m07_5_generic_struct.rs`: `Wrapper { v: 5 }` renders as `Wrapper<i32>` in the slot's type column. `m07_5_turbofish.rs`: `id::<bool>(false)` with explicit type arg.

**Notes.** Sized XL. 9th invocation of the closed-enum-with-revisions rule (additive `Ty::Var(_)` or `Ty::Param(_)` for substitution). The headline pedagogy isn't the syntax but the **monomorphization visibility** — distinct frames per substitution makes the cost model tangible. M07.5 doesn't enable polymorphism on its own (no `T: Trait` bounds); the payoff lands in M07.6 when bounds become expressible.

---

### M07.6 — Traits (declarations, impls, static dispatch via bounds)

- **Kind**: feature (revision-style — adds polymorphism to the user-defined-types surface; turns M07.5's `T` into a useful constraint mechanism via `T: Trait` bounds)
- **Status**: planned
- **Complexity**: XL (modules: 5+, bullets: 10, boundaries: 3)
- **Depends on**: M07.5 (generics — `T: Trait` bound is the headline payoff)
- **Authority**: Rust language reference for trait declarations + inherent impls + trait bounds; CLAUDE.md › Pedagogical goal — traits ARE Rust's polymorphism mechanism; without them the language feels half-built after M07.4 introduces user types.

**Goal.** Introduce trait declarations + trait impls + trait-bound generics. Headline scenario: `trait Show { fn show(&self) -> String; } impl Show for Point { fn show(&self) -> String { ... } } fn print<T: Show>(x: T) { ... }`. Method dispatch extended with a third layer (builtins → inherent impls → trait impls). Static dispatch only — `&dyn Trait` and vtables deferred.

**In scope.**
- **Trait declaration**: `trait Foo { fn bar(&self) -> i32; fn baz(&self) -> i32 { self.bar() + 1 } }` as a new `Item::Trait { name, items, span }`. Items are fn signatures (no body — required method) or fn decls (with body — default method).
- **Trait impl block**: `impl Foo for Point { fn bar(&self) -> i32 { self.x } }` as `Item::Impl { trait_name: Option<String>, ty_name, items, span }` (extends M07.4's `Item::Impl` with optional trait_name).
- **TraitRegistry + TraitImplRegistry** at typeck: trait_name → required+default methods; (trait_name, struct_name) → method overrides.
- **Method dispatch — third layer**: extends M07.4's fall-through (builtins → inherent → trait impls). For `p.bar()` where `p: Point`: try Point's inherent impl first; if missing, search trait impls in scope; pick the matching one. Ambiguity (two traits define `bar`) → error with both candidates.
- **Default methods**: when a trait provides a default body and the impl doesn't override, dispatch resolves to the default.
- **Trait bounds on generics**: `fn print<T: Show>(x: T) { x.show(); }` — the body can call `x.show()` because the bound proves it. Call site checks the arg's type implements Show.
- **Multiple bounds** via `+`: `fn print<T: Show + Clone>(x: T)` — both traits available in the body.
- **Restrictions**: static dispatch only (no `&dyn Foo`); single-trait-per-bound chain (no nested generics like `T: Foo<Bar>`); no associated types / consts on traits; no supertraits; no blanket impls; no derive macros for traits.

**Out of scope.**
- **Trait objects `&dyn Foo`** — needs vtable machinery + heap-or-table representation; deferred to a future "dynamic dispatch" milestone if pedagogically valuable.
- **Associated types** (`trait Iter { type Item; fn next(&mut self) -> Option<Self::Item>; }`) — deferred.
- **Associated consts** on traits — deferred.
- **Supertraits** (`trait Foo: Bar`) — deferred.
- **Blanket impls** (`impl<T: Foo> Bar for T`) — deferred.
- **Derive macros** (`#[derive(Debug, Clone)]`) — deferred (would need built-in implementations for the std traits).
- **Trait inheritance / multiple inheritance edge cases** — out of scope by virtue of the no-supertrait restriction.
- **Auto-traits** (`Send`, `Sync`) — deferred to M08 when threads land.
- **Where clauses** on traits — deferred.
- **Trait method orphan rules** — out of scope (M07.6 has only one file; orphan rules require multi-file/crate awareness).

**Entry criteria.**
- M07.5 closed (generics + type params + monomorphization machinery).
- M07.4 closed (inherent impl dispatch infrastructure that trait dispatch layers onto).

**Exit criteria.**
- `trait Show { fn show(&self) -> String; }` parses; `impl Show for Point { fn show(&self) -> String { ... } }` typechecks; `let s = p.show();` dispatches via the trait impl.
- Default methods work: `trait Counter { fn count(&self) -> i32; fn double(&self) -> i32 { self.count() * 2 } }`; impl provides `count` only; `p.double()` dispatches to the default.
- `fn print<T: Show>(x: T)` typechecks; body can call `x.show()`; call site rejects types that don't impl Show.
- Multi-bound `fn show_n_clone<T: Show + Clone>(x: T)` works.
- Dispatch order verified: inherent impl wins when both inherent + trait method share a name.
- Missing required method in impl → typeck error naming the unimplemented method.
- ≥ 4 new `m07_6_*.rs` reference programs: trait decl + impl, default method, generic bound, multi-bound.
- All M01–M07.5 tests pass byte-identical.
- WASM bundle growth ≤ +25% vs M07.5 baseline.

**Demo.** Browser. `m07_6_trait_basic.rs`: `trait Show { fn show(&self) -> String; } impl Show for Point { ... } let s = p.show();` — dispatch resolves to the trait impl; frame opens for `Point::<Show>::show` (or similar mangled name). `m07_6_default_method.rs`: trait with default body; impl skips the override; the trait's default method body runs. `m07_6_generic_bound.rs`: `fn print<T: Show>(x: T) { let s = x.show(); }` — call site with a type implementing Show works; with one that doesn't, typeck error. `m07_6_multi_bound.rs`: `T: Show + Clone`.

**Notes.** Sized XL. 10th invocation of the closed-enum-with-revisions rule (additive `Item::Trait`; extends `Item::Impl` with `trait_name: Option<String>`). The headline pedagogy is the **bound payoff** — `fn print<T: Show>` shows why generics + traits together unlock polymorphism (whereas either alone is half-built). Considered alternatives: combining generics + traits into one mega-milestone (rejected — would slip XL to XXL+); landing traits before generics (rejected — without bounds, traits feel like syntactic organization without a punch). The two-milestone sequence (M07.5 → M07.6) lets each ship a clean pedagogical headline.

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
