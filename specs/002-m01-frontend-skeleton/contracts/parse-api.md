# Contract — Public Parse API + AST Shape

This contract defines what M01 exposes to its consumers (M02 resolver, M03 evaluator, future milestones). Once M01 closes, this contract is the surface other milestones rely on. Breaking it requires a coordinated change.

## Entry point

```rust
pub fn rustviz::parse(file: FileId, source_map: &SourceMap)
    -> Result<ast::Program, ParseError>;
```

- **Input**: a `FileId` previously registered in `source_map`. The function looks up the source text via `source_map.get(file)`.
- **Output**: on success, an owned `Program` AST whose every node carries a `Span` referencing `file`. On failure, a single `ParseError` (FR-006).
- **Idempotence**: parsing the same source twice yields equal (`PartialEq`) ASTs. Required for SC-004 deterministic snapshots.

## Re-exports from `lib.rs`

The crate root re-exports a stable surface:

```rust
pub use parse::{parse, FileId, SourceMap, Span, ParseError, ast};
```

Anything not re-exported is implementation detail and may change without notice.

## Public types (stable from M01 closure onward)

| Type             | Stability  | Notes                                                            |
|------------------|------------|------------------------------------------------------------------|
| `FileId`         | stable     | Newtype `u32`. Constructor is private; only `SourceMap` allocates.|
| `Span`           | stable     | `{ start: u32, end: u32, file: FileId }`. `Copy`.                |
| `SourceMap`      | stable     | API: `add`, `get`, `line_col`. Internal layout opaque.           |
| `ParseError`     | stable     | `{ message: String, span: Span }`.                               |
| `ast::Program`   | stable     | Root.                                                            |
| `ast::Item`      | stable for M01 variants | New variants may be added in later milestones (additive). |
| `ast::FnDecl`    | stable for M01 fields   | New fields may be added (additive).                       |
| `ast::Type`      | stable for M01 variants | New variants may be added in later levels.                |
| `ast::Block`     | stable     |                                                                  |
| `ast::Stmt`      | stable for M01 variants | New variants may be added.                                |
| `ast::Expr`      | stable for M01 variants | New variants will be added in M06 (borrow), M07 (heap), M08 (threads). |
| `ast::BinOp`     | stable for M01 variants | New variants in later milestones if needed.              |
| `ast::UnOp`      | stable for M01 variants |                                                          |

"Additive" means: new variants / new fields with sensible defaults can be added in later milestones without breaking M01 consumers that don't match those variants. Removing or renaming an M01-introduced item is a breaking change requiring a coordinated update.

## AST traversal expectations

Consumers (M02, M03) walk the AST via:
- Exhaustive `match` on `Expr` / `Stmt` / `Item` / `Type` — Rust's match-completeness will catch missed variants when later milestones add new ones. (Consumers should `match ... { ... }` without `_ =>` catch-all unless they have a meaningful default.)
- `node.span` available on every node for error reporting and pedagogical event emission (M03+).

## Grammar contract (informal)

The grammar M01 accepts:

```
program     := item*
item        := fn_decl
fn_decl     := "fn" IDENT "(" param_list? ")" ("->" type)? block
param_list  := param ("," param)* ","?
param       := IDENT ":" type
type        := path | "(" ")"
path        := IDENT ( "::" IDENT )*

block       := "{" stmt* expr? "}"
stmt        := let_stmt
             | expr_stmt
let_stmt    := "let" "mut"? IDENT (":" type)? "=" expr ";"
expr_stmt   := expr ";"

expr        := /* Pratt parser with the precedence table from data-model.md VR-12 */
             | atom
atom        := INT | BOOL | IDENT | "(" expr ")" | block | if_expr | call
if_expr     := "if" expr block ("else" block)?
call        := atom "(" arg_list? ")"     // call binds tighter than unary
arg_list    := expr ("," expr)* ","?

INT         := /-?[0-9]+/
BOOL        := "true" | "false"
IDENT       := /[A-Za-z_][A-Za-z0-9_]*/   (excluding keywords)
```

Whitespace and line comments (`// ...`) are stripped by the lexer between tokens. `&` and `&mut` are lexer-rejected.

## Error format contract

```
ParseError {
    message: <human-readable, single line, no trailing newline>,
    span: <byte range into the file that caused the error>,
}
```

- For lexer errors, `span` is the offending bytes (e.g. the `&` character).
- For parser errors, `span` is the unexpected token (or zero-length at EOF — VR-17).
- The reference-rejection message contains `"Level 2"` (VR-18) so consumers can pattern-match on it if needed.

A `Display` impl renders `"<message> at <line>:<col>"` when a `SourceMap` is available (line/col computed via `SourceMap::line_col`).

## What this contract does NOT cover (deferred to later milestones)

- `BindingId` / `Resolver` — added by M02.
- Type validation — M02 onward.
- Multiple errors per parse — currently impossible (FR-006); revisit if recovery lands (deferred per `MILESTONES.md` › Deferred).
- `&` / `&mut` tokens — added by M06.
- Arena allocation — possibly added by M02 if needed; M01 consumers should treat owned boxes as the API.
- Serialization / deserialization — not promised. AST types derive `serde::Serialize` only as a dev-dep for `insta` YAML snapshots; this is not a public commitment.

## Stability rules

- **From M01 close**: the types re-exported from `lib.rs` and their M01 variants/fields are stable. Renames or removals are breaking.
- **Additive changes** (new variants on `Expr`, new fields on `FnDecl`, new items) are non-breaking *if* consumers used exhaustive matches that the compiler will now flag.
- **Behavioral changes** (e.g. operator precedence) are breaking even if signatures don't change. M01 commits to Rust-standard precedence (VR-12); changing it requires coordinating with M02/M03 and updating snapshots.
