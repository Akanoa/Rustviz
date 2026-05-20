---

description: "Task list for M01 — Frontend Skeleton (lexer + parser)"
---

# Tasks: M01 — Frontend Skeleton (lexer + parser)

**Input**: Design documents from `/specs/002-m01-frontend-skeleton/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/parse-api.md ✓, quickstart.md ✓

**Tests**: Tests ARE part of M01's deliverable — the MILESTONES.md M01 block, spec SC-001, and contract C-8 all require snapshot tests. Test tasks appear inside each user story phase.

**Organization**: tasks are grouped by user story so each story can be implemented and verified independently. The MVP is US1 (Phase 3); US2 and US3 add error robustness and the `&` rejection on top.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: user-story tag (US1, US2, US3). Setup, Foundational, and Polish phases have no story tag.
- Include exact file paths (relative to repo root `/home/noa/Documents/projects/lab/rustviz/`).

## Path Conventions

Single Rust library crate at the repo root. Source under `src/`, integration tests under `tests/`, snapshots under `tests/snapshots/`. All artifacts for this feature's planning live under `specs/002-m01-frontend-skeleton/`.

---

## Phase 1: Setup

**Purpose**: scaffold the Cargo project and create the empty module skeleton M01 will fill in.

- [X] T001 Run `cargo init --lib` at the repo root to create `Cargo.toml` and a default `src/lib.rs`. Verify the working tree is clean afterwards except for the new files.
- [X] T002 Edit `Cargo.toml`: set `name = "rustviz"`, `edition = "2024"`, `rust-version = "1.82"` (or current stable); add `[dev-dependencies]` block with `insta = { version = "1", features = ["yaml"] }`, `serde = { version = "1", features = ["derive"] }`. No `[dependencies]` section needed for M01. Add a `[[test]]` entry `name = "m01"` pointing at `tests/m01.rs`.
- [X] T003 Add `.gitignore` at the repo root with Rust patterns per the implement-skill defaults: `target/`, `*.rs.bk`, `*.rlib`, `*.prof*`, `.idea/`, `.vscode/`, `*.log`, `.env*`. Also add `tests/snapshots/*.snap.new` so unreviewed `insta` snapshots don't get committed by accident.
- [X] T004 Create the directory skeleton: `mkdir -p src/parse tests/samples tests/snapshots`. Create empty placeholder files `src/parse.rs`, `src/parse/span.rs`, `src/parse/token.rs`, `src/parse/lexer.rs`, `src/parse/ast.rs`, `src/parse/parser.rs`, `src/parse/error.rs`, `tests/m01.rs`. Each placeholder gets a single-line module-level doc comment naming its role (per `specs/002-m01-frontend-skeleton/plan.md` Project Structure).
- [X] T005 Replace the default `src/lib.rs` with crate-level lint attributes (`#![warn(missing_docs, unused, dead_code, unreachable_pub)]`, `#![warn(clippy::all)]`), a one-line crate doc, and `pub mod parse;`. Re-exports come in T011 once the inner types exist.

**Checkpoint**: `cargo build` succeeds with empty placeholders (or compiler errors only about missing items in `parse.rs`, which T010 fixes).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: implement the shared types every user story depends on — spans, tokens, AST, errors, and the public API stub.

**⚠️ CRITICAL**: no user-story phase begins until Phase 2 closes (`cargo build` succeeds with the full type surface, even if `parse()` is `unimplemented!()`).

- [X] T006 [P] Implement `Span`, `FileId`, `SourceMap`, `SourceFile` in `src/parse/span.rs` per `specs/002-m01-frontend-skeleton/data-model.md` entities and VR-1…VR-5. `Span` derives `Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize`. `FileId` is a `pub struct FileId(pub u32)` with the same derives. `SourceMap` exposes `new()`, `add(name, src) -> FileId`, `get(file) -> Option<&SourceFile>`, `line_col(span) -> Option<(u32, u32)>`. `line_starts` is precomputed in `SourceFile::new`.
- [X] T007 [P] Implement `Token` and `TokenKind` in `src/parse/token.rs` per data-model.md R-013 and VR-6/VR-7. `TokenKind` enum has the closed variant set (Int, Bool, Ident, all keywords, all operators incl. `Arrow`, all punctuation, `Eof`). NO `&` / `Amp` / `AmpMut` variants — M01 rejects at the lexer. Derive `Debug, Clone, PartialEq, serde::Serialize`.
- [X] T008 [P] Define the full AST in `src/parse/ast.rs` per data-model.md AST section: `Program`, `Item`, `FnDecl`, `Param`, `Type` (variants `Path { segments, span }` and `Unit { span }` — adopt the more general path form per research.md open question resolution; record this decision in a one-line module doc comment), `Block`, `Stmt`, `LetStmt`, `Expr` (all variants from data-model.md including `LitInt`, `LitBool`, `Ident`, `Unary`, `Binary`, `Call`, `Paren`, `Block`, `If`), `BinOp`, `UnOp`. Every node gets a `span: Span` field. Derive `Debug, Clone, PartialEq, serde::Serialize` on every type.
- [X] T009 [P] Define `ParseError` in `src/parse/error.rs` per data-model.md VR-15…VR-18. Fields `pub message: String, pub span: Span`. Implement `Display` rendering `"<message>"` (without line/col — line/col rendering is a separate helper that takes a `SourceMap`). Derive `Debug, Clone, PartialEq`. Add `impl std::error::Error for ParseError {}`.
- [X] T010 Edit `src/parse.rs` to declare the submodules (`pub mod span; pub mod token; pub mod lexer; pub mod ast; pub mod parser; pub mod error;`) and define the public entry point `pub fn parse(file: span::FileId, source_map: &span::SourceMap) -> Result<ast::Program, error::ParseError>` returning `unimplemented!()` for now. The signature MUST match `specs/002-m01-frontend-skeleton/contracts/parse-api.md`.
- [X] T011 Edit `src/lib.rs` to add re-exports per `specs/002-m01-frontend-skeleton/contracts/parse-api.md`: `pub use parse::{parse, ast}; pub use parse::span::{FileId, Span, SourceMap}; pub use parse::error::ParseError;`. The `ast` module re-export gives consumers `rustviz::ast::Expr` etc. without exposing the full `parse::` module hierarchy.
- [X] T012 Run `cargo build` and `cargo check`. Both must succeed with zero errors. Warnings are acceptable at this point (the `unimplemented!()` body will generate `unreachable_code` warnings — these go away once Phase 3 lands).

**Checkpoint**: The full public type surface compiles. `parse()` is a stub. Ready to fill in user-story logic.

---

## Phase 3: User Story 1 — Parse a Level 1 program into a spanned AST (Priority: P1) 🎯 MVP

**Goal**: a contributor can call `parse()` on a valid L1 program and get back a fully-spanned `Program` AST. Snapshot tests pin three sample programs (arithmetic, precedence, full-L1-surface) plus the empty-input edge case.

**Independent Test**: `cargo test --test m01` passes with snapshots for `m01_arithmetic`, `m01_precedence`, `m01_full_l1`, `m01_empty`. Visually verify every snapshot has non-empty spans on every node.

### Implementation

- [X] T013 [US1] Implement the lexer in `src/parse/lexer.rs`: `pub fn lex(file: FileId, source_map: &SourceMap) -> Result<Vec<Token>, ParseError>`. State machine over `src.as_bytes()` tracking the current byte offset. Skip whitespace (`b' '`, `b'\t'`, `b'\n'`, `b'\r'`) and line comments (`//` to newline or EOF). Recognize integer literals (digits only, parse to `i64` — handle the unary minus at parser level, not lexer), boolean keyword literals (`true` / `false` → `TokenKind::Bool(b)`), identifiers and keywords (`[A-Za-z_][A-Za-z0-9_]*` then check against the keyword table: `let`, `mut`, `fn`, `if`, `else`, `return` → keyword variant; else `Ident(s)`), all single-char operators and punctuation, multi-char operators with lookahead (`==`, `!=`, `<=`, `>=`, `&&`, `||`, `->`). Emit `Eof` at end. **NO** `&` handling yet — this lands in T025 (US3) along with its test. For M01 Phase 3, if `&` is encountered, leave the behavior undefined / panicking — US1 tests don't exercise it.
- [X] T014 [US1] Implement the parser scaffold in `src/parse/parser.rs`: `pub fn parse_tokens(tokens: Vec<Token>) -> Result<Program, ParseError>`. Define `struct Parser { tokens: Vec<Token>, cursor: usize }` with helpers `peek() -> &Token`, `bump() -> Token`, `expect(kind) -> Result<Token, ParseError>`. Implement program parsing: loop reading items until `Eof`. Implement `parse_item` (currently only `fn_decl` → call `parse_fn_decl`).
- [X] T015 [US1] Extend `src/parse/parser.rs` with `parse_fn_decl`, `parse_param_list`, `parse_param`, `parse_type` (path form `IDENT (:: IDENT)*` or unit `()`). Spans must cover from the `fn` keyword to the closing `}`.
- [X] T016 [US1] Extend `src/parse/parser.rs` with `parse_block` and statement parsing: `parse_block` opens `{`, reads `parse_stmt` in a loop until `}` or a token that starts an expression-without-semicolon (the tail expression). `parse_stmt` dispatches on `let` keyword → `parse_let_stmt`; else → expression-followed-by-`;` → `Stmt::Expr`. `parse_let_stmt`: consume `let`, optional `mut`, identifier, optional `: type`, `=`, expr, `;`. Set `Stmt::Expr` span and `Block.tail` distinction by tracking whether the last item before `}` ended with a semicolon.
- [X] T017 [US1] Extend `src/parse/parser.rs` with expression parsing via Pratt: `parse_expr(min_bp: u8)`. Atom parsing: `LitInt`, `LitBool`, `Ident`, `Paren` (rec), `Block` (rec via `parse_block`), `If` (parse `if cond block (else block)?`), unary `-`/`!`. Postfix: call `(args, ...)`. Binary loop using the precedence table from data-model.md VR-12: `* / %` → 70, `+ -` → 60, `< <= > >=` → 50, `== !=` → 40, `&&` → 30, `||` → 20, `=` (let-init, not as expression operator — handled in `parse_let_stmt`). Right-binding power = left-binding power for left-assoc; right-assoc adds +1 to right. Build `Expr::Binary` / `Expr::Unary` with correct spans (start of LHS to end of RHS).
- [X] T018 [US1] Implement the public `parse()` entry in `src/parse.rs` (replacing the T010 stub): look up the source file, call `lexer::lex(...)`, on `Ok(tokens)` call `parser::parse_tokens(tokens)`, return the result. Errors propagate.
- [X] T019 [P] [US1] Create sample programs under `tests/samples/`:
  - `m01_arithmetic.rs`: `fn main() { let x = 2 + 3; }` — US1 AS-1.
  - `m01_precedence.rs`: `fn main() { let y = 2 + 3 * 4; let z = (2 + 3) * 4; let w = a && b || c; }` — edge case for precedence boundaries.
  - `m01_full_l1.rs`: a program exercising every L1 feature — `let mut`, fn with parameters and return type, scopes, blocks-as-expressions, `if` as expression and as statement, all operators, calls.
  - `m01_empty.rs`: empty file (0 bytes) — US1 AS-3.
- [X] T020 [US1] Implement `tests/m01.rs`: a single `#[test]` function per sample that reads the sample file, registers it with a fresh `SourceMap`, calls `rustviz::parse(...)`, and snapshots the `Result` via `insta::assert_yaml_snapshot!(result)`. Use the `insta::with_settings!` macro to set `snapshot_path = "snapshots"` and `prepend_module_to_snapshot = false` for readable filenames. One test function per sample (4 in this phase): `parses_arithmetic`, `parses_precedence`, `parses_full_l1`, `parses_empty`.
- [X] T021 [US1] Run `cargo test --test m01`. Initial run will produce unreviewed snapshots at `tests/snapshots/m01__parses_*.snap.new`. Run `cargo insta review`, inspect each, accept if the AST shape is correct (visually verify every span is non-trivial — no `start: 0, end: 0` placeholders unless the empty case). Re-run `cargo test --test m01` to confirm all snapshots pass.

**Checkpoint**: Phase 3 closes when `cargo test --test m01` exits 0 with 4 snapshot tests passing. The MVP is shipped: M01 successfully parses L1 programs into spanned ASTs. US2 and US3 add error-path coverage.

---

## Phase 4: User Story 2 — Span-bearing errors on parse failure (Priority: P1)

**Goal**: invalid input produces a single `ParseError` whose `message` is informative and whose `span` points at the offending token. Snapshot tests pin two error cases.

**Independent Test**: `cargo test --test m01` (in addition to Phase 3 tests) passes for `m01_unexpected_token` and `m01_multi_error`. Visually verify the error snapshots' messages and span positions.

### Implementation

- [X] T022 [US2] Audit the parser error paths in `src/parse/parser.rs`. For each `expect()` failure point, ensure: (a) the message names the expected kind AND the found kind (e.g. `"expected `;`, found `}`"`), (b) the span is the unexpected token's span, (c) for unexpected-EOF (`expect()` on an `Eof` token), the span is `Span { start: src.len(), end: src.len(), file }` (zero-length at EOF, data-model.md VR-17). Add a small helper `Parser::error_expected(kind: &str, found: &Token) -> ParseError` to keep messages consistent. Touch only `src/parse/parser.rs`.
- [X] T023 [US2] Create error-case samples and tests:
  - `tests/samples/m01_unexpected_token.rs`: `fn main() { let x = 5 }` — missing `;`. Expected span points at `}`. (US2 AS-1.)
  - `tests/samples/m01_multi_error.rs`: `fn main() { let = 5; let = 6; }` — two `let`-without-identifier errors in sequence. Snapshot should show only ONE error (US2 AS-2 — stop at first).
  - Add two test functions to `tests/m01.rs`: `errors_on_unexpected_token`, `errors_on_first_of_multi`. Each snapshots the `Err(ParseError)` via `insta::assert_yaml_snapshot!`. Run `cargo test --test m01`, run `cargo insta review`, accept. Verify snapshot files contain `"Level 2"` is NOT present (these aren't `&` errors) and that `span` has non-zero `end` for non-EOF cases.

**Checkpoint**: 6 snapshot tests pass total (4 happy + 2 error). All errors carry useful messages and accurate spans.

---

## Phase 5: User Story 3 — Lexer rejects `&` with a Level-2 message (Priority: P2)

**Goal**: any `&` outside of comments triggers a lexer error whose message contains the exact substring `"Level 2"` (per data-model.md VR-18 and research.md R-014).

**Independent Test**: `cargo test --test m01` passes for `m01_reject_ampersand`. The snapshot file contains `"Level 2"` in the message; the span points at the `&` byte.

### Implementation

- [X] T024 [US3] Extend the lexer in `src/parse/lexer.rs` to handle `&`: when the lexer encounters byte `b'&'` outside a comment or whitespace context, return `Err(ParseError { message: "references are a Level 2 feature, not yet supported in this version of rustviz".into(), span })` where `span` covers exactly the `&` byte (length 1). Do not look ahead for `mut` — a bare `&` is enough; `&mut` will hit the same error on the leading `&`. Confirm via inspection that `&` inside a line comment is skipped by the comment-handling code BEFORE this match arm runs (US3 AS-3).
- [X] T025 [US3] Create `tests/samples/m01_reject_ampersand.rs` containing two cases concatenated as one file or split into two files:
  - Primary: `fn main() { let r = &x; }`. Expected: lexer error at the `&` byte (column 21 or so, depending on layout).
  - Comment case: `fn main() { /* removed: was &x */ // borrow & later\n let x = 5; }` — `&` inside comments must NOT trigger the error; the program should parse successfully.
  Add to `tests/m01.rs` as separate test functions `lexer_rejects_ampersand` (snapshot the Err) and `lexer_ignores_ampersand_in_comment` (snapshot the Ok parse). Run, review, accept snapshots. Verify the rejection snapshot contains the substring `"Level 2"`.

