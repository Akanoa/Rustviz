# Data Model — M07.6 Entities

Trait system additions: **1 new AST Item** (`Item::Trait`), **1 new AST struct** (`TraitDecl`), **1 new AST enum** (`TraitItem`), **1 AST extension** (`ImplBlock.trait_name: Option<String>`), **1 AST field promotion** (`TypeParam.bound: Option<String>` → `bounds: Vec<String>`), **2 typeck-side registries** (`TraitRegistry`, `TraitImplRegistry`), **2 eval-side lookup tables** (`trait_default_bodies`, `trait_impl_bodies`). No new `Ty`/`Value`/`Pointee` variants; no new `MemEvent` variants.

## New (AST): `Item::Trait`

```rust
// In src/parse/ast.rs

pub enum Item {
    Fn(FnDecl),
    Struct(StructDecl),
    Impl(ImplBlock),
    /// **M07.6**: trait declaration `trait Name { fn item1; fn item2 { ... }; }`.
    /// Items are either required (signature-only, no body) or default (with body).
    Trait(TraitDecl),
}

pub struct TraitDecl {
    /// Trait name (e.g. `"Show"`).
    pub name: String,
    /// Trait items in declaration order.
    pub items: Vec<TraitItem>,
    /// Span from `trait` keyword through closing `}`.
    pub span: Span,
}

pub enum TraitItem {
    /// **M07.6**: required method — signature only, no body. Impl-side
    /// must provide the implementation.
    Required {
        /// Method name.
        name: String,
        /// Param list (first param is the self-receiver per ParamKind).
        params: Vec<Param>,
        /// Optional return type annotation.
        return_ty: Option<Type>,
        /// Span from `fn` keyword through `;`.
        span: Span,
    },
    /// **M07.6**: default method — has a body. Impl-side can override
    /// or fall through to this default.
    Default {
        /// Full FnDecl (params + return + body).
        decl: FnDecl,
    },
}
```

### Validation rules

