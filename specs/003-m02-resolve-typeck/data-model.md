# Data Model — M02 Entities

M02 adds two new modules to the crate, each with a small set of public types. AST entities from M01 are reused unmodified.

## Entity: `BindingId`

```rust
pub struct BindingId(pub u32);
```

Newtype wrapping a 32-bit unique identifier. Allocated sequentially starting at 0 during the resolve pass. Stable across runs given identical input (deterministic source-order allocation).

### Validation rules

- **VR-1**: `BindingId`s are dense (no gaps) within a single `resolve()` call.
- **VR-2**: Each `BindingId` corresponds to exactly one `BindingDecl` in the `Resolution.bindings` map.
- **VR-3**: Shadowing produces NEW `BindingId`s (spec FR-005). A program with `let x = 5; let x = true;` has two distinct ids for the two `x` bindings.

## Entity: `BindingKind`

```rust
pub enum BindingKind {
    Fn,
    Let { mutable: bool },
    Param,
}
```

How a binding was introduced. Used for diagnostics and (later) by the M03 evaluator to know whether a binding is mutable.

## Entity: `BindingDecl`

```rust
pub struct BindingDecl {
    pub name: String,
    pub kind: BindingKind,
    pub decl_span: Span,  // span of the introducing form (e.g. the `let` stmt's span, the param's span)
    pub name_span: Span,  // span of the binding name itself (for diagnostics)
}
```

### Validation rules

- **VR-4**: `name` matches the source text covered by `name_span`.
- **VR-5**: `decl_span` covers `name_span` (i.e. `decl_span.start <= name_span.start <= name_span.end <= decl_span.end`).
- **VR-6**: For `BindingKind::Fn`, `decl_span` is the function declaration's span (from `fn` keyword through closing `}`). For `Let`, it's the `let` stmt span. For `Param`, it's the parameter's full span.

## Entity: `Resolution`

```rust
pub struct Resolution {
    pub uses: IndexMap<Span, BindingId>,
    pub bindings: IndexMap<BindingId, BindingDecl>,
}
```

Output of `resolve(&Program) -> Result<Resolution, ParseError>`.

### Validation rules

- **VR-7**: Every key in `uses` is the `Span` of an `Expr::Ident` node (or a callee `Expr::Ident` inside an `Expr::Call`). The value is the `BindingId` of the binding the use resolves to.
- **VR-8**: Every `BindingId` value in `uses` is a key of `bindings`.
- **VR-9**: `IndexMap` (not `HashMap`/`BTreeMap`) so iteration order is deterministic AND matches tree-walk insertion order — see research.md R-002. Snapshot output reads top-down like the source.

## Entity: `Ty`

```rust
pub enum Ty {
    I32,
    Bool,
    Unit,
}
```

The L1 value-type lattice. Three concrete types. `Ty: Copy + Clone + Debug + PartialEq + Eq + Hash`.

## Entity: `FnSig`

```rust
pub struct FnSig {
    pub params: Vec<Ty>,
    pub ret: Ty,
}
```

Function signature. Stored separately from `Ty` because functions are not first-class values in L1 (R-009).

## Entity: `BindingType`

```rust
pub enum BindingType {
    /// Binding holds a value of this type.
    Var(Ty),
    /// Binding is a function with this signature.
    Fn(FnSig),
}
```

Type information for a `BindingDecl`, computed by typeck after resolve has assigned the `BindingId`.

## Entity: `TypeMap`

```rust
pub struct TypeMap {
    pub expr_types: IndexMap<Span, Ty>,
    pub binding_types: IndexMap<BindingId, BindingType>,
}
```

Output of `typeck(&Program, &Resolution) -> Result<TypeMap, ParseError>`.

### Validation rules

- **VR-10**: For every `Expr` node in `program` that produces a value (i.e. not the callee `Ident` of a function), the node's `Span` MUST be a key of `expr_types` and the value is its inferred `Ty`.
- **VR-11**: A function-Ident in callee position (e.g. the `f` in `f(1, 2)`) does NOT get an `expr_types` entry. Its type is looked up via `Resolution.uses[span] → binding_types[binding_id]`.
- **VR-12**: Every `BindingId` in `Resolution.bindings` has a corresponding entry in `binding_types`.
- **VR-13**: `BindingType::Var(ty)` for let/param bindings; `BindingType::Fn(sig)` for fn bindings.
- **VR-14**: Operator typing rules are applied per spec FR-007 and research.md R-011.

## Reused: `ParseError` (from M01)

No new error type. Both passes return `Result<_, ParseError>` reusing M01's struct. Error messages follow research.md R-007 (resolve) and R-011 (typeck) catalogs.

## State transitions

No stateful entities. Resolution and TypeMap are produced once per `resolve()` / `typeck()` call and consumed read-only afterwards.

## Relationships

```
Program (M01) ──► resolve() ──► Resolution
                                  ├── uses: Span → BindingId
                                  └── bindings: BindingId → BindingDecl

(Program, Resolution) ──► typeck() ──► TypeMap
                                         ├── expr_types: Span → Ty
                                         └── binding_types: BindingId → BindingType
```

## Internal: `Resolver` (private)

```rust
struct Resolver {
    scopes: Vec<HashMap<String, BindingId>>, // innermost last
    next_id: u32,
    resolution: Resolution,
}
```

Not exported. Created at the start of `resolve()` and discarded when done.

## Internal: `Typechecker` (private)

```rust
struct Typechecker<'a> {
    program: &'a Program,
    resolution: &'a Resolution,
    types: TypeMap,
    current_fn_ret: Option<Ty>, // expected return type while checking a fn body
}
```

Not exported.
