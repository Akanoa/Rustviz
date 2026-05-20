# Implementation Plan: M02 — Name Resolution + Lightweight Typeck

**Branch**: `003-m02-resolve-typeck` | **Date**: 2026-05-20 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/003-m02-resolve-typeck/spec.md`

## Summary

Implement two new analysis passes — `src/resolve.rs` (Ident → BindingId, scope checks, "use of undeclared variable" errors) and `src/typeck.rs` (annotation validation, type propagation, type-mismatch errors) — over M01's AST. Public surface adds `resolve(&Program) -> Result<Resolution, ParseError>` and `typeck(&Program, &Resolution) -> Result<TypeMap, ParseError>`. The library now exposes a complete L1 frontend: parse → resolve → typeck → (M03 evaluator next milestone).

Authority chain: `MILESTONES.md` › M02 → `spec.md` (this feature) → this plan. No scope decisions in this plan.

## Technical Context

**Language/Version**: Rust 2024 edition, same toolchain as M01 (1.85+). `Cargo.toml` gains one new regular dep (`indexmap` — see research.md R-002) and one new `[[test]]` target (`m02`).
**Primary Dependencies**: `indexmap` (new, regular dep — for tree-walk-order side tables); existing `insta` dev-dep.
**Storage**: in-memory; metadata stored in `IndexMap<Span, ...>` side tables for determinism + tree-walk iteration order.
**Testing**: `cargo test --test m02` integration suite with `insta::assert_debug_snapshot!`, same pattern as M01.
**Target Platform**: library crate for host; WASM portability preserved (no new deps).
**Project Type**: Rust library, single crate (no workspace change).
**Performance Goals**: not a goal; resolving + typechecking a 1 KB L1 program completes in well under 50 ms — implicit, no benchmark required.
**Constraints**: stop-at-first-error (locked-in from M01); reuse M01's `ParseError` type (no new error hierarchy); spans as the side-table key via `IndexMap` (no AST mutation, tree-walk iteration order); deterministic snapshots (SC-005); ≤ ~1500 LOC across `src/resolve.rs` + `src/typeck.rs` (SC-006); zero warnings under `-D warnings` (SC-007); M01 tests still pass (SC-008).
**Scale/Scope**: ~6 binding kinds (Fn, Let-immutable, Let-mut, Param) collapse to 3 conceptual kinds; 3 value types (I32, Bool, Unit); typing rules for ~14 operators + control flow. Estimated ~700–1000 LOC total. AI agents under maintainer direction.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is still the unfilled speckit template. Same vacuous PASS as features 001 and 002.

**Post-design re-check**: still vacuous, still PASS.

## Project Structure

### Documentation (this feature)

```text
specs/003-m02-resolve-typeck/
├── plan.md                 # This file
├── spec.md                 # Feature spec
├── research.md             # Phase 0: API shape + algorithm + metadata-keying decisions
├── data-model.md           # Phase 1: Resolution, BindingId, Ty, TypeMap entities
├── quickstart.md           # Phase 1: how to call resolve+typeck, add tests, debug
├── contracts/
│   └── m02-api.md          # Phase 1: public resolve()/typeck() API + stability rules
├── checklists/
│   └── requirements.md     # From /speckit-specify
└── tasks.md                # NOT created here — /speckit-tasks output
```

### Source Code (repository root)

Following M01's flat-file convention. CLAUDE.md's "Planned code layout" sketches `resolve/` and `typeck/` as directories; for M02's expected size (~700–1000 LOC), flat files are sufficient. If either grows past ~600 LOC, split into a `resolve.rs` + `resolve/` (or `typeck.rs` + `typeck/`) module on the M01 pattern.

```text
src/
├── lib.rs                  # Re-exports updated to add resolve + typeck surface.
├── parse.rs                # Unchanged from M01.
├── parse/                  # Unchanged from M01.
│   └── ...
├── resolve.rs              # NEW — Resolution, BindingId, BindingDecl, scope walker, resolve().
└── typeck.rs               # NEW — Ty, FnSig, TypeMap, type inference + validation, typeck().

tests/
├── m01.rs                  # Unchanged.
├── m02.rs                  # NEW — integration suite (snapshot tests).
├── samples/
│   ├── m01_*.rs            # Unchanged.
│   ├── m02_shadow.rs       # NEW
│   ├── m02_fn_params.rs    # NEW
│   ├── m02_if_expr.rs      # NEW
│   ├── m02_undeclared.rs   # NEW
│   └── m02_type_mismatch.rs # NEW (plus any additional cases the spec demands)
└── snapshots/
    └── m02_*.snap          # NEW — managed by insta.
```

`Cargo.toml` gains one new `[[test]]` entry:

```toml
[[test]]
name = "m02"
path = "tests/m02.rs"
```

**Structure Decision**: flat files for `resolve.rs` and `typeck.rs`; tests under the existing `tests/` directory keyed by milestone (`m02.rs` driver, `m02_*.rs` samples, `m02_*.snap` snapshots). No workspace change.

## Complexity Tracking

> No constitutional violations. Table omitted.
