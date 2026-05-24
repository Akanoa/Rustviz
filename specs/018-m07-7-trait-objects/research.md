# Research — M07.7 Implementation Decisions

18 design decisions across parser (new `dyn` + `as` keywords), AST (Type::DynTrait + Expr::Cast), typeck (Ty::DynRef + cast checking + dispatch routing), eval (Value::DynRef + vtable interning + coercion), UI (VTABLES panel + fat-pointer rendering + two-step dispatch arrows), and protocol amendment.

## Parser

### R-001 — `dyn` keyword lexed; `Type::DynTrait { trait_name, span }` AST shape

- **Decision**: `dyn` becomes a new lexer keyword (`TokenKind::Dyn`). When `parse_type` sees `Dyn` followed by an identifier, build `Type::DynTrait { trait_name, span }`. The `&` and `&mut` wrap is handled by the existing `Type::Ref { inner: Type::DynTrait, mutable, .. }` — same Ref pattern from M06+.
- **Bare `dyn TraitName`** (inside `Box<dyn Show>`): parses as `Type::DynTrait` directly; the wrapping `Box<_>` provides the indirection. Bare `dyn` not behind any reference is typeck-rejected as unsized.
- **Rationale**: minimal AST surface; reuses M06+'s `Type::Ref` for the borrow case; `Box<dyn Trait>` falls out from the existing generic-type machinery.

### R-002 — `as` keyword + `Expr::Cast { inner, target_ty }`

- **Decision**: add `as` keyword if not present (verify in M07.4-6 — likely absent). `parse_expr` postfix loop: after any expression, if `As` token follows, parse `Type`, build `Expr::Cast { inner, target_ty, span }`.
- **Cast precedence**: `as` binds tighter than binary operators but looser than method calls. In M07.7 we only need it for `&p as &dyn Show`; future numeric casts (`5 as f64`) would reuse the same node.
- **Rationale**: minimal new syntax for the explicit-coercion case; pedagogically clear about the source's intent.

### R-003 — Implicit coercion at fn-arg sites + let bindings

- **Decision**: typeck and eval auto-coerce `&T` to `&dyn Trait` when:
  1. The target type (param type OR let-annotation OR cast target) is `Ty::DynRef { trait_name, .. }`.
  2. The source type is `Ty::Ref { Ty::Struct(name), .. }` (or other concrete) AND that name has an impl of the trait in `TraitImplRegistry`.
- **No coercion at expression-statement positions** (`let r = &p;` without `: &dyn Show` annotation → `r: &Point`, not `&dyn Show`).
- **Rationale**: Rust's standard "deref coercion + unsized-coercion at fn-arg sites" behavior. Pedagogically: makes the common case (`print(&p)`) work without forcing every learner to write `print(&p as &dyn Show)`.

## AST

### R-004 — `Expr::Cast { inner: Box<Expr>, target_ty: Type, span: Span }`