**Checkpoint**: 8 snapshot tests pass total (4 happy + 2 error + 2 lexer-rejection). M01 exit criteria fully met.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: verify the cross-cutting success criteria (SC-002 spans-on-every-node, SC-004 determinism, SC-005 LOC cap, SC-006 zero warnings) and close out the audit log.

- [X] T026 [P] Verify SC-006: run `RUSTFLAGS="-D warnings" cargo build --release` and `RUSTFLAGS="-D warnings" cargo test --test m01`. Both MUST succeed with zero warnings. If warnings appear (e.g. `unused_imports`, `dead_code`), fix the underlying code rather than silencing the lint.
- [X] T027 [P] Verify SC-005: run `find src/parse -name '*.rs' -print0 | xargs -0 wc -l` and confirm the total is ≤ 2000. If exceeded, identify the source of bloat (likely the parser) and decide: legitimate L1 complexity → bump the cap with rationale appended to `specs/002-m01-frontend-skeleton/checklists/requirements.md`; scope creep → carve work out and defer to M02 or a follow-up.
- [X] T028 [P] Verify SC-002 and SC-003: open each snapshot under `tests/snapshots/`. For success snapshots, visually confirm every AST node has a non-empty span. For error snapshots, confirm exactly one error is reported (no list/multiple). For the M05 / M03 forward-compat: confirm no `Span { start: 0, end: 0 }` placeholders on real syntax (the empty-program snapshot is the only exception — its `Program.span` may legitimately be `(0, 0)` for empty input).
- [X] T029 [P] Verify SC-004 (determinism): run `cargo test --test m01` twice in succession. The second run must produce zero new `*.snap.new` files. If drift appears, identify the non-deterministic source (likely an iteration order or a HashMap usage in `SourceMap` — switch to `BTreeMap` if so).
- [X] T030 Append the post-implementation audit log to `specs/002-m01-frontend-skeleton/checklists/requirements.md` under a new `## Post-implementation audit (2026-05-20)` section. Table of which SCs passed, any findings from T026–T029, any LOC-cap rationale from T027.
- [X] T031 Run the final clean verification: `cargo clean && RUSTFLAGS="-D warnings" cargo build --release && RUSTFLAGS="-D warnings" cargo test --test m01`. Capture timing as anecdote (not a hard SC). Report PASS/FAIL.
- [X] T032 Stage the changed files: `git add Cargo.toml Cargo.lock .gitignore src/ tests/ specs/002-m01-frontend-skeleton/`. Run `git status` and report. **Do not commit** — committing is the maintainer's call (project policy).

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: no dependencies. T001 must precede T002…T005 (project must exist before its files can be configured).
- **Phase 2 (Foundational)**: depends on Phase 1. T006–T009 parallel across different files; T010 depends on T006/T007/T008/T009; T011 depends on T010; T012 depends on T011.
- **Phase 3 (US1)**: depends on Phase 2 closing clean. Within Phase 3: T013 (lexer) independent of parser tasks; T014–T017 sequential (all touch `parser.rs`); T018 depends on T013 + T017; T019 [P] independent of impl tasks; T020 depends on T018 + T019; T021 depends on T020.
- **Phase 4 (US2)**: depends on Phase 3 closing (need a working parser to add error-cases to). T022 → T023 sequential.
- **Phase 5 (US3)**: depends on Phase 3 closing. Can run in parallel with Phase 4 in principle — they touch different files (US2 → parser.rs, US3 → lexer.rs). Sequence chosen above for predictable execution.
- **Phase 6 (Polish)**: depends on Phases 4 and 5 closing. T026–T029 parallel; T030–T032 sequential.

