---

description: "Task list for producing MILESTONES.md, the rustviz milestone roadmap document"
---

# Tasks: Reliable Milestone Splitting Plan

**Input**: Design documents from `/specs/001-milestone-roadmap/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/milestone-schema.md ✓, quickstart.md ✓

**Tests**: This feature's deliverable is a markdown document, not code. There are no automated tests. "Tests" are conformance audits (contract checks C-1…C-8) executed as tasks in Phase 7 and as the per-story verification phases.

**Organization**: Tasks are grouped by user story so each story can be verified independently. US1 and US2 are both P1 and both expressed in the milestone-block content, but US2 has its own verification phase to check the demoability and independent-shippability properties of those blocks.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files OR read-only with no fix dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3, US4)
- Include exact file paths in descriptions (relative to repo root unless absolute)

## Path Conventions

This is a documentation-only feature. All deliverable paths are relative to the repo root `/home/noa/Documents/projects/lab/rustviz/`. The single deliverable file is `MILESTONES.md`. Supporting artifacts live under `specs/001-milestone-roadmap/`.

---

## Phase 1: Setup

**Purpose**: Create the `MILESTONES.md` skeleton at the repo root with the contract-required top-level structure (empty sections to be filled in subsequent phases).

- [X] T001 Create `MILESTONES.md` at repo root with the exact top-level structure from `specs/001-milestone-roadmap/contracts/milestone-schema.md`: h1 `# rustviz Milestone Roadmap`, `**Source of truth**:` line pointing to `CLAUDE.md`, `**Last audit**:` line with today's date (2026-05-20), one-paragraph intro explaining what the document is and how to read it (point readers to `specs/001-milestone-roadmap/quickstart.md` for the read/audit/revise procedures), then empty `## Dependency graph`, `## Milestones`, and `## Deferred` sections — no content yet, just headings in the right order

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Build the cross-cutting artifacts every user story depends on — the scope-bullet inventory (US3 audit input), the deferred bucket content (US3 closure), and the dependency graph (US4 anchor).

**⚠️ CRITICAL**: No user-story phase can begin until Phase 2 is complete.

- [X] T002 [P] Create the canonical CLAUDE.md scope-bullet inventory at `specs/001-milestone-roadmap/scope-inventory.md` — enumerate every scope-bearing bullet from `CLAUDE.md`: each bullet under `## Architecture` (3 layers), `## Event model` (7 event categories: Threads, Frames, Stack slots, Heap, Borrows, Sync, Pedagogy), `## The three panels` (Editor, Stacks, Heap, Pointers — 4 bullets), `## Supported Rust subset (by levels)` (4 bullets: L1, L2, L3, L4), and `## Immediate roadmap` (5 numbered steps). Output as a flat numbered list `SB-001`…`SB-NNN`, each line citing the CLAUDE.md section + verbatim short quote. This is the audit source-of-truth for T013 (US3 coverage check). Per `specs/001-milestone-roadmap/quickstart.md` step 1.

- [X] T003 [P] Write the `## Deferred` section content in `MILESTONES.md` — one bullet per deferred item from `specs/001-milestone-roadmap/research.md` R-011: (1) Detailed `Send`/`Sync` inference, (2) Parser error recovery, (3) Multi-file support, (4) Lifetime visualization beyond scope-level, (5) Levels beyond L4. Each bullet follows the contract format `**<title>** — <reason>. (CLAUDE.md › <section> › "<quote>")` where applicable. Citations must satisfy VR-15 (verbatim substring).

- [X] T004 Draw the dependency graph in the `## Dependency graph` section of `MILESTONES.md` — code-fenced ASCII chain `M01 → M02 → M03 → M04 → M05 → M06 → M07 → M08`, with one branch annotation showing `M05` also depends directly on `M03` per `specs/001-milestone-roadmap/research.md` dependency-graph section. The drawn order must be a valid topological sort.

**Checkpoint**: `MILESTONES.md` has structure, deferred bucket, and graph. Scope inventory is built. Ready to write milestone blocks.

---

## Phase 3: User Story 1 — Maintainer plans next slice (Priority: P1) 🎯 MVP

