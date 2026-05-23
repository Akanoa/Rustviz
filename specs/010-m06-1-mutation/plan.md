# Implementation Plan: M06.1 — Mutation: Assignment + Deref Read/Write

**Branch**: `010-m06-1-mutation` | **Date**: 2026-05-22 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/010-m06-1-mutation/spec.md`

## Summary

Add three connected mutation forms: plain assignment to `mut` bindings (`x = v;`), deref-as-rvalue (`let y = *r;`), and deref-as-lvalue (`*r = v;`). Closes M06's pedagogical gap (`&mut` was observation-only) AND M03's cosmetic-`mut` gap (no statement re-assigned bindings). AST gains `Expr::Deref` and `Stmt::Assign`. Typeck enforces place-expression restriction on lhs (Ident or Deref(Ident)), mutability rules, and integrates with the existing M06 borrow tracker. Eval reads target slot values for deref-as-rvalue and emits an existing `SlotWrite` event for both assignment forms — no new event variant, no Ty/Value changes, no JS/CSS work. **Visualization is "free"**: the stacks panel already animates `SlotWrite` value changes; the red arrow from M06 persists through the mutation.

Authority chain: `MILESTONES.md` › M06.1 → `spec.md` (this feature) → this plan.

## Technical Context

**Language/Version**: Rust 2024 edition (same toolchain as M01–M06). No new toolchain requirements.
**Primary Dependencies**: existing `indexmap`, `serde`, `serde_json`, `wasm-bindgen`, `js-sys`, `console_error_panic_hook`. **No new Rust deps**. **No JS deps changes**. **No `Cargo.toml` changes** (existing pipeline-test infrastructure handles M06.1's tests; no new `[[test]]` target).
**Storage**: in-memory; no new files. M01/M02/M03 snapshot tests should stay byte-identical (existing samples don't use assignment or deref).
**Testing**: existing `cargo test --test m01 / m02 / m03` should pass; new `cargo test --lib pipeline::tests` covering direct assign, immutable-binding rejected, deref-read, deref-write, deref-on-shared rejected, deref-on-non-reference rejected, borrowed-binding-assignment rejected (≥ 7 new tests); manual M05/M06 QA per the SC-008 procedure.
**Target Platform**: same as M01–M06 (host + `wasm32-unknown-unknown`).
**Project Type**: Rust library + companion M04/M05/M06 UI; touches 4 existing source modules. No JS work.
**Performance Goals**: same as M03 — pipeline runs in well under 100 ms for L2 programs ≤ 50 lines. M06.1 adds two AST variants and a few typeck/eval cases; negligible perf impact.
**Constraints**: M01/M02/M03 byte-identical; WASM bundle ≤ +20% vs M06 baseline (87,354 B → ≤ 104,825 B) per SC-007; zero warnings under `-D warnings` (SC-008); existing M06 features (SVG arrows, aliasing rules) preserved.
**Scale/Scope**: 4 source files modified (parse/ast.rs, parse/parser.rs, typeck.rs, eval.rs, resolve.rs for the traversal) + 3 sample pairs + ~7 unit tests. **Estimated ~250 LOC net change**. Sizing: **M** per the rubric.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is still the unfilled speckit template. Same vacuous PASS as features 001–009.

**Post-design re-check**: still vacuous, still PASS.

## Project Structure

### Documentation (this feature)

```text
specs/010-m06-1-mutation/
├── plan.md                 # This file
├── spec.md                 # Feature spec
├── research.md             # Phase 0: ~16 design decisions
├── data-model.md           # Phase 1: Expr::Deref, Stmt::Assign, place-expression set extension, SlotWrite reuse
├── quickstart.md           # Phase 1: dev workflow + manual QA procedure
└── checklists/
    └── requirements.md     # From /speckit-specify
