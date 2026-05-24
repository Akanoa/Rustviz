# Contract — M07.5 Protocol Delta

**9th invocation** of the closed-enum-with-revisions rule. M07.5 adds one variant (`Ty::Param(String)`) and extends one existing variant (`Ty::Struct` gains `type_args: Vec<Ty>` with serde-default-empty). No new `MemEvent` variants. No new `Pointee` variants. No new `Value` variants.

## Closed-enum rule — ninth invocation

| Milestone | Change |
|---|---|
| M03.1 | Added `MemEvent::ReturnValue`; removed `FrameEnter.params`. |
| M03.2 | Restructured `Ty` and `Value` (kind-based). |
| M06 | Added `Ty::Ref`, `Value::Ref`. |
| M07 | Added `Ty::Box/Vec/String`, `Value::Box/Vec/String`. Restructured `Value::Ref` (target_slot → target: Pointee). |
| M07.1 | Added `Ty::Slice(Box<Ty>)` and `Value::Slice { .. }`. |
| M07.2 | Added `StaticAddr`, `Pointee::Static`, `Ty::Str`, `MemEvent::StaticAlloc/BytesCopy`, `ArrowTarget::Static`. Removed `Value::Str`. |
| M07.3 | Added `Ty::Array(Box<Ty>, u64)` and `Value::Array { elements, elem_ty }`. |
| M07.4 | Added `Ty::Struct`, `Value::Struct`. Extended `Value::Ref` with `field_path: Vec<String>` (serde-default-empty). First field-extension of an existing variant. |
| **M07.5** | Adds `Ty::Param(String)`. **Extends `Ty::Struct`** with `type_args: Vec<Ty>` (serde-default-empty). Second field-extension; same pattern as M07.4. |

## `Ty` — additive variant + struct extension

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
        // EXTENSION in M07.5:
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        type_args: Vec<Ty>,
    },
    // NEW in M07.5:
    Param(String),
}
```

JSON shape for `Ty::Param`: `{ "Param": "T" }`.

JSON shape for `Ty::Struct` **when `type_args` is empty** (M07.4 case stays byte-identical): `{ "Struct": { "name": "Point", "fields": [...] } }` — `type_args` field omitted thanks to `skip_serializing_if`.

JSON shape for `Ty::Struct` **when `type_args` is non-empty** (M07.5 generic instantiation): `{ "Struct": { "name": "Wrapper", "fields": [["v", { "Int": "I32" }]], "type_args": [{ "Int": "I32" }] } }`.

## `Value` — no changes

M07.5 doesn't add any `Value` variants. The headline pedagogy (monomorphization-visible frames) is a labeling change on `FrameEnter.fn_name`, not a value-shape change.

## `MemEvent` — no changes

All M07.5 scenarios reuse existing events:
- Generic fn call → `FrameEnter { fn_name: "id::<i32>", .. }` (mangled name via the existing `display_name` parameter on `call_decl`, added in M07.4).
- Param + return → existing `SlotAlloc` + `SlotWrite` + `ReturnValue` + `FrameLeave`. The Ty in `SlotAlloc.ty` is the SUBSTITUTED concrete type (typeck applies substitution before any binding's type is recorded).
- Generic struct literal → existing `SlotWrite` with `Value::Struct { name, fields, type_args }` (the field-typed values are concrete; the struct's name carries `<T>` via the type_args field which renders as `"Wrapper<i32>"` in the slot label).

## `Pointee` — no changes

## `ArrowTarget` — no changes

## Behavioral guarantees (post-M07.5)

- **B-M75-1**: Every generic-fn call produces a `FrameEnter` whose `fn_name` is the mangled `source_name::<sub_ty_name>` (e.g. `"id::<i32>"`).
- **B-M75-2**: Two calls to the same generic fn with the same substitution produce TWO distinct `FrameEnter` events with the same `fn_name`, distinct `frame_id`s (the cost model "each call entry is a fresh frame" stays consistent with non-generic fns).
- **B-M75-3**: `SlotAlloc.ty` inside a generic fn's body always carries a concrete (post-substitution) `Ty` — `Ty::Param(_)` is never observed at the event level.
- **B-M75-4**: `Value::Struct` for a generic-struct instantiation carries the standard `name + fields` (concrete-typed field values) AND a non-empty `type_args` list (drives `"Wrapper<i32>"` rendering in the slot label).
- **B-M75-5**: Existing M06+ borrow snapshots stay byte-identical (no protocol changes touch `Value::Ref` or borrow events).
- **B-M75-6**: Existing M07.4 struct snapshots stay byte-identical (the new `type_args: Vec<Ty>` on `Ty::Struct` is serde-skipped when empty — every M07.4 sample's `Ty::Struct` has `type_args: []`).
- **B-M75-7**: Existing M01/M02 AST snapshots stay byte-identical EXCEPT possibly M01's `parses_full_l1.snap` if the snapshot's serializer renders `type_params: []` despite the skip-if-empty annotation (depends on Debug vs JSON serialization).
- **B-M75-8**: Bound syntax (`T: Foo`) → typeck error pointing at M07.6.
- **B-M75-9**: Multi-type-param decls (`<T, U>`) → typeck error pointing at the single-param restriction.
- **B-M75-10**: Generic call inside generic fn → typeck error pointing at the M07.5 nested-substitution restriction.

## What this contract does NOT cover (deferred)

- **Trait bounds on generics** (`fn id<T: Foo>(...)`) — M07.6.
- **Multiple type parameters** (`fn pair<T, U>(a: T, b: U)`) — deferred.
- **Where clauses** — deferred.
- **Const generics** (`<const N: usize>`) — deferred.
- **Lifetime generics** (`<'a>`) — deferred (scope-level lifetime handling unchanged).
- **GATs / higher-kinded types** — never (out of scope for the project entirely).
- **Default type params** — deferred.
- **Generic methods** (method-level `<U>` inside a generic struct's impl) — deferred.
- **Specific-instantiation impls** (`impl Wrapper<i32>` separately from `impl<T> Wrapper<T>`) — deferred.
- **Nested generic calls** (`fn outer<T>(x: T) -> T { id::<T>(x) }`) — deferred (substitution-during-substitution).
- **Inference from type annotations** (`let x: i32 = id();`) — deferred (workaround: turbofish).