**Goal**: Each of the 8 milestone blocks (M01–M08) exists in `MILESTONES.md` with all 12 fields required by the contract (Kind, Status, Complexity with axes, Depends on, Authority, Goal, In scope, Out of scope, Entry criteria, Exit criteria, Demo, Notes). A maintainer reading the document cold can identify the next milestone and its exit criteria within 10 minutes (SC-001).

**Independent Test**: Hand `MILESTONES.md` plus `CLAUDE.md` to someone unfamiliar with the project. They identify the active milestone, its scope, and its exit criteria without follow-up questions. Verified by running through `specs/001-milestone-roadmap/quickstart.md` "Read" section steps 1–5.

> **Note on parallelism**: T005–T012 all write distinct sections (different milestone blocks) of the same file `MILESTONES.md`. Content derivation is fully independent and can be done in parallel, but file writes must be sequential to avoid merge conflicts. Mark as sequential here; parallelize content derivation off-band if multiple agents are working concurrently.

- [X] T005 [US1] Write the M01 block in `MILESTONES.md` per `specs/001-milestone-roadmap/contracts/milestone-schema.md`. **Kind**: foundation. **Status**: planned. **Complexity**: `L (modules: 4, bullets: 3, boundaries: 0)`. **Depends on**: —. **Authority**: `CLAUDE.md › Planned code layout › "src/parse/ … span.rs, lexer.rs, ast.rs, parser.rs"`; `CLAUDE.md › Immediate roadmap › "Integrate the parse/ skeleton"`; `CLAUDE.md › Locked-in decisions › "No parser framework"`. **Goal**: deliver the parser front-end (span, lexer, AST, recursive-descent parser) sufficient to consume Level 1 syntax. **In scope**: span tracking with byte offsets + `FileId`, separate lexer producing `Vec<Token>`, hand-rolled recursive-descent parser, AST types with spans at every level, rejection of `&` at lexer per CLAUDE.md locked-in decision, "stop at first parse error" behavior. **Out of scope**: name resolution, type checking, evaluation, `&`/`&mut` tokens (added in M06), error recovery (deferred). **Entry criteria**: cargo project exists with empty `src/parse/` module tree. **Exit criteria**: snapshot tests under `tests/snapshots/m01_*.snap` cover ≥3 sample L1 programs and pass; lexer rejects `&` with a clear error; parser stops at first error with a span-bearing message. **Demo**: `Format: snapshot`, `Inputs: tests/samples/m01_*.rs (3 files)`, `Outputs: tests/snapshots/m01_*.snap`, `Command: cargo test --test m01`. **Notes**: code drafted in `conversation.html` per CLAUDE.md immediate-roadmap step 1.

- [X] T006 [US1] Write the M02 block in `MILESTONES.md`. **Kind**: foundation. **Status**: planned. **Complexity**: `M (modules: 2, bullets: 2, boundaries: 1)`. **Depends on**: `M01`. **Authority**: `CLAUDE.md › Planned code layout › "resolve/ … Ident → BindingId"`; `CLAUDE.md › Planned code layout › "typeck/ … annotation checks, type propagation"`; `CLAUDE.md › Immediate roadmap › "Name resolver"`; `CLAUDE.md › Immediate roadmap › "Lightweight typeck"`. **Goal**: resolve identifiers to binding IDs and validate annotations / propagate obvious types over the M01 AST. **In scope**: `Ident → BindingId` resolution, "use of undeclared variable" errors with spans, annotation validation for L1 types, simple type propagation across `let`/`fn`/`if`-expression results. **Out of scope**: trait resolution, generic inference, lifetime inference, borrow checking. **Entry criteria**: M01 closed (AST + spans available). **Exit criteria**: snapshot tests cover at least one resolution-failure case and one type-mismatch case under `tests/snapshots/m02_*.snap`; resolver returns a stable `BindingId` for every use site. **Demo**: `Format: snapshot`, `Inputs: tests/samples/m02_*.rs`, `Outputs: tests/snapshots/m02_*.snap (including error snapshots)`, `Command: cargo test --test m02`.