### Story-Level Dependencies

- US1 (P1, MVP) is strictly first; the parser must exist before US2 / US3 can extend it.
- US2 (P1) and US3 (P2) can be done in either order after US1. They touch different files (parser.rs vs lexer.rs) so they can also be done in parallel by separate agents.

### Parallel Opportunities

- **T006, T007, T008, T009** (Phase 2): four [P] tasks in different files; ideal for parallel agent execution.
- **T013 vs T014–T017**: lexer and parser are independent files; if two agents are available, one can build the lexer (T013) while another builds the parser (T014–T017).
- **T019** vs **T013–T017**: sample programs are inputs, not consumers, of the parser; can be created in parallel with implementation.
- **Phase 4 + Phase 5** can run concurrently after Phase 3.
- **T026, T027, T028, T029** (Phase 6 audits): all read-only across separate concerns.

---

## Parallel Example: Phase 2 Foundational

```bash
# Four independent files — perfect for parallel agents:
Task T006: "Implement Span/FileId/SourceMap in src/parse/span.rs per data-model.md"
Task T007: "Implement Token/TokenKind in src/parse/token.rs per data-model.md R-013"
Task T008: "Define AST types in src/parse/ast.rs per data-model.md"
Task T009: "Define ParseError in src/parse/error.rs per data-model.md VR-15..18"

# After all four finish:
Task T010: "Wire parse.rs entry point + module declarations"
Task T011: "Re-export public surface in lib.rs"
Task T012: "Verify cargo build succeeds"
```

