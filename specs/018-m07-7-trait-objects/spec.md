# Feature Specification: M07.7 — Trait objects (`&dyn Trait`, vtables, dynamic dispatch)

**Feature Branch**: `018-m07-7-trait-objects`
**Created**: 2026-05-24
**Status**: Draft
**Input**: User description: "M07.7 — trait objects: &dyn Trait, vtables, dynamic dispatch"

**Authoritative scope source**: [`MILESTONES.md` › M07.7 — Trait objects](../../MILESTONES.md). The In scope / Out of scope / Entry criteria / Exit criteria / Demo lines in that block are the contract for this feature. This spec elaborates user stories, requirements, and success criteria; it does not redefine scope.

## User Scenarios & Testing *(mandatory)*

M07.7 introduces **trait objects** — Rust's dynamic-dispatch mechanism. M07.6 shipped static dispatch via generic bounds (`fn print<T: Show>(x: T)` — compile-time monomorphization, distinct `print::<Point>` frame per concrete type). M07.7 ships dynamic dispatch (`fn print(x: &dyn Show)` — ONE compiled `print`, runtime vtable lookup per call site).

The headline pedagogy is the **fat pointer + vtable**: a `&dyn Show` value is 16 bytes (data ptr + vtable ptr) — twice as wide as `&T`'s 8-byte single pointer. The vtable ptr targets a per-`(type, trait)` table in a new VTABLES panel. Method dispatch is visible as a two-step indirection: value's vtable_ptr → vtable box, then vtable's method slot → method body. The contrast with M07.6's monomorphization makes the runtime/compile-time tradeoff tangible.

### User Story 1 - Basic `&dyn Trait` borrow + method dispatch (Priority: P1)

A learner writes `trait Show { fn show(&self) -> i32; } impl Show for Point { ... } let d: &dyn Show = &p; let s = d.show();`. The stacks panel shows `d : &dyn Show` rendered as a **fat pointer** — two labeled cells in the slot's value area: `data: → p` and `vtable: → <Point as Show>`. A new VTABLES panel (alongside STATIC MEMORY) shows the `<Point as Show>` vtable as a box containing the trait's methods. The `d.show()` call dispatches through the vtable: at the call step, the value's vtable_ptr lights up, an arrow points at the vtable box, and a second arrow points from the vtable's `show` slot to the method body (or the resulting frame card when it opens).

**Why this priority**: THIS is the foundational pedagogy. Without the fat-pointer rendering + vtable visualization, dynamic dispatch is just "method call that works somehow". With it, the runtime indirection becomes concrete. P1.

**Independent Test**: load `m07_7_dyn_basic.rs`, step past `let d: &dyn Show = &p`, observe d's fat-pointer slot + the new VTABLES panel with the `<Point as Show>` vtable. Step past `d.show()`, observe the two-step dispatch arrow.

**Acceptance Scenarios**:

1. **Given** `trait Show { fn show(&self) -> i32; } impl Show for Point { fn show(&self) -> i32 { self.x } } let d: &dyn Show = &p;`, **When** the pipeline runs, **Then** typeck succeeds with `d : &dyn Show`; d's slot renders as a fat pointer with `data` and `vtable` cells; the VTABLES panel shows the `<Point as Show>` vtable.
2. **Given** `let s = d.show();`, **When** the pipeline runs, **Then** typeck dispatches via the vtable; eval emits a `FrameEnter` for `<Point as Show>::show` (same UFCS-style format as M07.6 static dispatch); `s = 1_i32`.
3. **Given** the call step, **When** the user observes the visualization, **Then** TWO dispatch arrows are visible: data → p (the receiver), and vtable_ptr → vtable box → method.
4. **Given** the explicit coercion `let d: &dyn Show = &p as &dyn Show;`, **When** the pipeline runs, **Then** it works identically to the implicit coercion form `let d: &dyn Show = &p;`.

---

### User Story 2 - `&dyn Trait` parameter + implicit coercion (Priority: P1)