- [X] T007 [US1] Write the M03 block in `MILESTONES.md`. **Kind**: foundation. **Status**: planned. **Complexity**: `M (modules: 2, bullets: 3, boundaries: 1)`. **Depends on**: `M02`. **Authority**: `CLAUDE.md › Event model › "MemEvent is the centerpiece"`; `CLAUDE.md › Supported Rust subset › "Level 1: primitives, let/let mut, functions, scopes, moves of non-Copy types"`; `CLAUDE.md › Immediate roadmap › "Define MemEvent and write the level-1 evaluator"`; `CLAUDE.md › Planned code layout › "eval/ … AST walker, emits MemEvent"`; `CLAUDE.md › Planned code layout › "event.rs … MemEvent enum"`. **Goal**: define the `MemEvent` enum (all categories — `Frames`, `Stack slots`, `Pedagogy/Note`, plus stubs/variants for Threads/Heap/Borrows/Sync to land their data later) and an L1 evaluator that walks M02's resolved+typed AST and emits a `Vec<MemEvent>`. **In scope**: `MemEvent` variants for `FrameEnter`/`FrameLeave`, `SlotAlloc`/`SlotWrite`/`SlotMove`/`SlotDrop`, `Note{kind, message, span}`; `SourceSpan` carried on every event; `Pointee` enum (`Slot(SlotId) | Heap(HeapAddr)`); L1 evaluator covering primitives, let/let mut, functions (call & return), scopes, moves of non-Copy types, blocks-as-expressions, `if` expressions, operators with precedence. **Out of scope**: borrows (M06), heap (M07), threads/sync (M08), UI (M04). **Entry criteria**: M02 closed. **Exit criteria**: running the evaluator on at least 3 L1 sample programs produces a deterministic `Vec<MemEvent>` matching a snapshot; the `SlotMove` vs `SlotDrop` distinction is observable on a sample that triggers it. **Demo**: `Format: snapshot`, `Inputs: tests/samples/m03_*.rs (L1 programs including moves of non-Copy)`, `Outputs: tests/snapshots/m03_*.snap (event-stream dumps)`, `Command: cargo test --test m03`. **Notes**: the `Note` infrastructure lands here so later milestones (M06+) can emit pedagogical notes without retrofitting.

- [X] T008 [US1] Write the M04 block in `MILESTONES.md`. **Kind**: foundation. **Status**: planned. **Complexity**: `L (modules: 4, bullets: 4, boundaries: 2)`. **Depends on**: `M03`. **Authority**: `CLAUDE.md › Architecture › "UI (web, WASM bindings)"`; `CLAUDE.md › The three panels › "Editor (Monaco or CodeMirror)"`; `CLAUDE.md › The three panels › "Stacks: one column per thread"`; `CLAUDE.md › Architecture › "The UI replays the stream with a cursor (play / pause / step / rewind)"`; `CLAUDE.md › Immediate roadmap › "First UI prototype: single stack panel, static replay of a pre-recorded trace"`. **Goal**: deliver a browser UI shell that loads a pre-recorded `Vec<MemEvent>` (from M03) and replays it through a play/pause/step/rewind cursor, with the editor panel highlighting the current event's span and a single-column stacks panel showing slot allocations. **In scope**: WASM bindings to expose the event stream, minimal HTML host, editor panel (Monaco or CodeMirror — decide at M04 start, noted in `## Notes`), single stacks panel column rendering `SlotAlloc`/`SlotWrite`/`SlotMove`/`SlotDrop`, replay cursor with play/pause/step/rewind, source-span highlighting in the editor. **Out of scope**: live interpretation (M05), heap panel (M07), multi-thread stacks (M08), pointer overlay (M06). **Entry criteria**: M03 closed; a pre-recorded `.events.json` trace from at least one L1 sample program exists in `tests/samples/`. **Exit criteria**: opening the browser, loading the pre-recorded trace, and stepping through it visibly highlights matching spans in the editor and updates the stacks panel; cursor responds to play/pause/step/rewind. **Demo**: `Format: browser`, `Inputs: tests/samples/m04_pre_recorded.events.json + matching .rs source`, `Outputs: (browser-observed steps) — 1. open page, 2. trace loaded, 3. click play and observe slot x highlight at step 3, 4. step backward returns to prior state`, `Command: trunk serve --open` (or equivalent — finalize when M04 starts). **Notes**: editor framework choice (Monaco vs CodeMirror) is open per `specs/001-milestone-roadmap/research.md` R-011 open question; resolve at M04 start.