- **VR-1**: trait name unique within the program (typeck rejects duplicates).
- **VR-2**: trait item names unique within the trait.
- **VR-3**: trait items must have a self-receiver as the first param (matches M07.4's method requirement).
- **VR-4**: no generic methods (no method-level `<U>`) — matches M07.5's restriction.
- **VR-5**: no `Self` return type — matches M07.6's no-Self restriction.

## Modified (AST): `ImplBlock` adds `trait_name`

```rust
pub struct ImplBlock {
    pub ty_name: String,
    /// **M07.6**: `None` = inherent impl (M07.4 behavior); `Some(name)` =
    /// trait impl `impl <name> for <ty_name>`.
    pub trait_name: Option<String>,
    pub items: Vec<FnDecl>,
    pub span: Span,
}
```

### Validation rules

- **VR-6**: when `trait_name: Some(_)`, the named trait must exist (typeck phase 1).
- **VR-7**: when `trait_name: Some(_)`, every fn in `items` must be a method on the named trait (reject "extra method").
- **VR-8**: when `trait_name: Some(_)`, every REQUIRED method of the named trait must be in `items` (reject "missing required method"; default methods are optional).
- **VR-9**: a `(trait_name, ty_name)` pair must be unique program-wide (reject duplicate trait impls).

## Modified (AST): `TypeParam.bound` promoted to `bounds: Vec<String>`

```rust
pub struct TypeParam {
    pub name: String,
    /// **M07.5 → M07.6**: was `bound: Option<String>` (M07.5: parser-stored,
    /// typeck-rejected); now `bounds: Vec<String>` (M07.6: multi-bound
    /// supported, typeck-checked).
    pub bounds: Vec<String>,
    pub span: Span,
}
```

### Validation rules

- **VR-10**: each bound name must refer to an existing trait (typeck phase 1 verifies; reject "unknown trait `<name>` in bound").
- **VR-11**: bounds can be empty (matches M07.5 no-bound case — equivalent to "no constraints; body can only do generic-safe ops").
- **VR-12**: duplicate bounds (`T: Show + Show`) — accept (no-op); not worth a special error.

## New (typeck registries): `TraitRegistry` + `TraitImplRegistry`

```rust
// In src/typeck.rs — built during phase 1; consumed during phase 2 + eval.

pub struct TraitRegistry {
    pub schemas: IndexMap<String, TraitSchema>,
}

pub struct TraitSchema {
    /// Required methods — signature only; the impl must provide them.
    pub required_methods: IndexMap<String, FnSig>,
    /// Default methods — body provided by the trait; impl can override
    /// or fall through. Stored as FnDecl reference for body re-walk at
    /// dispatch time.
    pub default_methods: IndexMap<String, FnSig>,
}

pub struct TraitImplRegistry {
    pub impls: IndexMap<(String, String), TraitImpl>,
}

pub struct TraitImpl {
    /// Methods that this impl provides (signature info only at typeck;
    /// eval-side uses a separate FnDecl-reference table to get bodies).
    pub overrides: IndexMap<String, FnSig>,
}
```

### Validation rules

- **VR-13**: `TraitRegistry.schemas` built in phase 1 by walking `Item::Trait` decls. Reject duplicates.
- **VR-14**: `TraitImplRegistry.impls` built in phase 1 by walking `Item::Impl { trait_name: Some(_), .. }`. Reject duplicates per VR-9.
- **VR-15**: phase 2 dispatch consults both registries: `overrides` first, then `TraitRegistry.schemas[trait].default_methods` as fall-through for default methods not overridden.

## New (eval-side): `trait_default_bodies` + `trait_impl_bodies` lookup tables

```rust
// In src/eval.rs — populated at Evaluator::new alongside fn_decls/methods/assoc_fns.

struct Evaluator<'a> {
    // ... existing fields
    /// **M07.6**: trait default-method bodies. Key: (trait_name, method_name).
    /// Used when a trait-method dispatch falls through to the default
    /// (impl didn't override).
    trait_default_bodies: HashMap<(String, String), &'a ast::FnDecl>,
    /// **M07.6**: trait-impl method bodies. Key: (trait_name, type_name, method_name).
    /// Used when a trait-method dispatch resolves to an explicit impl override.
    trait_impl_bodies: HashMap<(String, String, String), &'a ast::FnDecl>,
}
```

### Validation rules

- **VR-16**: `trait_default_bodies` populated by walking `Item::Trait` items and stashing `Default { decl }` references.
- **VR-17**: `trait_impl_bodies` populated by walking `Item::Impl { trait_name: Some(_), items }` and stashing each item's FnDecl reference.
- **VR-18**: lookup order at dispatch time mirrors typeck: `trait_impl_bodies[(trait, type, method)]` first; fall through to `trait_default_bodies[(trait, method)]`.

## Mangled frame name (eval-side derivation)

```rust
// Built inline at the trait-method-dispatch eval call site.
fn mangle_trait_method(type_name: &str, trait_name: &str, method_name: &str) -> String {
    format!("<{type_name} as {trait_name}>::{method_name}")
}
```

### Validation rules

- **VR-19**: Format: `<Point as Show>::show` (UFCS-style; Rust-standard).
- **VR-20**: Distinct from inherent dispatch (`Point::show`) — the `<as>` marker visually distinguishes trait dispatch.
- **VR-21**: For generic-fn-body dispatch (`fn print<T: Show>(x: T) { x.show() }` called as `print(p)`), the inner frame uses the substituted concrete type: `<Point as Show>::show`, not `<T as Show>::show`.

## New: M07.6 reference samples

| File | Content | Pedagogy |
|---|---|---|
| `tests/samples/m07_6_trait_basic.rs` | `trait Show { fn show(&self) -> i32; } impl Show for Point { fn show(&self) -> i32 { self.x } } let s = p.show();` | Trait decl + impl + dispatch; frame labeled `<Point as Show>::show`. |
| `web/samples/m07_6_trait_basic.rs` | Mirror. | |
| `tests/samples/m07_6_default_method.rs` | `trait Counter { fn count(&self) -> i32; fn double(&self) -> i32 { self.count() * 2 } } impl Counter for Point { fn count(&self) -> i32 { self.x } } let v = p.double();` | Default method routes to trait body; nested dispatch to impl's override. |
| `web/samples/m07_6_default_method.rs` | Mirror. | |
| `tests/samples/m07_6_generic_bound.rs` | `trait Show { fn show(&self) -> i32; } impl Show for Point { fn show(&self) -> i32 { self.x } } fn print<T: Show>(x: T) -> i32 { x.show() } let r = print(p);` | Generic bound — `fn print<T: Show>(x: T) { x.show() }`; the bound proves the method exists. |
| `web/samples/m07_6_generic_bound.rs` | Mirror. | |
| `tests/samples/m07_6_multi_bound.rs` | `... fn show_n_count<T: Show + Counter>(x: T) -> i32 { x.show() + x.count() } let r = show_n_count(p);` | Multi-bound — both Show and Counter active inside the body. |
| `web/samples/m07_6_multi_bound.rs` | Mirror. | |
