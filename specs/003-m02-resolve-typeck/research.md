# Research — M02 Implementation Decisions

Decision / Rationale / Alternatives for the resolve+typeck passes.

## API shape

### R-001 — Two separate entry points, not combined

- **Decision**: Expose two public functions: `resolve(&Program) -> Result<Resolution, ParseError>` and `typeck(&Program, &Resolution) -> Result<TypeMap, ParseError>`. Callers chain with `?`.
- **Rationale**: distinct passes with distinct outputs and distinct error families. M03 may want resolution without re-running typeck if only binding ids are needed for some lookup. Two functions make the data flow explicit. The combined "analyze()" convenience would just chain these — callers can do it themselves.
- **Alternatives considered**:
  - Single `analyze() -> AnalyzedProgram` — bundles outputs but hides the fact that resolution must complete before typeck starts. Rejected for API clarity.
  - Three pass functions (resolve / typeck-annotations / typeck-bodies) — over-decomposed. Rejected.

## Metadata storage

### R-002 — Use `Span` as side-table key (via `IndexMap`), do not mutate the AST

- **Decision**: `Resolution.uses: IndexMap<Span, BindingId>` and `TypeMap.expr_types: IndexMap<Span, Ty>` (and likewise for `bindings` / `binding_types`). The M01 AST is consumed read-only; resolve/typeck produce these side tables. Insertion happens during the tree walk in source pre-order, so iteration order = tree-walk order.
- **Rationale**:
  - **No M01 AST changes**: M01 just closed with a stable contract; mutating the AST would force a coordinated change to the contract and re-snapshotting all M01 tests.
  - **Spans are unique per syntactic position**: every Ident use, every Expr node has a distinct span (different byte range or different file). Using spans as keys is safe.
  - **`IndexMap` (not `BTreeMap`)**: SC-005 demands deterministic snapshots, which `IndexMap` provides via insertion order — and since insertion happens in a deterministic tree walk, snapshot order matches the natural top-down reading order of the source. `BTreeMap<Span, _>` would also be deterministic but sorts by `(start, end, file)`, which interleaves leaves and outer nodes in a confusing way (e.g. for `(2 + 3)`: LitInt2 → Binary → LitInt3 → Paren) — and `Span` isn't `Ord` in M01, so adopting `BTreeMap` would force an M01 contract touch. `IndexMap` requires only `Hash + Eq`, which `Span` already has. `HashMap` is rejected because its iteration order is non-deterministic.
  - **O(1) lookup** for M03 (which will index `TypeMap` heavily when emitting `MemEvent`s).
- **Alternatives considered**:
  - Add a `NodeId` to every AST node — requires touching M01's ast.rs (and re-snapshotting M01 tests). Rejected to keep M01 closed.
  - A new "resolved AST" type that mirrors the M01 AST with extra fields — doubles the type surface and requires keeping two AST shapes in sync. Rejected.
  - `BTreeMap<Span, _>` — needs `Ord` derived on `Span` (M01 contract touch) and sorts in an unintuitive order for nested expressions. Rejected (see decision rationale).
  - `HashMap<Span, _>` — non-deterministic iteration, breaks SC-005. Rejected.
  - `Vec<(Span, _)>` populated in tree order — gives the same iteration order as `IndexMap` but loses O(1) lookup. Rejected; M03 will need fast lookup.

### R-003 — Snapshot format: side tables, not annotated AST

- **Decision**: snapshots show `Resolution` + `TypeMap` directly (their `Debug` impls — `IndexMap` derives `Debug` and renders entries in insertion order). The AST is NOT re-printed in the snapshot — the reader looks at the `.rs` sample alongside the `.snap` file.
- **Rationale**: M02's job is to produce these tables. The snapshot focuses attention on the M02 output, not on M01's already-pinned AST shape. Keeps snapshots small and reviewable (~30–60 lines per sample vs. 200+ if AST were inlined).
- **Alternatives considered**:
  - Combined "annotated AST" view (custom pretty-printer) — readable but adds a tree-walker we'd need to maintain. Rejected; the side-table view is enough for review.
  - Inline AST + side tables — duplicative, see above. Rejected.

## Resolver

### R-004 — Two-phase resolver: forward-declare items, then walk