- [X] T009 [US1] Write the M05 block in `MILESTONES.md`. **Kind**: feature. **Status**: planned. **Complexity**: `S (modules: 1, bullets: 1, boundaries: 1)`. **Depends on**: `M04`, `M03`. **Authority**: `CLAUDE.md › Supported Rust subset › "Level 1: primitives, let/let mut, functions, scopes, moves of non-Copy types"`; `CLAUDE.md › Architecture › "The interpreter never writes to the UI directly. It emits a typed event stream."`. **Goal**: connect M03's evaluator to M04's UI shell so that editing an L1 program in the editor produces a live event stream that the stacks panel replays. **In scope**: glue layer that re-runs the M01–M03 pipeline on editor input (debounced), pipes the resulting `Vec<MemEvent>` into the M04 replay cursor, error display in the editor for parse/resolve/typeck failures. **Out of scope**: anything beyond L1 (references, heap, threads). **Entry criteria**: M04 closed (UI shell replays static traces); M03 closed (evaluator emits events). **Exit criteria**: typing a valid L1 program in the editor and clicking "run" produces a fresh trace that the stacks panel can replay; a parse/resolve/typeck error is shown in the editor with a span underline. **Demo**: `Format: browser`, `Inputs: live editor input (sample L1 programs in tests/samples/m05_*.rs)`, `Outputs: (browser-observed steps) — 1. type let x = 5; in editor, 2. click run, 3. observe SlotAlloc x:i32=5 in stacks panel, 4. step through and observe x highlighting`, `Command: trunk serve --open`. **Notes**: M05 is the project's first publicly demoable artifact; consider tagging the closing commit and capturing a short screen-recording for the project's README.

- [X] T010 [US1] Write the M06 block in `MILESTONES.md`. **Kind**: feature. **Status**: planned. **Complexity**: `L (modules: 3, bullets: 5, boundaries: 2)`. **Depends on**: `M05`, `M03`. **Authority**: `CLAUDE.md › Supported Rust subset › "Level 2: & and &mut, aliasing rules, scope-level lifetimes"`; `CLAUDE.md › Event model › "Borrows: BorrowShared, BorrowMut, BorrowEnd (with BorrowId to materialize the borrow's lifetime visually)"`; `CLAUDE.md › The three panels › "Pointers: SVG overlay across the panels"`; `CLAUDE.md › Locked-in decisions › "Reject & at the lexer in level 1 … Replace with Amp/AmpMut tokens when level 2 lands"`. **Goal**: extend the front-end and evaluator to handle `&` and `&mut` and the aliasing rules; introduce the SVG pointer overlay rendering blue arrows for `&` and red arrows for `&mut`. **In scope**: lexer accepts `&` and `&mut` (replacing the M01 rejection), parser/AST/resolver/typeck extensions for borrow types, evaluator emits `BorrowShared`/`BorrowMut`/`BorrowEnd` with `BorrowId`, scope-level lifetime tracking, SVG pointer overlay component in the UI, aliasing-rule violation detection emitting pedagogical `Note` events. **Out of scope**: heap (M07), threads (M08), generic/named lifetimes (deferred), Rc/Arc pointers (M08). **Entry criteria**: M05 closed (live L1 works). **Exit criteria**: a sample program `let x = 5; let r = &x;` renders with a blue arrow from the `r` slot to the `x` slot in the browser; a violation like overlapping `&mut` references emits a Note that the UI displays. **Demo**: `Format: browser`, `Inputs: tests/samples/m06_*.rs`, `Outputs: (browser-observed steps) — 1. type let r = &x sample, 2. observe blue arrow r → x, 3. type aliasing violation, 4. observe Note message and span underline`, `Command: trunk serve --open`. **Notes**: candidate for in-place split into `M06a` (borrow tracking + events) and `M06b` (pointer overlay) if mid-implementation it exceeds L on any axis.

