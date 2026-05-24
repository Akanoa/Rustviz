# Contract — M07.6 Protocol Delta

**10th invocation** of the closed-enum-with-revisions rule. M07.6 adds AST surface (parser-side; not part of the wire protocol) and changes one string-format convention (`FrameEnter.fn_name` rendering for trait-method dispatch). **No new `MemEvent` variants. No new `Ty`/`Value`/`Pointee` variants.**

## Closed-enum rule — tenth invocation

| Milestone | Change |
|---|---|
| M03.1 | Added `MemEvent::ReturnValue`; removed `FrameEnter.params`. |
| M03.2 | Restructured `Ty` and `Value` (kind-based). |
| M06 | Added `Ty::Ref`, `Value::Ref`. |
| M07 | Added `Ty::Box/Vec/String`, `Value::Box/Vec/String`. Restructured `Value::Ref`. |
| M07.1 | Added `Ty::Slice(Box<Ty>)` and `Value::Slice { .. }`. |
| M07.2 | Added `StaticAddr`, `Pointee::Static`, `Ty::Str`, `MemEvent::StaticAlloc/BytesCopy`. |
| M07.3 | Added `Ty::Array(Box<Ty>, u64)` and `Value::Array { .. }`. |
| M07.4 | Added `Ty::Struct`, `Value::Struct`. Extended `Value::Ref` with `field_path`. |
| M07.5 | Added `Ty::Param(String)`. Extended `Ty::Struct` with `type_args`. |
| **M07.6** | AST-only additions (`Item::Trait`, `ImplBlock.trait_name`, `TypeParam.bounds`); no event-protocol changes; no new wire-side variants. String-format convention for trait-method dispatch frame names. |

## AST changes (parser-side only; not on the wire)

- **NEW**: `Item::Trait(TraitDecl)`.
- **EXTENSION**: `ImplBlock.trait_name: Option<String>` (None = inherent — M07.4 behavior; Some = trait impl).
- **PROMOTION**: `TypeParam.bound: Option<String>` (M07.5) → `bounds: Vec<String>` (M07.6).

These changes don't affect the trace JSON because AST nodes aren't serialized in traces. They DO affect M01's `parses_full_l1.snap` Debug-format snapshot (because Debug derives include the new fields). Re-baseline once for the empty-Vec → empty-Vec field transition; subsequent snapshots stay byte-identical.

## `MemEvent` — no changes

All M07.6 scenarios reuse existing events:
- Trait method dispatch → `FrameEnter { fn_name: "<Point as Show>::show", .. }` (mangled name; new string format but same event shape).
- Default-method dispatch → `FrameEnter { fn_name: "<Point as Show>::double", .. }` (same format; body executed comes from the trait, not the impl).
- Generic bound check → no event; pure typeck-side validation.
- Method dispatch on a type-param-typed value (inside generic body) → `FrameEnter { fn_name: "<Point as Show>::show", .. }` (substituted concrete type).

## `Value` — no changes

## `Pointee` — no changes

## `Ty` — no changes

## String-format convention: trait-method frame names

```text
Inherent dispatch:  Point::show           (M07.4 format)
Trait dispatch:     <Point as Show>::show  (M07.6 NEW format — UFCS-style)
```

This is a labeling convention, not a protocol change. Existing M07.4 method-call samples stay byte-identical (their dispatches were inherent, so the label format is unchanged). New M07.6 trait dispatches use the `<as>` form.

## Behavioral guarantees (post-M07.6)

- **B-M76-1**: Every trait-method dispatch produces a `FrameEnter` whose `fn_name` matches `<TypeName as TraitName>::MethodName`.
- **B-M76-2**: Default-method dispatch (impl didn't override) executes the trait declaration's body; the resulting frame still uses the type's name in the mangled label.
- **B-M76-3**: Method dispatch tie-breaker: builtins > inherent impls > trait impls. Match the M07.4 pattern, extended with a third layer.
- **B-M76-4**: A generic fn with a trait bound `<T: Show>` can call any Show method on a value of type `T` inside its body. Typeck rejects method calls not proven by any bound.
- **B-M76-5**: A generic fn call with an arg whose concrete type doesn't satisfy a declared bound → typeck error naming both the concrete type and the missing-bound trait.
- **B-M76-6**: An impl block for a trait must provide every required method; reject "missing required method" if any required method is unimplemented.
- **B-M76-7**: An impl block for a trait must not include methods outside the trait's declared items; reject "method `<name>` not on trait `<Trait>`".
- **B-M76-8**: Duplicate trait declarations or duplicate `(trait, type)` impls rejected at typeck.
- **B-M76-9**: Method-name ambiguity (multi-bound `T: A + B`, both A and B declare `name`) → typeck error with both candidates listed; UFCS suggested as a path forward (even though UFCS is out of scope).
- **B-M76-10**: Impl-on-builtin (`impl Show for i32`) works — the trait dispatch key is `(trait_name, type_rendered_name)` which is stable for any type.
- **B-M76-11**: Existing M03 snapshots stay byte-identical (no event-shape changes, no Ty/Value shape changes).
- **B-M76-12**: M01's `parses_full_l1.snap` and similar AST snapshots may re-baseline once for the `TypeParam.bound` → `bounds` promotion (Debug output changes); subsequent snapshots stay byte-identical.

## What this contract does NOT cover (deferred)

- **Trait objects `&dyn Trait`** — requires vtable machinery + heap-or-table representation; deferred to a future dynamic-dispatch milestone.
- **Associated types** (`trait Iter { type Item; }`) — deferred.
- **Associated consts** on traits — deferred.
- **Supertraits** (`trait B: A`) — deferred (no inheritance).
- **Blanket impls** (`impl<T: Foo> Bar for T`) — deferred.
- **Derive macros** (`#[derive(Debug, Clone)]`) — deferred.
- **Where clauses** — deferred; bounds in `<T: Trait>` only.
- **Generic trait methods** (`trait Foo { fn bar<U>(&self, u: U); }`) — deferred (mirrors M07.5's no-method-level-type-params).
- **`Self` return type** (`trait Foo { fn clone(&self) -> Self; }`) — deferred.
- **UFCS** (`Show::show(&p)`) — deferred. Method-call dot syntax only.
- **Multi-segment trait paths** (`mod::Show`) — deferred. Single-segment only.
- **Auto-traits** (`Send`, `Sync`) — deferred to M08 (threads).
- **Visibility modifiers** (`pub trait`, `pub fn`) — deferred (not yet in language subset).
- **Trait method overloading on receiver type** — Rust doesn't allow this; M07.6 inherits the restriction.
