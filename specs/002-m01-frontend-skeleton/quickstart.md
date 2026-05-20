# Quickstart — M01 development

Audience: maintainer + future contributors working inside M01 or extending it in M02+.

## Run the M01 test suite

```bash
cargo test --test m01
```

All snapshots under `tests/snapshots/m01_*` must pass. If a snapshot fails:

1. Look at the diff `insta` prints.
2. If the change is intentional (you changed the parser and the new output is correct), run `cargo insta review` to accept it interactively.
3. If the change is unintentional (regression), fix the parser.

## Add a new test

1. Create `tests/samples/m01_<name>.rs` with the input program. Plain Rust source; doesn't need to be runnable.
2. Add an entry to the table in `tests/m01.rs` (the driver) referencing the sample. The driver loops over the table and asserts a snapshot per sample.
3. Run `cargo test --test m01`. The first run will create an unreviewed snapshot at `tests/snapshots/m01_<name>.snap.new`.
4. Run `cargo insta review`. Accept the snapshot if it matches expectations.
5. Commit both the sample and the snapshot.

## Debug a parse failure

When `parse()` returns `Err(ParseError)`:

```rust
let mut sm = SourceMap::new();
let file = sm.add("debug.rs".into(), src.to_owned());
match rustviz::parse(file, &sm) {
    Ok(p) => println!("{p:#?}"),
    Err(e) => {
        let (line, col) = sm.line_col(e.span).unwrap_or((0, 0));
        eprintln!("{}:{}: {}", line, col, e.message);
    }
}
```

The `Display` impl on `ParseError` does the same thing if you pass a `SourceMap` in; for one-off scripts, the snippet above is fine.

## What M01 accepts (in 30 seconds)

- `fn name(args) -> ret { ... }` — function decls.
- `let x = expr;` / `let mut x: T = expr;` — bindings (init required).
- `expr;` — expression statements.
- `{ stmts; tail_expr }` — blocks with optional tail expression.
- `if cond { ... } else { ... }` — both branches; else optional unless used as expression.
- Operators: `+ - * / %`, `== != < <= > >=`, `&& || !`, unary `-`, parens.
- Calls: `f(a, b)`.
- Literals: integers (`i64`) and booleans.
- Identifiers: `[A-Za-z_][A-Za-z0-9_]*`.
- Types: path-like (`i32`, `bool`) or unit (`()`).
- Line comments `//` and whitespace.

## What M01 explicitly rejects

- `&` or `&mut` anywhere → lexer error mentioning Level 2 (R-014).
- Anything else not in the L1 grammar → parser error at the offending token.

## What to do when extending in M06 / M07 / M08

When you arrive in a later milestone and need to extend the AST:

1. **Don't** remove variants. Add new ones.
2. **Don't** rename fields. Add new ones with defaults.
3. **Do** update `contracts/parse-api.md` (when M06+ adds the new variants — record them in that file's "later milestones" notes).
4. **Do** add tests for the new syntax in a `tests/m0X.rs` driver and `tests/samples/m0X_*.rs`. Don't pile L2+ tests into `tests/m01.rs`.

## Where the LOC cap kicks in

M01 has a soft 2000-LOC cap on `src/parse/` (SC-005). If you hit it:

1. Stop and look at what's growing. Is it the parser (probably) or something that should be M02 (probably scope creep)?
2. If it's actually parser code for L1, the cap was wrong — bump it and note why.
3. If it's M02-flavored code, stop and reassign to M02.

## Insta basics (for first-time users)

- Snapshots live under `tests/snapshots/<test_name>__<snapshot_name>.snap`.
- `cargo insta review` walks unreviewed snapshots interactively (accept / reject / skip).
- `cargo insta accept` accepts all pending snapshots non-interactively. Use sparingly.
- `cargo insta pending-snapshots` lists pending without running tests.
- Snapshots are committed to git — they ARE the expected behavior.

## Running the build cleanly

```bash
RUSTFLAGS="-D warnings" cargo build --release
RUSTFLAGS="-D warnings" cargo test --test m01
```

CI should run these. Locally, warnings show in `cargo build` output but don't fail it (R-003).