- [X] T011 [US1] Write the M07 block in `MILESTONES.md`. **Kind**: feature. **Status**: planned. **Complexity**: `L (modules: 3, bullets: 5, boundaries: 2)`. **Depends on**: `M06`, `M03`. **Authority**: `CLAUDE.md › Supported Rust subset › "Level 3: Box, Vec (with visible realloc), String"`; `CLAUDE.md › Event model › "Heap: HeapAlloc, HeapRealloc, HeapFree"`; `CLAUDE.md › The three panels › "Heap: free-form area where each HeapAlloc creates a box"`; `CLAUDE.md › The three panels › "HeapRealloc animates: the box moves and every arrow pointing to it follows"`; `CLAUDE.md › Pedagogical goal › "this &v[0] becomes UB after v.push()"`. **Goal**: extend the evaluator to model heap allocation and reallocation for `Box`, `Vec`, and `String`; deliver the heap panel UI with allocation boxes and the realloc-snap animation. **In scope**: evaluator handling for `Box::new`, `Vec::new`/`push`/indexing, `String::from`/`push_str`, `HeapAlloc`/`HeapRealloc`/`HeapFree` event emission with sizes and types, heap panel rendering boxes (size proportional to allocation, label = type), pointer overlay extension to draw black owning arrows for `Box`/`Vec`/`String` and follow arrows through realloc animations, dangling-borrow detection after realloc emitting Notes. **Out of scope**: threads (M08), other heap-allocating types beyond Box/Vec/String. **Entry criteria**: M06 closed (pointer overlay exists). **Exit criteria**: a sample program creating a `Vec`, taking `&v[0]`, then `push`ing, visibly animates the heap box moving and either snaps the borrow arrow to the new address or emits a UB-Note (decision: emit Note describing UB, since rustc would reject); the animation is reproducible across runs. **Demo**: `Format: browser`, `Inputs: tests/samples/m07_vec_realloc.rs + others`, `Outputs: (browser-observed steps) — 1. step to Vec creation, observe heap box, 2. step to &v[0], observe blue arrow into heap box, 3. step to push, observe box animate to new position and Note appear`, `Command: trunk serve --open`. **Notes**: the realloc animation is the pedagogical centerpiece — do not ship without it (research.md R-009).

- [X] T012 [US1] Write the M08 block in `MILESTONES.md`. **Kind**: feature. **Status**: planned. **Complexity**: `L (modules: 3, bullets: 4, boundaries: 2)`. **Depends on**: `M07`. **Authority**: `CLAUDE.md › Supported Rust subset › "Level 4: thread::spawn, Arc, Mutex, Send/Sync"`; `CLAUDE.md › Event model › "Threads: ThreadSpawn, ThreadJoin, ThreadPark"`; `CLAUDE.md › Event model › "Synchronization: LockAcquire, LockRelease, ArcClone, ArcDrop"`; `CLAUDE.md › The three panels › "Stacks: one column per thread … Spawning a thread slides a new column in from the right. A thread parked on a mutex greys out and draws a dotted line"`; `CLAUDE.md › The three panels › "Pointers: dashed purple = Arc/Rc"`. **Goal**: extend the evaluator and UI to handle `thread::spawn`, `Arc`, and `Mutex` happy-path; stacks panel grows to multiple columns; parked threads grey out with a dotted line to the held mutex; pointer overlay adds dashed purple for `Arc`/`Rc`. **In scope**: evaluator handling for `thread::spawn`/`join`, `Arc::new`/`clone`/drop, `Mutex::new`/`lock`/`unlock`, `ThreadSpawn`/`Join`/`Park` events, `ArcClone`/`ArcDrop`/`LockAcquire`/`LockRelease` events, stacks-panel multi-column rendering with slide-in-from-right animation, parked-thread visual treatment, dashed purple Arc/Rc arrows in the overlay. **Out of scope**: full `Send`/`Sync` auto-trait inference (deferred per research R-011), poisoned-mutex behavior, channels, async. **Entry criteria**: M07 closed (heap exists, owning arrows exist). **Exit criteria**: a two-thread `Arc<Mutex<T>>` sample replays with both stack columns visible; one thread parked on the mutex visibly greys out and draws a dotted line to the held mutex slot; Arc clones show dashed purple arrows; the contention is reproducible. **Demo**: `Format: browser`, `Inputs: tests/samples/m08_arc_mutex.rs`, `Outputs: (browser-observed steps) — 1. spawn thread, observe second column slide in, 2. one thread locks, observe arc-arrow + lock indicator, 3. other thread attempts lock, observe parked state and dotted line, 4. unlock, observe parked thread resume`, `Command: trunk serve --open`. **Notes**: borderline-XL by per-event counting (9 atomic events across Threads + Sync) but L by event-category counting per the rubric (research R-007). If sizing-axis tracking reveals XL during implementation, split into `M08a` (threads + multi-col stacks) and `M08b` (Arc/Mutex/sync + dashed purple).

