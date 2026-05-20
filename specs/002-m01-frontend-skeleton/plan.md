# Implementation Plan: M01 — Frontend Skeleton (lexer + parser)

**Branch**: `002-m01-frontend-skeleton` | **Date**: 2026-05-20 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/002-m01-frontend-skeleton/spec.md`

## Summary

Scaffold the rustviz Cargo library crate and implement the parser front-end (`src/parse/`) for the Level 1 syntax subset documented in CLAUDE.md. Deliver a `parse(file_id, &SourceMap) -> Result<Program, ParseError>` public API plus `cargo test --test m01` integration suite using `insta` snapshot tests. All AST nodes carry byte-offset spans; lexer rejects `&` with an L1-specific message; parser stops at the first error.

Authority chain: `MILESTONES.md` › M01 → `spec.md` (this feature) → this plan. No scope decisions live in this plan — only how-to-build decisions.

## Technical Context

**Language/Version**: Rust 2024 edition (latest stable), MSRV pinned to current stable at scaffold time (recorded in `Cargo.toml`)
**Primary Dependencies**: `insta` (snapshot testing). No parser framework (CLAUDE.md locked-in decision). No `thiserror`/`anyhow` for M01 — error type is a single hand-rolled struct.
**Storage**: N/A (in-memory only; SourceMap holds source text)
**Testing**: `cargo test --test m01` runs the integration suite. Snapshots under `tests/snapshots/` reviewed with `cargo insta review`.
**Target Platform**: Library crate built for host (Linux x86_64 today; WASM target enabled later — M04+). M01 itself does not target WASM.
**Project Type**: single-crate Rust library (workspace deferred — see research R-010)
**Performance Goals**: not a goal at M01 (correctness first); parsing a 1 KB L1 program completes in well under 50 ms on host — implicit, no benchmark required.
**Constraints**: stop-at-first-parse-error (locked-in); reject `&` at the lexer (locked-in); spans = byte offset + `FileId` (locked-in); ≥ 5 snapshot tests covering happy + precedence + 2 errors + empty (SC-001); deterministic snapshots (SC-004); ≤ ~2000 LOC under `src/parse/` (SC-005); zero warnings (SC-006).
**Scale/Scope**: L1 syntax surface (primitives, let/let mut, fn, scopes, blocks-as-expr, if-expr, operators). Estimated ~6–8 token kinds + ~8 AST node variants. Implementation by AI agents under maintainer direction.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is the unfilled speckit template (placeholders, no ratified principles). No concrete gates.

**Decision**: PASS by vacuity (same as feature 001).

**Post-design re-check**: still vacuous, still PASS.

## Project Structure

### Documentation (this feature)

```text
specs/002-m01-frontend-skeleton/
├── plan.md                 # This file
├── spec.md                 # Feature spec
├── research.md             # Phase 0: scaffold + library + AST + parser decisions
├── data-model.md           # Phase 1: Token/Span/SourceMap/AST entity definitions
├── quickstart.md           # Phase 1: how to add tests, run, debug
├── contracts/
│   └── parse-api.md        # Phase 1: public parse() API + AST shape contract
├── checklists/
│   └── requirements.md     # From /speckit-specify
└── tasks.md                # NOT created here — /speckit-tasks output
```

### Source Code (repository root)

Faithful to `CLAUDE.md › Planned code layout` with the Rust 2018+ `parse.rs` + `parse/` module convention (avoids `mod.rs` boilerplate; per-file responsibility matches CLAUDE.md sketch).

```text
Cargo.toml                  # Library crate; rust-version + edition pinned; insta dev-dep
src/
├── lib.rs                  # Crate root; re-exports the public parse API
└── parse.rs                # Module root; declares submodules; defines parse() entry point
src/parse/
├── span.rs                 # Span, FileId, SourceMap, SourceFile
├── token.rs                # Token, TokenKind
├── lexer.rs                # Lexer state machine; &str + FileId → Vec<Token>
├── ast.rs                  # AST types (Program, Item, Stmt, Expr, ...) — all carry Span
├── parser.rs               # Pratt for expressions + recursive descent for items/stmts/patterns
└── error.rs                # ParseError, LexError (or unified Diagnostic)

tests/
├── m01.rs                  # Integration test: cargo test --test m01
└── samples/                # Input .rs programs used by the integration test
    ├── m01_arithmetic.rs
    ├── m01_precedence.rs
    ├── m01_full_l1.rs
    ├── m01_unexpected_token.rs
    ├── m01_reject_ampersand.rs
    └── m01_empty.rs
tests/snapshots/            # insta snapshot files (auto-managed by cargo insta)
```

**Structure Decision**: single library crate, no workspace yet (research R-010). Faithful to CLAUDE.md's `parse/` subdirectory naming. Tests are integration tests under `tests/` (Cargo convention) keyed to milestone IDs.

## Complexity Tracking

> No constitutional violations. Table omitted.