## Parallel Example: Phase 3 split (lexer || parser)

```bash
# Agent A: lexer
Task T013: "Implement lexer in src/parse/lexer.rs (no & handling yet)"

# Agent B: parser scaffold then layers
Task T014: "Parser scaffold (Parser struct, helpers, parse_item)"
Task T015: "Parser items (fn_decl, params, type)"
Task T016: "Parser blocks + statements (let_stmt, expr_stmt)"
Task T017: "Parser expressions via Pratt with precedence table"

# Agent C: samples (in parallel with both above)
Task T019: "Create tests/samples/m01_*.rs sample programs"

# After all of A+B+C close:
Task T018: "Public parse() in src/parse.rs"
Task T020: "tests/m01.rs driver + snapshot assertions"
Task T021: "Run + insta review + commit snapshots"
```

---

## Implementation Strategy

### MVP First (US1 only)

1. Complete **Phase 1**: scaffold (T001–T005).
2. Complete **Phase 2**: foundational types (T006–T012).
3. Complete **Phase 3**: lexer + parser + 4 happy-path snapshots (T013–T021).
4. **STOP and VALIDATE**: `cargo test --test m01` passes 4 tests; visually inspect snapshots for non-empty spans.
5. The MVP ships: M01 produces a spanned AST for L1 input.

