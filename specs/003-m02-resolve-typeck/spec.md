# Feature Specification: M02 — Name Resolution + Lightweight Typeck

**Feature Branch**: `003-m02-resolve-typeck`
**Created**: 2026-05-20
**Status**: Draft
**Input**: User description: "M02"

**Authoritative scope source**: [`MILESTONES.md` › M02 — Name resolution + lightweight typeck](../../MILESTONES.md). The In scope / Out of scope / Entry criteria / Exit criteria / Demo lines in that block are the contract for this feature. This spec elaborates user stories, requirements, and success criteria; it does not redefine scope.

## User Scenarios & Testing *(mandatory)*

The "users" of this feature are internal: the M03 evaluator milestone (which consumes resolved + typed AST to emit `MemEvent`s) and the contributor writing snapshot tests. M02 takes M01's `Program` (a parsed AST) and produces (a) a stable `BindingId` for every introduced binding and every use site, and (b) a `Ty` (one of `i32`, `bool`, `()`) for every expression. Snapshots pin both happy paths and the two error cases CLAUDE.md explicitly calls out: "use of undeclared variable" and annotation/operator type mismatches.

### User Story 1 — Resolve identifiers and propagate types on a valid program (Priority: P1)

A contributor writes a valid Level 1 program and runs the M02 pipeline on the M01 AST. They get back a resolved + typed view: every `Ident` use carries a `BindingId` pointing at its declaration, and every expression carries an inferred `Ty`. Snapshot tests pin the output for a small set of representative programs (let-bindings, arithmetic, comparisons, function calls, blocks-as-expressions, if-as-expression, shadowing across nested scopes, function parameters).

**Why this priority**: M02 has no MVP if the happy path doesn't work. M03 cannot start without `BindingId` and `Ty` annotations available for every AST node — those are the data the event emitter keys off.

**Independent Test**: write `tests/samples/m02_shadow.rs` (a program with shadowing and uses), run `cargo test --test m02`, observe a snapshot under `tests/snapshots/` showing each binding's `BindingId`, each use's `BindingId` reference, and each expression's `Ty`. Verify shadowing produces distinct `BindingId`s.

**Acceptance Scenarios**:

1. **Given** a program `fn main() { let x = 5; let y = x + 1; }`, **When** resolve+typeck runs, **Then** `x` (let) and `y` (let) each get a unique `BindingId`; the `x` use in `x + 1` resolves to the `let x` BindingId; `x + 1` has type `i32`; `y` has type `i32`.
2. **Given** a program with shadowing `fn main() { let x = 5; let x = true; }`, **When** resolve+typeck runs, **Then** the two `let x` introduce distinct `BindingId`s; the inner shadows the outer; types of the two `x` bindings differ (`i32` vs `bool`) without error.
3. **Given** a function with parameters `fn add(a: i32, b: i32) -> i32 { a + b }`, **When** resolve+typeck runs, **Then** `a` and `b` use sites inside the body resolve to the parameter `BindingId`s; the tail expression `a + b` has type `i32`; the body's tail type matches the declared return type `i32`.
4. **Given** an if-as-expression `let v = if c { 1 } else { 2 };` where `c` is `bool`, **When** resolve+typeck runs, **Then** both branches' tail expressions have type `i32`; the whole `if` has type `i32`; `v` has type `i32`.
5. **Given** an if-as-statement `if c { foo(); }`, **When** resolve+typeck runs, **Then** the `if` has type `()` (no else, body has no tail expression) and is accepted as a statement.

---

### User Story 2 — Undeclared variable error with span (Priority: P1)