- **Decision**: phase 1 — scan top-level items and allocate a `BindingId` for each function (forward declaration). Phase 2 — walk each function's body with a scope stack, allocating `BindingId`s for params (entering the body's outer scope) and let-stmts (in source order; each let becomes visible from the *next* statement, not within its own RHS).
- **Rationale**:
  - **Forward function references** (spec FR-006) require the two-phase approach; let-only forward-vis isn't needed because let-bindings can't legally forward-reference in Rust.
  - **"Let becomes visible from the next stmt"** is Rust's actual rule (let-init can't refer to its own LHS) and matches how shadowing works (`let x = 5; let x = x + 1;` — RHS x = outer 5).
- **Alternatives considered**:
  - Single-pass — fails on `fn main() { f(); } fn f() {}`. Rejected.
  - Three-phase with let-binding gathering — unnecessary for L1's strict scoping rules. Rejected.

### R-005 — Scope stack of `Vec<HashMap<String, BindingId>>`

- **Decision**: the resolver maintains a `Vec<HashMap<String, BindingId>>` where each entry is a scope (top-level fns at index 0, then function-body scope, then nested block scopes). Lookup walks innermost-to-outermost. Push on block entry, pop on exit. Bindings inserted into the current scope's map.
- **Rationale**: standard, simple, O(depth) per lookup which is fine for L1 program sizes. `HashMap` is OK *inside* a scope because lookup goes by name and the scope's iteration order never leaks into output (output goes into the IndexMap which is populated in tree-walk order — see R-002). Determinism of overall output is controlled by the outer iteration order (we iterate the AST in source order, not the scope maps).
- **Alternatives considered**:
  - Flat list of bindings, linear scan on lookup — O(n) per lookup; unnecessarily slow on big programs. Acceptable for M02 but no win. Rejected.
  - Persistent (immutable) scope map — Clojure-style; overkill for stack-shaped scoping. Rejected.

### R-006 — `BindingId` allocation

- **Decision**: `BindingId(u32)`, allocated sequentially starting at 0. Phase 1 allocates ids for top-level fn items in source order. Phase 2 allocates ids for params (in declaration order) and let-stmts (in source order, including shadowed copies — each shadow gets a NEW id).
- **Rationale**: deterministic, dense, easy to debug. Shadowing creates new ids (spec FR-005) so consumers (M03 evaluator) can distinguish them at runtime.
- **Alternatives considered**:
  - Start at 1, reserve 0 for "unbound" — `Option<BindingId>` is clearer than a sentinel. Rejected.
  - String identity (no `BindingId` at all) — defeats the purpose; M03 needs distinct ids for shadowing. Rejected.

### R-007 — Resolver error catalog