### Incremental Delivery

1. **MVP** = Phases 1–3 (US1 ✓). Parser works on happy paths.
2. **Hardening 1** = Phase 4 (US2 ✓). Errors have informative messages and accurate spans.
3. **Hardening 2** = Phase 5 (US3 ✓). `&` rejection in place with the L2-pointer message.
4. **Ready to commit** = Phase 6 (SC-002 through SC-006 verified). Clean build, deterministic snapshots, LOC budget respected.

### Single-Agent Strategy (current case)

One AI agent works through phases sequentially:
1. Phase 1 → Phase 2. T006–T009 can be done in any order (no inter-dependencies).
2. Phase 3: T013 (lexer) before T014–T017 (parser), since the parser's tests in T020 need the lexer to produce tokens for `parse()` to chain. Alternatively, the parser can be written first against fake `Vec<Token>` inputs, but lexer-first is simpler.
3. Phase 4 → Phase 5 in sequence (different files, but the audit log in T030 captures both at once).
4. Phase 6 audits → audit log → stage.

### Parallel-Agent Strategy (if multiple agents available)

After Phase 1 closes:
- Agent A: T006 (span)
- Agent B: T007 (token)
- Agent C: T008 (ast)
- Agent D: T009 (error)

After Phase 2 closes:
- Agent A: T013 (lexer) → later T024 (US3 extension)
- Agent B: T014 → T015 → T016 → T017 (parser layers) → later T022 (US2 error audit)
- Agent C: T019 (samples) → T020 (test driver — needs A+B done)
- Agent D: T028 LOC-cap monitoring on a tick

Stitch in T018 once T013 and T017 close; T021 once T020 closes.

---

## Notes

- [P] tasks = different files, no dependencies. Watch out: T014–T017 all touch `src/parse/parser.rs` and are NOT parallelizable despite "different concerns".
- [Story] tag maps task to its user story. Setup/Foundational/Polish carry no story tag per the format spec.
- Tests are integration tests, not unit tests. M01 deliberately leans on `cargo test --test m01` only; if granular unit tests want to land later, that's M02's call.
- Do not commit until T032 reports clean. Committing is the maintainer's explicit action.
- If T013 (lexer) or T014–T017 (parser) reveal that the AST shape from T008 is wrong (e.g. a node needs more fields), update T008's output AND data-model.md AND contracts/parse-api.md in lock-step. The contract is what M02 will rely on.
- The research R-018 open question (typed Type variants vs general `Type::Path`) is resolved in T008 to use `Type::Path` + `Type::Unit` (more general; M02 will do the resolution to `i32`/`bool`/etc.). Record this in T008's module doc comment.
- Avoid: vague task descriptions, missing file paths, putting M02 (resolver) or M03 (event) work into M01.