A learner writes `fn print(x: &dyn Show) -> i32 { x.show() } let r = print(&p);`. Inside `print`, x is `&dyn Show`; the body's `x.show()` dispatches dynamically. Unlike M07.6's `fn print<T: Show>(x: T)` (which produces `print::<Point>` per type), this version produces ONE `print` frame regardless of how many types call it. The pedagogical contrast is the headline learning moment.

**Why this priority**: this IS the polymorphism payoff contrast. M07.6's static dispatch generates a frame per type at the call site; M07.7's dynamic dispatch generates ONE frame and resolves the call internally. Side-by-side, the cost-model difference becomes concrete. P1.

**Independent Test**: load `m07_7_dyn_param.rs`, step through `print(&p)`, observe ONE `print` frame (no monomorphization) containing a nested vtable-dispatch flow.

**Acceptance Scenarios**:

1. **Given** `fn print(x: &dyn Show) -> i32 { x.show() } let r = print(&p);`, **When** the pipeline runs, **Then** typeck succeeds with `r : i32`; trace contains ONE `FrameEnter` named `print` (no `print::<Point>` mangling); the body's `x.show()` produces a nested `<Point as Show>::show` frame via vtable dispatch.
2. **Given** `print(&p)` AND `print(&q)` where both Point and Q impl Show, **When** the pipeline runs, **Then** TWO separate calls to the SAME `print` frame fire (not two distinct mangled frames). Each call's inner dispatch resolves to the appropriate type's vtable.
3. **Given** `print(&5)` where `i32: Show` is NOT implemented, **When** typeck runs, **Then** error "the type `i32` cannot be coerced to `&dyn Show` because it does not implement `Show`".
4. **Given** implicit coercion `print(&p)` (no `as &dyn Show`), **When** the pipeline runs, **Then** it works — Rust's standard implicit-coercion-at-fn-arg behavior.

---

### User Story 3 - `Box<dyn Trait>` (heap-owning trait object) (Priority: P1)

A learner writes `let b: Box<dyn Show> = Box::new(p);`. The heap allocation stores the Point's data; b is a fat pointer (heap data ptr + vtable ptr). Method calls via b dispatch through the vtable. This combines M07's `Box` machinery with M07.7's vtable dispatch.

**Why this priority**: `Box<dyn Trait>` is the standard pattern for heterogeneous heap-stored polymorphism. Without it, dynamic dispatch is limited to borrows of stack values — a learner can't see "the dyn trait owns its data on the heap, with a vtable pointer alongside". P1.

**Independent Test**: load `m07_7_box_dyn.rs`, step past `let b: Box<dyn Show> = Box::new(p);`, observe the heap block + the vtable in the VTABLES panel + b's fat-pointer slot.

**Acceptance Scenarios**:

1. **Given** `let b: Box<dyn Show> = Box::new(p);`, **When** the pipeline runs, **Then** typeck succeeds with `b : Box<dyn Show>`; a heap allocation stores the Point's bytes; b's slot renders as a fat pointer with data (heap addr) + vtable.
2. **Given** `let s = b.show();`, **When** the pipeline runs, **Then** the call dispatches through the vtable; emits `FrameEnter` for `<Point as Show>::show`; `s = 1_i32`.
3. **Given** b goes out of scope, **When** the cursor passes `}`, **Then** the heap allocation is freed (existing M07 Drop machinery); the vtable persists in the VTABLES panel (vtables never deallocate).

---

### User Story 4 - Static vs dynamic dispatch side-by-side (Priority: P2) 🎯 HEADLINE CONTRAST

A learner writes a paired-comparison sample with two functions: `fn s<T: Show>(x: T) -> i32 { x.show() }` (M07.6 static) and `fn d(x: &dyn Show) -> i32 { x.show() }` (M07.7 dynamic). Both called with the same Point. The trace shows:
- Static call: `s::<Point>` frame (monomorphized — same source, type in the name)
- Dynamic call: `d` frame (one body, no mangling) containing a vtable dispatch arrow

**Why this priority**: this is the SHIP-DEFINING pedagogical contrast. Once a learner has seen both side-by-side, the static-vs-dynamic dispatch tradeoff is no longer abstract — it's a visual difference in the trace. P2 (not P1 because US1-US3 are foundational; this is the "internalize the difference" cap).

