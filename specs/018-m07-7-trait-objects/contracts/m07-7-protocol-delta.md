# Contract — M07.7 Protocol Delta

**11th invocation** of the closed-enum-with-revisions rule. M07.7 adds AST surface + new `Ty` variants + new `Value` variants + a new `VtableAddr` addressing namespace + a new `MemEvent::VtableAlloc` event. First new MemEvent variant since M07.2 (which added `StaticAlloc` + `BytesCopy`). No new `Pointee` variants — vtables ride on dedicated fields.

## Closed-enum rule — eleventh invocation

| Milestone | Change |
|---|---|
| M03.1 | Added `MemEvent::ReturnValue`; removed `FrameEnter.params`. |
| M03.2 | Restructured `Ty` and `Value` (kind-based). |
| M06 | Added `Ty::Ref`, `Value::Ref`. |
| M07 | Added `Ty::Box/Vec/String`, `Value::Box/Vec/String`. Restructured `Value::Ref`. |
| M07.1 | Added `Ty::Slice`, `Value::Slice`. |
| M07.2 | Added `StaticAddr`, `Pointee::Static`, `Ty::Str`, `MemEvent::StaticAlloc/BytesCopy`. |
| M07.3 | Added `Ty::Array`, `Value::Array`. |
| M07.4 | Added `Ty::Struct`, `Value::Struct`. Extended `Value::Ref` with `field_path`. |
| M07.5 | Added `Ty::Param`. Extended `Ty::Struct` with `type_args`. |
| M07.6 | AST-only additions (`Item::Trait`, `ImplBlock.trait_name`, `TypeParam.bounds`); no event-protocol changes. |
| **M07.7** | Added `Ty::DynRef`, `Ty::BoxDyn`, `Value::DynRef`, `Value::BoxDyn`, `VtableAddr`, `MemEvent::VtableAlloc`. First new MemEvent variant since M07.2. AST: `Type::DynTrait`, `Expr::Cast`. |

## `Ty` — additive variants

```rust
pub enum Ty {
    // ... existing variants
    // NEW in M07.7:
    DynRef {
        trait_name: String,
        mutable: bool,
    },
    BoxDyn {
        trait_name: String,
    },
}
```

JSON shapes:
- `DynRef`: `{ "DynRef": { "trait_name": "Show", "mutable": false } }`
- `BoxDyn`: `{ "BoxDyn": { "trait_name": "Show" } }`

## `Value` — additive variants

```rust
pub enum Value {
    // ... existing variants
    // NEW in M07.7:
    DynRef {
        borrow_id: BorrowId,
        target: Pointee,
        vtable: VtableAddr,
        mutable: bool,
        trait_name: String,
    },
    BoxDyn {
        addr: HeapAddr,
        vtable: VtableAddr,
        trait_name: String,
    },
}
```

JSON shapes:
- `DynRef`: `{ "DynRef": { "borrow_id": N, "target": <Pointee>, "vtable": M, "mutable": false, "trait_name": "Show" } }`
- `BoxDyn`: `{ "BoxDyn": { "addr": N, "vtable": M, "trait_name": "Show" } }`

## New addressing namespace: `VtableAddr`

```rust
pub struct VtableAddr(pub u32);
```

JSON: bare integer (the `u32` payload). Distinct from `HeapAddr`, `StaticAddr`, `SlotId`, `FrameId`, `BorrowId`.

## `MemEvent` — additive variant

```rust
pub enum MemEvent {
    // ... existing variants
    VtableAlloc {
        addr: VtableAddr,
        trait_name: String,
        type_name: String,
        methods: Vec<String>,
        span: Span,
    },
}
```

JSON shape: `{ "VtableAlloc": { "addr": N, "trait_name": "Show", "type_name": "Point", "methods": ["show"], "span": <Span> } }`. Fires ONCE per unique `(trait, type)` pair (content-deduplicated).

## `Pointee` — no changes

