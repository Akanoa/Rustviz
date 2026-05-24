# Data Model — M07.7 Entities

Trait-object additions: **1 new AST type** (`Type::DynTrait`), **1 new AST expr** (`Expr::Cast`), **2 new `Ty` variants** (`DynRef`, `BoxDyn`), **2 new `Value` variants** (`DynRef`, `BoxDyn`), **1 new addressing newtype** (`VtableAddr`), **1 new `MemEvent` variant** (`VtableAlloc`), **2 typeck-side registries** (eval-side mirror via `vtable_addrs`), **UI views** (`VtableView`, `DynView`).

No new `Pointee` variants (vtables ride on `Value::DynRef.vtable` and `Value::BoxDyn.vtable` fields). All wire-side additions are pure-additive — existing snapshots stay byte-identical for programs that don't use trait objects.

## New (AST type): `Type::DynTrait`

```rust
// In src/parse/ast.rs

pub enum Type {
    // ... existing variants
    /// **M07.7**: trait-object type `dyn TraitName`. The `&` / `&mut` wrap
    /// is handled by the outer `Type::Ref { inner: Type::DynTrait, .. }`
    /// pattern; bare `Type::DynTrait` only appears inside `Box<dyn _>`
    /// (handled by the existing `Type::Generic` machinery).
    DynTrait {
        /// Single-segment trait name (e.g. `"Show"`).
        trait_name: String,
        /// Span from `dyn` keyword through the trait name.
        span: Span,
    },
}
```

### Validation rules

- **VR-1**: Bare `Type::DynTrait` (not wrapped in `Type::Ref` or `Type::Generic`-as-Box) typeck-rejects as "unsized type — wrap in `&` or `Box`".
- **VR-2**: `trait_name` must reference a registered trait (typeck phase 2 verifies).
- **VR-3**: Multi-segment dyn paths (`dyn mod::Show`) parser-accepted as identifier-only (single-segment in M07.7).

## New (AST expr): `Expr::Cast`

```rust
pub enum Expr {
    // ... existing variants
    /// **M07.7**: cast expression `inner as TargetType`. In M07.7 only used
    /// for `&p as &dyn Show` coercion; future numeric/string casts would
    /// reuse this AST node.
    Cast {
        /// The value being cast.
        inner: Box<Expr>,
        /// Destination type.
        target_ty: Type,
        /// Span from `inner` start through `target_ty`'s end.
        span: Span,
    },
}
```

### Validation rules

- **VR-4**: In M07.7, only `&T → &dyn Trait` casts are supported (typeck rejects others with "M07.7 supports only trait-object coercion casts").
- **VR-5**: The inner expression must be a borrow (`Ty::Ref { .. }`); casting an owned value rejects.
- **VR-6**: Cast target's trait must have an impl for the source's pointee type (verified via `TraitImplRegistry`).

## Modified: `Ty` — adds `DynRef` + `BoxDyn`

```rust
pub enum Ty {
    // ... existing variants
    /// **M07.7**: borrow form of a trait object — `&dyn Trait` or
    /// `&mut dyn Trait`. The `&` wrap is collapsed into this variant so
    /// dispatch logic doesn't need to unwrap `Ty::Ref { Ty::DynTrait, .. }`
    /// at every check. Nominal equality by `(trait_name, mutable)`.
    DynRef {
        trait_name: String,
        mutable: bool,
    },
    /// **M07.7**: heap-owning form of a trait object — `Box<dyn Trait>`.
    /// The Box stores both the data ptr (heap addr) AND the vtable ptr —
    /// fat-pointer heap allocation. Separate from `Ty::Box(_)` for clarity
    /// (regular Box vs Box-of-dyn have distinct runtime shapes).
    BoxDyn {
        trait_name: String,
    },
}
```

### Validation rules

- **VR-7**: `Ty::name()` for `DynRef { trait_name, mutable: false }` returns `"&dyn TraitName"`; `mutable: true` returns `"&mut dyn TraitName"`.
- **VR-8**: `Ty::name()` for `BoxDyn { trait_name }` returns `"Box<dyn TraitName>"`.
- **VR-9**: `Ty::is_copy()` for `DynRef { mutable: false, .. }` returns `true` (shared refs are Copy); `mutable: true` returns `false`. `BoxDyn` returns `false` (heap-owning, non-Copy).
- **VR-10**: Equality is nominal by trait_name (+ mutable for DynRef).

