# Implementation Plan: M06 — Level 2: References and Borrows

**Branch**: `009-m06-borrows` | **Date**: 2026-05-22 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/009-m06-borrows/spec.md`

## Summary

Add Rust's borrow semantics — `&T`, `&mut T`, scope-level lifetimes, aliasing rules — to the lattice + event protocol + UI. Lexer accepts `&` and `&mut`; parser produces borrow expressions and reference types; typeck runs a borrow tracker enforcing the one-mut-xor-many-shared rule statically; eval emits `BorrowShared`/`BorrowMut`/`BorrowEnd` events with proper `BorrowId` + `Pointee::Slot` payloads; M04/M05 grow a SVG arrow overlay drawing blue (shared) / red (mutable) arrows between slot cards. The three borrow `MemEvent` variants already exist (declared in M03 with empty payloads reserved); M06 fills them. `Ty` gains a non-Copy `Ref { inner: Box<Ty>, mutable: bool }` variant — a cascade refactor making `Ty` methods take `&self`. `Value` gains `Ref { borrow_id, target_slot, mutable }`.

Authority chain: `MILESTONES.md` › M06 → `spec.md` (this feature) → this plan.

## Technical Context

**Language/Version**: Rust 2024 edition (same toolchain as M01–M05). No new toolchain requirements.
**Primary Dependencies**: existing `indexmap`, `serde`, `serde_json`, `wasm-bindgen`, `js-sys`, `console_error_panic_hook`. **No new Rust deps**. No JS dep changes (existing `@codemirror/*` import map sufficient).
**Storage**: in-memory; no new files. M03 snapshot tests should stay byte-identical (existing samples don't construct `Value::Ref` or `Ty::Ref`). M02 may re-baseline if any TypeMap snapshot Debug output shifts (unlikely — additive enum variants don't change existing variant formats).
**Testing**: existing `cargo test --test m01 / m02 / m03` should pass; new `cargo test --lib typeck::borrow_tracker::tests` for the aliasing-rule logic; new `cargo test --lib pipeline::tests` covering shared, mutable, aliasing-error, scoped-borrow cases (≥ 6 new tests); manual M05+M06 QA per the SC-008 procedure.
**Target Platform**: same as M01–M05 (host + `wasm32-unknown-unknown`).
**Project Type**: Rust library + companion M04/M05 UI; touches 8 existing modules + adds a new SVG overlay in the web layer.
**Performance Goals**: same as M03 — pipeline runs in well under 100 ms for L2 programs ≤ 50 lines. Borrow tracker is O(borrows per binding × scopes) — small constant in practice. SVG overlay re-renders on every cursor step; should stay sub-frame (16 ms).
**Constraints**: M01 byte-identical; M02/M03 may re-baseline if minor Debug format ripples (unlikely); WASM bundle ≤ +50% vs M03.2 baseline (84 KB → ≤ 126 KB gzipped) per SC-007; zero warnings under `-D warnings` (SC-008); existing M03.2/M05 features preserved.
**Scale/Scope**: ~8 source files modified + 1 new module (`typeck::borrow_tracker` or inline) + 4 new sample pairs + significant JS additions (SVG overlay) + CSS. Estimated ~800 LOC net change. Sizing: **L** per the rubric.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is still the unfilled speckit template. Same vacuous PASS as features 001–008.

**Post-design re-check**: still vacuous, still PASS.

## Project Structure

### Documentation (this feature)

```text
specs/009-m06-borrows/
├── plan.md                 # This file
├── spec.md                 # Feature spec
├── research.md             # Phase 0: ~17 design decisions
├── data-model.md           # Phase 1: Ty::Ref, Value::Ref, ActiveBorrow, BorrowView, BorrowTracker
├── quickstart.md           # Phase 1: dev workflow + manual QA procedure
├── contracts/
│   └── m06-protocol-delta.md  # Phase 1: M03 protocol delta (Ty + Value additive growth, borrow event payloads)
├── checklists/
│   └── requirements.md     # From /speckit-specify
└── tasks.md                # NOT created here — /speckit-tasks output
```

### Source Code (repository root) — files M06 touches

```text
src/
├── parse/
│   ├── token.rs            # MODIFIED — new TokenKind::Amp and AmpMut variants.
│   ├── lexer.rs            # MODIFIED — lex `&` and `&mut` (replaces M01's rejection).
│   │                       #            Three-char lookahead for `&mut` (no whitespace allowed
│   │                       #            between `&` and `mut`).
│   ├── ast.rs              # MODIFIED — Expr::Borrow { inner, mutable, span }; Type::Ref { inner, mutable, span }.
│   └── parser.rs           # MODIFIED — parse_atom recognizes Amp/AmpMut as prefix borrow ops;
│                           #            parse_type recognizes Amp/AmpMut as reference type prefix.
├── typeck.rs               # MODIFIED — `Ty::Ref { inner: Box<Ty>, mutable: bool }`. Drops `Copy`
│                           #            derive on Ty (Box<Ty> is not Copy); refactors methods to
│                           #            take `&self`. Adds `BorrowTracker` struct + helpers in a
│                           #            new inner module. Adds place-expression check on borrow
│                           #            expressions. Adds typeck for `Expr::Borrow` (produces
│                           #            Ty::Ref) and `Type::Ref` (produces Ty::Ref).
├── event.rs                # MODIFIED — `Value::Ref { borrow_id: BorrowId, target_slot: SlotId, mutable: bool }`.
│                           #            (BorrowShared/BorrowMut/BorrowEnd variants already exist
│                           #            since M03; payloads were already typed with BorrowId +
│                           #            Pointee — no enum changes needed there.)
├── eval.rs                 # MODIFIED — evaluate Expr::Borrow: allocate BorrowId, emit BorrowShared
│                           #            or BorrowMut, construct Value::Ref. On scope exit, emit
│                           #            BorrowEnd for each borrow created in that scope. Scope
│                           #            tracking grows a `borrows_in_scope: Vec<BorrowId>` field.
├── ui.rs                   # MODIFIED — World tracks active borrows (push on BorrowShared/Mut,
│                           #            remove on BorrowEnd). StateSnapshot grows a
│                           #            `borrows: Vec<BorrowView>` field. New `BorrowView` view
│                           #            type with serde derives.
├── lib.rs                  # MODIFIED — re-export new types: `BorrowView`, optionally `BorrowKind`.
└── (other files unchanged)

tests/
├── m01.rs                  # Unchanged. Snapshots byte-identical (M01 doesn't reference Ty or Value's structure beyond AST tokens; new Amp/AmpMut tokens only appear in M06+ samples).
├── m02.rs, m03.rs          # Unchanged code; snapshots stay byte-identical (existing samples don't construct ref-Ty or ref-Value).
└── samples/
    ├── (existing samples)  # Unchanged.
    └── m06_*.rs            # NEW (4 files): shared_borrow, mut_borrow, aliasing_error, scoped_borrow.

web/
├── samples/                # MODIFIED — add 4 m06_*.rs mirrors.
├── index.html              # MODIFIED — `<svg id="arrow-overlay">` element in `<main>`; dropdown
│                           #            grows 4 M06 entries.
├── index.js                # MODIFIED — `renderArrows(state.borrows)` reads slot card positions
│                           #            via `getBoundingClientRect`, draws SVG paths. Re-renders
│                           #            on every state update + window resize.
├── style.css               # MODIFIED — `.arrow-shared` (blue) and `.arrow-mut` (red) classes;
│                           #            position-related styling for the SVG layer.
└── Trunk.toml              # Unchanged.

# M03's contract documents the third invocation of the closed-enum-with-revisions rule:
specs/004-m03-event-eval/contracts/m03-api.md   # MODIFIED — note M06 as the third invocation
                                                #   (after M03.1, M03.2). M06 adds new variants
                                                #   to Ty and Value (additive). No restructure.
```

**Structure Decision**: no new top-level module. The `BorrowTracker` lives inline in `src/typeck.rs` (or a private `mod borrow_tracker` within typeck.rs). The largest single file change is `src/typeck.rs` (Ty restructure → !Copy + borrow tracker + place-expression check + Expr::Borrow typeck), estimated ~250 LOC growth.

## Complexity Tracking

> No constitutional violations. Table omitted.

### Notable non-trivial complexity

- **`Ty` drops `Copy`**: foundational cascade affecting every site that takes `Ty` by value. Mechanical refactor — change method signatures from `(self)` to `(&self)`. Estimated ~50 sites updated.
- **Borrow tracker scope discipline**: per-scope active-borrow map maintained alongside the existing local-slot scope stack. Must drop borrows precisely at scope exit (matching when the evaluator emits BorrowEnd). Off-by-one risks; covered by US4 acceptance tests.
- **SVG overlay positioning**: arrows must update on every cursor step + window resize. DOM bounding-box queries on slot cards. Visual flicker if poorly synchronized; positioning logic settled in research R-005.
