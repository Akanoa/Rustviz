# Feature Specification: M01 — Frontend Skeleton (lexer + parser)

**Feature Branch**: `002-m01-frontend-skeleton`
**Created**: 2026-05-20
**Status**: Draft
**Input**: User description: "implement M01 from MILESTONES.md"

**Authoritative scope source**: [`MILESTONES.md` › M01 — Frontend skeleton (lexer + parser)](../../MILESTONES.md). The In scope / Out of scope / Entry criteria / Exit criteria / Demo lines in that block are the contract for this feature. This spec elaborates user stories, requirements, and success criteria; it does not redefine scope.

## User Scenarios & Testing *(mandatory)*

The "users" of this feature are internal: the M02 resolver milestone (which consumes the AST + spans this feature produces) and the contributor who writes / reads the snapshot tests that prove M01 works. There is no end-user-facing surface in M01 — the visualizer's beginner audience won't see anything until M04+. M01's job is to produce a typed, spanned AST and to fail loudly with span-bearing errors when input is malformed.

### User Story 1 — Parse a Level 1 program into a spanned AST (Priority: P1)

A contributor writes a small Level 1 Rust program (primitives, let-bindings, functions, blocks-as-expressions, if-expressions, operators) and runs the parser on it. They get back a typed AST where every node carries a `Span` (byte offset + FileId) pointing to its source text. Snapshot tests pin this output so regressions are visible.

**Why this priority**: this is the entire point of M01. Without an AST nothing downstream exists. M02 (resolver) consumes this directly; M03 (evaluator) consumes it transitively. If this story works, M01 is fundamentally done; the rest is robustness.

**Independent Test**: write `tests/samples/m01_arithmetic.rs` with a Level 1 program, run `cargo test --test m01`, observe a snapshot under `tests/snapshots/m01_arithmetic.snap` containing the AST. Verify spans on every node by visual inspection of the snapshot.

**Acceptance Scenarios**:

1. **Given** a valid L1 program `fn main() { let x = 2 + 3; }`, **When** the parser runs, **Then** the output is an AST with a function, a let-binding, a binary-expression, and two integer literals — each with a non-empty `Span`.
2. **Given** a program using all L1 syntax (let/let mut, fn with parameters, scopes, blocks-as-expressions, if-as-expression, all operators with precedence), **When** the parser runs, **Then** every node has its span and operator precedence is respected (e.g. `2 + 3 * 4` parses as `2 + (3 * 4)`).
3. **Given** an empty input `""`, **When** the parser runs, **Then** the result is an empty program (zero items) without error.

---

### User Story 2 — Span-bearing errors on parse failure (Priority: P1)

When the input is invalid, the contributor gets a single error pointing at the offending token's span. The error message is clear enough that a beginner could in principle act on it, even though M01's audience is internal. The parser stops at the first error (per CLAUDE.md locked-in decision).

**Why this priority**: every later milestone displays errors back to the editor (M05 onward shows them in the browser). If errors lack spans now, every later milestone has to retrofit. Doing this right at M01 is the cheapest moment.

**Independent Test**: write `tests/samples/m01_unexpected_token.rs` with a deliberately broken program (e.g. `fn main() { let = 5; }` — missing identifier). Run `cargo test --test m01`, observe a snapshot containing the error message and its span. The snapshot pins both message and span position.

**Acceptance Scenarios**:

1. **Given** a program with a missing semicolon `fn main() { let x = 5 }`, **When** the parser runs, **Then** a single error is returned pointing at the closing `}` (or the position where the `;` was expected) with a non-empty span.
2. **Given** a program with two errors in sequence, **When** the parser runs, **Then** only the first error is reported (stop-at-first-error per locked-in decision).
3. **Given** a program whose error is on line N column M, **When** the error is rendered for a human, **Then** the line/column derived from the span byte-offset + SourceMap matches N/M.

---

### User Story 3 — Lexer rejects `&` with a pedagogical L1-specific message (Priority: P2)

When the input contains `&` or `&mut`, the lexer rejects it before the parser ever sees it, with an error explaining that references are a Level 2 feature not yet supported. This pre-empts the vague "unexpected token" message the parser would otherwise produce and prepares the upgrade path for M06 (where `Amp`/`AmpMut` tokens land).

**Why this priority**: explicitly called out as a CLAUDE.md locked-in decision ("Reject `&` at the lexer in level 1 … Replace with `Amp`/`AmpMut` tokens when level 2 lands"). Doing it at the lexer level means M06 can flip a single switch rather than re-routing logic. P2 because the parser-level error would technically work today; the lexer-level rejection is about pedagogy and future-proofing.

**Independent Test**: write `tests/samples/m01_reject_ampersand.rs` containing `let r = &x;`. Run `cargo test --test m01`, observe a snapshot containing the lexer error with a span pointing at the `&` and a message mentioning "Level 2" or "references" or equivalent.

**Acceptance Scenarios**:

1. **Given** input containing `&`, **When** the lexer runs, **Then** an error is returned with the `&` token's span and a message that mentions references being a future-level feature.
2. **Given** input containing `&mut`, **When** the lexer runs, **Then** the same kind of error is returned (the lexer does not need to distinguish `&` from `&mut` — both are rejected with the same family of message in M01).
3. **Given** input where `&` appears inside a comment (e.g. `// borrow & later`), **When** the lexer runs, **Then** no error is reported — comments are stripped before token recognition.

---

### Edge Cases

