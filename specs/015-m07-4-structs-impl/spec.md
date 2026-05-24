# Feature Specification: M07.4 — Structs + `impl` blocks (named-field composite types with methods)

**Feature Branch**: `015-m07-4-structs-impl`
**Created**: 2026-05-24
**Status**: Draft
**Input**: User description: "M07.4 — structs with named fields + impl blocks (methods + associated fns)"

**Authoritative scope source**: [`MILESTONES.md` › M07.4 — Structs + impl blocks](../../MILESTONES.md). The In scope / Out of scope / Entry criteria / Exit criteria / Demo lines in that block are the contract for this feature. This spec elaborates user stories, requirements, and success criteria; it does not redefine scope.

## User Scenarios & Testing *(mandatory)*

M07.4 introduces user-defined `struct` types with named fields, along with `impl` blocks providing associated functions and methods. Structs are the primary tool a learner uses to **model data** — without them, every example is a synthetic toy mixing primitives and stdlib types. This milestone gives the learner the ability to write `struct Point { x: i32, y: i32 }`, construct values with the literal syntax, access fields, take borrows of individual fields, and define behavior in `impl` blocks.

The pedagogical headline is the **byte layout**: a struct's fields lay out contiguously in the stack slot, each at a known offset. The slot's visual shows one byte-cell strip per field (labeled with the field name) so the learner sees the memory composition. A field borrow `&p.x` produces a slot-target reference with a `.x` annotation — hover highlights just the bytes of field `x`, making "borrows can target a sub-region of a value" concrete.

`impl` blocks introduce the dispatch story: associated functions like `Point::new(1, 2)` and methods like `p.x()` extend the existing M07 method-dispatch infrastructure with a user-defined registry built during typeck.

### User Story 1 - Struct declaration, literal, and field access (Priority: P1)

A learner types `struct Point { x: i32, y: i32 } fn main() { let p = Point { x: 1, y: 2 }; let a = p.x; }`. The stacks panel shows `p : Point` with two inline byte-cell strips (each 4 bytes for the i32 fields) labeled `x` and `y` showing values `1_i32` and `2_i32`. `let a = p.x` produces `a : i32 = 1_i32` via field access (copy semantics — `p` remains usable).

**Why this priority**: this IS the foundational pedagogy. Without struct decl + literal + field access, no other story is possible. The visible byte composition + field-name labels in the stack slot is the structural payoff. P1.

**Independent Test**: load `m07_4_struct_basic.rs`, step through; observe p's slot with two labeled byte-cell strips totaling 8 bytes, and `a = 1_i32` after the field access step.

**Acceptance Scenarios**:

1. **Given** `struct Point { x: i32, y: i32 } let p = Point { x: 1, y: 2 };`, **When** the pipeline runs, **Then** typeck succeeds with `p : Point`; the SlotWrite for `p` carries `Value::Struct { fields: [(x, Int{1}), (y, Int{2})], schema: PointSchema }` (exact shape plan-phase's call); the page renders `p`'s slot with 8 byte-cells in two strips labeled `x` and `y`.
2. **Given** `let a = p.x;`, **When** the pipeline runs, **Then** `a`'s SlotWrite carries `Value::Int { kind: I32, bits: 1 }`; `p` remains usable (no SlotMove on the field access).
3. **Given** field-shorthand syntax `let x = 1; let y = 2; let p = Point { x, y };`, **When** the pipeline runs, **Then** typeck succeeds and the struct is constructed identically to `Point { x: x, y: y }`.
4. **Given** missing-field literal `let p = Point { x: 1 };`, **When** the pipeline runs, **Then** typeck error "missing field `y` in struct literal `Point`".
5. **Given** extra-field literal `let p = Point { x: 1, y: 2, z: 3 };`, **When** the pipeline runs, **Then** typeck error "no field `z` on struct `Point`".
6. **Given** wrong-type field literal `let p = Point { x: true, y: 2 };`, **When** the pipeline runs, **Then** typeck error pointing at the `true` with "expected i32, found bool".
7. **Given** scope exit at `}`, **When** the cursor passes it, **Then** `p`'s slot disappears with the frame (no per-field destructor event in M07.4 since fields are Copy primitives — Drop pedagogy reserved for future non-Copy field types).

---

### User Story 2 - Field borrow `&p.x` with per-field hover (Priority: P1)

A learner types `let r = &p.x;`. The stacks panel shows `r : &i32`. A blue borrow arrow connects `r`'s slot to `p`'s slot. **The arrow carries a `.x` field-name annotation** (analogous to slice arrows' `[len: N]` label). **Hovering the arrow lights up just the `x` field's byte-cells** in `p`'s slot, NOT the whole struct — making "borrows can target a sub-region of a composite value" tangible.

**Why this priority**: extends the borrow-into-composite pedagogy from slices (which view a range of elements) to structs (which view a named field). The per-field hover highlight is the structural payoff. P1.

**Independent Test**: load `m07_4_field_borrow.rs`, step past `let r = &p.x`, observe blue arrow with `.x` annotation; hover highlights only `x`'s bytes in `p`.

**Acceptance Scenarios**:

1. **Given** `let r = &p.x;`, **When** the pipeline runs, **Then** typeck succeeds with `r : &i32`; the borrow value carries field metadata identifying field `x` (exact shape — `Value::Ref` extension vs. new variant — plan-phase's call).
2. **Given** the borrow arrow renders, **When** the user observes the arrow overlay, **Then** the arrow displays a visible `.x` (or similar) field annotation distinguishing it from a whole-binding borrow.
3. **Given** the user hovers the arrow, **When** the hover handler fires, **Then** only `x`'s byte-cells in `p`'s slot light up (and only the `x` field-name label, if labels are highlightable).
4. **Given** the borrow's scope ends, **When** the cursor passes `}`, **Then** the borrow arrow disappears.
5. **Given** `let r = &p.z;` (nonexistent field), **When** the pipeline runs, **Then** typeck error "no field `z` on struct `Point`".

---

### User Story 3 - Method definition + dispatch (Priority: P1)

A learner types `impl Point { fn x(&self) -> i32 { self.x } } let v = p.x();`. The method call dispatches to the impl-block's method; `v` becomes `1_i32`. The method's body executes with `self` bound to `&p`.

**Why this priority**: methods are how Rust attaches behavior to types. Without methods, structs feel like passive data containers. The `self.x` field access inside the method body completes the pedagogy: "you can write functions that act on the struct's data using the same field-access syntax". P1.

**Independent Test**: load `m07_4_method.rs`, step past `let v = p.x()`, observe `v = 1_i32` after the method call returns.

**Acceptance Scenarios**:

1. **Given** `impl Point { fn x(&self) -> i32 { self.x } }`, **When** typeck runs, **Then** an entry `(Point, "x")` is added to the user-defined method dispatch table.
2. **Given** `let v = p.x();`, **When** the pipeline runs, **Then** typeck dispatches via the user table; eval enters a new frame for the method, binds `self` to `&p`, evaluates `self.x` → `1_i32`, returns `1_i32`; `v`'s SlotWrite carries `Value::Int { kind: I32, bits: 1 }`.
3. **Given** the method body, **When** the cursor enters the method frame, **Then** the stacks panel shows the new frame card with `self : &Point` row and the borrow arrow from `self` to the caller's `p` slot.
4. **Given** `impl Point { fn dist(&self) -> i32 { self.x } } let v = p.dist();`, **When** the pipeline runs, **Then** dispatch correctly resolves to `dist` (verifying multiple methods in one impl block work).
5. **Given** `impl Point { fn set_x(&mut self, v: i32) { ... } }`, **When** typeck checks `p.set_x(5)`, **Then** typeck requires `p` to be a `mut` binding (rejects with a clear error when not). Mutation through `self.x = v` itself is partial scope per M07.4 — plan-phase decides whether `&mut self` methods can actually modify fields, or only read.
6. **Given** `let v = p.bogus();` (no method `bogus` defined), **When** typeck runs, **Then** typeck error "no method `bogus` on type `Point`".

---

### User Story 4 - Associated function (no `self`) (Priority: P2)

A learner types `impl Point { fn new(x: i32, y: i32) -> Point { Point { x, y } } } let p = Point::new(1, 2);`. The path-call `Point::new(1, 2)` dispatches to the impl-block's associated function. `p` becomes a `Value::Struct` with `x=1, y=2`. Inside `new`, the field-shorthand `Point { x, y }` constructs the return value.

**Why this priority**: associated functions are the Rust-idiomatic constructor pattern. Useful but slightly less foundational than instance methods (US3 covers the dispatch machinery already). P2.

**Independent Test**: load `m07_4_associated_fn.rs`, step through `let p = Point::new(1, 2)`, observe a stack frame for `new` opens, runs, returns, then `p` is assigned the constructed struct.

**Acceptance Scenarios**:

1. **Given** `impl Point { fn new(x: i32, y: i32) -> Point { Point { x, y } } }`, **When** typeck runs, **Then** an entry `(["Point", "new"], FnSig)` is added to the user-defined path-fn dispatch table.
2. **Given** `let p = Point::new(1, 2);`, **When** the pipeline runs, **Then** the path-call dispatches via the user table; eval enters a new frame for `new` with `x = 1` and `y = 2` parameter bindings; the body constructs `Point { x, y }` (field-shorthand); the frame returns the struct; `p`'s SlotWrite carries the new struct.
3. **Given** mixing user-defined and built-in path calls (`let v: Vec<i32> = Vec::new(); let p = Point::new(1, 2);`), **When** the pipeline runs, **Then** dispatch correctly resolves Vec::new to the M07 hardcoded built-in AND Point::new to the user-defined impl entry.

---

### Edge Cases

- **Empty struct** `struct Empty {}` — typeck-rejected ("structs in M07.4 must have at least one field"). Out of scope to support; learners would just use `()` for that purpose.
- **Single-field struct** `struct Wrap { v: i32 }` — valid; renders one byte-cell strip.
- **Three+ fields** `struct Box3D { x: f64, y: f64, z: f64 }` — valid; renders three strips with mixed sizes if field types differ.
- **Field access on `&Self`** inside a method (`self.x` where `self: &Self`) — auto-deref applies; `self.x` works the same as `(*self).x` for field READS. Plan-phase confirms whether method-body field access is shorthand-compiled to deref + access or has a direct AST handling.
- **Field assignment** `p.x = 5;` — partial scope. With `let mut p`, plan-phase decides whether to support this (extends M06.1's place-expression set to include `Expr::FieldAccess`). Default: support read-only field access in M07.4; field assignment deferred.
- **Self field assignment in method** `&mut self` methods doing `self.x = 5;` — same as above; depends on plan-phase decision.
- **Struct declared inside a function** — out of scope (only file-level struct declarations).
- **Methods returning self** (`fn clone_x(&self) -> Point { Point { x: self.x, y: self.x } }`) — should work since Copy types allow returning new structs.
- **Method calling another method** (`fn double_x(&self) -> i32 { self.x() * 2 }`) — should work; method dispatch resolves recursively.
- **Recursive method** (`fn ackermann(self, n: i32) -> i32 { if n == 0 { 1 } else { self.ackermann(n - 1) } }`) — should work; recursion depth limit applies (existing M03 behavior).
- **Path with `Point::` and unknown function** `Point::bogus()` — typeck error "no associated function `bogus` for type `Point`".
- **Method call with mismatched arg types** — typeck error matching the method's parameter type.
- **Two impl blocks for the same struct** — out of scope; M07.4 supports one impl block per struct. Defining `impl Point { fn x() }` and `impl Point { fn y() }` separately produces a typeck error at the second block: "M07.4 supports only one impl block per struct; merge into a single block".
- **Method with same name as a struct field** (`struct Point { x: i32 } impl Point { fn x(&self) -> i32 { self.x } }`) — both `p.x` (field access) and `p.x()` (method call) are syntactically distinguishable via the trailing `()`. Both work simultaneously. Pedagogically interesting.
- **Forward reference** — `impl` block referencing a struct declared later in the same file should work (typeck does a 2-pass: collect struct + impl declarations, then check bodies).
- **Field types with the same name as the struct** (`struct A { a: A }`) — recursive struct, out of scope (no `Box` indirection to break the cycle in M07.4 since field types are restricted to primitives).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST parse `struct Name { field1: T1, field2: T2, ... }` as a new top-level `Item::Struct` declaration. At least one field required.
- **FR-002**: System MUST parse `Path { field1: expr1, field2: expr2, ... }` as a new `Expr::StructLit` expression. Field-shorthand `Path { x, y }` (when local has same name as field) MUST be supported.
- **FR-003**: System MUST parse `receiver.name` (where name is an identifier NOT followed by `(`) as a new `Expr::FieldAccess` expression. Postfix `(` continues to mean method call (M07's `Expr::MethodCall`).
- **FR-004**: System MUST parse `impl Path { fn item1; fn item2; ... }` as a new top-level `Item::Impl` declaration. Items are function declarations with optional self-receivers.
- **FR-005**: System MUST extend `parse_fn_decl` (or equivalent) to accept self-receivers: `&self`, `&mut self`, `self` as the first parameter (no type — type is inferred from the enclosing impl block).
- **FR-006**: System MUST extend the type lattice with a `Struct` representation carrying the field schema (field names + types in declaration order).
- **FR-007**: System MUST extend the value representation with a `Struct` variant carrying the field values in declaration order.
- **FR-008**: System MUST typecheck struct literals: every declared field must appear; no extra fields; each field's expression type must match its declared type (with literal-narrowing via `try_coerce_to`); field-shorthand resolves to the bound local of the matching name.
- **FR-009**: System MUST typecheck field access: receiver must be of struct type; field name must exist in the struct's schema; result type is the field's type.
- **FR-010**: System MUST typecheck field borrow `&p.x`: result is `&FieldTy` (Ref to the field's type). The borrow's `Value::Ref` carries field metadata enabling per-field hover-highlight (exact shape — extending Ref with `field_name: Option<String>` vs. a new `Value::FieldRef` variant — plan-phase's call).
- **FR-011**: System MUST collect `impl` blocks during typeck and build a user-defined dispatch registry: method table `(struct_name, method_name) → FnSig` and path-fn table `Vec<String> → (FnSig, kind)`. Built before any function body typeck (2-pass).
- **FR-012**: System MUST extend method-call typeck dispatch to consult the user-defined method table after the M07 hardcoded built-ins.
- **FR-013**: System MUST extend path-call typeck dispatch to consult the user-defined path-fn table after the M07 hardcoded built-ins.
- **FR-014**: System MUST evaluate struct literals by constructing `Value::Struct { fields: [(name, value), ...], schema }`.
- **FR-015**: System MUST evaluate field access by looking up the field in `Value::Struct.fields` and returning a clone.
- **FR-016**: System MUST evaluate field borrow by constructing `Value::Ref { target: Pointee::Slot(p_slot), .. }` with field metadata, AND registering the borrow in the active-borrow machinery (or lazy-materializing if Slot-target borrows skip BorrowShared events per M07.3 pattern).
- **FR-017**: System MUST evaluate method calls by entering a new frame for the impl-block's method, binding `self` to the receiver (by-ref for `&self` / `&mut self`, by-move for `self`), executing the body, returning the result.
- **FR-018**: System MUST evaluate associated function calls similarly to method calls but without the `self` binding (just bind parameters).
- **FR-019**: System MUST render struct values inline in the stack slot — one byte-cell strip per field, labeled with the field name above each strip. Visually consistent with M07.3's array inline cells but per-field instead of per-element.
- **FR-020**: System MUST render field-borrow arrows with a visible field-name annotation (analogous to slice arrows' `[len: N]`). Hover-highlight covers only the borrowed field's byte-cells in the source slot.
- **FR-021**: System MUST ship at least 4 new reference programs (`tests/samples/m07_4_*.rs` + `web/samples/`) covering: struct + field access, field borrow, method call, associated function call.

### Key Entities

- **Struct declaration** (`Item::Struct`): top-level AST item with `name: String`, `fields: Vec<(name, Type, Span)>`, `span: Span`. Declaration order determines byte layout AND drop order.
- **Struct literal** (`Expr::StructLit`): expression with `path: Vec<String>` (the struct name), `fields: Vec<(name: String, value: Expr, span: Span)>` (each maybe in field-shorthand form), `span: Span`.
- **Field access** (`Expr::FieldAccess`): expression with `receiver: Box<Expr>`, `name: String`, `span: Span`.
- **Impl block** (`Item::Impl`): top-level AST item with `ty_name: String` (the struct's name), `items: Vec<FnDecl>` (associated functions + methods), `span: Span`. Each fn_decl may have a self-receiver as its first param.
- **Self-receiver** (extension to `Param`): a sentinel param shape — `name = "self"`, `ty = inferred from impl block`, with a flag distinguishing `self` (owned), `&self` (shared borrow), `&mut self` (mutable borrow).
- **Struct type** (`Ty::Struct`): the type-system representation. Carries the struct's name + field schema (name + type per field). Two `Ty::Struct` are equal iff they're the same named type (nominal typing).
- **Struct value** (`Value::Struct`): the runtime representation. Carries the struct's name + field values in declaration order. Cloning deep-copies each field.
- **User-defined method table**: typeck-side registry mapping `(struct_name, method_name) → FnSig` for dispatch.
- **User-defined path-fn table**: typeck-side registry mapping `Vec<String> → FnSig` for `Type::function(args)` calls.
- **Field-annotated borrow**: `Value::Ref` (or new `Value::FieldRef`) carrying which field of the source slot the borrow points at — drives the per-field hover-highlight in the renderer.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After M07.4 ships, `struct Point { x: i32, y: i32 } let p = Point { x: 1, y: 2 };` typechecks; the page renders `p`'s slot with two labeled byte-cell strips (`x` 4 bytes, `y` 4 bytes); zero heap events fire.
- **SC-002**: `p.x` and `p.y` typecheck as `i32`; eval returns the correct values (1 and 2 respectively); `p` remains usable after each field access.
- **SC-003**: `&p.x` produces a borrow value with field metadata; the arrow renders with a `.x` annotation; hover highlights ONLY `x`'s bytes in `p`'s slot.
- **SC-004**: `impl Point { fn x(&self) -> i32 { self.x } } let v = p.x();` typechecks; method dispatches; new frame opens for `x` method; returns `1_i32`; `v` gets `1_i32`.
- **SC-005**: `impl Point { fn new(x: i32, y: i32) -> Point { Point { x, y } } } let p = Point::new(1, 2);` typechecks; associated function dispatches; constructs the struct.
- **SC-006**: Missing-field, extra-field, wrong-type-field, and unknown-method/field errors fire with clear messages.
- **SC-007**: ≥ 4 new `m07_4_*.rs` reference programs ship.
- **SC-008**: Existing M01–M07.3 tests pass byte-identical (additive `Ty::Struct` + `Value::Struct` variants don't affect existing variants' Debug output).
- **SC-009**: WASM bundle growth ≤ +25% vs M07.3 baseline (substantial new surface: AST nodes for struct/impl, typeck registry for user-defined types/methods, eval method dispatch, UI inline rendering for structs).
- **SC-010**: Zero warnings under `RUSTFLAGS="-D warnings" cargo build --release` AND `cargo test`. Both host and WASM targets clean.

## Assumptions

- **Field types restricted to primitives** in M07.4 (Int / Float / Bool / Unit). Non-Copy field types (Vec, String, Box, Slice, Array, &str, other Structs) — out of scope. Matches M07's Vec-of-primitives restriction. Future M07.x lifts this.
- **Field declaration order = byte layout order**: cells render left-to-right in declaration order; drop order would mirror this (though M07.4's primitive-only restriction means no observable destructors fire).
- **Single-segment struct paths only**: `Point` works; `mod::Point` doesn't (matches M07's restriction).
- **One impl block per struct**: multiple impl blocks for the same type — out of scope.
- **No generic structs / generic methods**: `Point<T>` — out of scope. Generic methods inside impl blocks — out of scope.
- **No traits / trait impls / trait objects**: structs + inherent impls only.
- **No derive macros**: `#[derive(Debug, Clone)]` — out of scope. Print/clone use ad-hoc methods.
- **No struct update syntax** `Point { x: 10, ..p }` — out of scope.
- **No tuple structs** `struct Pair(i32, i32)` — out of scope.
- **No unit structs** `struct Marker;` — out of scope.
- **No pattern matching on struct fields** `let Point { x, y } = p;` — out of scope (no pattern matching anywhere yet).
- **Field-shorthand** `Point { x, y }` when local has same name — IN scope. Small parser concession with big ergonomic payoff.
- **Field assignment partial scope**: `p.x = 5;` (with `let mut p`) and `self.x = v;` inside `&mut self` methods — plan-phase decides whether to support. Default: support if it falls out naturally from extending M06.1's place-expression set to `Expr::FieldAccess`; otherwise defer.
- **Auto-deref for `self.x`**: methods take `&self` (typed `&Self`), so `self.x` is sugar for `(*self).x`. Plan-phase decides whether to handle as parser/typeck sugar or AST-level deref.
- **Slot-target field borrow lifecycle**: per M07.3's pattern, `Pointee::Slot(_)` targets skip `BorrowShared`/`BorrowEnd` events. The UI materializes the arrow lazily at SlotWrite time. Same machinery as M07.3.
- **Two-pass typeck**: a first pass collects all struct declarations + impl-block signatures (so forward references work — `impl Point` referencing struct declared later in file). A second pass typechecks function bodies with full type/method visibility.
- **`Value::Ref` field metadata shape**: plan-phase decides between extending `Value::Ref` with `field_name: Option<String>` (and matching `byte_offset` for the highlight) vs. introducing a new `Value::FieldRef` variant. Recommendation: extend Ref since it's a strict superset and matches the existing pattern.
- **Method-dispatch tie-breaker**: hardcoded M07 built-ins (`Vec::push`, etc.) always win over user-defined methods. Pedagogically clean — the builtins are part of "the standard library", user impls extend it.
- **Inline byte-cell rendering**: extends M07.3's per-element layout to per-field. The slot's value area shows: `[field-name][byte-cells]` per field, with cells grouped horizontally and fields stacked vertically.
- **Bundle target ≤ +25%**: substantial new surface — AST nodes, typeck registry, eval method-call frame entry, UI per-field rendering. Larger than M07.1/M07.2/M07.3 but still well-bounded.
- **Sized XL** per the rubric: ~5 source modules (parse/{ast,parser}, resolve, typeck, eval, ui) + per-field UI rendering + 4 sample pairs + ≥ 8 unit tests. Estimated ~1200-1500 LOC net change. Comparable to M07 (heap milestone).
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION because reasonable defaults exist):
  1. **`Value::Ref` field metadata** — extension vs. new variant. Recommendation: extension with `field_name: Option<String>` + computed byte offset.
  2. **Field-assignment scope** — IN if it falls out cleanly from M06.1's place-expression extension; otherwise defer. Plan-phase budgets the work.
  3. **Auto-deref handling** for `self.x` — parser-level sugar (rewriting `self.x` to `(*self).x` at parse time) vs. typeck-level rule (allowing `Expr::FieldAccess` on `&T`). Recommendation: typeck-level rule (more flexible).
- **Foundation for future work**: M07.4 unlocks user-defined types as a first-class concept. Future milestones (generics, traits, enums, derives) all layer on top. After M07.4 the project has shipped every "you can model your domain" tool a learner needs.
