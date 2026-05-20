# Quickstart — M02 development

Audience: maintainer + contributors working inside M02 or consuming its output from M03+.

## Run the M02 test suite

```bash
cargo test --test m02
```

M01 tests remain unchanged and must continue to pass (SC-008):

```bash
cargo test --test m01     # must still pass
cargo test                # runs both m01 and m02 + any unit tests
```

Snapshot review uses `cargo insta review` (same workflow as M01).

## Use the M02 API in code

```rust
use rustviz::{parse, resolve, typeck, SourceMap};

let mut sm = SourceMap::new();
let file = sm.add("input.rs".into(), src);

let program = parse(file, &sm)?;
let resolution = resolve(&program)?;
let types = typeck(&program, &resolution)?;

// resolution.uses: Span → BindingId
// resolution.bindings: BindingId → BindingDecl
// types.expr_types: Span → Ty
// types.binding_types: BindingId → BindingType
```

Errors at any stage are `ParseError` — same shape as M01, integrate with the same `Display` formatting.

## Add a new test

1. Create `tests/samples/m02_<name>.rs` with the input program.
2. Add a `sample_test!(<test_fn_name>, "m02_<name>")` line in `tests/m02.rs`.
3. Run `cargo test --test m02`. First run creates `tests/snapshots/m02_*.snap.new`.
4. `cargo insta review` (or `INSTA_UPDATE=always cargo test --test m02` for non-interactive accept).
5. Visually inspect the snapshot:
   - `resolution.uses` should map every `Ident` span to a `BindingId`.
   - `resolution.bindings` should list each introduced binding with name, kind, and span.
   - `types.expr_types` should map every value-producing expression to a `Ty`.
   - `types.binding_types` should map each `BindingId` to either `Var(Ty)` or `Fn(FnSig)`.

## Debug a resolve/typeck failure

```rust
let mut sm = SourceMap::new();
let file = sm.add("debug.rs".into(), src.to_owned());
let program = parse(file, &sm).unwrap();
match resolve(&program) {
    Ok(r) => {
        println!("resolution: {r:#?}");
        match typeck(&program, &r) {
            Ok(t) => println!("types: {t:#?}"),
            Err(e) => {
                let (line, col) = sm.line_col(e.span).unwrap_or((0, 0));
                eprintln!("typeck error at {line}:{col}: {}", e.message);
            }
        }
    }
    Err(e) => {
        let (line, col) = sm.line_col(e.span).unwrap_or((0, 0));
        eprintln!("resolve error at {line}:{col}: {}", e.message);
    }
}
```

## What M02 accepts (in 30 seconds)

Same syntactic surface as M01 plus the analysis-level rules:

- Forward references between top-level fn items.
- Let-bindings strictly before-use, with shadowing (each new `let` creates a new `BindingId`).
- Lexical block scoping with arbitrary nesting.
- Type annotations validated against initializer / body types.
- Operators typed per Rust's standard rules (numeric `+ - * / %` require `i32`; comparison `< <= > >=` require `i32`, return `bool`; equality `== !=` require matching operands, return `bool`; logical `&& ||` require `bool`; unary `-` on `i32`, `!` on `bool`).
- `if` as expression requires else and matching branch types. `if` as statement (no else, body has unit type) returns `Unit`.

## What M02 explicitly rejects

- `use of undeclared variable z` → resolve error.
- Duplicate fn parameter names → resolve error.
- Annotation/initializer mismatch (`let x: bool = 5;`) → typeck error.
- Operator type mismatch (`5 + true`) → typeck error.
- Non-bool `if` condition → typeck error.
- Mismatched `if` branches → typeck error.
- Wrong return type → typeck error.
- Non-Ident callee in a call expression → typeck error.

## Implementer notes (internal)

When extending in M06+:

- Adding new `Ty` variants (e.g. `Ref { mutable: bool, target: Box<Ty> }` for M06): exhaustive matches in `src/typeck.rs` will surface every site that needs updating.
- Adding new `BindingKind` variants: same story — match the new kind in `src/resolve.rs` when categorizing a binding.
- The resolver doesn't need changes for new types — type and binding-kind concerns are orthogonal.

## LOC and warnings checks (M02 equivalents of M01's SC-005/006)

```bash
# Stay under the soft cap (1500 LOC for resolve + typeck combined):
find src/resolve.rs src/typeck.rs -name '*.rs' -print0 | xargs -0 wc -l

# Zero warnings:
RUSTFLAGS="-D warnings" cargo build --release
RUSTFLAGS="-D warnings" cargo test --test m02
```

If either file is approaching 600 LOC alone, consider promoting it to `resolve.rs` + `resolve/` (M01 module pattern). Otherwise keep flat.

## Insta basics

Same as M01:
- Snapshots committed to git.
- `cargo insta review` for interactive accept.
- `INSTA_UPDATE=always` env var for non-interactive accept (used by AI implementer; humans review interactively).