Vtables ride on `Value::DynRef.vtable` and `Value::BoxDyn.vtable` fields, NOT on a new `Pointee::Vtable(_)` variant. Rationale: vtables aren't borrow targets in the M07.4-7 sense (you don't take `&p` of a vtable); they're a separate addressing namespace for "function tables".

## AST — additive Type + Expr

```rust
pub enum Type {
    // ... existing
    DynTrait { trait_name: String, span: Span },
}

pub enum Expr {
    // ... existing
    Cast { inner: Box<Expr>, target_ty: Type, span: Span },
}
```

Not part of the wire protocol (AST is parser-side only). Affects M01's AST-snapshot tests only if the new variants are constructed by an existing sample (they aren't — pre-M07.7 samples use no `dyn` or `as` syntax).

## Behavioral guarantees (post-M07.7)

- **B-M77-1**: Constructing the first `Value::DynRef` (or `Value::BoxDyn`) targeting a `(trait, type)` pair emits one `MemEvent::VtableAlloc` for that pair. Subsequent constructions to the same pair share the addr; no new event fires.
- **B-M77-2**: Vtables never deallocate (no `VtableFree` event).
- **B-M77-3**: Method dispatch on `Value::DynRef` (or `Value::BoxDyn`) resolves to the concrete type's impl override OR (for non-overridden defaults) the trait's default body — same lookup as M07.6 direct trait dispatch.
- **B-M77-4**: Trait-object method dispatch produces `FrameEnter { fn_name: "<TypeName as TraitName>::method", .. }` — same UFCS-style format as M07.6 static dispatch. The concrete type is resolved at runtime via the target's underlying value.
- **B-M77-5**: Casting `&T` to `&dyn Trait` requires `T` impls `Trait`; rejection cites both type and trait by name.
- **B-M77-6**: Implicit coercion at fn-arg sites works — `fn print(x: &dyn Show); print(&p);` succeeds without an explicit `as` cast.
- **B-M77-7**: Calling an inherent (non-trait) method through a `&dyn Trait` value → typeck error.
- **B-M77-8**: One `print` frame per call site for `fn print(x: &dyn Show)` regardless of argument type — no monomorphization-style per-type mangling on the OUTER frame (matches Rust's runtime-dispatch semantic).
- **B-M77-9**: `Box<dyn Trait>` heap allocations follow M07's standard alloc + Drop lifecycle; the Box-of-dyn fat pointer (heap addr + vtable addr) doesn't change Box's normal lifecycle.
- **B-M77-10**: Existing M01–M07.6 snapshots stay byte-identical (no event-shape changes for non-trait-object programs; no Ty/Value shape changes for existing variants).
- **B-M77-11**: M01's `parses_full_l1.snap` and similar AST snapshots stay byte-identical because no existing sample constructs `Type::DynTrait` or `Expr::Cast`.

## What this contract does NOT cover (deferred)

- **Multi-trait objects** `&dyn A + B` — Rust mostly doesn't allow either; deferred.
- **Upcasting** `&dyn Child` → `&dyn Parent` — requires supertraits (not in M07.6); deferred.
- **`?Sized` and custom DSTs** — only `dyn Trait` (always behind borrow / Box) supported.
- **`impl Trait`** in argument or return position — sugar for generic bounds (M07.6) but distinct syntax; deferred.
- **`Vec<Box<dyn Trait>>`** heterogeneous collection — explicitly out of scope for M07.7's headline.
- **`fn` pointers as values** — unrelated; deferred.
- **Bare `dyn Trait`** (not behind borrow / Box) — unsized; requires indirection.
- **Object-safety checks** for hypothetical violations — M07.6 restrictions (no `Self` return, no generic methods) already prevent the violation patterns; no new check needed.
- **Auto-traits** (`Send`/`Sync`) — deferred to M08 (threads).
- **Trait-object method dispatch with associated types or supertraits** — neither feature exists in M07.6.
