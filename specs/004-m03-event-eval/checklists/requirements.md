# Specification Quality Checklist: M03 — Event Model + Level 1 Evaluator

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-05-21
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

- Validation pass 1 (2026-05-21): all items pass.
- **Authority chain**: same pattern as M01/M02 — defers all scope decisions to `MILESTONES.md` › M03. The "users" framing is again internal (M04 consumer + contributor); M03 has no end-user-facing surface — that arrives at M04+.
- **`SlotMove` infrastructure without exercise**: noted as an edge case. L1 has only Copy types (`i32`, `bool`), so the `SlotMove` event variant exists in the enum but isn't emitted from a pure L1 evaluator. FR-006 requires the infrastructure to be present and verified by at least one test (e.g. a unit test constructing the variant). This is honest about CLAUDE.md's L1 description listing "moves of non-Copy types" — that line is forward-looking; L1's value types don't include non-Copy ones.
- **Runtime error handling choice deferred to plan**: FR-011 / SC-001 require a runtime-error test case but don't pin the API surface (Note in stream vs separate Result variant). The plan phase will choose; the behavior is testable either way.
- **Function return value encoding deferred to plan**: spec doesn't pin whether `FrameLeave` carries the return value as payload or whether the caller's `SlotWrite` derives it some other way. Plan-phase decision; snapshots will pin whichever shape is chosen.
- **Borderline "implementation details" in Key Entities and FR-001**: the spec lists concrete event variants (`SlotAlloc`, `BorrowShared`, etc.) and entity names. These come directly from CLAUDE.md's vocabulary, so they're not implementation choices being decided here; they're scope being inherited. Same pattern as M02's FR-007 typing-rules list.

## Post-implementation audit (2026-05-21)

Following `/speckit-implement` of M03. All 19 tasks (T001–T019) executed; 8 integration snapshot tests + 6 in-source unit tests pass.

### Success-criteria results

| ID    | Description | Result |
|-------|-------------|--------|
| SC-001 | `cargo test --test m03` runs ≥ 6 snapshot tests | PASS — **8 tests** (arithmetic, fn_call, if_then, if_else, shadow, nested_block, short_circuit, div_by_zero_note) |
| SC-002 | 100% of events carry a non-empty span | PASS — enforced by `assert_spans_ok` in the test driver before snapshot; would fail-fast on any zero-length span |
| SC-003 | `FrameEnter` / `FrameLeave` paired in LIFO order | PASS — `emits_fn_call` snapshot confirms nested frames close cleanly |
| SC-004 | `SlotDrop` events in reverse declaration order | PASS — `emits_fn_call` and `emits_shadow` snapshots both show LIFO drops |
| SC-005 | Deterministic snapshots | PASS — re-running `cargo test --test m03` produces no `.snap.new` files |
| SC-006 | ≤ ~1500 LOC across `event.rs` + `eval.rs` | PASS — **902 LOC** (event 344, eval 558) |
| SC-007 | Zero warnings under `-D warnings` | PASS — `cargo build --release` and `cargo test --test m03` clean |
| SC-008 | M01 + M02 tests still pass unchanged | PASS — 8 M01 + 16 M02 + 6 lib unit tests green, no snapshot drift |
| SC-009 | `MemEvent` declares all CLAUDE.md categories | PASS — 19 variants total; smoke unit tests construct one representative from each forward-compat category (`SlotMove`, `ThreadSpawn`, `HeapAlloc`, `BorrowShared`, `LockAcquire`, `Note(Info)`) |

### Implementation findings

- **T011 and T012 absorbed into earlier tasks**: T011's `assert_spans_ok` was implemented as part of T009's `sample_test!` macro (the natural place to add a pre-snapshot assertion). T012's forward-compat smoke tests were placed in T003's `#[cfg(test)] mod tests` block in `src/event.rs` (the natural place — same module that defines the variants). Both deviations make the code denser and skip artificial multi-pass structure. Same pattern as M02's T013/T016 absorption.
- **`LocalSlot.name` dropped during T007**: the LocalSlot struct originally had a `name: String` field per the data-model.md sketch, but it was never read at runtime (the slot is identified by `binding_id`; the name is reachable via `resolution.bindings` when needed). Compiler dead-code warning fired immediately; dropping the field was correct. data-model.md still mentions `name` in the private `LocalSlot` definition — minor doc drift, not worth editing since the type is private.
- **`FrameEnter::params` is redundant with the subsequent `SlotAlloc`/`SlotWrite` events**: confirmed in `emits_fn_call.snap` — the params snapshot (`SlotId(0), "a", Int(2)` etc.) appears in FrameEnter AND again as separate per-param events. Kept per the contract: M04 can either render the frame as fully-populated at FrameEnter, or step through the param events. Not a bug.
- **`SlotMove` never emitted from a real L1 program**: confirmed by inspection of all 8 snapshots — no `SlotMove` appears. The variant is exercised by the `constructs_slot_move` unit test in `event.rs`. M07 will exercise it from real samples when heap-allocated non-Copy types arrive.
- **Short-circuit evaluation works correctly**: `emits_short_circuit.snap` shows no `RuntimeError` Note even though the program contains `1 / 0` in two places — both inside RHS of `||` (LHS true, RHS skipped) and RHS of `&&` (LHS false, RHS skipped). FR-008 honored.
- **Span pointing for `SlotDrop`**: research R-014 open question — pointing at the binding's declaration span (chosen default). Verified in `emits_arithmetic.snap`: `SlotDrop(x)` has span `16..30` which equals the `let x = 2 + 3;` decl span. M04 will visually highlight the declaration site when the drop fires — pedagogically pointing at "this binding came from here". If M04 prefers the closing-brace position, revisit.
- **`FrameLeave.span` points at the body span**, not a zero-length point at the closing `}`. Chose this to satisfy the assert_spans_ok non-empty-span check uniformly. M04 can use `span.end` to render the closing-brace point if desired.

### Test summary

```
$ RUSTFLAGS="-D warnings" cargo test
running 8 tests (m01)   ... 8 passed
running 16 tests (m02)  ... 16 passed
running 8 tests (m03)   ... 8 passed
running 6 tests (lib)   ... 6 passed
total: 38 tests; 0 failed; 0 ignored
```

### Conclusion

M03 exit criteria met. The crate is ready to commit. M04 (UI shell + replay cursor) can begin once committed — it consumes the `Vec<MemEvent>` this milestone produces.