- **Decision**:
  - "use of undeclared variable `<name>`" — span at the use site (Ident expr's span). Plain identifier on the LHS of an undefined ref.
  - "duplicate parameter `<name>`" — span at the second parameter's name. Same name appears twice in the param list.
  - "`<name>` is a function; functions are not first-class values in L1" — typeck-side (R-011 below), not resolver-side. Listed here for completeness.
- **Rationale**: limited to what FR-003 / FR-011 demand. Other resolution failures (e.g. shadowing fn with let — actually allowed in Rust, so M02 follows suit) are not errors.
- **Alternatives considered**:
  - Reject shadowing fn with let — Rust allows it; rustviz follows. Adopted.

## Typeck

### R-008 — Bottom-up inference with explicit operator table

- **Decision**: typeck walks each Expr bottom-up, recursively inferring children's types first, then applying the operator's rule. For let-stmts, the init's type is inferred; if an annotation is present, the annotation is matched against the inferred type. For fn-decls, the body block is typechecked against the declared return type.
- **Rationale**: L1 has no generics, no type inference variables, no unification — straight bottom-up is sufficient and produces clean errors. The operator table is small (~14 operators) and lives in `typeck.rs`.
- **Alternatives considered**:
  - HM-style inference with unification — vastly overkill for L1. Reserved for if/when we add generics (likely never in L1–L4). Rejected.
  - Top-down propagation with expected types — useful when we have unannotated lambdas; L1 doesn't. Rejected.

### R-009 — `Ty` enum: value types only

- **Decision**: `pub enum Ty { I32, Bool, Unit }`. Function signatures are represented separately as `pub struct FnSig { params: Vec<Ty>, ret: Ty }`. `FnSig` is NOT a `Ty` variant.
- **Rationale**: functions are not first-class in L1 (spec assumption). Keeping function signatures out of `Ty` means consumers (M03 evaluator) don't have to handle a "function value" case when reading expression types.
- **Alternatives considered**:
  - `Ty::Fn(FnSig)` variant — invites the question "what does it mean to *use* an Ident of function type as a value?". L1 doesn't support that use, so the variant would never appear in `TypeMap`. Rejected.
  - Encode FnSig in BindingInfo only — adopted (FnSig lives next to each `Binding`, not in TypeMap).

### R-010 — `TypeMap` granularity

- **Decision**: `TypeMap.expr_types: IndexMap<Span, Ty>` holds the inferred type of every Expr node that produces a value. Plus `TypeMap.binding_types: IndexMap<BindingId, BindingType>` where `BindingType` is either `Var(Ty)` (let / param) or `Fn(FnSig)` (function decl).
- **Rationale**:
  - Two side tables, but they answer different questions: "what's the type of this expression?" vs. "what's the type of this binding?".
  - Snapshot displays both; consumers query whichever they need.
- **Alternatives considered**:
  - One table — needs a sum type `Either<Ty, FnSig>`. Worse ergonomics. Rejected.

### R-011 — Typeck error catalog

- **Decision**: errors thrown:
  - **Annotation mismatch**: `let x: T = init;` where `T != type_of(init)`. Span at the init expression.
  - **Operator-arity mismatch**: `5 + true` — span at the binary expr, message names both operand types.
  - **Comparison-operand mismatch**: `5 < true` — span at the binary expr.
  - **Logical-operand mismatch**: `5 && true` — span at the binary expr, operand must be bool.
  - **Unary-operand mismatch**: `-true` / `!5` — span at the unary expr.
  - **`if` condition type**: span at cond, expects bool.
  - **`if` branch type mismatch**: span at the if expr; both branches must agree.
  - **`if` without else used as value**: span at the if expr; body had a non-unit tail expression but there's no else branch.
  - **Function return-type mismatch**: span at the body's tail expression (or the fn span if the body is empty and ret is non-unit).
  - **Call to non-function**: span at the callee; "`<name>` is a function" / "`<name>` is not callable" as appropriate.
  - **Call arity mismatch**: span at the call expr; "function `f` expects N argument(s), found M".
  - **Argument type mismatch**: span at the argument expr; "argument N: expected T, found U".
  - **L1 callee restriction**: span at the callee Expr; "L1 only supports direct function calls (callee must be a simple identifier)".
- **Rationale**: covers all four US3 acceptance scenarios + the edge cases. Each error has a span and a concrete message.
- **Alternatives considered**:
  - Generic "type mismatch" message everywhere — less helpful for the M02 audience (M03 implementer + future users). Rejected.

## Module layout

### R-012 — Flat `resolve.rs` + `typeck.rs` (no submodule directories yet)

- **Decision**: a single `src/resolve.rs` and a single `src/typeck.rs`. Each contains all the types and functions for its pass.
- **Rationale**: estimated 300–500 LOC each fits comfortably in one file. CLAUDE.md's "Planned code layout" sketches `resolve/` and `typeck/` directories; we follow the spirit (per-pass module) but not the letter (no submodules until needed).
- **Alternatives considered**:
  - `resolve.rs` + `resolve/{scope.rs, binding.rs}` — premature; collapse back if either file passes 600 LOC.

## Testing

### R-013 — Test driver mirrors M01 closely

- **Decision**: `tests/m02.rs` follows the M01 driver structure (a `sample_test!` macro, one test per sample, `with_settings!` overriding `snapshot_path` and `prepend_module_to_snapshot`, `assert_debug_snapshot!` on the M02 output).
- **Rationale**: consistency with M01 keeps the snapshot-review workflow uniform. Same `INSTA_UPDATE=always` first-run trick when accepting new snapshots.

### R-014 — Test driver wraps M01 + M02 calls in a single helper

- **Decision**: a helper `analyze_sample(name) -> AnalyzeResult` parses the sample, then runs resolve and typeck. `AnalyzeResult` is a struct holding `Result<(Resolution, TypeMap), ParseError>` so both happy paths and either error class fit in one snapshot type.
- **Rationale**: callers (each `#[test]`) get a single line of work; the snapshot includes everything M02 produced.
- **Alternatives considered**:
  - Two separate helpers (one for resolve-only, one for typeck) — would let resolve-error tests skip typeck. But the combined helper already short-circuits on resolve failure (the `?` chain). Rejected.

## Constitution

### R-015 — Same vacuous PASS

- **Decision**: `.specify/memory/constitution.md` still unfilled. No gates apply.
- **Note**: when constitution principles are eventually written, this plan and the M01/M02 source must be re-evaluated.

## Open questions

None remaining. The snapshot-ordering question (previously open) was resolved by switching to `IndexMap` (R-002), which gives tree-walk iteration order by construction.