**Independent Test**: load `m07_7_static_vs_dyn.rs`, step through both calls, observe the two distinct dispatch flows in the trace.

**Acceptance Scenarios**:

1. **Given** `fn s<T: Show>(x: T) -> i32 { x.show() } fn d(x: &dyn Show) -> i32 { x.show() } let a = s(p); let b = d(&p);`, **When** the pipeline runs, **Then** typeck succeeds; the trace shows `s::<Point>` frame for the static call AND `d` frame (no mangling) for the dynamic call.
2. **Given** the dynamic call's body, **When** the cursor enters `x.show()`, **Then** the vtable-dispatch arrow is visible AND a nested `<Point as Show>::show` frame opens.
3. **Given** the static call's body, **When** the cursor enters `x.show()`, **Then** NO vtable arrow appears (compile-time-resolved); a nested `<Point as Show>::show` frame opens via direct dispatch.

---

### Edge Cases

- **`&dyn Trait` for a trait the type doesn't impl** — typeck error: "the type `<T>` cannot be coerced to `&dyn <Trait>` because it does not implement `<Trait>`".
- **Implicit coercion vs explicit `as`** — both work; `as` is purely syntactic, no different semantic.
- **`&mut dyn Trait`** — IN scope. Similar to `&dyn Trait` but the underlying borrow is mutable. Required for mutating dispatch (`d.mutate()` where `d: &mut dyn Foo` and `Foo::mutate(&mut self)`).
- **`Box<dyn Trait>` going through `Box::new(...)`** — IN scope. Combines M07's Box machinery (heap allocation + Drop) with vtable dispatch.
- **Vtable for the same (type, trait) pair across multiple trait-object call sites** — typeck/eval interns: only ONE vtable per (type, trait) pair; multiple `&dyn Show` borrows of Point all point at the same vtable box. The VTABLES panel grows by one per unique pair.
- **Bound `T: Show` with `&dyn Show` arg** (`fn foo<T: Show>(x: T); foo(&p)` where the arg is a borrow of a trait object) — out of scope; M07.7 doesn't try to combine bound-generic with trait-object args. Each is its own dispatch style.
- **Calling default-method through dyn** — IN scope. If the trait has a default method and the impl doesn't override, dispatching `d.default_method()` through `d: &dyn Trait` finds the default body. Reuses M07.6's default-method machinery; the vtable's default-method slot points at the trait's default body (not the impl's).
- **Multi-trait objects `&dyn A + B`** — out of scope (Rust mostly doesn't allow this either; only one trait + auto-traits).
- **Upcasting `&dyn Child` → `&dyn Parent`** — out of scope (no supertraits in M07.6).
- **`?Sized` and custom DSTs** — out of scope. Only `dyn Trait` (always behind a borrow / Box).
- **Trait-object safety enforcement for hypothetical violations** — M07.6 already restricts the violation patterns (no Self return, no generic methods); M07.7 inherits, no new enforcement needed.
- **`Vec<Box<dyn Trait>>` heterogeneous collection** — out of scope explicitly. Combining Vec + Box + dyn is too rich for the headline; defer to a stretch goal or future milestone.
- **`fn` pointers as values** — unrelated; deferred.
- **Bare `dyn Trait` (not behind borrow/Box)** — out of scope; unsized type requires indirection.
- **Trait-object methods with explicit `&self` substitution at the vtable** — invisible to the learner; vtable just shows method names mapped to bodies.
- **Inherent-impl method on a trait-object value** (`d.inherent_method()` where d: `&dyn Show` and Point has an inherent `fn inherent_method`) — out of scope. Trait-object values only expose the trait's methods (Rust's standard behavior). Reject with clear error: "method `<name>` is not in trait `<Trait>` (trait objects can only call trait methods)".
- **Reference to a temporary trait object** (`(&Point { x: 1, y: 2 }) as &dyn Show`) — out of scope; the receiver must be a directly-bound local (matches M07.4-7 receiver restriction).
- **`impl Trait` in argument position** (`fn foo(x: impl Show)`) — out of scope; sugar for generic bound (M07.6) but distinct syntax. Deferred.
- **`impl Trait` in return position** (`fn make() -> impl Show`) — out of scope; would need anonymous-type machinery.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST parse `&dyn TraitName` and `&mut dyn TraitName` as a new `Type::DynTrait { trait_name: String, mutable: bool, span: Span }` AST type shape. Lexer keyword `dyn`.
- **FR-002**: System MUST parse `Box<dyn TraitName>` via the existing `Type::Generic` shape; the inner type `dyn TraitName` parses as `Type::DynTrait` and the generic accepts it.
- **FR-003**: System MUST parse `&p as &dyn Show` explicit coercion. Token: `as` keyword (likely already exists; verify).
- **FR-004**: System MUST extend the type lattice with `Ty::DynRef { trait_name: String, mutable: bool }` (the borrow-and-trait-object form; the actual heap-form for Box wraps this).
- **FR-005**: System MUST extend the value representation with `Value::DynRef { borrow_id, target: Pointee, vtable: VtableAddr, mutable, trait_name }`. The target is a Pointee (Slot for stack borrows; Heap for `Box<dyn Trait>`). The vtable is a new addressing namespace.
- **FR-006**: System MUST add `VtableAddr(u32)` as a new addressing newtype (analog of `StaticAddr` from M07.2). Each `(type, trait)` pair gets exactly one vtable; the addr is interned (content-deduplicated by `(type, trait)`).
- **FR-007**: System MUST add `MemEvent::VtableAlloc { addr, trait_name, type_name, methods: Vec<String>, span }` (analog of M07.2's `StaticAlloc`). Fires ONCE per unique `(trait, type)` pair when the first `&dyn Trait` value is constructed targeting that type.
- **FR-008**: System MUST add a new "VTABLES" UI panel (alongside STATIC MEMORY). Each vtable renders as a labeled box showing the trait's method names with their dispatch target (`<Type as Trait>::method_name`).
- **FR-009**: System MUST extend `SlotRowView` with a `dyn_view: Option<DynView>` field carrying the fat-pointer rendering data: data label (binding name or heap addr), vtable label (`<Point as Show>`).
- **FR-010**: System MUST visualize trait-object dispatch as TWO arrows at the call site: (a) the value's data ptr → the receiver location (existing borrow-arrow path), (b) the value's vtable ptr → the vtable box → the resolved method's frame card.
- **FR-011**: System MUST extend `typecheck_method_call` to handle `Ty::DynRef`-typed receivers: look up the trait's methods via the existing `TraitRegistry`; dispatch is dynamic at eval time but typecheck against the trait's declared signature.
- **FR-012**: System MUST extend `eval_method_call` to handle `Value::DynRef` receivers: look up the body via `trait_impl_bodies[(trait, concrete_type_from_vtable, method)]` first; fall through to `trait_default_bodies[(trait, method)]` for non-overridden defaults. Build the mangled frame name `<ConcreteType as Trait>::method`.
- **FR-013**: System MUST emit ONE `print` frame per call site for `fn print(x: &dyn Show)`, regardless of how many concrete types are passed across call sites. Matches Rust's "trait objects compile to one fn, runtime indirection" semantic.
- **FR-014**: System MUST reject coercion from `&T` to `&dyn Trait` when `T` doesn't implement `Trait` — typeck error with both the type and trait named.
- **FR-015**: System MUST reject inherent-impl methods called via `&dyn Trait` (only trait methods reachable through trait objects).
- **FR-016**: System MUST handle default methods through dyn dispatch identically to M07.6 (fall through to trait default body when impl doesn't override).
- **FR-017**: System MUST ship at least 4 new reference programs (`tests/samples/m07_7_*.rs` + `web/samples/`) covering: basic &dyn cast + dispatch, &dyn parameter (single-type call), Box<dyn Trait>, static-vs-dyn comparison.
- **FR-018**: System MUST preserve all M01–M07.6 existing tests byte-identical for programs that don't use trait objects. Snapshots stay byte-identical via serde-default-empty on new fields.
- **FR-019**: System MUST intern vtables — emit `VtableAlloc` ONCE per unique `(trait, type)` pair, regardless of how many `&dyn Trait` values point at it.

### Key Entities

- **Type::DynTrait** (AST type): `{ trait_name: String, mutable: bool, span: Span }`. Wraps the trait name + mutability inside an `&` reference. Pure syntactic; resolved at typeck.
- **Ty::DynRef** (typeck type): `{ trait_name: String, mutable: bool }`. The borrow form of a trait object. Distinct from `Ty::Ref { Ty::Struct(_), .. }` because the receiver type is unknown at compile time — it's whatever concrete type satisfies the bound at the call site.
- **Value::DynRef** (runtime value): the fat pointer — `{ borrow_id, target: Pointee, vtable: VtableAddr, mutable, trait_name }`. Sibling of `Value::Ref` and `Value::Slice` (both fat-pointer-shaped).
- **VtableAddr**: stable identifier for a vtable instance; content-deduplicated by `(trait, type)` pair (matches the linker-merging pattern from M07.2's static memory).
- **MemEvent::VtableAlloc**: emitted once per unique `(trait, type)` pair when the first `&dyn Trait` value is constructed targeting that type.
- **DynView** (UI): per-slot fat-pointer rendering carrier — `{ data_label: String, vtable_label: String, vtable_addr: u32 }`.
- **VtableView** (UI): per-vtable rendering in the VTABLES panel — `{ addr, trait_name, type_name, methods: Vec<(String, String)> }` where each method is `(name, dispatch_target_label)`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After M07.7 ships, `let d: &dyn Show = &p; let s = d.show();` typechecks; d's slot renders as a fat pointer with two labeled cells (data, vtable); a VTABLES panel appears showing the `<Point as Show>` vtable.
- **SC-002**: The `d.show()` step produces TWO visible dispatch arrows: data → p AND vtable_ptr → vtable box → method.
- **SC-003**: A frame `<Point as Show>::show` opens; `s = 1_i32`.
- **SC-004**: `fn print(x: &dyn Show) { x.show() } print(&p);` typechecks; trace contains ONE `print` frame (no monomorphization) AND a nested `<Point as Show>::show` frame.
- **SC-005**: `print(&p)` AND `print(&q)` (in the same program, where both impl Show) produce TWO calls to the same `print` frame — not two distinct mangled frames. Each inner dispatch resolves to the appropriate type's vtable.
- **SC-006**: `let b: Box<dyn Show> = Box::new(p);` typechecks; heap allocation visible; b renders as a fat pointer with heap addr in data + vtable in vtable.
- **SC-007**: `print(&5)` where `i32: Show` is not implemented → typeck error naming both `i32` and `Show`.
- **SC-008**: Calling an inherent (non-trait) method through `&dyn Trait` → typeck error.
- **SC-009**: Default-method dispatch through dyn works — impl provides only `count`, calling `d.double()` on a `d: &dyn Counter` routes to the trait's default body.
- **SC-010**: Vtable interning — emitting `VtableAlloc` ONCE per unique `(trait, type)` pair across the entire trace. Multiple `&dyn Show` borrows of Point all share one vtable.
- **SC-011**: Side-by-side comparison sample shows distinct dispatch flavors in the trace: static `s::<Point>` (per-type frame mangling) vs dynamic `print` (one frame, vtable lookup).
- **SC-012**: ≥ 4 new `m07_7_*.rs` reference programs ship.
- **SC-013**: Existing M01–M07.6 tests pass byte-identical (additive AST/Value fields, serde-default-empty preserves existing snapshots).
- **SC-014**: WASM bundle growth ≤ +25% vs M07.6 baseline (~378 KB → ≤ ~473 KB raw). Substantial new surface: AST node, Value variant, MemEvent variant, VTABLES panel + CSS + dispatch arrow rendering, fat-pointer slot rendering.
- **SC-015**: Zero warnings under `RUSTFLAGS="-D warnings" cargo build --release` AND `cargo test`. Both host and WASM targets clean.

## Assumptions

- **Single-trait objects only**: `&dyn Show + Counter` rejected. Rust mostly doesn't allow this either; matches the M07.6 single-bound-per-T-position restriction.
- **No bare `dyn Trait`**: unsized types require indirection; M07.7 only supports `&dyn`, `&mut dyn`, and `Box<dyn>`.
- **No `impl Trait` argument/return-position sugar**: deferred.
- **Vtable interning by `(trait, type)`**: matches Rust's actual linker behavior (one vtable per pair across the whole binary). Visually: one VTABLES box per pair, never more.
- **Vtables persist for trace duration**: never deallocate (analog of static memory blocks from M07.2).
- **`Box<dyn Trait>` IN scope**: the standard heap-owning trait-object pattern. Combines M07's Box machinery (alloc + Drop) with M07.7's vtable dispatch.
- **`Vec<Box<dyn Trait>>` OUT of scope explicitly**: combining Vec + Box + dyn is too much for the headline scope; defer to a future stretch milestone.
- **Frame-name format for trait-object dispatch**: same `<Point as Show>::show` UFCS-style as M07.6. The runtime-resolved concrete type appears in the frame name — so static and dynamic dispatch produce IDENTICAL inner frames once dispatch resolves; only the outer frame differs (`print::<Point>` for static, `print` for dynamic).
- **Trait-object methods through default-method dispatch**: works identically to M07.6 default methods (vtable's method slot points at the trait's default body when no impl override exists).
- **Inherent methods unreachable through dyn**: trait objects only expose the trait's methods. M07.7 rejects `d.inherent()` with a clear error.
- **`as &dyn Trait` explicit cast IN scope**: standard Rust syntax; required for some disambiguation cases AND pedagogically clean (makes the coercion explicit in the source).
- **Implicit coercion at fn-arg sites IN scope**: standard Rust ergonomics; `print(&p)` works without explicit `as`.
- **Object-safety check** (no `Self` return, no generic methods): M07.6 already enforces both at the trait-decl level — every M07.6-valid trait is automatically object-safe. M07.7 inherits the restrictions; no new check needed.
- **UI surface is the meaty piece**: similar to M07.4's struct view. The fat-pointer rendering in the slot + the VTABLES panel + the two-step dispatch arrows are the iterative pieces. **Expect a UX checkpoint after the first cut** before iterating on visual polish.
- **Bundle target ≤ +25%**: substantial new surface (AST node, Value variant, MemEvent variant, VTABLES panel + CSS, fat-pointer rendering + CSS, dispatch arrow rendering). Comparable to M07.4's struct-view investment.
- **Sized XL** per the rubric: ~5-6 source modules touched (parse/{ast, parser, lexer for `dyn`}, typeck, eval, ui [vtables panel + fat-pointer + arrow]) + 4 sample pairs + ≥ 10 unit tests. Estimated ~1500-1800 LOC net change. Comparable to or slightly larger than M07.4 (the new UI panel + arrows + fat-pointer rendering push the upper bound).
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION because reasonable defaults exist):
  1. **`Pointee::Vtable(VtableAddr)` vs separate `vtable: VtableAddr` field on `Value::DynRef`** — recommendation: separate field on `Value::DynRef`, since vtables aren't borrow targets like Slot/Heap/Static.
  2. **Vtable interning timing** — eagerly at typeck phase 1 (one VtableAlloc per used `(trait, type)` pair) vs lazily at first construction. Recommendation: lazy (matches M07.2's StaticAlloc pattern; only used vtables get emitted).
  3. **Dispatch arrow CSS / visual styling** — iterative; UX checkpoint after first cut. Two arrows means more visual weight; may need different stroke / color / animation. Recommendation: dashed orange or muted blue for the vtable indirection (visually distinct from M07.4's solid black ownership / blue shared-borrow / red mutable-borrow arrows).
- **Foundation completion**: M07.7 closes the Level 4 polymorphism story (M07.5 generics + M07.6 traits-static + M07.7 traits-dynamic). The project ships every Rust polymorphism mechanism a learner needs.