- **Decision**: minimal new variant. `inner` is the value being cast; `target_ty` is the destination type (always `Type::Ref { Type::DynTrait, .. }` in M07.7 — but the variant doesn't restrict, allowing future numeric casts).
- **Rationale**: matches Rust syntax; small footprint.

### R-005 — `Type::DynTrait { trait_name: String, span: Span }`

- **Decision**: standalone AST type. Bare `dyn TraitName`; the borrow form is `Type::Ref { inner: Type::DynTrait, .. }`.
- **Why not include `mutable` here**: mutability is on the outer `Type::Ref`. Keeping `Type::DynTrait` minimal allows reuse inside `Box<dyn TraitName>` (no mutability concept at the inner type).
- **Rationale**: clean separation of dyn-ness from mutability.

## Typeck

### R-006 — `Ty::DynRef { trait_name: String, mutable: bool }`

- **Decision**: new typeck `Ty` variant representing the borrow form of a trait object. The wrapping is collapsed (`&dyn Show` → `Ty::DynRef { Show, false }`) so dispatch logic doesn't need to unwrap `Ty::Ref { Ty::DynTrait, .. }` at every check.
- **`Ty::name()`**: `format!("{}dyn {trait_name}", if mutable { "&mut " } else { "&" })`.
- **`Ty::is_copy()`**: `true` for `mutable: false` (shared refs are Copy, including dyn refs); `false` for `mutable: true` (matches Rust's `&mut` non-Copy rule).
- **Equality**: nominal by `(trait_name, mutable)`.
- **Rationale**: collapsing the `Ref { DynTrait }` wrap into a single Ty variant simplifies dispatch code paths.

### R-007 — `Box<dyn Trait>` handling

- **Decision**: introduce `Ty::BoxDyn { trait_name: String }` for the heap-owning form (analog of `Ty::Box(inner)` but specialized for dyn). Eval-side `Value::BoxDyn { addr, vtable, trait_name }` carries the fat-pointer info — heap addr (data ptr) + vtable addr.
- **Alternative considered**: extend `Ty::Box(Box<Ty>)` to accept `Ty::DynRef` as inner — but then `Value::Box { addr }` would be ambiguous (regular Box vs dyn Box are different shapes). Rejected.
- **Recommendation**: separate variants for clarity. `Ty::Box(_)` stays the regular-Box case; `Ty::BoxDyn { .. }` is the dyn-Box case.

### R-008 — Method dispatch fourth layer: `Value::DynRef` receivers

- **Decision**: extend `eval_method_call` with a `Value::DynRef` case (after M07's builtins, M07.4's inherent, M07.6's trait-impl dispatch). For a `Value::DynRef { target, vtable, trait_name, .. }` receiver:
  1. Look up the target's concrete type (from the slot's or heap's Value::Struct).
  2. Look up `trait_impl_bodies[(trait_name, type_name, method)]` first; fall through to `trait_default_bodies[(trait_name, method)]` if no override (M07.6's pattern).
  3. Build the mangled name `<TypeName as TraitName>::method` (same as M07.6 static).
  4. Construct self_value the same way as M07.6 (fresh borrow_id, target = the receiver's underlying target).
- **Rationale**: reuses all M07.6 dispatch infrastructure; the only new step is the concrete-type-via-target lookup.

### R-009 — Inherent-method rejection through dyn

- **Decision**: when a method call's receiver is `Ty::DynRef { trait_name, .. }`, ONLY look up methods declared by `trait_name` (in `TraitRegistry.schemas[trait_name].required_methods` + `default_methods`). If the method isn't found, error: "method `<name>` is not in trait `<TraitName>` (trait objects can only call trait methods)".
- **Rationale**: matches Rust's behavior. Pedagogically: makes the "you only see the trait's surface through dyn" intuition concrete.

### R-010 — Cast validation: `&T as &dyn Trait` requires `T: Trait`

- **Decision**: at typecheck of `Expr::Cast { inner, target_ty }`:
  1. Lower `target_ty` via `ty_from_ast_resolving_structs` → if not `Ty::DynRef { .. }`, accept (defer numeric casts to a future milestone).
  2. Typecheck `inner` → must be `Ty::Ref { Ty::Struct(name), .. }` (or `Ty::Ref` to another concrete type that impls the trait).
  3. Verify `TraitImplRegistry.impls.contains_key((target.trait_name, name))`.
  4. If not, error: "the type `<T>` cannot be coerced to `&dyn <Trait>` because it does not implement `<Trait>`".
- **Mutability matching**: cast from `&T` to `&mut dyn Trait` rejected; `&mut T` to `&dyn Trait` accepted (mut-to-shared downgrade is fine).

## Eval

### R-011 — Vtable interning (analog of M07.2 static-memory)

- **Decision**: `Evaluator.vtable_addrs: HashMap<(String, String), VtableAddr>` content-deduplicates. `intern_vtable(trait_name, type_name) → VtableAddr` checks the map; on first use, allocates a fresh `VtableAddr`, populates the methods list (from `traits.schemas[trait_name].required_methods.keys() + default_methods.keys()`), emits `MemEvent::VtableAlloc { addr, trait_name, type_name, methods, span }`.
- **Lifetime**: vtables NEVER deallocate (analog of M07.2's static-memory blocks).
- **Rationale**: matches Rust's linker behavior (one vtable per (trait, type) pair across the whole binary).

### R-012 — `Value::DynRef` construction at `as` cast + at fn-arg coercion

- **Decision**: at `Expr::Cast` eval, get inner's `Value::Ref { target, mutable, .. }`. Intern the vtable for `(trait_name, type_name_of_target)`. Construct `Value::DynRef { borrow_id, target, vtable, mutable, trait_name }` — fresh borrow_id per cast (each cast = fresh borrow at the value layer; matches M07.6's "each call site = fresh borrow" principle).
- **At fn-arg coercion**: same construction at the call_decl arg-binding step. When the param type is `Ty::DynRef` and the arg is `Value::Ref`, transform inline before binding to the param slot.
- **Rationale**: keeps eval-side dispatch uniform; coercion is just a Value transformation.

### R-013 — Frame-name format: same as M07.6 inner

- **Decision**: for trait-object dispatch, the resolved method's frame name is `<ConcreteType as TraitName>::method` — same UFCS-style format as M07.6 static dispatch. The OUTER frame (containing the call) is what differs: `print::<Point>` (static) vs `print` (dynamic).
- **Pedagogical contrast** (US4): when the learner steps through `m07_7_static_vs_dyn.rs`, the outer frames are visually different (`print::<Point>` for static, `print` for dynamic), but the INNER trait-method frames are identical (`<Point as Show>::show` in both cases). Makes the "same destination, different dispatch path" insight concrete.

## UI

### R-014 — UX checkpoint: VTABLES panel layout + fat-pointer + dispatch arrows

This is the meaty piece. **Locked-in data shape**:

```rust
// In src/ui.rs

pub struct VtableView {
    pub addr: u32,
    pub trait_name: String,
    pub type_name: String,
    /// Each entry: (method_name, dispatch_target_label).
    /// Target_label format: `<TypeName as TraitName>::method_name` (or
    /// `<TraitName>::method_name (default)` for default-method slots).
    pub methods: Vec<(String, String)>,
}

pub struct DynView {
    /// Label for the data ptr — typically the binding name of the
    /// targeted slot (e.g. `"p"`) or `"heap[N]"` for Box-of-dyn.
    pub data_label: String,
    /// Label for the vtable ptr — `<TypeName as TraitName>` form.
    pub vtable_label: String,
    /// Vtable addr for arrow targeting at hover.
    pub vtable_addr: u32,
}
```

**Recommended visual** (Proposal A for the UX checkpoint):

```text
┌─────────────────────────────────────────────────────────────────────┐
│ Stacks            │ Heap          │ VTABLES           │ Static (RO) │
├───────────────────┼───────────────┼───────────────────┼─────────────┤
│  main()           │  (empty)      │ ┌───────────────┐ │             │
│  p: Point         │               │ │<Point as Show>│ │             │
│   ┌──┐┌──┐        │               │ │  show         │ │             │
│   │1 ││2 │        │               │ │   → impl body │ │             │
│   └──┘└──┘        │               │ └───────────────┘ │             │
│                   │               │                   │             │
│  d: &dyn Show     │               │                   │             │
│   ┌──data────┐    │               │                   │             │
│   │  → p     │    │               │                   │             │
│   ├──vtable──┤    │               │                   │             │
│   │  → <P:S> │ ─────────────────► │                   │             │
│   └──────────┘    │               │                   │             │
└───────────────────┴───────────────┴───────────────────┴─────────────┘
```

Two cells in d's slot value area: `data` and `vtable`. Each has a labeled target. Hover the vtable cell → arrow lights up to the corresponding VTABLES box.

**Dispatch arrow on `d.show()`**: at the call step, draw TWO arrows simultaneously:
1. `data` ptr → p in main's frame (existing borrow-arrow path; data is a sub-region of the value).
2. `vtable` ptr → vtable box → `show` method's frame card.

The second is a NEW arrow type — dashed orange (recommendation; iterate at checkpoint). Visually distinct from solid blue borrows / red mut borrows / black owning.

**Alternative — Proposal B (compact)**: render `d` as a single labeled row `d : &dyn Show → p [vtable: <Point as Show>]` and only show the fat-pointer split on hover. Smaller visual footprint but loses the "fat pointer is 16 bytes" pedagogy.

**Plan-phase recommendation: Proposal A.** Pedagogically loud — the two-cell rendering makes the 16-byte fat-pointer composition tangible. Vertical real estate cost is acceptable.

**UX checkpoint procedure** (mirrors M07.4's struct-view checkpoint):
1. Land all non-UI plumbing first (AST, typeck, eval, protocol shape).
2. Implement Proposal A's first cut (VTABLES panel + fat-pointer slot + dispatch arrows).
3. **PAUSE** for user review.
4. Iterate on tweaks (arrow color, panel positioning, label format).
5. Continue with US2-US4 samples + tests.

### R-015 — VTABLES panel positioning

- **Decision**: place VTABLES between the HEAP and STATIC MEMORY panels. The pedagogical flow left-to-right: STACKS → HEAP → VTABLES → STATIC. Each panel represents a memory region; vtables sit between "user-mutable runtime data" (heap) and "compile-time-immutable data" (static) — fits the conceptual continuum.
- **Alternative**: put VTABLES on the far right (after STATIC). Rejected because the visual flow at dispatch ("value's vtable_ptr crosses panels") reads cleaner left-to-right.
- **Plan-phase**: confirm at UX checkpoint.

### R-016 — Dispatch arrow class + style

- **Decision**: new CSS class `.arrow-vtable-dispatch`. Initial style: **dashed orange, 2px stroke, animated dash** to convey "indirection / runtime lookup". Distinct from M06-7's solid blue/red/black palette.
- **Alternative**: muted purple solid line. Rejected — too similar to M07.2's BytesCopy arrow.
- **Plan-phase**: iterate at UX checkpoint with user.

## Protocol

### R-017 — 11th invocation of the closed-enum-with-revisions rule

- **Decision**: amend M03's contract. Additions:
  - **New `Ty` variants**: `DynRef { trait_name, mutable }` and `BoxDyn { trait_name }`.
  - **New `Value` variants**: `DynRef { borrow_id, target, vtable, mutable, trait_name }` and `BoxDyn { addr, vtable, trait_name }`.
  - **New addressing namespace**: `VtableAddr(u32)` newtype (analog of `StaticAddr` from M07.2).
  - **New `MemEvent` variant**: `VtableAlloc { addr, trait_name, type_name, methods, span }` (analog of `StaticAlloc`).
  - **NO new `Pointee` variants** — vtables ride on a separate field on `Value::DynRef` / `Value::BoxDyn`.
  - **AST additions** (parser-side): `Type::DynTrait`, `Expr::Cast`. Not part of wire protocol.
- **Precedent chain**: M03.1 → M03.2 → M06 → M07 → M07.1 → M07.2 → M07.3 → M07.4 → M07.5 → M07.6 → **M07.7**.
- **Snapshot byte-identity**: M03 stays byte-identical for programs that don't construct trait objects (additive variants only; no existing samples use dyn). M01/M02 should also stay byte-identical (no Debug-format shape changes for existing AST nodes).

### R-018 — First new MemEvent variant since M07.2

- **Observation**: M07.3 through M07.6 added zero new `MemEvent` variants (just additive Ty/Value variants and string-format conventions). M07.7 adds `VtableAlloc` — the first new event since M07.2's `StaticAlloc` + `BytesCopy`.
- **Why now**: trait objects introduce a NEW memory region (vtables) that's distinct from stack/heap/static. The vtable allocations need their own event so the UI can populate the VTABLES panel. Without `VtableAlloc`, there'd be no signal to materialize a vtable box; we'd have to scan all `Value::DynRef` events post-hoc to discover vtables — fragile.
- **Pattern match with M07.2**: M07.2 introduced `StaticAlloc` for the same reason (new memory region = new alloc event). M07.7's `VtableAlloc` is the direct analog. Content-deduplicated (one per `(trait, type)` pair); never freed.
