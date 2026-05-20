# Research — M01 Implementation Decisions

This file records the concrete how-to-build decisions for M01. Each is Decision / Rationale / Alternatives.

## Scaffolding

### R-001 — Cargo project shape

- **Decision**: Single library crate (`cargo init --lib` at the repo root). Crate name `rustviz`.
- **Rationale**: Every later milestone (M02 resolver, M03 evaluator, M04 UI) consumes the parser as a library. WASM bindings come later (M04+) as a separate concern (likely a separate crate added then). Starting as a lib avoids a refactor.
- **Alternatives considered**:
  - Binary crate — no consumer story for downstream milestones. Rejected.
  - Workspace with multiple crates from day one — premature; we don't know yet whether the UI / WASM / interpreter split warrants separate crates. Defer until M04 (research R-010).

### R-002 — Rust edition + MSRV

- **Decision**: Rust 2024 edition. MSRV set to the current stable at scaffold time (likely 1.82+ as of 2026-05).
- **Rationale**: Latest features (let-else, pattern guards, etc.) reduce friction. MSRV pinning prevents future-Rust-only syntax from sneaking in.
- **Alternatives considered**:
  - 2021 edition — older; no reason to start there. Rejected.
  - Track nightly — adds toolchain churn; rejected.

### R-003 — Warnings policy

