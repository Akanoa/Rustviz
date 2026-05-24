# Research — M07.5 Implementation Decisions

14 decisions across parser, AST, typeck (substitution + inference), eval (mangled fn names), UI (type-label rendering), and protocol amendment.

## Parser

### R-001 — Type-param list `<T>` parsed in fn + struct decls

- **Decision**: in `parse_fn_decl`, after the fn name, if `<` follows, parse comma-separated `TypeParam`s until `>`. Each `TypeParam` is just `Ident` (no bounds, no defaults). Same in `parse_struct_decl` after the struct name.
- **Multi-param + bound syntax**: parser accepts `<T, U>` and `<T: Foo>` to keep the parser permissive; typeck rejects with the M07.5-specific message. Trade-off: clearer M07.5 error messages (typeck mentions M07.6 for bounds; cites the single-param restriction for `<T, U>`) vs the simpler "reject at parse" approach (which loses the M07.5-vs-M07.6 framing).
- **Rationale**: matches Rust's grammar; the rejections are pedagogical.

### R-002 — Turbofish `id::<bool>(false)` parsed via `Path` + `::<` lookahead

- **Decision**: `parse_atom` recognizes `Ident :: Ident` as `Expr::Path { segments, type_args: [] }` (existing M07 path). M07.5 extends: after the last segment, if `ColonColon` followed by `Lt` follows, parse comma-separated type-args until `Gt`. Result: `Expr::Path { segments, type_args: Vec<Type> }`.
- **Disambiguation from comparison**: `id::<bool>` requires `ColonColon` before `Lt`. Plain `expr < other` (comparison) has no `ColonColon` before `Lt`. No conflict.
- **Rationale**: Rust-standard turbofish syntax; the `::<` two-token sequence is the disambiguator.

### R-003 — Generic-struct-literal `Wrapper::<i32> { v: 5 }` parsed via path + struct-lit lookahead

- **Decision**: same `Expr::Path` parsing as R-002 (`Wrapper::<i32>`). After parsing the path-with-type-args, if `LBrace` follows (and we're not in cond-position per M07.4's restriction), consume it as a `StructLit` body. The result is `Expr::StructLit { path, fields, type_args, span }`.
- **Single-segment turbofish on struct lit**: `Wrapper { v: 5 }` (no turbofish) still works via the existing M07.4 ident-then-brace path; inference fills T from the field value.
- **Rationale**: matches Rust grammar; the disambiguator is "what follows the closing `>`": `(` → call, `{` → struct lit.

### R-004 — Type annotations `Wrapper<i32>` in let-binding annotations