## Modified: `Value` — adds `DynRef` + `BoxDyn`

```rust
pub enum Value {
    // ... existing variants
    /// **M07.7**: fat-pointer trait-object value (borrow form). Contains
    /// data ptr (target Pointee) + vtable ptr (VtableAddr). The borrow_id
    /// tracks the underlying borrow for lifecycle / aliasing.
    DynRef {
        borrow_id: BorrowId,
        /// What the data ptr points at — a stack slot or a heap allocation.
        target: Pointee,
        /// What the vtable ptr points at — a vtable in the VTABLES region.
        vtable: VtableAddr,
        mutable: bool,
        /// The trait this dyn-ref exposes (e.g. `"Show"`).
        trait_name: String,
    },
    /// **M07.7**: fat-pointer trait-object value (heap-owning form via Box).
    /// Heap addr stores the underlying data; vtable addr identifies the
    /// per-(type, trait) vtable.
    BoxDyn {
        addr: HeapAddr,
        vtable: VtableAddr,
        trait_name: String,
    },
}
```

### Validation rules

- **VR-11**: `Value::DynRef`'s `target` MUST point at a slot or heap allocation whose underlying value is a `Value::Struct` (or any concrete type that impls the trait — for M07.7 restricted to structs).
- **VR-12**: `Value::DynRef.vtable` is interned content-deduplicated by `(trait_name, type_name_of_target)`. Multiple DynRef values to the same target+trait pair share the same vtable addr.
- **VR-13**: `Value::BoxDyn` similar — `vtable` interned same way.

## New addressing newtype: `VtableAddr`

```rust
// In src/event.rs

/// **M07.7**: identifier for a vtable in the VTABLES region. Distinct from
/// HeapAddr / StaticAddr / SlotId — vtables are a separate memory region
/// (analog of M07.2's static memory). Monotonic; never reused.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct VtableAddr(pub u32);
```

### Validation rules

- **VR-14**: `VtableAddr.0` is monotonic across the trace.
- **VR-15**: VtableAddr never appears in `Pointee` (vtables aren't borrow targets in the M07.4-7 sense). It only appears on `Value::DynRef.vtable` and `Value::BoxDyn.vtable` fields.

## New (event): `MemEvent::VtableAlloc`

```rust
pub enum MemEvent {
    // ... existing variants
    /// **M07.7**: a vtable was allocated in the VTABLES region. Fires
    /// ONCE per unique `(trait_name, type_name)` pair when the first
    /// trait-object value targeting that pair is constructed. Content-
    /// deduplicated — multiple `&dyn Show` borrows of Point share one
    /// vtable. Never freed (vtables persist for the trace's lifetime).
    /// Analog of `MemEvent::StaticAlloc` from M07.2.
    VtableAlloc {
        addr: VtableAddr,
        trait_name: String,
        type_name: String,
        /// Method names in trait-declaration order. Each name carries a
        /// dispatch-target label rendered as `<TypeName as TraitName>::method`
        /// (or `<TraitName>::method (default)` for unoverridden defaults).
        methods: Vec<String>,
        span: Span,
    },
}
```

### Validation rules

- **VR-16**: One `VtableAlloc` per unique `(trait_name, type_name)` pair across the entire trace.
- **VR-17**: `methods` Vec carries trait-declaration order; eval-side runtime dispatch uses name-based lookup (NOT positional vtable-slot lookup) — the order is purely for display.
- **VR-18**: Vtables never deallocate (no `VtableFree` event).

## Modified: `SlotRowView` — adds `dyn_view`

```rust
// In src/ui.rs

pub struct SlotRowView {
    pub slot_id: u32,
    pub name: String,
    pub ty: String,
    pub value: Option<String>,
    pub inline_cells: Option<InlineCellsView>,
    pub struct_view: Option<StructView>,
    /// **M07.7**: present when the slot holds a `Value::DynRef` or
    /// `Value::BoxDyn`. The JS renders a two-cell fat-pointer view in
    /// the slot's value area (data label + vtable label). Mutually
    /// exclusive with `value` / `inline_cells` / `struct_view`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dyn_view: Option<DynView>,
}

pub struct DynView {
    /// Label for the data ptr — typically `"p"` (binding name of target)
    /// or `"heap[N]"` for `Box<dyn Trait>`.
    pub data_label: String,
    /// Label for the vtable ptr — `"<TypeName as TraitName>"` form.
    pub vtable_label: String,
    /// Vtable addr — drives the hover-to-highlight wiring in JS.
    pub vtable_addr: u32,
}
```

### Validation rules

- **VR-19**: `dyn_view` is `Some(_)` iff the slot's value is `Value::DynRef` or `Value::BoxDyn`. Mutually exclusive with `value`, `inline_cells`, `struct_view`.
- **VR-20**: `data_label` resolved at apply_event time from the target's binding name (Slot case) or heap-addr string (Heap case).
- **VR-21**: `vtable_label` derived from the vtable's `(trait_name, type_name)` pair.

## New (UI): `VtableView` + state-snapshot extension

```rust
// In src/ui.rs

pub struct VtableView {
    pub addr: u32,
    pub trait_name: String,
    pub type_name: String,
    /// Each entry: (method_name, dispatch_target_label).
    /// Target label: `<TypeName as TraitName>::method` for overrides;
    /// `<TraitName>::method (default)` for unoverridden defaults.
    pub methods: Vec<(String, String)>,
}

pub struct StateSnapshot {
    // ... existing fields
    /// **M07.7**: live vtables in the VTABLES panel. Each pushed via
    /// `VtableAlloc` events; never removed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vtables: Vec<VtableView>,
}
```

### Validation rules

- **VR-22**: `vtables` Vec populated by `apply_event` on `MemEvent::VtableAlloc`. One entry per VtableAlloc; never removed.
- **VR-23**: `methods` per VtableView populated at apply_event time from the event's `methods` list, with dispatch-target labels resolved via lookup in the M07.6 trait registries (which the snapshot has access to indirectly via the world's vtable-targets table).