- **Decision**: `#![warn(missing_docs, unused, dead_code, unreachable_pub, clippy::all)]` in `lib.rs`. Treat warnings as errors in CI later via `RUSTFLAGS="-D warnings"`, not via `#![deny(warnings)]` in code.
- **Rationale**: Warn-level keeps local development fluid (warnings show in IDE but don't block compilation). Deny in CI catches regressions. Avoids the `#![deny(warnings)]` antipattern where future compiler versions adding new warnings break builds.
- **Alternatives considered**:
  - `#![deny(warnings)]` — brittle across compiler updates. Rejected.
  - No warn settings — invites dead code accumulation. Rejected.

## Dependencies

### R-004 — Snapshot testing library

- **Decision**: `insta` (latest stable, dev-dependency only).
- **Rationale**: De facto Rust snapshot library; clean `cargo insta review` workflow; YAML/Ron/Debug output modes; widely understood. Supports inline snapshots if needed later. SC-001 / SC-004 (deterministic, byte-exact snapshots) require a snapshot tool — `insta` is the lowest-friction option.
- **Alternatives considered**:
  - `expect-test` — similar concept, smaller ecosystem, no `cargo insta review` equivalent. Rejected.
  - Hand-rolled `assert_eq!` against expected strings inlined in tests — works but loses diff tooling and review workflow. Rejected for ergonomics.

### R-005 — Error library (`thiserror` / `anyhow`)

- **Decision**: None. Hand-roll `ParseError` as a single struct with `message: String, span: Span`.
- **Rationale**: M01 has exactly one error variant (a single message + span). `thiserror` derive saves nothing for one variant. `anyhow` is for error wrapping in apps, not libraries. Keeping the dependency surface minimal is faster to compile and easier to reason about.
- **Alternatives considered**:
  - `thiserror` — overkill for one variant; revisit if errors proliferate (e.g. M02 may want a typed-error hierarchy). Rejected.
  - `anyhow` — wrong tool (library error type, not app error context). Rejected.

### R-006 — Other deps

- **Decision**: None beyond `insta` (dev-dependency). No `nom`, `pest`, `lalrpop`, `chumsky`, `combine` — CLAUDE.md locked-in decision rejects parser frameworks.
- **Rationale**: Hand-rolled parser is the locked-in approach (CLAUDE.md "No parser framework"). Adding no production dependencies means the crate stays portable to WASM with zero porting friction at M04.

## AST + spans

### R-007 — Span representation

- **Decision**: `Span { start: u32, end: u32, file: FileId }` where `FileId(u32)` is a newtype. Half-open `[start, end)` byte indices into the source file.
- **Rationale**: `u32` is sufficient — no rustviz example program will exceed 4 GB. Halves memory vs `usize` (8 bytes on 64-bit). `FileId` newtype prevents mixing file ids with other `u32` values. Byte indices (not char indices) align with how `&str` and `[u8]` work in Rust.
- **Alternatives considered**:
  - `usize` for offsets — wastes memory; the limit is unreachable. Rejected.
  - `(usize, usize)` slice indices without `FileId` — loses multi-file readiness (CLAUDE.md FR-010). Rejected.
  - `Range<u32>` instead of `start`/`end` fields — `Range` is non-`Copy`, awkward to pass around. Rejected.

### R-008 — AST shape

- **Decision**: Owned types. Recursive shapes use `Box<Expr>` / `Box<Block>`. No lifetimes back into source. Every AST node has a `span: Span` field.
- **Rationale**: Simplifies M02 / M03 consumption — they can move / clone subtrees freely without lifetime entanglement. Spec assumption already locked owned types in. Boxed recursion is the standard Rust idiom for recursive enums (`Expr` containing `Expr` children).
- **Alternatives considered**:
  - `&'src str` references for identifiers / literals — couples AST lifetime to input lifetime; complicates downstream APIs. Rejected.
  - Arena-allocated nodes with `&'a` references — premature; M02 can introduce this if profiling shows allocation pressure. Rejected.
  - `Rc<Expr>` for sharing — wrong tool; AST isn't shared. Rejected.

### R-009 — Pratt parser for expressions

- **Decision**: Pratt parser (operator-precedence table) for expression parsing; recursive descent for statements, items, blocks, patterns, types.
- **Rationale**: Rust's expression precedence has ~10 levels; Pratt handles them in one function with a precedence table. Recursive descent for the non-expression grammar is cleaner because those forms have unique shapes (`fn` head, `let` head, `if` head). Mixing the two strategies is standard.
- **Alternatives considered**:
  - Pure recursive descent with precedence climbing — works, but produces deeply nested per-precedence-level functions; harder to extend in M06 when borrow-ops arrive. Rejected.
  - Top-down operator precedence (TDOP) split per token — Pratt is one form of TDOP; treating them as different is overengineering. Adopted.

## Module layout

### R-010 — Single crate, no workspace

- **Decision**: One crate (`rustviz`) at the repo root. No workspace. WASM/UI split deferred to M04 if needed.
- **Rationale**: We don't yet know whether the UI / WASM / interpreter will be separate crates. Premature workspace structure adds ceremony with no payoff. Easy to refactor into a workspace later.
- **Alternatives considered**:
  - Workspace with `rustviz-parse`, `rustviz-eval`, `rustviz-ui` crates from day one — sketches the future but locks in module boundaries before we've validated them. Rejected.

### R-011 — `parse.rs` + `parse/` convention (no `mod.rs`)

- **Decision**: Use Rust 2018+ module convention — `src/parse.rs` declares the submodules and defines the public `parse()` entry; `src/parse/{span,token,lexer,ast,parser,error}.rs` hold implementations.
- **Rationale**: Faithful to CLAUDE.md's "Planned code layout" sketch (which shows the named files directly under `parse/`). Adds two files CLAUDE.md doesn't list (`token.rs`, `error.rs`) — both are clean separations of concern that would otherwise bloat `lexer.rs` and `parser.rs`. The `mod.rs` form would obscure the module root file.
- **Alternatives considered**:
  - `mod.rs` form — older convention, less editor-friendly (every module's root is named `mod.rs`). Rejected.
  - Lump tokens into `lexer.rs` and errors into `parser.rs` per CLAUDE.md literal — works for now but `lexer.rs` grows to ~400+ lines mixing two concerns. Splitting is cleaner and still within CLAUDE.md spirit.

### R-012 — Public API surface

- **Decision**: A single entry point `pub fn parse(file: FileId, source_map: &SourceMap) -> Result<ast::Program, ParseError>`. The crate root (`lib.rs`) re-exports `parse::parse`, `parse::ast`, `parse::SourceMap`, `parse::FileId`, `parse::Span`, `parse::ParseError`. Internal types stay `pub(crate)`.
- **Rationale**: One entry point keeps the contract small and stable. SourceMap as input (not source `&str`) means the caller manages file registration. M02 will add `resolve()`; M03 will add `evaluate()`; each gets its own entry point.
- **Alternatives considered**:
  - `parse(src: &str) -> Result<Program, ParseError>` — works for single-file but rebinds the SourceMap problem at every call site. Rejected.
  - Builder pattern — overengineered for one function. Rejected.

## Tokens

### R-013 — Token kinds (M01-only)

- **Decision**: `TokenKind` enum variants for L1 only:
  - Literals: `Int(i64)`, `Bool(bool)`
  - Identifiers: `Ident(String)`
  - Keywords: `Let`, `Mut`, `Fn`, `If`, `Else`, `Return`, `True`, `False`
  - Operators: `Plus`, `Minus`, `Star`, `Slash`, `Percent`, `Eq`, `EqEq`, `BangEq`, `Lt`, `Le`, `Gt`, `Ge`, `AndAnd`, `OrOr`, `Bang`, `Arrow`
  - Punctuation: `LParen`, `RParen`, `LBrace`, `RBrace`, `Comma`, `Semi`, `Colon`
  - End: `Eof`
- **Rationale**: This is the closed set L1 needs. New kinds get added when later levels need them (e.g. `Amp` / `AmpMut` in M06, `LBracket` / `RBracket` if/when collection indexing lands). Closed enum makes match-completeness errors useful.
- **Alternatives considered**:
  - Open-ended `Punct(char)` variants — loses match-completeness checking. Rejected.
  - Variant per multi-char operator decomposed (e.g. emit two `Eq` tokens for `==`) — defeats the lexer's job. Rejected explicitly per FR-003.

### R-014 — `&` lexer rejection error message

- **Decision**: The message reads: `"references are a Level 2 feature, not yet supported in this version of rustviz"`. The span points exactly at the `&` byte (or `&mut` if matched).
- **Rationale**: Calls out the level so a beginner has a hook ("oh, levels"); avoids vague "unexpected character". The L2 reference is a forward-pointer to when this will work.
- **Alternatives considered**:
  - `"unexpected character '&'"` — vague, not pedagogical. Rejected.
  - `"references not supported"` — true but doesn't position the constraint as temporary. Rejected.

## Errors

### R-015 — Single error type

- **Decision**: `pub struct ParseError { pub message: String, pub span: Span }`. Both lexer errors and parser errors use this type. `Result<T, ParseError>` everywhere internally.
- **Rationale**: Spec FR-006 says stop-at-first-error; one error type is sufficient. Unifying lexer and parser errors avoids `enum Diagnostic { Lex(LexError), Parse(ParseError) }` wrapping that adds nothing.
- **Alternatives considered**:
  - Separate `LexError` + `ParseError` — unifies later anyway when callers want one error stream. Rejected pre-emptively.
  - `Diagnostic` enum with kind tag — over-structured for one variant.

## Testing

### R-016 — Snapshot format

- **Decision**: Use `insta::assert_yaml_snapshot!` for AST snapshots (with `serde::Serialize` derive on AST types) and `insta::assert_snapshot!` (plain text) for error-message snapshots.
- **Rationale**: YAML is readable for hierarchical AST; plain text is enough for single-line errors. Adds `serde` (+ optional `serde_yaml` via insta features) as dev-dep.
- **Alternatives considered**:
  - `insta::assert_debug_snapshot!` — uses `Debug`; output is dense and hard to read for deeply nested ASTs. Rejected.
  - Ron format — Rust-flavored but less universally readable. Rejected.

### R-017 — Sample file locations

- **Decision**: Inputs in `tests/samples/m01_*.rs`. Snapshots in `tests/snapshots/` (managed by `insta`). One `tests/m01.rs` driver enumerates samples and asserts a snapshot per sample.
- **Rationale**: Cargo integration-test convention puts each `tests/<name>.rs` file as a separate test binary; `cargo test --test m01` runs exactly the M01 driver. Sample programs as standalone `.rs` files mean syntax highlighting works in editors and they can be opened standalone.
- **Alternatives considered**:
  - Inline `&'static str` samples in `tests/m01.rs` — loses editor highlighting, harder to scan. Rejected.
  - `tests/samples/*.rs` flat (no `m01_` prefix) — later milestones (M02, M03) will add their own samples; the prefix prevents collisions. Adopted.

## Scope discipline

### R-018 — What M01 must NOT do

- **Decision**: No name resolution (no `BindingId` assignment beyond `Ident(String)` in AST). No type checking (annotations parsed but not validated). No event emission. No UI. No WASM. No error recovery. No incremental parsing. No formatting / pretty-printing.
- **Rationale**: Spec defers all these to later milestones (M02 resolver, M03 evaluator, etc.). Listing the exclusions defends against scope leak (SC-005 soft cap on LOC reinforces this).
- **Alternatives considered**: doing some of these "while we're here" — explicitly rejected; the milestone roadmap exists to prevent exactly this.

## Open question — not blocking

- **Whether to parse types into a `Type` AST node or a placeholder `Unit`-only annotation in M01**. L1 has only primitive types (`i32`, `bool`, `()`). Could parse `: i32` into `Type::I32` and `: bool` into `Type::Bool` (most expressive), or just into `Type::Path { segments: Vec<String> }` (more general, future-proof). Both work for M01. Decide at implementation start; record in `data-model.md`.