```

**No `contracts/` directory** — M06.1 has no protocol-side changes. M03's `m03-api.md` doesn't need amendment (per R-015 in research.md): the `SlotWrite` variant's payload and semantics already accommodate re-assignment writes; only the SOURCE positions where it's emitted from change. No SHAPE change, no new variant, no restructure.

### Source Code (repository root) — files M06.1 touches

```text
src/
├── parse/
│   ├── ast.rs              # MODIFIED — add `Expr::Deref { inner, span }` and `Stmt::Assign { lhs, rhs, span }`. Update `Expr::span()` for the new variant.
│   ├── parser.rs           # MODIFIED — parse prefix `*` in `parse_atom` (same precedence as `&`/`-`/`!`). Parse assignment statement: at block level, if the parsed expression is followed by `=`, treat as an assignment statement.
│   ├── token.rs            # Unchanged — `*` and `=` tokens already exist (Star, Eq).
│   └── lexer.rs            # Unchanged.
├── resolve.rs              # MODIFIED — traverse `Expr::Deref` (resolve `inner`) and `Stmt::Assign` (resolve both `lhs` and `rhs`).
├── typeck.rs               # MODIFIED — typecheck `Expr::Deref(inner)`: inner must be `Ty::Ref { inner: T, .. }`, deref's type is T. Typecheck `Stmt::Assign`: validate lhs is a place expression (Ident or Deref(Ident)); mutability check (let mut for Ident; Ref{mutable:true} for Deref); borrow-tracker check (only for direct Ident assign — see R-008); rhs type matches lhs type (with M03.2 coercion).
├── eval.rs                 # MODIFIED — evaluate `Expr::Deref(inner)` as rvalue: resolve `inner` to a `Value::Ref`, read the target slot's current value via new `lookup_slot_value` helper, return it. Evaluate `Stmt::Assign { lhs, rhs }`: eval rhs, then dispatch on lhs shape:
│                           #   - `Expr::Ident(x)`: find x's slot, emit `SlotWrite { slot_id: x.slot, value: rhs_v, span }`. Update in-memory LocalSlot value via new `update_slot_value` helper.
│                           #   - `Expr::Deref(Expr::Ident(r))`: read r's Value::Ref, get target_slot, emit `SlotWrite { slot_id: target_slot, value: rhs_v, span }`. Update in-memory target slot via same helper.
└── (other files unchanged — event.rs, ui.rs, all JS/HTML/CSS untouched)

tests/
├── m01.rs / m02.rs / m03.rs  # Unchanged. Snapshots stay byte-identical (existing samples don't use assign or deref).
└── samples/
    ├── (existing samples)    # Unchanged.
    └── m06_1_*.rs            # NEW (3 files): assign_basic, deref_read, deref_write.

web/
├── samples/                # MODIFIED — add 3 m06_1_*.rs mirrors.
├── index.html              # MODIFIED — dropdown grows 3 M06.1 entries.
├── index.js / style.css    # **UNCHANGED** — visualization is "free" via existing SlotWrite animation.
└── Trunk.toml              # Unchanged.
```

**Structure Decision**: M06.1 is fundamentally a language-layer extension — AST + typeck + eval. **Zero web-side work** except for sample/dropdown registration. This is the smallest milestone since M03.1 (lines-of-code-wise).

## Complexity Tracking

> No constitutional violations. Table omitted.

### Notable non-trivial complexity

- **Assignment as a statement requires parser care**: after parsing an expression at statement level, peek for `=` to decide whether to wrap in `Stmt::Assign` or `Stmt::Expr`. The existing flow already distinguishes "expression statement with trailing `;`" vs "tail expression" — adding "expression followed by `=` then another expression then `;`" is one new branch. Care needed not to break existing `let x = ...` parsing (which already consumes `=` in `parse_let_stmt`).
- **`Expr::Deref` evaluation as rvalue requires looking up the target slot's CURRENT value**: not just at construction time. The Value::Ref carries `target_slot: SlotId`; eval walks the call stack to find the LocalSlot with that id and returns its current value. New helper `lookup_slot_value(SlotId) -> Option<Value>` complementing the existing `lookup_local_slot`.
- **`*r = v` requires updating the target slot's stored value AND emitting SlotWrite**: stack walk to update the LocalSlot's `value` field. Without this, subsequent `let y = *r;` after `*r = 7;` would read the OLD value. Both effects happen in one assignment evaluation.
- **Borrow tracker integration for direct assignment**: `let mut x = 5; let r = &x; x = 7;` must reject because `x` is borrowed by `r`. The M06 tracker has `active: IndexMap<BindingId, Vec<ActiveBorrow>>`. The new check: when typechecking `Stmt::Assign` with `Expr::Ident(x)` lhs, look up `tracker.active.get(&x.binding_id)` — if non-empty, reject. NO check for `*r = v` (per R-008 — the active `&mut` is what permits the write; nothing else can take a conflicting borrow during its lifetime).
- **Reassigning a ref-holding binding** (`let mut r = &x; r = &y;`): in-scope syntactically but the M06 borrow tracker doesn't model it correctly — old `&x` borrow doesn't get a BorrowEnd until scope close. Visual edge case documented in quickstart.md; left as a known limitation for M06.1.
