# Implementation Plan: M03.2 — Scalar Lattice Expansion (integer + float types)

**Branch**: `008-m03-2-scalar-lattice` | **Date**: 2026-05-22 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/008-m03-2-scalar-lattice/spec.md`

## Summary

Extend M03's type lattice from `{I32, Bool, Unit}` to cover the full Rust integer family (12 variants) plus IEEE 754 floats (2 variants). Pick the **unified `Value` representation** (`Value::Int { kind: IntKind, bits: i128 }` + `Value::Float { kind: FloatKind, value: f64 }`) over per-type enum-variant explosion — one arithmetic dispatch per op instead of 12+, cleaner JSON wire format. Integer overflow halts the trace via the existing `Note { kind: RuntimeError }` mechanism; float `Inf` / `NaN` are valid Rust and surface via `Note { kind: Info }` emitted once per de-novo producing operation (no spam on propagation). The lexer learns a single new token (float literal: digits + `.` + digits); the parser gets a new AST node (`FloatLit(f64)`); typeck enforces type-annotation-driven assignment and overflow / cross-type / unsigned-negation rules. Snapshot churn for all M03 traces is expected (Value's Debug format changes); M01 / M02 / M04 / M05 are untouched.

Authority chain: `MILESTONES.md` › M03.2 → `spec.md` (this feature) → this plan.

## Technical Context

**Language/Version**: Rust 2024 edition (same toolchain as M01–M05). No new toolchain requirements.
**Primary Dependencies**: existing `indexmap`, `serde`, `serde_json`, `wasm-bindgen`, `js-sys`, `console_error_panic_hook`. **No new Rust deps**. No JS dep changes.
**Storage**: in-memory; no new files. M03 snapshot tests re-baselined (Value's Debug format changes); `web/traces/` remains obsolete (M05 already removed the trunk hook).
**Testing**: M01/M02 byte-identical (don't reference `Value`); M03 snapshots re-baselined (predictable diff per R-007 below); new `cargo test --lib typeck::tests` + `--lib eval::tests` cover the 14 new types' typeck + arithmetic; new `cargo test --lib pipeline::tests` covers end-to-end for at least 3 new types; manual M05 QA per the SC-008 procedure.
**Target Platform**: same as M01–M05 (host + `wasm32-unknown-unknown`).
**Project Type**: Rust library + companion M04/M05 UI; touches 5 existing modules (`parse/{lexer,token,ast,parser}`, `typeck`, `event`, `eval`, `ui`). No new modules.
**Performance Goals**: same as M03 — pipeline runs in well under 100 ms for L1 programs ≤ 50 lines. Adding 14 type variants doesn't change asymptotic behavior; arithmetic dispatch is O(1) per op via the unified `IntKind` / `FloatKind` match.
**Constraints**: M01/M02 snapshots byte-identical (SC-005); WASM bundle growth ≤ +5% vs M05 baseline 63,144 B gzipped (SC-007); zero warnings under `-D warnings` (SC-008); existing M03 evaluator semantics for `i32`/`bool`/`()` unchanged (only the Debug format of `Value` changes, which re-baselines M03 snapshots).
**Scale/Scope**: ~5 source files modified + 3 new sample files. Estimated ~500 LOC net code change (driven by the 12-variant `IntKind` arithmetic dispatch + lexer float-literal case + typeck range checks). Sizing: **M** per the rubric.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is still the unfilled speckit template. Same vacuous PASS as features 001–007.

**Post-design re-check**: still vacuous, still PASS.

## Project Structure

### Documentation (this feature)

```text
specs/008-m03-2-scalar-lattice/
├── plan.md                 # This file
├── spec.md                 # Feature spec
├── research.md             # Phase 0: ~14 design decisions
├── data-model.md           # Phase 1: IntKind / FloatKind / new Value / new Ty variants
├── quickstart.md           # Phase 1: dev workflow + manual QA procedure
├── contracts/
│   └── m03-2-protocol-delta.md   # Phase 1: M03 contract delta (Ty + Value additive growth)
├── checklists/
│   └── requirements.md     # From /speckit-specify
└── tasks.md                # NOT created here — /speckit-tasks output
```

### Source Code (repository root) — files M03.2 touches

```text
src/
├── parse/
│   ├── token.rs            # MODIFIED — new `Float(f64)` token variant; rename existing `Int(i64)` if shape changes.
│   ├── lexer.rs            # MODIFIED — recognize float literals (digits + `.` + digits). Two-char lookahead at `.`.
│   ├── ast.rs              # MODIFIED — new `FloatLit(f64)` AST node alongside existing `IntLit(i64)`.
│   └── parser.rs           # MODIFIED — parse the Float token into FloatLit.
├── typeck.rs               # MODIFIED — `Ty` enum gains 14 variants (12 int + 2 float); `IntKind` / `FloatKind`
│                           #            helper enums with `min_value`, `max_value`, `contains`, `is_signed`,
│                           #            `name` methods; literal range checks; cross-type arithmetic rules;
│                           #            unsigned-negation rejection.
├── event.rs                # MODIFIED — `Value` enum restructured: `Int { kind: IntKind, bits: i128 }` +
│                           #            `Float { kind: FloatKind, value: f64 }` + existing `Bool` + `Unit`.
│                           #            Debug format changes (snapshot drift expected).
├── eval.rs                 # MODIFIED — arithmetic ops dispatched over `IntKind` / `FloatKind`; integer
│                           #            overflow detection (i128 checked_op + IntKind range gate);
│                           #            float Inf/NaN detection + Info-note emission (only on de-novo
│                           #            creation, not propagation).
├── ui.rs                   # MODIFIED — `render_value` shows the type-tag suffix (`5_u8`, `2.5_f64`,
│                           #            `NaN_f64`, `+Inf_f32`).
├── lib.rs                  # MODIFIED — re-export `IntKind`, `FloatKind`.
└── pipeline.rs             # Unchanged (the M05 pipeline runner already returns Result<Vec<MemEvent>, CompileError>).

tests/
├── m01.rs, m02.rs          # Unchanged. Snapshots byte-identical (don't reference Value).
├── m03.rs                  # Unchanged code; snapshots ALL re-baselined (Value's Debug format changes).
├── snapshots/
│   └── emits_*.snap        # MODIFIED — re-baselined via `INSTA_UPDATE=always cargo test --test m03`.
│                           #            Predictable diff: every `Int(N)` becomes `Int { kind: I32, bits: N }`.
└── samples/
    ├── m03_*.rs, m05_*.rs  # Unchanged.
    └── m03_2_*.rs          # NEW (3 files): basic_u8, u8_overflow, float_nan.

web/
├── samples/                # MODIFIED — add 3 `m03_2_*.rs` (mirrors `tests/samples/m03_2_*.rs`).
└── index.html              # MODIFIED — dropdown gets 3 new entries for the M03.2 samples.

# M03's contract document gets the standard amendment:
specs/004-m03-event-eval/contracts/m03-api.md   # MODIFIED — relax Ty + Value closed rule for revision
                                                #   milestones (same precedent M03.1 set for MemEvent).
```

**Structure Decision**: no new modules. The 14 new type variants extend existing enums; arithmetic dispatch lives in existing `eval.rs`; the lexer's new token type stays in `parse/token.rs`. The single largest file change is `typeck.rs` (new helper enums + range-check methods + arithmetic typeck rules), estimated ~150 LOC.

## Complexity Tracking

> No constitutional violations. Table omitted.