- **Empty input** → empty program AST, no error. Tested in US1 acceptance scenario 3.
- **Multi-character tokens** (`==`, `!=`, `<=`, `>=`, `->`) — the lexer must produce single token instances, not two adjacent ones. Snapshot tested.
- **Trailing comment without newline** (`let x = 5; // comment` with no final `\n`) — the lexer must consume the comment to EOF without error.
- **Operator precedence boundaries** — `2 + 3 * 4` vs `(2 + 3) * 4` produce different ASTs; both must be snapshot tested.
- **Block as expression vs block as statement** — `let x = { 1 + 2 };` vs `{ 1 + 2; }` must parse differently (the former has a tail expression, the latter doesn't).
- **`if` as expression vs `if` as statement** — `let x = if c { 1 } else { 2 };` requires both branches with matching tail expressions; `if c { foo(); }` has no else and no tail. Both supported.
- **UTF-8 in source text** — spans must remain valid byte offsets. The lexer must not split a multi-byte character. The first test that proves it: a comment containing a non-ASCII character (e.g. `// café`), where lexing succeeds and byte offsets on surrounding tokens are correct.
- **EOF in the middle of a multi-char token** — e.g. input ending in `=` after `=` could be the start of `==`. The lexer must commit to `=` once EOF is seen, not hang or error.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST lex an input `&str` to a `Vec<Token>` where each token carries its span (byte offset range + FileId).
- **FR-002**: System MUST parse the token vector to a typed AST representing the Level 1 subset documented in CLAUDE.md (primitives, let / let mut, functions, scopes, blocks-as-expressions, if-as-expression, operators with precedence). Every AST node MUST carry a span.
- **FR-003**: System MUST handle multi-character lookahead tokens (`==`, `!=`, `<=`, `>=`, `->`) as single tokens, not two adjacent ones.
- **FR-004**: System MUST strip whitespace and line comments (`//`) before producing tokens, without losing track of byte offsets for subsequent tokens.
- **FR-005**: System MUST reject `&` (and `&mut`) at the lexer with a span-bearing error mentioning that references are a future-level feature. The lexer MUST NOT emit `&` as a token in M01.
- **FR-006**: System MUST stop at the first parse error and return that single error. It MUST NOT attempt error recovery.
- **FR-007**: Parser errors MUST carry a non-empty span pointing at the offending token (or, for "unexpected EOF", the end of input).
- **FR-008**: Operator precedence MUST follow Rust's standard precedence for the supported operators (e.g. `*` binds tighter than `+`, comparison binds looser than arithmetic, `=` in let-bindings is rightmost).
- **FR-009**: System MUST provide a `SourceMap` mapping byte offsets to (line, column) so error rendering can produce human-readable positions on demand.
- **FR-010**: System MUST support multiple files via `FileId` from day one (the `SourceMap` is indexed by `FileId`), even though M01 only exercises single-file inputs in its tests. This satisfies the CLAUDE.md locked-in decision and avoids a refactor later.
- **FR-011**: System MUST expose a Cargo test target `m01` (i.e. `cargo test --test m01` runs the M01 snapshot suite) so the demo command in MILESTONES.md works as written.

### Key Entities

- **Token**: a lexical unit with a `kind` (keyword / identifier / literal / punctuation / operator) and a `span` (byte range + FileId).
- **Span**: `start: usize, end: usize, file: FileId` — half-open byte range into a source file.
- **SourceMap**: maps a `FileId` to the original `&str` + a precomputed line-start index for fast line/column lookup.
- **AST node**: every syntax form (Expr, Stmt, Item, Block, Pattern, Type) carries a `span`. Concrete variants cover the L1 subset only.
- **ParseError / LexError**: a structured error with `message` and `span`. A single error is returned from a failing parse; no error list.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `cargo test --test m01` exits 0 after running ≥ 5 snapshot tests covering: (a) one valid L1 program exercising the full L1 surface, (b) one operator-precedence sample, (c) one unexpected-token error case, (d) one lexer `&`-rejection case, (e) one empty-input case.
- **SC-002**: 100% of AST nodes in any successful parse output carry a non-empty span pointing at source text the node was derived from. Verified by visual inspection of snapshot files (no `Span(0, 0)` placeholders).
- **SC-003**: For any single-error input, exactly one error is returned (not zero, not more than one). Verified by the error snapshot tests asserting cardinality.
- **SC-004**: Snapshot tests are deterministic — running `cargo test --test m01` twice in succession produces no snapshot drift (same byte-exact output).
- **SC-005**: Total source under `src/parse/` stays under ~2000 lines of code (excluding tests). This is a soft cap to defend against scope creep into M02 (resolution); if hit, stop and reconsider what's being done.
- **SC-006**: `cargo build --release` succeeds with no warnings (treat warnings as errors via crate-level `#![deny(warnings)]` or `#[deny(unused, dead_code)]` as appropriate).

## Assumptions

- The Cargo project does not yet exist at the repo root. Scaffolding it (running `cargo init --lib`) is the first implementation task, executed as part of M01's "scaffold the Cargo project" entry-criteria step from MILESTONES.md.
- The parser is hand-rolled recursive descent (CLAUDE.md locked-in decision: no parser framework, Elyze rejected).
- Snapshot tests use a snapshot library (likely `insta`) — choice of library is a plan-phase decision, not a spec-phase decision.
- Error messages are in English. User-facing message language is not yet decided (CLAUDE.md says "decide per audience"), but M01's errors are internal-only.
- AST node types are owned (no lifetimes / borrowing back to input) for simplicity. M02 may later wrap them in arenas; not M01's concern.
- The `&` rejection error message at the lexer mentions the future Level 2 transition in some form. Exact wording is plan-phase.
- M01 does not produce any `MemEvent` — that's M03. M01 only produces tokens and AST.
- Implementation is by AI agents under maintainer direction; sizing follows the S/M/L complexity rubric in `specs/001-milestone-roadmap/research.md`. M01 is rated L on that rubric.