## Vtable interning (eval-side)

```rust
// In src/eval.rs

struct Evaluator<'a> {
    // ... existing fields
    /// **M07.7**: content-deduplicated vtable addressing. Key is
    /// `(trait_name, type_name)`; value is the interned VtableAddr.
    /// Analog of M07.2's `static_region.by_content`.
    vtable_addrs: HashMap<(String, String), VtableAddr>,
    /// **M07.7**: monotonic counter for vtable addresses.
    next_vtable_addr: u32,
}
```

### Validation rules

- **VR-24**: `intern_vtable(trait_name, type_name)` returns the existing addr if the pair is in the map; otherwise allocates a fresh addr, populates the methods list from `traits.schemas[trait_name].required_methods.keys() + default_methods.keys()`, emits `MemEvent::VtableAlloc`, and inserts the entry.
- **VR-25**: `next_vtable_addr` monotonic; never decremented.

## New: M07.7 reference samples

| File | Content | Pedagogy |
|---|---|---|
| `tests/samples/m07_7_dyn_basic.rs` | `trait Show { fn show(&self) -> i32; } impl Show for Point { ... } let d: &dyn Show = &p; let s = d.show();` | Fat pointer + vtable + two-step dispatch arrow visible. |
| `web/samples/m07_7_dyn_basic.rs` | Mirror. | |
| `tests/samples/m07_7_dyn_param.rs` | `fn print(x: &dyn Show) -> i32 { x.show() } let r = print(&p);` (or `print(&p); print(&q);` for two-type case) | ONE `print` frame for any type — vs M07.5/M07.6's per-type monomorphization. |
| `web/samples/m07_7_dyn_param.rs` | Mirror. | |
| `tests/samples/m07_7_box_dyn.rs` | `let b: Box<dyn Show> = Box::new(p); let s = b.show();` | Heap-owning trait object; fat pointer to heap + vtable. |
| `web/samples/m07_7_box_dyn.rs` | Mirror. | |
| `tests/samples/m07_7_static_vs_dyn.rs` | `fn s<T: Show>(x: T) -> i32 { x.show() } fn d(x: &dyn Show) -> i32 { x.show() } let a = s(p); let b = d(&p);` (paired comparison) | Side-by-side dispatch flavors: static (`s::<Point>` monomorphized) vs dynamic (`d` one-frame + vtable). 🎯 HEADLINE CONTRAST |
| `web/samples/m07_7_static_vs_dyn.rs` | Mirror. | |