When the input references an identifier with no in-scope declaration, the resolver returns a single `ParseError` (re-using M01's error type) whose message names the identifier and whose span points at the offending use site.

**Why this priority**: this is CLAUDE.md's first explicit deliverable for the resolver: "Ident → BindingId, 'use of undeclared variable' errors". Skipping it makes M02 incomplete.

**Independent Test**: write `tests/samples/m02_undeclared.rs` containing `fn main() { let y = z + 1; }` (z is undeclared). Run `cargo test --test m02`, observe a snapshot containing the error with `z`'s span.

**Acceptance Scenarios**:

1. **Given** `fn main() { let y = z + 1; }`, **When** resolve runs, **Then** a single error is returned naming `z` and pointing at the use of `z` (not at `let y` or the binary expr).
2. **Given** a program with two undeclared identifiers in sequence, **When** resolve runs, **Then** only the first error is reported (stop-at-first, matching M01's policy).
3. **Given** a program using a function before its declaration `fn main() { f(); } fn f() {}`, **When** resolve runs, **Then** the `f()` call succeeds (functions are forward-declared at the program level).

---

### User Story 3 — Type mismatch error with span (Priority: P2)

When the input has a type annotation that conflicts with the initializer, or applies an operator to incompatible operands, or returns a value whose type doesn't match the function's declared return type, the typeck pass returns a single error describing the mismatch with the offending expression's span.

**Why this priority**: typeck is the second half of M02 ("Lightweight typeck: validate annotations, propagate obvious types"). Its primary visible deliverable is rejecting type errors. P2 because resolve (US1, US2) is the harder, more foundational half — typeck error messages can be improved iteratively.

**Independent Test**: write `tests/samples/m02_type_mismatch.rs` containing `fn main() { let x: bool = 5; }`. Run `cargo test --test m02`, observe a snapshot containing a type-mismatch error with the offending span.

**Acceptance Scenarios**:

1. **Given** `let x: bool = 5;`, **When** typeck runs, **Then** an error is returned describing the mismatch between annotation `bool` and initializer type `i32`, with span pointing at the initializer (or the let stmt — both are acceptable; the snapshot pins which).
2. **Given** `let x = 5 + true;`, **When** typeck runs, **Then** an error is returned naming the operator `+` and its operand-type mismatch with span at the binary expr.
3. **Given** `fn f() -> i32 { true }`, **When** typeck runs, **Then** an error is returned describing the return-type mismatch with span at the body's tail expression.
4. **Given** `if 5 { ... } else { ... }`, **When** typeck runs, **Then** an error is returned saying the condition must be `bool`, found `i32`, with span at the condition.

---

### Edge Cases

- **Shadowing chain**: `let x = 5; let x = x + 1;` — the second `let x` should resolve its RHS `x` to the FIRST `let x` (the new binding shadows from the *next* statement onward, not within its own RHS). The two `let x` get distinct BindingIds.
- **Block scoping**: `let x = 5; { let x = 6; }; x` — outer `let x`, inner `let x` (shadow within block), then `x` after the block resolves to outer.
- **Forward references**: function items at the top level are visible to each other regardless of source order. `let`-bindings are NOT forward-visible (M02 follows Rust's let-before-use rule).
- **Empty function body**: `fn f() {}` — body type is `()`; no return-type annotation means implicit `()`; OK.
- **If without else, in statement position**: `if c { ... };` — body must have type `()` (no tail expression or unit tail). If used in expression position with no else, error: "missing else branch" or treat as `()` and error if context expects non-unit.
- **Block with only stmts**: `{ let x = 5; }` has type `()`. Block with tail expr `{ let x = 5; x }` has the tail's type.
- **Parameter name collision**: `fn f(x: i32, x: i32) -> i32 { x }` — duplicate parameter names. M02 should reject this with an error pointing at the second occurrence. (CLAUDE.md doesn't explicitly call this out but it falls naturally from "scope checks".)
- **`true`/`false` as identifiers**: lexed as Bool literals, never identifiers — no resolution needed for them. (Confirmed by M01 lexer.)
- **The `_` identifier**: M02 treats `_` as a normal binding name in let-stmts. Pattern-style discard semantics are out of scope. A subsequent use of `_` resolves to the most recent `let _`.
- **`return` statement**: M02 doesn't introduce `return` semantics. CLAUDE.md L1 grammar has the `return` keyword but L1 functions return their tail expression. `return` keyword usage is deferred to a later level. M02 may either reject `return` as "deferred" or simply not parse it at the M01 level (M01 already doesn't include `return` in parse_stmt). Confirmed: M01 doesn't accept `return` — M02 has no `return` to worry about.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST assign a unique `BindingId` to every introduced binding (function name, let-binding name, function parameter name) in the input program. `BindingId`s are stable across runs given the same input (FR-008).
- **FR-002**: For every identifier use site in the AST, system MUST resolve it to the `BindingId` of its declaration (the most recent in-scope binding with that name), and record the mapping such that consumers can look it up.
- **FR-003**: System MUST return a single `ParseError` (M01's type, reused) with a span when an identifier use site has no in-scope declaration. Message MUST name the identifier.
- **FR-004**: System MUST honor lexical block scoping: inner-block bindings shadow outer-block bindings of the same name; after the inner block ends, the outer binding is visible again.
- **FR-005**: System MUST treat shadowing within a scope as introducing a NEW `BindingId` (e.g. `let x = 5; let x = true;` produces two distinct `BindingId`s; later use sites resolve to the inner one).
- **FR-006**: Function items at the top level MUST be visible to each other regardless of source order (forward references work). Let-bindings MUST NOT be forward-visible.
- **FR-007**: System MUST infer a `Ty` (one of `i32`, `bool`, `()`) for every expression node. The inference rules implement: integer literal → `i32`; boolean literal → `bool`; identifier → its binding's type; binary arithmetic op (`+ - * / %`) → operand types must match and be `i32`, result `i32`; binary comparison op (`< <= > >=`) → operands must match and be `i32`, result `bool`; equality op (`== !=`) → operands must match (`i32` or `bool`), result `bool`; logical op (`&& ||`) → operands `bool`, result `bool`; unary `-` → operand `i32`, result `i32`; unary `!` → operand `bool`, result `bool`; block → its tail expression's type, or `()` if no tail; `if` → both branches' types must match (or, with no else, type is `()` and then-branch must be `()`); call → callee must be a function, argument types must match parameter types, result is the function's return type; parenthesized expression → inner type.
- **FR-008**: System MUST validate type annotations against inferred types: `let x: T = init` requires `T` matches `init`'s type; `fn f(...) -> T { body }` requires `body`'s type matches `T`; `if cond { ... }` requires `cond` is `bool`.
- **FR-009**: System MUST stop at the first type or resolution error and return that single error (matching M01's stop-at-first policy).
- **FR-010**: Type and binding errors MUST carry a non-empty span pointing at the offending expression / use site (or the offending annotation).
- **FR-011**: System MUST reject duplicate parameter names within the same function signature with a span at the second occurrence.
- **FR-012**: System MUST expose a Cargo test target `m02` (i.e. `cargo test --test m02` runs the M02 snapshot suite).

### Key Entities

- **BindingId**: a stable, unique identifier for each introduced binding. New `BindingId`s are assigned to every fresh binding, including shadowed copies.
- **Binding**: the declaration site (fn-name, fn-param, let-stmt) that a `BindingId` refers to. Carries the declared name, span, and type.
- **Ty (L1 type)**: a primitive type — `I32`, `Bool`, `Unit`. (More variants may land later; M02 has only these three. `Fn(args, ret)` is a related concept M02 uses internally to represent function signatures, but is not necessarily a `Ty` variant exposed to consumers.)
- **ResolutionMap**: associates each `Ident` use site (by span / node identity) with its `BindingId`.
- **TypeMap**: associates each expression node with its inferred `Ty`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `cargo test --test m02` exits 0 after running ≥ 5 snapshot tests covering: (a) one valid program exercising shadowing across scopes, (b) one function-with-parameters program, (c) one program using if-as-expression, (d) one undeclared-variable error case, (e) one type-mismatch error case.
- **SC-002**: 100% of identifier use sites in any successful resolve output carry a `BindingId` that matches an existing declaration. Verified by snapshot inspection (no missing or dangling references).
- **SC-003**: 100% of expression nodes in any successful typeck output carry an inferred `Ty`. Verified by snapshot inspection.
- **SC-004**: For any single-error input, exactly one error is returned (matching M01's policy). Verified by snapshot tests asserting cardinality.
- **SC-005**: Snapshot tests are deterministic — running `cargo test --test m02` twice produces no snapshot drift.
- **SC-006**: Total source under `src/resolve/` + `src/typeck/` stays under ~1500 LOC (excluding tests). Soft cap; reconsider before crossing.
- **SC-007**: `cargo build --release` succeeds with zero warnings under `RUSTFLAGS="-D warnings"`.
- **SC-008**: M01 tests still pass — `cargo test --test m01` exits 0 unchanged.

## Assumptions

- M01 is closed and on `main`. The M01 public API (`parse`, `ast`, `Span`, `FileId`, `SourceMap`, `ParseError`) is the input surface.
- The M02 public API adds a single entry point that takes a `Program` (from M01) and returns either resolved + typed output or a `ParseError`. Exact API shape (one combined entry, or `resolve` + `typeck` separately) is a plan-phase decision.
- M02 reuses M01's `ParseError` type — no new error type introduced. Adding a typed error hierarchy is a future consideration (likely M03 or later).
- L1 type lattice is exactly three types: `i32`, `bool`, `()`. Integer literals are always `i32` (no `i64`, `u32`, etc. in L1). Boolean literals are always `bool`. Unit is the implicit value of blocks without a tail expression and of `if` without else.
- Functions are first-class items in the resolution table but are NOT first-class values in L1 (you cannot assign a function to a variable, only call it). M02 enforces this in typeck (callee must be a direct Ident referring to a function binding).
- The `return` keyword tokenized by M01 is not parsed in L1 — `parse_stmt` and `parse_atom` don't dispatch on it. M02 has no `return` semantics to implement.
- Snapshot output format (combined view of resolved + typed AST vs. side-table format) is a plan-phase decision. Readability is the primary criterion.
- Implementation is by AI agents under maintainer direction; sizing per the S/M/L rubric from `specs/001-milestone-roadmap/research.md` — M02 is rated M (modules: 2, bullets: 2, boundaries: 1).
