# Data Model — M07.5 Entities

Generic-parameter surface: **1 new AST node** (`TypeParam`), **2 AST extensions** (`FnDecl.type_params`, `StructDecl.type_params`), **3 expr/type extensions** (`Type::Path.type_args`, `Expr::Path.type_args`, `Expr::StructLit.type_args`), **1 new `Ty` variant** (`Ty::Param(String)`), **1 `Ty::Struct` extension** (`type_args: Vec<Ty>`), **1 typeck-side side table** (`TypeMap.call_substs: IndexMap<Span, Vec<(String, Ty)>>`).

All variant-level changes are additive at the JSON layer; the field extensions use serde-default-empty so existing M02/M03+ snapshots stay byte-identical.

## New (AST): `TypeParam`

```rust
// In src/parse/ast.rs

pub struct TypeParam {
    /// Type parameter name (e.g. `"T"`, `"U"`).
    pub name: String,
    /// Span covering the name.
    pub span: Span,
}
```

### Validation rules

- **VR-1**: M07.5 enforces single-element `type_params` lists at typeck (parser accepts any count for clearer error messages).
- **VR-2**: Type-param names accept any identifier; convention is single-letter (T, U, V) but `fn id<MyType>(x: MyType)` parses.
- **VR-3**: Type-param names within one decl must be unique (`fn foo<T, T>(...)` rejected at typeck/parser).

## Modified (AST): `FnDecl` + `StructDecl` add `type_params`

```rust
pub struct FnDecl {
    pub name: String,
    /// **M07.5**: type parameters declared on the fn. Empty for non-generic
    /// fns; serde-default-empty keeps existing AST snapshots byte-identical.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_params: Vec<TypeParam>,
    pub params: Vec<Param>,
    pub return_ty: Option<Type>,
    pub body: Block,
    pub span: Span,
}

pub struct StructDecl {
    pub name: String,
    /// **M07.5**: type parameters declared on the struct. Empty for
    /// non-generic structs; serde-default-empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_params: Vec<TypeParam>,
    pub fields: Vec<StructField>,
    pub span: Span,
}
```

### Validation rules

- **VR-4**: typeck-side rejection of multi-element `type_params` cites the M07.5 single-param restriction.
- **VR-5**: AST snapshots for non-generic decls stay byte-identical (Vec::is_empty skip).

## Modified (AST): `Type::Path` + `Expr::Path` + `Expr::StructLit` add `type_args`

```rust
pub enum Type {
    Path {
        segments: Vec<String>,
        /// **M07.5**: type-args list for `Wrapper<i32>` annotations. Empty
        /// for plain `i32` / `bool` etc. (serde-default-empty).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        type_args: Vec<Type>,
        span: Span,
    },
    // ... existing variants
}

pub enum Expr {
    Path {
        segments: Vec<String>,
        /// **M07.5**: turbofish type-args for `id::<bool>` / `Wrapper::<i32>`.
        /// Empty for non-turbofish paths (e.g. `Vec::new`); serde-default-empty.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        type_args: Vec<Type>,
        span: Span,
    },
    StructLit {
        path: Vec<String>,
        fields: Vec<StructLitField>,
        /// **M07.5**: turbofish type-args on the path (`Wrapper::<i32> { v: 5 }`).
        /// Empty for non-turbofish literals (the inferred case). Serde-default-empty.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        type_args: Vec<Type>,
        span: Span,
    },
    // ... existing variants
}
```

### Validation rules

- **VR-6**: `Type::Path` arms that previously matched `Type::Generic` (Box<T>, Vec<T>) migrate to this unified shape; `Type::Generic` may be retired in the migration (plan-phase decides: keep both for back-compat or fully merge).
- **VR-7**: Empty `type_args` on `Expr::Path` is the existing M07 path shape (`Vec::new`, `Point::new`).
- **VR-8**: Existing snapshots stay byte-identical (serde-default-empty skip).

## Modified (typeck): `Ty` adds `Param` variant + `Struct.type_args` field