**Checkpoint**: All 8 milestone blocks exist in `MILESTONES.md`. User Story 1 is testable: open the document, identify the next milestone (M01, since all are `planned`), read its block, know what to build and how to prove done.

---

## Phase 4: User Story 2 — Independently shippable and demoable (Priority: P1)

**Goal**: Every milestone block in `MILESTONES.md` has a demo artifact that is runnable on `main` at the milestone's closing commit and exercises only the features in that milestone — no later-milestone code required.

**Independent Test**: For each milestone N, verify the `Demo.` block lists at least one runnable artifact, and the artifact's described inputs/outputs / browser steps exercise only `In scope` items of milestone N or earlier-milestone capabilities.

- [X] T013 [US2] Audit each milestone block in `MILESTONES.md` against contract validation rules VR-7, VR-11, VR-12, VR-13 from `specs/001-milestone-roadmap/data-model.md`: every Demo block has `Format`, `Inputs`, `Outputs`, `Command`; `Format: snapshot` appears only on M01–M03; `Format: browser` requires M04 in `Depends on` transitive closure; `Command` is runnable from repo root. Fix any non-conformance in place. Report findings in a short audit log appended to `specs/001-milestone-roadmap/checklists/requirements.md` under a new "## Post-implementation audit" section.

- [X] T014 [US2] Audit each milestone block's `Exit criteria` and `In scope` lines against VR-6 (mentions a runnable artifact) and against the independent-shippability requirement: no exit criterion may reference behavior, code, or events that the milestone's `Depends on` set does not transitively cover. For example, M07's exit criteria must not mention `Arc`/`Mutex` (M08-only). Fix any violations in place. Report in the same post-implementation audit log as T013.

**Checkpoint**: Demo artifacts and exit criteria are conformant. User Story 2 is testable: pick any milestone, verify its Demo runs without later-milestone code.

---

## Phase 5: User Story 3 — CLAUDE.md scope coverage (Priority: P2)

**Goal**: Every scope bullet enumerated in `specs/001-milestone-roadmap/scope-inventory.md` (T002 output) is referenced by exactly one milestone's `Authority` line, or appears in the `## Deferred` section with a stated reason. No orphans, no double-owners.

**Independent Test**: For each `SB-NNN` in the inventory, locate its claimant in `MILESTONES.md` (Authority line or Deferred bullet). Coverage = 100%.

- [X] T015 [US3] Run the coverage audit from `specs/001-milestone-roadmap/quickstart.md` "Audit" steps 1–4 against `MILESTONES.md` using `specs/001-milestone-roadmap/scope-inventory.md` as the input list. For each scope bullet `SB-NNN`, search `MILESTONES.md` for an Authority citation whose section + quote matches, OR a Deferred bullet that covers it. Outcomes per bullet: Owned (1 match) ✓, Deferred ✓, Orphan ✗ (add Authority cite to the most appropriate milestone, or add to Deferred with reason), Multi-owned ✗ (remove duplicate Authority cites, keep one). Append the per-bullet result table to `specs/001-milestone-roadmap/checklists/requirements.md` post-implementation audit section. Coverage must hit 100% before this task closes.

**Checkpoint**: Conformance check C-6 (coverage) passes. User Story 3 is testable: pick any CLAUDE.md scope bullet, find its claimant in the document.

---

## Phase 6: User Story 4 — Dependency DAG (Priority: P3)

**Goal**: The dependency graph induced by the `Depends on:` field across all non-`absorbed`, non-`deferred` milestone blocks is acyclic, and the document's milestone order is a valid topological sort.

**Independent Test**: Parse `Depends on:` lines from `MILESTONES.md` → construct directed graph → verify no cycles → verify the milestone order in `## Milestones` is a valid topological sort. Also verify the `## Dependency graph` ASCII diagram matches the parsed graph.

