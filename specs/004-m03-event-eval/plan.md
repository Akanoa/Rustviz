# Implementation Plan: M03 — Event Model + Level 1 Evaluator

**Branch**: `004-m03-event-eval` | **Date**: 2026-05-21 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/004-m03-event-eval/spec.md`

## Summary

Add `src/event.rs` (the full `MemEvent` enum with every CLAUDE.md category as a variant — Frames, Stack slots, Heap, Borrows, Sync, Threads, Pedagogy) and `src/eval.rs` (an AST walker that consumes M02's resolved + typed AST and emits a `Vec<MemEvent>` for L1 programs). Public surface adds `evaluate(&Program, &Resolution, &TypeMap) -> Result<Vec<MemEvent>, ParseError>`. L1 evaluator emits the Frames + Stack-slots + Note subset; the other variants are defined for M06–M08 to fill in payloads additively. Runtime errors surface as `Note` events with `NoteKind::RuntimeError` and stop the stream; the `Vec` returned still contains events up to that point so M04 can replay them.

Authority chain: `MILESTONES.md` › M03 → `spec.md` (this feature) → this plan.

## Technical Context

**Language/Version**: Rust 2024 edition, same toolchain as M01/M02. No `Cargo.toml` changes other than registering the new `[[test]]` target `m03`.
**Primary Dependencies**: existing `indexmap` regular dep (used in M02), existing `insta` dev-dep. No new deps.
**Storage**: in-memory; the event stream is a `Vec<MemEvent>` accumulated as the evaluator walks the AST.
**Testing**: `cargo test --test m03` integration suite with `insta::assert_debug_snapshot!`, same pattern as M01/M02. Plus one in-source unit test in `src/event.rs` to verify `SlotMove` variant construction (FR-006 — L1 doesn't exercise the move path from real programs).
**Target Platform**: library crate for host; WASM portability preserved (no new deps, no platform-specific code).
**Project Type**: Rust library, single crate.
**Performance Goals**: not a goal; evaluating a typical L1 program (≤ 50 statements) completes in well under 50 ms — implicit, no benchmark.
**Constraints**: deterministic event stream (FR-010 / SC-005); stop-at-first-error (locked-in from M01); reuse M01's `ParseError` for static failures; runtime errors as `Note` events (research R-003); spans on every event (FR-007 / SC-002); recursion depth limit 100 (FR-011); ≤ ~1500 LOC across `event.rs` + `eval.rs` (SC-006); zero warnings (SC-007); M01 + M02 tests still pass (SC-008).
**Scale/Scope**: ~20 enum variants in `MemEvent` covering all CLAUDE.md event categories. L1 evaluator handles ~10 AST node forms. Estimated ~600–900 LOC total. AI agents under maintainer direction.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is still the unfilled speckit template. Same vacuous PASS as features 001–003.

**Post-design re-check**: still vacuous, still PASS.

## Project Structure

### Documentation (this feature)

```text
specs/004-m03-event-eval/
├── plan.md                 # This file
├── spec.md                 # Feature spec
├── research.md             # Phase 0: API + runtime-error + frame-leave + slot-id decisions
├── data-model.md           # Phase 1: MemEvent enum + all payload-type definitions
├── quickstart.md           # Phase 1: how to call evaluate, add tests, debug
├── contracts/
│   └── m03-api.md          # Phase 1: public evaluate() API + MemEvent stability rules
├── checklists/
│   └── requirements.md     # From /speckit-specify
└── tasks.md                # NOT created here — /speckit-tasks output
```

### Source Code (repository root)

Faithful to CLAUDE.md's "Planned code layout": `event.rs` as a flat file for the enum, `eval.rs` for the evaluator. Both estimated under 600 LOC; if either crosses that, split using the M01 `parse.rs` + `parse/` convention.

```text
src/
├── lib.rs                  # Re-exports updated to add event + eval surface.
├── parse.rs                # Unchanged from M01.
├── parse/                  # Unchanged from M01.
├── resolve.rs              # Unchanged from M02.
├── typeck.rs               # Unchanged from M02.
├── event.rs                # NEW — MemEvent enum + all payload types (SlotId, FrameId, Value, Pointee, HeapAddr, BorrowId, NoteKind).
└── eval.rs                 # NEW — Evaluator struct + evaluate() entry; depth-first AST walker.

tests/
├── m01.rs                  # Unchanged.
├── m02.rs                  # Unchanged.
├── m03.rs                  # NEW — integration suite (snapshot tests).
├── samples/
│   ├── m01_*.rs            # Unchanged.
│   ├── m02_*.rs            # Unchanged.
│   ├── m03_arithmetic.rs   # NEW
│   ├── m03_fn_call.rs      # NEW
│   ├── m03_if_then.rs      # NEW
│   ├── m03_if_else.rs      # NEW
│   ├── m03_shadow.rs       # NEW
│   ├── m03_nested_block.rs # NEW
│   ├── m03_div_by_zero.rs  # NEW (runtime error)
│   └── m03_short_circuit.rs # NEW
└── snapshots/
    └── m03_*.snap          # NEW — managed by insta.
```

`Cargo.toml` gains one new `[[test]]` entry:

```toml
[[test]]
name = "m03"
path = "tests/m03.rs"
```

**Structure Decision**: flat files for `event.rs` and `eval.rs`; tests under existing `tests/` directory keyed by milestone. No workspace change. No new production deps.

## Complexity Tracking

> No constitutional violations. Table omitted.