```rust
pub enum Ty {
    Int(IntKind),
    Float(FloatKind),
    Bool,
    Unit,
    Ref { inner: Box<Ty>, mutable: bool },
    Box(Box<Ty>),
    Vec(Box<Ty>),
    String,
    Slice(Box<Ty>),
    Str,
    Array(Box<Ty>, u64),
    Struct {
        name: String,
        fields: Vec<(String, Ty)>,
        /// **M07.5**: type-args for generic instantiations. Empty for
        /// non-generic structs (M07.4 case); serde-default-empty.
        /// Non-empty for `Wrapper<i32>` etc. — drives `Ty::name()` to
        /// render `"Wrapper<i32>"` and drives nominal equality
        /// (`Wrapper<i32>` ≠ `Wrapper<bool>`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        type_args: Vec<Ty>,
    },
    // NEW in M07.5:
    /// **M07.5**: type parameter (unresolved). Carries the param's name
    /// (`"T"`) so error messages can reference it. Substituted at call
    /// sites with concrete types via `apply_subst`. Should be unreachable
    /// at eval time (typeck substitutes before any binding_types entry is
    /// recorded).
    Param(String),
}
```

### Validation rules

- **VR-9**: `Ty::Param(_)` `is_copy()` returns `false` (no bounds in M07.5 means no assumed Copy).
- **VR-10**: `Ty::name()` for `Param("T")` returns `"T"`.
- **VR-11**: `Ty::Struct { name, fields, type_args }` equality is by (name, type_args) — fields are derived from the schema, redundant for identity (nominal-with-instantiation typing).
- **VR-12**: `Ty::name()` for `Struct { name, type_args: [] }` returns `"Point"`; for non-empty type_args renders `"Wrapper<i32>"`.

## New (typeck side table): `TypeMap.call_substs`

```rust
pub struct TypeMap {
    pub expr_types: IndexMap<Span, Ty>,
    pub binding_types: IndexMap<BindingId, BindingType>,
    /// **M07.5**: per-call-site substitution recorded by typeck. Eval reads
    /// this to build the mangled `FrameEnter.fn_name`. Empty for non-generic
    /// calls; one entry per generic-fn / generic-method / generic-assoc-fn
    /// call site.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub call_substs: IndexMap<Span, Vec<(String, Ty)>>,
}
```

### Validation rules

- **VR-13**: `call_substs[call_span]` is `Some(Vec)` iff the resolved fn has type params (the Vec has one entry per type-param in declaration order).
- **VR-14**: M07.5 max-entry-count is 1 (single type-param restriction). Multi-param entries permitted by shape; rejected by typeck.

## Mangled fn name (eval-side derivation)

```rust
// Built inline at the eval call site, fed into call_decl's `display_name`.
fn mangle_call(source_name: &str, subst: &[(String, Ty)]) -> String {
    if subst.is_empty() {
        source_name.to_owned()
    } else {
        let arg_str = subst
            .iter()
            .map(|(_, t)| t.name())
            .collect::<Vec<_>>()
            .join(", ");
        format!("{source_name}::<{arg_str}>")
    }
}
```

### Validation rules

- **VR-15**: Format matches Rust's standard `id::<i32>` rendering.
- **VR-16**: Multi-substitution (future) would render `pair::<i32, bool>` — established here as the format but only exercised with single-T subs in M07.5.

## New: M07.5 reference samples

| File | Content | Pedagogy |
|---|---|---|
| `tests/samples/m07_5_id_fn.rs` | `fn id<T>(x: T) -> T { x } fn main() { let a = id(5); let b = id(true); }` | Two distinct frames — `id::<i32>` and `id::<bool>` — visible in the trace; monomorphization made concrete. |
| `web/samples/m07_5_id_fn.rs` | Mirror. | |
| `tests/samples/m07_5_generic_struct.rs` | `struct Wrapper<T> { v: T } fn main() { let w = Wrapper { v: 5 }; let a = w.v; }` | `w : Wrapper<i32>` in the slot's type label; field access works like non-generic case. |
| `web/samples/m07_5_generic_struct.rs` | Mirror. | |
| `tests/samples/m07_5_turbofish.rs` | `fn id<T>(x: T) -> T { x } fn main() { let v = id::<bool>(false); }` | Turbofish forces `T = bool`; frame labeled `id::<bool>`. |
| `web/samples/m07_5_turbofish.rs` | Mirror. | |