- [X] T016 [US4] Build the dependency graph from the `Depends on:` lines in `MILESTONES.md`, verify acyclicity, verify the document's milestone order (M01 first, M08 last) is a valid topological sort, and verify the `## Dependency graph` section's ASCII diagram (T004 output) matches the parsed edges. If the parsed graph and the diagram disagree, the diagram is wrong — update it to match the data. Report verification result in the post-implementation audit log.

**Checkpoint**: Conformance check C-5 (DAG) passes. User Story 4 is testable: read the graph, confirm it matches the Depends-on data.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Final conformance pass (contract checks C-1 through C-8), citation validity, and audit-log closure. These tasks gate the document's "ready to commit" state.

- [X] T017 [P] Run conformance checks C-1 (structural — top-level section order), C-2 (unique milestone IDs), C-3 (field order in every milestone block: Kind → Status → Complexity → Depends on → Authority → Goal → In scope → Out of scope → Entry criteria → Exit criteria → Demo → Notes), and C-4 (field value types — Kind ∈ {foundation, feature}, Status ∈ {planned, active, closed, absorbed, deferred}, Complexity ∈ {S, M, L} with non-XL axes) against `MILESTONES.md` per `specs/001-milestone-roadmap/contracts/milestone-schema.md`. Fix any violations in place. Report results in the post-implementation audit log.

- [X] T018 [P] Run citation-validity check C-7 against `MILESTONES.md`: for every Authority citation `CLAUDE.md › <section> › "<quote>"`, verify that `<section>` exists as a heading in the current `CLAUDE.md` (VR-14) AND that `<quote>` appears verbatim under that section (VR-15, whitespace-normalized substring match). For any citation that fails, either update the wording (if CLAUDE.md reworded) or flag in the audit log as a real scope drift requiring spec attention.

- [X] T019 Update the `**Last audit**:` line at the top of `MILESTONES.md` to today's date (2026-05-20) once T013–T018 all close clean.

- [X] T020 Update `specs/001-milestone-roadmap/checklists/requirements.md` "Notes" section with a final closure entry: which conformance checks passed, any deferred audit findings, and the date of the post-implementation pass.

- [X] T021 Stage the final `MILESTONES.md` (and any audit-log updates to `specs/001-milestone-roadmap/checklists/requirements.md` and the `specs/001-milestone-roadmap/scope-inventory.md` if T002 produced it as a tracked file) and report `git status` + a short summary of what changed. **Do not commit** — committing is the maintainer's call per project policy.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies. Starts immediately.
- **Phase 2 (Foundational)**: Depends on Phase 1 (skeleton must exist before headings can be filled).
- **Phase 3 (US1)**: Depends on Phase 2 (graph + deferred bucket in place before milestone blocks reference them).
- **Phase 4 (US2)**: Depends on Phase 3 (cannot audit demos before blocks exist).
- **Phase 5 (US3)**: Depends on Phase 3 AND T002 (needs both blocks and inventory). Can run in parallel with Phase 4 (T013/T014 and T015 read the same file but write to a shared audit log — sequencing T015 after T014 avoids audit-log conflicts).
- **Phase 6 (US4)**: Depends on Phase 3 (needs Depends-on lines). Can run in parallel with Phases 4 and 5 if audit log writes are sequenced.
- **Phase 7 (Polish)**: Depends on all of Phases 4, 5, 6 closing clean.

### User Story Dependencies (within a maintainer's incremental delivery)