- **Decision**: in `parse_type`, after the path identifier(s), if `<` follows (and we're at a type-context position — distinguishable from the expression `<` because the parser is inside `parse_type`), parse comma-separated type-args until `>`. Result: `Type::Path { segments, type_args: Vec<Type> }`.
- **Trade-off vs separate `Type::Generic`**: M07.4 already added `Type::Generic { segments, args, span }` for `Box<T>` / `Vec<T>`. M07.5 has a choice: (a) reuse `Type::Generic` for user-defined generics like `Wrapper<i32>`, OR (b) merge `Type::Path` and `Type::Generic` by adding `type_args: Vec<Type>` to `Type::Path`. **Recommendation: option (b)** — fewer AST variants, the empty-`type_args` case stays equivalent to today's `Type::Path`.
- **Rationale**: option (b) collapses `Type::Generic` into `Type::Path`'s extension. Migration: typeck's `ty_from_ast` for `Type::Generic` arm moves into the `Type::Path` arm with `type_args.len() > 0`.
- **Decision: defer the Type::Generic-vs-extend decision to implementation**. Plan-phase recommends extend-Path; if it breaks too many call sites, fall back to keeping `Type::Generic` and adding turbofish-style usage there. Either works.

## AST

### R-005 — `TypeParam { name: String, span: Span }`

- **Decision**: minimal shape. Order significant in the `Vec<TypeParam>` (positional substitution). M07.5 enforces single-element (typeck-rejects multi-element) but the Vec shape is built for future expansion.
- **Rationale**: matches the precedent of `StructField` and `Param`.

### R-006 — `FnDecl` + `StructDecl` extension

- **Decision**: add `type_params: Vec<TypeParam>` field to both. Default-empty for non-generic decls. `#[serde(default, skip_serializing_if = "Vec::is_empty")]` keeps existing AST snapshots byte-identical (for non-generic decls).
- **Rationale**: precedent from M07.4's `field_path` extension on `Value::Ref`.

### R-007 — `Expr::Path` + `Expr::StructLit` + `Type::Path` extensions

- **Decision**: all three gain `type_args: Vec<Type>` with serde-default-empty. Existing expressions / types without turbofish stay byte-identical on the wire.
- **Rationale**: same precedent as R-006.

## Typeck

### R-008 — `Ty::Param(String)` for unresolved type parameters

- **Decision**: new `Ty` variant carrying the param's name. Inside a generic fn's body, `T` resolves to `Ty::Param("T")`. At call sites, the substitution map replaces `Ty::Param("T")` with the concrete type (e.g. `Ty::Int(I32)`).
- **`Ty::name()`**: returns the bare param name (`"T"`).
- **`Ty::is_copy()`**: returns `false` for `Ty::Param(_)` — without bounds (M07.6), we can't assume Copy. In M07.5 this is OK because substituted types at call sites are concrete and have their own `is_copy()` answer; `Ty::Param` only appears inside the generic body's signature, never as a final binding type.
- **Rationale**: simplest representation; `String`-keyed substitution map is fine for single-param scope.

### R-009 — `Ty::Struct` extension with `type_args: Vec<Ty>`

- **Decision**: add `type_args: Vec<Ty>` field to `Ty::Struct`. Empty for non-generic structs (M07.4 case stays byte-identical). For `Wrapper<i32>`, `type_args = [Ty::Int(I32)]`.
- **`Ty::name()`**: returns `"Point"` when `type_args.is_empty()`; returns `"Wrapper<i32>"` (or `"Wrapper<i32, bool>"` etc.) when non-empty. Renders via `format!("{}<{}>", name, type_args.iter().map(|t| t.name()).collect::<Vec<_>>().join(", "))`.
- **Equality**: nominal by name + type_args (`Wrapper<i32>` ≠ `Wrapper<bool>`).
- **Rationale**: parallel to M07.4's `Value::Ref` field-path extension — additive field, serde-default-empty, no impact on existing snapshots.

### R-010 — Substitution stack `Typechecker.subst: Vec<HashMap<String, Ty>>`

- **Decision**: a stack of substitution maps. Pushed at a generic-fn body-typecheck entry (built from the call site's inferred or turbofish substitution); popped at body exit. M07.5 max depth = 1 (no nested generics); the stack abstraction supports future lifting.
- **`apply_subst(&self, ty: &Ty) -> Ty`**: recursively walks `ty`. For `Ty::Param(name)`, looks up in the TOP of the subst stack (innermost binding); returns the concrete type or panics if not bound (shouldn't happen if typeck pushed correctly). For `Ty::Struct { type_args, .. }`, recurses on `type_args` to substitute T placeholders. For other variants, no-op or recurse on inner types as needed.
- **Rationale**: minimal scoped-substitution; matches Rust's actual substitution semantics for a single level.

### R-011 — Inference: direct match on first generic-typed param

- **Decision**: at a call site, walk param types in declaration order. For the first `Ty::Param("T")`, take `T = arg_ty` (the arg at the corresponding index, post-typecheck). For subsequent `Ty::Param("T")`, verify `arg_ty == T_inferred`; mismatch → error "cannot infer T from conflicting args: <ty> vs <ty>". If `T` doesn't appear in any param's type, error "cannot infer T from arguments; add `::<...>` turbofish annotation" — with the call-site span.
- **No HM / unification / variance**: keep it simple. Rejects some Rust programs (e.g. `let x: i32 = id();` where T comes from the annotation, not args) that full HM would accept; the workaround is turbofish.
- **Rationale**: matches the M07.5 sample scope (id-fn called with concrete arg; wrapper struct with concrete field value; turbofish for the explicit case).

### R-012 — Inference for generic-struct literals

- **Decision**: at a struct literal `Wrapper { v: <expr> }` where Wrapper has a type-param T whose field uses T (i.e. `v: T`), infer T from the corresponding field's value type. Multi-field-same-T cases work analogously. If T appears in multiple fields and they conflict → error.
- **With explicit annotation `let w: Wrapper<i32> = Wrapper { v: true };`**: substitution comes from the annotation; field-value mismatches surface as the standard "expected i32, found bool" error.
- **Rationale**: extends R-011 to struct literals.

## Eval

### R-013 — Mangled fn name via `call_decl(decl, display_name, args, span)`

- **Decision**: typeck records the substitution per call-site span in a new `TypeMap.call_substs: IndexMap<Span, Vec<(String, Ty)>>` side table. Eval looks up `call_substs.get(call_span)` to build the mangled name: `format!("{}::<{}>", source_name, types.iter().map(|t| t.name()).collect::<Vec<_>>().join(", "))`. Pass as `display_name` to `call_decl` (already added in M07.4 for method dispatch).
- **Pre-substituted bindings**: typeck's `binding_types` for the fn's params already carries the SUBSTITUTED concrete types (via `apply_subst` during phase-2 body typecheck). Eval reads these directly; no substitution work needed at eval time.
- **`Ty::Param` at eval time**: unreachable. Defensive: ui's `render_ty(Ty::Param(name))` returns `"<{name}>"` as a fallback (highlights leaks during development without crashing).
- **Rationale**: minimal eval-side change. Mangled-name machinery exists from M07.4; M07.5 adds the substitution-record-and-lookup glue.

### R-014 — Call-site span as the substitution registry key

- **Decision**: keyed by the `Expr::Call` (or `Expr::MethodCall`) span. Identical spans = identical call-site = identical substitution (already true since each call-site span is unique by source position).
- **Rationale**: stable, deterministic, no extra IDs needed.

## UI

### R-015 — `Ty::name()` extension drives type-label rendering automatically

- **Decision**: `SlotRowView.ty` is built from `render_ty(ty)` which calls `Ty::name()`. M07.5 just extends `Ty::name()` for `Struct { type_args }` to render `"Wrapper<i32>"`. No JS changes, no new UI fields, no new CSS.
- **Rationale**: the existing typed-label rendering path is exactly the right place for monomorphization-aware display.

## Protocol

### R-016 — 9th invocation of the closed-enum-with-revisions rule

- **Decision**: amend M03's contract to note M07.5 as the 9th invocation. M07.5's changes:
  - **Additive variant on `Ty`**: `Param(String)`.
  - **Additive extension of existing `Ty::Struct`**: `type_args: Vec<Ty>` field with serde-default-empty.
  - **No new MemEvent variants**. `FrameEnter` carries the mangled fn name (`id::<i32>`) — no event-shape change.
  - **No new Pointee variants**, no new Value variants.
- **Precedent chain**: M03.1 → M03.2 → M06 → M07 → M07.1 → M07.2 → M07.3 → M07.4 → **M07.5**.
- **Snapshot byte-identity**: M01 may re-baseline once for `type_params` Vec on FnDecl/StructDecl (depends on whether the serializer renders the empty Vec or omits it — Debug format with `Vec::new()` may render `type_params: []` even with serde skip). M02 and M03 snapshots stay byte-identical because (a) no existing sample constructs `Ty::Param`, (b) `Ty::Struct.type_args` serde-default-empty omits the field for non-generic structs.
