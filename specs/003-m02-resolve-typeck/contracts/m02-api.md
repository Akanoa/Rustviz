# Contract — M02 Public API

The surface M03 (and any other downstream consumer) relies on once M02 closes. Breaking changes require coordinated updates.

## Entry points

```rust
pub fn rustviz::resolve(program: &ast::Program)
    -> Result<Resolution, ParseError>;

pub fn rustviz::typeck(program: &ast::Program, resolution: &Resolution)
    -> Result<TypeMap, ParseError>;
```

- **`resolve`**: produces a `Resolution` mapping every `Ident` use site (by `Span`) to its `BindingId`, plus a registry of all `BindingDecl`s.
- **`typeck`**: given a resolved program, produces a `TypeMap` mapping every value-producing `Expr` (by `Span`) to its inferred `Ty`, plus binding type information.
- Both stop at the first error and return a single `ParseError` (FR-009).
- Idempotence: same input → byte-identical output (required for SC-005).

## Re-exports from `lib.rs`

```rust
pub use parse::{parse, ast};
pub use parse::error::ParseError;
pub use parse::span::{FileId, SourceMap, Span};
pub use resolve::{resolve, BindingDecl, BindingId, BindingKind, Resolution};
pub use typeck::{typeck, BindingType, FnSig, Ty, TypeMap};
```

Anything not re-exported is implementation detail.

## Stable types (from M02 close onward)

| Type            | Stability  | Notes                                                                    |
|-----------------|------------|--------------------------------------------------------------------------|
| `BindingId`     | stable     | Newtype `u32`. Allocated by `resolve`.                                   |
| `BindingKind`   | stable for M02 variants | New variants may be added in later milestones (additive — e.g. M06 may add `Borrow`). |
| `BindingDecl`   | stable for M02 fields | Additive field changes OK.                                              |
| `Resolution`    | stable     | Backed by `indexmap::IndexMap` so iteration order is the tree-walk pre-order in which entries were inserted (deterministic + readable). Treat fields as read-only.                                 |
| `Ty`            | stable for M02 variants | New variants will be added in later milestones (M06: borrow types; M07: heap-allocated; M08: shared/sync). |
| `FnSig`         | stable     |                                                                          |
| `BindingType`   | stable for M02 variants | Additive.                                                                |
| `TypeMap`       | stable     | Two-table layout (`expr_types`, `binding_types`), both `indexmap::IndexMap` keyed for tree-walk-order iteration. |

"Additive" semantics same as M01's contract: new variants/fields don't break consumers that exhaustively match — Rust's match-completeness lints will surface needed updates.

## Behavioral guarantees

- **B-1**: `resolve(&program).is_ok()` implies every `Expr::Ident` in `program` has a key in `Resolution.uses`. (Verified by spec SC-002.)
- **B-2**: `typeck(&program, &resolution).is_ok()` implies every value-producing `Expr` in `program` has a key in `TypeMap.expr_types`. (Verified by spec SC-003.)
- **B-3**: Shadowing creates new `BindingId`s. `let x = 5; let x = true;` produces two distinct ids; the use site of `x` after the second `let` resolves to the second id.
- **B-4**: Functions are forward-declared at the top level. `fn main() { f(); } fn f() {}` resolves `f` to the second fn's id.
- **B-5**: Let-bindings are not forward-visible. `fn main() { x; let x = 5; }` is an error at the `x` use site.
- **B-6**: `Ty::Unit` is the inferred type for: blocks with no tail expression, `if` without else, function bodies whose declared (or implicit) return type is `()`.
- **B-7**: `if` without else: the `then_block` body must have type `()` (no non-unit tail). Otherwise an error is reported.

## Errors

`ParseError { message: String, span: Span }` — same shape as M01.

Message conventions (informal; the exact wording is in `research.md` R-007 / R-011 catalogs and can be refined):

- Resolver: `"use of undeclared variable `name`"`, `"duplicate parameter `name`"`.
- Typeck: `"expected type `T`, found `U`"`, `"binary operator `op` requires both operands to be `T`, found `U` and `V`"`, `"`if` condition must be `bool`, found `T`"`, `"branches of `if` have different types: `T` vs `U`"`, `"function `name` expects N argument(s), found M"`, `"argument N: expected `T`, found `U`"`, `"`name` is a function; functions are not first-class values in L1"`, `"L1 only supports direct function calls"`.

These messages may be refined during implementation; the exact wording isn't part of the contract beyond the L1-pedagogy-friendly tone. Snapshots pin the exact strings for the M02 test suite, so any change requires updating snapshots.

## What this contract does NOT cover (deferred)

- **Borrow checking** — M06.
- **Lifetime inference** — M06+.
- **Trait resolution** — out of scope for the L1–L4 plan entirely (deferred bucket in `MILESTONES.md`).
- **Type inference variables / unification** — none in M02; if added later, this contract gets a new section.
- **Multi-error reporting** — currently impossible (FR-009).

## Stability rules

- From M02 close: types re-exported above + their M02 variants/fields are stable.
- Additive changes are non-breaking iff downstream consumers used exhaustive matches.
- Behavioral changes (e.g. flipping shadowing to error) are breaking even if signatures don't change. M02 commits to permissive shadowing.