- **US1 (P1)**: After Phase 2. The MVP — closing US1 means the document exists and is readable, even before audits.
- **US2 (P1)**: Strictly after US1 (its tasks audit US1's output).
- **US3 (P2)**: Strictly after US1, can parallelize with US2.
- **US4 (P3)**: Strictly after US1, can parallelize with US2/US3.

### Parallel Opportunities

- **T002 and T003**: Both Phase 2, write to different files (scope-inventory.md vs MILESTONES.md). [P] ✓
- **T002 and T004**: Both Phase 2; T002 writes scope-inventory, T004 writes MILESTONES.md graph section. [P] ✓
- **T003 and T004**: Both write distinct sections of `MILESTONES.md`. Strictly speaking same-file → sequential, but content derivation is independent.
- **T005–T012**: Same file → sequential writes, but content derivation per milestone is fully independent.
- **T017 and T018**: Both audits, read-only first pass. [P] ✓ for the read pass; sequential for any fix writes.
- **T013, T014, T015, T016**: Independent audits; content of fixes may conflict on `MILESTONES.md`. Run audits in parallel, apply fixes sequentially.

---

## Parallel Example: Phase 2 Foundational

```bash
# Run in parallel (different files / independent derivation):
Task T002: "Create scope-bullet inventory at specs/001-milestone-roadmap/scope-inventory.md"
Task T003: "Write ## Deferred section content (in-memory draft for MILESTONES.md)"
Task T004: "Draw dependency graph ASCII for ## Dependency graph (in-memory draft for MILESTONES.md)"

# Then serialize T003 and T004 writes to MILESTONES.md (one after the other).
```

## Parallel Example: Phase 7 audits (read-only first pass)

```bash
# All read-only, can run in parallel:
Task T017: "C-1 to C-4 structural & field-order conformance on MILESTONES.md"
Task T018: "C-7 citation validity on MILESTONES.md against CLAUDE.md"

# Any fixes from either task are applied sequentially to MILESTONES.md.
```

---

## Implementation Strategy

### MVP First (US1 only — "the document exists and is readable")

1. Complete **Phase 1**: skeleton (T001).
2. Complete **Phase 2**: scope inventory, deferred bucket, dependency graph (T002–T004).
3. Complete **Phase 3**: write all 8 milestone blocks (T005–T012).
4. **STOP and VALIDATE**: read `MILESTONES.md` cold. Can you identify the next milestone and its exit criteria in 10 minutes? If yes, MVP shipped.
5. The document is now usable. US2/US3/US4 audits improve confidence but the MVP is functional.

### Incremental Delivery

1. **MVP** = Phases 1–3 complete (US1 ✓). Document is readable.
2. **Hardening 1** = Phase 4 complete (US2 ✓). Demos verified.
3. **Hardening 2** = Phase 5 complete (US3 ✓). CLAUDE.md coverage verified.
4. **Hardening 3** = Phase 6 complete (US4 ✓). DAG verified.
5. **Ready to commit** = Phase 7 complete (C-1…C-8 all pass).

### Single-Agent Strategy (current case)

One AI agent works through phases sequentially:
1. Phase 1 → Phase 2 (T002 and T003/T004 can be derived in parallel if the agent context allows; otherwise serial).
2. Phase 3: serialize the 8 milestone-block writes (T005–T012). Each block is independent content but shares the file.
3. Phases 4–6: audits in priority order (US2 before US3 before US4).
4. Phase 7: structural + citation audits, then date bump, then stage.

### Parallel-Agent Strategy (if multiple agents available)

After Phase 2 closes:
- Agent A: writes M01, M02, M03 blocks (T005–T007).
- Agent B: writes M04, M05 blocks (T008–T009) — these depend on M01–M03's content being agreed but the block-writing is independent.
- Agent C: writes M06, M07, M08 blocks (T010–T012).
- Merge serially into `MILESTONES.md` to avoid file conflicts.
- Then Phases 4/5/6 audits run by separate agents (only if audit-log writes are sequenced).

---

## Notes

- [P] tasks = different files OR read-only with no fix dependencies.
- [Story] label maps task to specific user story for traceability. T001–T004 (Setup + Foundational) and T017–T021 (Polish) have no story label per the format spec.
- The deliverable is a single markdown file (`MILESTONES.md` at repo root) plus supporting artifacts under `specs/001-milestone-roadmap/`.
- "Tests" in the conventional sense don't apply — conformance checks C-1…C-8 are the equivalent and live in the audit tasks (T013–T018).
- Do not commit `MILESTONES.md` until T021 reports clean. Committing is the maintainer's explicit action.
- If during T005–T012 a milestone's sizing axes turn out to exceed L, stop and split that milestone in place (e.g. M06 → M06a + M06b) per `specs/001-milestone-roadmap/research.md` R-007 and update the dependency graph (T004 output) before continuing.
- Avoid: vague task descriptions, missing file paths, milestone blocks that omit any of the 12 required fields, Authority citations that don't match CLAUDE.md verbatim.
