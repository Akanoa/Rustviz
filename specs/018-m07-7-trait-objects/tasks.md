---

description: "Task list for M07.7 — Trait objects (`&dyn Trait`, vtables, dynamic dispatch)"
---

# Tasks: M07.7 — Trait objects (`&dyn Trait`, vtables, dynamic dispatch)

**Input**: Design documents from `/specs/018-m07-7-trait-objects/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/m07-7-protocol-delta.md ✓, quickstart.md ✓

**Tests**: M01/M02/M03 snapshots should stay byte-identical (additive Ty/Value/MemEvent variants + AST additions; no existing sample constructs trait objects). New `cargo test --lib pipeline::tests` covering: basic &dyn cast + dispatch, &dyn parameter (single call), Box<dyn Trait>, default-method through dyn, vtable interning, &i32→&dyn Show coercion error, inherent-method via dyn rejected, static-vs-dyn paired comparison. **≥ 10 new tests**. Manual M07.7 QA per the quickstart procedure with explicit UX checkpoint.

**Organization**: 4 user stories (US1+US2+US3 P1; US4 P2 — the ship-defining contrast). Sized XL — comparable to or slightly larger than M07.4. ~6 source modules + new VTABLES UI panel + fat-pointer slot rendering + two-step dispatch arrows + 4 sample pairs.

**EXPLICIT UX CHECKPOINT**: research R-014 (VTABLES panel + fat-pointer slot + dispatch arrows) is the iterate-on-this proposal. Phase 3 lands the first cut of Proposal A; Phase 3-end UX checkpoint pauses for user review before proceeding to Phases 4-7. Mirrors M07.4's struct-view workflow.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1/US2/US3/US4 tag, mandatory on user-story phases only
- File paths relative to repo root

## Path Conventions

~6 existing source files modified + new VTABLES UI panel (HTML/CSS/JS additions) + 4 sample pairs. See `specs/018-m07-7-trait-objects/plan.md` Project Structure.

---

## Phase 1: Setup

**Purpose**: pre-flight — confirm starting state.

- [X] T001 Verify pre-conditions: branch `018-m07-7-trait-objects` checked out; `cargo test` from `main` passes (baseline confirmed: 163 tests).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Lexer keywords (`dyn`, `as`) + AST surface (Type::DynTrait, Expr::Cast) + parser + resolve + new Ty/Value variants + VtableAddr + MemEvent::VtableAlloc + vtable-interning machinery + typeck cast + dispatch routing + eval Value::DynRef construction. Required by all four user stories. Single-pass cohesive scaffolding since trait-object surface cuts across parser/typeck/eval/ui.

- [X] T002 Amend M03's contract in `specs/004-m03-event-eval/contracts/m03-api.md` — append an entry under the closed-enum-with-revisions section noting M07.7 as the 11th invocation (additive Ty::DynRef/BoxDyn + Value::DynRef/BoxDyn + VtableAddr + MemEvent::VtableAlloc + AST Type::DynTrait/Expr::Cast). Reference `specs/018-m07-7-trait-objects/contracts/m07-7-protocol-delta.md`. First new MemEvent variant since M07.2.

- [X] T003 [P] In `src/parse/lexer.rs`, extend KEYWORDS with `"dyn"` → `TokenKind::Dyn`. Verify `"as"` — if absent, add `"as"` → `TokenKind::As`.

- [X] T004 [P] In `src/parse/token.rs`, add `TokenKind::Dyn` and (if not present) `TokenKind::As` variants. Update `TokenKind::describe()` to return `"`dyn`"` and `"`as`"`.

- [X] T005 In `src/parse/ast.rs`, add new AST surface:
  - Add `Type::DynTrait { trait_name: String, span: Span }` variant.
  - Add `Expr::Cast { inner: Box<Expr>, target_ty: Type, span: Span }` variant.
  - Update `Expr::span()` to cover `Cast { span, .. } => *span`.
  - Update `type_span` (in parser.rs) to cover `Type::DynTrait { span, .. } => *span`.

- [X] T006 In `src/parse/parser.rs`, extend parsing:
  - **`parse_type`** extension: when seeing `Amp`/`AmpMut` followed by `Dyn` keyword followed by an ident, parse as `Type::Ref { inner: Type::DynTrait { trait_name, span }, mutable, span }`. The dyn part itself goes through a new helper `parse_dyn_trait_type` that consumes `Dyn` + ident.
  - **`parse_type`** ALSO handles bare `dyn TraitName` (inside `Box<dyn Show>`): when seeing `Dyn` keyword at type-context entry (not preceded by `&`), call `parse_dyn_trait_type` and return `Type::DynTrait`. The wrapping `Box<_>` machinery handles the indirection.
  - **`parse_expr` postfix loop**: after any expression, if `As` token follows, parse a `Type`, build `Expr::Cast { inner: lhs, target_ty, span }`. Precedence: tighter than binary ops, looser than method calls (typical `as` precedence). The cast becomes the new `lhs` for continued postfix parsing.

- [X] T007 [P] In `src/resolve.rs`, traverse the new AST surface:
  - `resolve_expr` adds an `Expr::Cast { inner, .. }` arm recursing on `inner` only. The `target_ty` doesn't need resolve traversal (trait-name resolution happens at typeck).

- [X] T008 In `src/typeck.rs` + `src/event.rs`, add new `Ty` variants:
  - Add `Ty::DynRef { trait_name: String, mutable: bool }`.
  - Add `Ty::BoxDyn { trait_name: String }`.
  - Update `Ty::name()` to handle both.
  - Update `Ty::is_copy()` to handle both.
  - Update every existing `Ty` exhaustive match (typeck.rs + eval.rs + ui.rs) — typically adding `Ty::DynRef { .. } | Ty::BoxDyn { .. } => { .. }` arms.

- [X] T009 In `src/event.rs`, add new `Value` variants and infrastructure:
  - Add `Value::DynRef { borrow_id, target, vtable, mutable, trait_name }` variant.
  - Add `Value::BoxDyn { addr, vtable, trait_name }` variant.
  - Add `pub struct VtableAddr(pub u32);` newtype.
  - Add `MemEvent::VtableAlloc { addr, trait_name, type_name, methods: Vec<String>, span }` variant.
  - Update `Value::type_name()` for the new variants (return `"&dyn"`, `"Box<dyn>"`).
  - Update every existing `Value` exhaustive match (eval.rs + ui.rs) — typically adding new arms.
  - Update every existing `MemEvent` exhaustive match (eval.rs + ui.rs) — typically adding a `VtableAlloc { .. } => { .. }` arm.

- [X] T010 In `src/typeck.rs`, extend `ty_from_ast_resolving_structs`:
  - Add a `Type::DynTrait { trait_name, .. }` arm. Verify the trait exists in `traits.schemas` (reject with "unknown trait" if not). Return `Ty::DynRef { trait_name, mutable: false }` by default — the outer `Type::Ref` arm needs to UPGRADE this to `Ty::DynRef { trait_name, mutable }` based on the Ref's mutability flag. **Coordination point**: the existing `Type::Ref` arm needs to detect when `inner` is `Type::DynTrait` and collapse the wrap into a single `Ty::DynRef { trait_name, mutable }` instead of producing `Ty::Ref { Ty::DynRef { mutable: false }, .. }`.
  - Add a `Type::Generic { segments: ["Box"], args: [Type::DynTrait { trait_name, .. }] }` arm → `Ty::BoxDyn { trait_name }`.

- [X] T011 In `src/typeck.rs`, extend `typecheck_expr` with `Expr::Cast` arm:
  - Lower `target_ty` via `ty_from_ast_resolving_structs`. If the target is `Ty::DynRef { trait_name, mutable }`:
    - Typecheck `inner`. Must be `Ty::Ref { inner: Ty::Struct(name) | other concrete, mutable: inner_mut }`.
    - Verify `trait_impls.impls.contains_key((trait_name, name))`. If not → error: `"the type `<T>` cannot be coerced to `&dyn <Trait>` because it does not implement `<Trait>`"`.
    - Verify mutability: `&mut T → &dyn Trait` is OK (shared downgrade); `&T → &mut dyn Trait` rejected.
    - Return `Ty::DynRef { trait_name, mutable }`.
  - Other cast forms (numeric, string) → typeck error "M07.7 supports only trait-object coercion casts; other casts are out of scope".

- [X] T012 In `src/typeck.rs`, extend `typecheck_call` with implicit-coercion at fn-arg sites:
  - When a param's type is `Ty::DynRef { trait_name, .. }` and the arg's typecheck result is `Ty::Ref { Ty::Struct(name) | other concrete, .. }`, treat as implicit coercion (no `as` needed): verify the trait impl exists; accept the coercion. The eval side will perform the Value::Ref → Value::DynRef conversion at the call site.

- [X] T013 In `src/typeck.rs`, extend `typecheck_method_call` with a fourth dispatch layer:
  - When receiver type is `Ty::DynRef { trait_name, .. }` (or `Ty::BoxDyn { trait_name }`):
    - Look up the method in `traits.schemas[trait_name].required_methods` then `default_methods`.
    - If found, return the method's sig.
    - If not found, error: `"method `<name>` is not in trait `<TraitName>` (trait objects can only call trait methods)"`.
  - Refuses inherent-method calls through dyn per spec FR-015.

- [X] T014 In `src/eval.rs`, add vtable interning:
  - Add `Evaluator.vtable_addrs: HashMap<(String, String), VtableAddr>` field.
  - Add `Evaluator.next_vtable_addr: u32` counter.
  - Add `intern_vtable(&mut self, trait_name: &str, type_name: &str, span: Span) -> VtableAddr` helper. Mirrors M07.2's `intern_static`:
    - If `(trait_name, type_name)` is in `vtable_addrs`, return existing.
    - Otherwise allocate fresh addr; build methods list from `types.<trait registry — read via typeck side or local mirror>`; emit `MemEvent::VtableAlloc { addr, trait_name, type_name, methods, span }`.
  - **Coordination point**: eval needs access to the trait registries OR a mirror. Simplest: at `Evaluator::new`, walk `Item::Trait` items and build `trait_methods: HashMap<String, Vec<String>>` listing the methods for each trait. Use this to populate the `methods` field at VtableAlloc emission.

- [X] T015 In `src/eval.rs`, extend `eval_expr` with `Expr::Cast` arm:
  - Eval `inner`. Must be `Value::Ref { borrow_id, target, mutable, .. }`.
  - Resolve the target's concrete type:
    - For `Pointee::Slot(slot_id)`, look up the slot's value (`Value::Struct { name, .. }` → use name).
    - For `Pointee::Heap(addr)`, look up the heap object's type (M07's `HeapObject::Box(v)` etc. → derive name).
  - Intern the vtable via `intern_vtable(trait_name, type_name, span)`.
  - Build `Value::DynRef { borrow_id, target, vtable, mutable, trait_name }`. Reuse the inner borrow's id (the cast doesn't create a new borrow).

- [X] T016 In `src/eval.rs`, extend the fn-call arg-binding logic for implicit dyn-coercion:
  - At `call_decl`'s param-binding step (before SlotWrite), check if the param's declared type is `Ty::DynRef { trait_name, mutable }` AND the arg value is `Value::Ref { .. }`. If so, intern the vtable for the target's concrete type AND construct `Value::DynRef` inline; bind that to the slot instead of the raw Value::Ref.

- [X] T017 In `src/eval.rs`, extend `eval_method_call` with the `Value::DynRef` / `Value::BoxDyn` receiver path:
  - For `Value::DynRef { target, vtable: _, trait_name, mutable, .. }`:
    - Look up the concrete type via `target` (same as the cast path).
    - Look up the method body via `trait_impl_bodies[(trait_name, type_name, name)]` first; fall through to `trait_default_bodies[(trait_name, name)]`.
    - Build mangled name `<TypeName as TraitName>::method` (reuse M07.6's UFCS format).
    - Construct self_value: same M07.6 logic for `&self`/`&mut self`; the receiver is the `Value::DynRef`'s target.
    - Allocate fresh borrow_id (per the M07.6 per-binding rule from memory).
    - Dispatch via `call_decl`.
  - For `Value::BoxDyn { addr, vtable: _, trait_name }`: similar but receiver target is `Pointee::Heap(addr)`.

- [X] T018 In `src/eval.rs`, extend `eval_path_call` (or wherever `Box::new` is handled) for the dyn-Box case:
  - When typeck recorded the target type as `Ty::BoxDyn { trait_name }`, the `Box::new(p)` call should produce a `Value::BoxDyn { addr, vtable, trait_name }` instead of a regular `Value::Box { addr }`. The heap allocation happens normally; the vtable is interned for `(trait_name, type_of(p))`.

- [X] T019 In `src/ui.rs`, scaffold the UI types (data shape only; rendering in Phase 3):
  - Add `pub struct DynView { data_label: String, vtable_label: String, vtable_addr: u32 }`.
  - Add `pub struct VtableView { addr: u32, trait_name: String, type_name: String, methods: Vec<(String, String)> }`.
  - Extend `SlotRowView` with `dyn_view: Option<DynView>` field (serde-default + skip-if-none).
  - Extend `StateSnapshot` with `vtables: Vec<VtableView>` field (serde-default + skip-if-empty).
  - Add `LiveSlot.dyn_view: Option<DynView>` (eval-side mirror; populated at apply_event SlotWrite).
  - Extend `World` (or `world` in apply_event) with `vtables: Vec<VtableView>` field.
  - Initialize all to defaults in the relevant constructors.

- [X] T020 In `src/ui.rs`, extend `apply_event` with new arms:
  - **`MemEvent::VtableAlloc { addr, trait_name, type_name, methods, .. }`**: build `VtableView` with methods mapped to their dispatch target labels (`<TypeName as TraitName>::method` for overrides; `<TraitName>::method (default)` for defaults — distinguish via TraitImplRegistry lookup if accessible from the UI side; otherwise simpler: always use `<TypeName as TraitName>::method` and lose the (default) annotation). Push to `world.vtables`.
  - **`MemEvent::SlotWrite { value: Value::DynRef { .. } | Value::BoxDyn { .. }, .. }`** (extension to existing SlotWrite arm): build `DynView` with:
    - `data_label`: lookup the target's binding name (for Slot target) or `heap[N]` (for Heap target).
    - `vtable_label`: `<TypeName as TraitName>` form.
    - `vtable_addr`: the VtableAddr's u32.
    - Set `slot.dyn_view = Some(view)`; set `slot.value = String::new()` (suppress text); set other rendering fields to None.

**Checkpoint**: `cargo build` should compile cleanly. Match-exhaustiveness will flag any `Ty` / `Value` / `MemEvent` patterns that need the new variants — fix exhaustively. `cargo test` passes: M01/M02/M03 byte-identical (no existing sample constructs trait objects). At this point, the pipeline accepts trait-object syntax + types + dispatch correctly, but the UI just shows fallback text (no fat-pointer rendering yet, no VTABLES panel yet).

---

## Phase 3: User Story 1 — Basic `&dyn Trait` borrow + method dispatch (Priority: P1) 🎯 MVP + 🎨 UX CHECKPOINT

**Goal**: `let d: &dyn Show = &p; let s = d.show();` typechecks; d's slot renders as a fat-pointer with two labeled cells (data + vtable); VTABLES panel shows the `<Point as Show>` vtable; dispatch arrows visible.

**Independent Test**: load `m07_7_dyn_basic.rs`, step past `let d: &dyn Show = &p`, observe d's fat-pointer slot + VTABLES panel. Step past `d.show()`, observe two-step dispatch arrows.

### Implementation (UI surface)

- [X] T021 [US1] In `web/index.html`, add the VTABLES panel section. Place between HEAP and STATIC MEMORY per research R-015. Element id: `vtables-panel`. Header: `VTABLES`.

- [X] T022 [US1] In `web/index.js`, implement `renderVtables(state.vtables)`:
  - Returns a `<div class="vtable-panel">` containing one `<div class="vtable-box">` per VtableView.
  - Each box has a header `<{type_name} as {trait_name}>` and a list of `<div class="vtable-method">` entries showing `method_name → target_label`.
  - Set `data-vtable-addr` on each box for arrow targeting.
  - Call from main `renderUi` after `renderStacks`/`renderHeap`/`renderStatic`.

- [X] T023 [US1] In `web/index.js`, extend the per-slot rendering loop (around `renderStacks`):
  - When `slot.dyn_view` is present, call a new helper `renderDynView(slot.dyn_view, slot.slot_id)` and append into the slot's value cell.
  - Helper builds a `<div class="dyn-fat-pointer">` with TWO labeled children: `<div class="dyn-cell dyn-data"><span class="dyn-cell-label">data:</span> → <span class="dyn-cell-target">{data_label}</span></div>` and `<div class="dyn-cell dyn-vtable" data-vtable-addr="{vtable_addr}"><span class="dyn-cell-label">vtable:</span> → <span class="dyn-cell-target">{vtable_label}</span></div>`.

- [X] T024 [US1] In `web/style.css`, add CSS for the VTABLES panel and fat-pointer rendering per research R-014:
  - `.vtable-panel` — panel container with header.
  - `.vtable-box` — one box per vtable (border, padding, max-width).
  - `.vtable-method` — per-method row with `data-method` attribute.
  - `.dyn-fat-pointer` — flex column inside the slot's value area; two cells stacked vertically.
  - `.dyn-cell` — flex row; label + target.
  - `.dyn-cell-label` — muted small text.
  - `.dyn-cell-target` — monospace.

- [X] T025 [US1] In `web/index.js`, extend `renderArrows` to draw the **two-step dispatch arrow** when a trait-object method-call frame opens (the current event is `FrameEnter` for `<Type as Trait>::method`):
  - First arrow: from the source slot's `dyn-data` cell → the receiver's location (existing borrow-arrow path).
  - Second arrow: from the source slot's `dyn-vtable` cell → the corresponding vtable box (queried via `[data-vtable-addr=N]`) → the method's frame card.
  - CSS class `arrow-vtable-dispatch` — distinct style per R-016 (initial: dashed orange, 2px stroke).
  - The dispatch arrows fade after the next cursor step (transient — only visible at the dispatch step), similar to M07.2's BytesCopy arrow.

- [X] T026 [US1] In `web/style.css`, add `.arrow-vtable-dispatch` styling: dashed orange (recommendation; iterate at UX checkpoint), 2px stroke, animated dash if budget permits. Same fade-on-leave as other arrows.

- [X] T027 [US1] Add 1 sample program pair: `tests/samples/m07_7_dyn_basic.rs` and `web/samples/m07_7_dyn_basic.rs`. Content:
  ```rust
  struct Point { x: i32, y: i32 }
  trait Show { fn show(&self) -> i32; }
  impl Show for Point { fn show(&self) -> i32 { self.x } }
  fn main() {
      let p = Point { x: 1, y: 2 };
      let d: &dyn Show = &p;
      let s = d.show();
  }
  ```

- [X] T028 [US1] In `src/pipeline.rs` `mod tests`, add unit tests:
  - `run_pipeline_dyn_basic`: asserts a `VtableAlloc` event for `("Show", "Point")` fires; a `SlotWrite` for d carries `Value::DynRef { trait_name: "Show", .. }`; a `FrameEnter` for `<Point as Show>::show` opens; s's SlotWrite has `Value::Int { I32, 1 }`.
  - `run_pipeline_dyn_coercion_error`: source `let d: &dyn Show = &5;` (where i32: Show absent) → typeck error mentioning `i32` and `Show`.
  - `run_pipeline_dyn_inherent_rejected`: source with inherent method called via dyn → typeck error.

- [X] T029 [US1] In `web/index.html`, add a dropdown `<option>` for `m07_7_dyn_basic.rs`.

- [X] T030 [US1] Verify US1 renders cleanly: `cd web && trunk serve`, load `Dyn basic`, step past the let-cast + method call. Take screenshot for the UX checkpoint.

**🎨 UX CHECKPOINT**: pause and present the rendered visualization to the user. Discuss:
- VTABLES panel positioning (between HEAP and STATIC vs other) — R-015 recommendation.
- Fat-pointer cell layout (vertical stack vs horizontal) — R-014 Proposal A vs B.
- Dispatch arrow color + style (dashed orange, muted purple, other) — R-016.
- Method-row formatting inside vtable boxes.

Iterate until the user signs off. Do NOT proceed to Phase 4 until approved.

---

## Phase 4: User Story 2 — `&dyn Trait` parameter + implicit coercion (Priority: P1)

**Goal**: `fn print(x: &dyn Show) -> i32 { x.show() } let r = print(&p);` typechecks; trace contains ONE `print` frame (no monomorphization) containing a nested `<Point as Show>::show` frame. Multiple calls share the same `print` frame body.

**Independent Test**: load `m07_7_dyn_param.rs`, step past `print(&p)`, observe ONE `print` frame + nested vtable dispatch.

### Implementation

- [X] T031 [US2] Add 1 sample program pair: `tests/samples/m07_7_dyn_param.rs` and `web/samples/m07_7_dyn_param.rs`. Content:
  ```rust
  struct Point { x: i32, y: i32 }
  trait Show { fn show(&self) -> i32; }
  impl Show for Point { fn show(&self) -> i32 { self.x } }
  fn print(x: &dyn Show) -> i32 {
      x.show()
  }
  fn main() {
      let p = Point { x: 1, y: 2 };
      let r = print(&p);
  }
  ```

- [X] T032 [US2] In `src/pipeline.rs` `mod tests`, add unit tests:
  - `run_pipeline_dyn_param`: asserts trace contains ONE `FrameEnter { fn_name: "print", .. }` (no `print::<Point>` mangling) AND a nested `FrameEnter { fn_name: "<Point as Show>::show", .. }`. r's SlotWrite has `Value::Int { I32, 1 }`.
  - `run_pipeline_dyn_param_two_types`: source with two impls (Point, Q) AND two calls `print(&p); print(&q);` — asserts BOTH calls go through the SAME `print` frame (two FrameEnter events with `fn_name: "print"`, neither mangled). Each inner dispatch resolves to the correct concrete type's method.
  - `run_pipeline_dyn_vtable_interned`: multiple `&dyn Show` borrows of Point — asserts only ONE `VtableAlloc` event fires for `("Show", "Point")` regardless of how many borrows construct DynRef values.

- [X] T033 [US2] In `web/index.html`, add a dropdown `<option>` for `m07_7_dyn_param.rs`.

**Checkpoint**: US2 fully functional. The implicit-coercion path at fn-arg sites works.

---

## Phase 5: User Story 3 — `Box<dyn Trait>` (Priority: P1)

**Goal**: `let b: Box<dyn Show> = Box::new(p); let s = b.show();` typechecks; heap allocation visible; b's slot renders as fat pointer with `data: → heap[N]` + `vtable: → <Point as Show>`; method dispatch through vtable; box freed at scope exit (vtable persists).

**Independent Test**: load `m07_7_box_dyn.rs`, step past `Box::new(p)` then `b.show()`, observe the heap block + fat pointer + dispatch + drop sequence.

### Implementation

- [X] T034 [US3] Add 1 sample program pair: `tests/samples/m07_7_box_dyn.rs` and `web/samples/m07_7_box_dyn.rs`. Content:
  ```rust
  struct Point { x: i32, y: i32 }
  trait Show { fn show(&self) -> i32; }
  impl Show for Point { fn show(&self) -> i32 { self.x } }
  fn main() {
      let p = Point { x: 1, y: 2 };
      let b: Box<dyn Show> = Box::new(p);
      let s = b.show();
  }
  ```

- [X] T035 [US3] In `src/pipeline.rs` `mod tests`, add unit tests:
  - `run_pipeline_box_dyn`: asserts a HeapAlloc event for the Box; a SlotWrite for b carries `Value::BoxDyn { trait_name: "Show", .. }`; a VtableAlloc fires for `("Show", "Point")`; method dispatch → `<Point as Show>::show`; s lands `Int(I32, 1)`.
  - At scope exit: HeapFree fires for the Box's allocation; no VtableFree (vtables persist).

- [X] T036 [US3] In `web/index.html`, add a dropdown `<option>` for `m07_7_box_dyn.rs`.

**Checkpoint**: US3 fully functional. Heap + vtable + dispatch all interact correctly.

---

## Phase 6: User Story 4 — Static vs dynamic side-by-side (Priority: P2) 🎯 HEADLINE CONTRAST

**Goal**: paired sample with `fn s<T: Show>(x: T)` (M07.6 static, monomorphized `s::<Point>`) AND `fn d(x: &dyn Show)` (M07.7 dynamic, one `d` frame + vtable arrow). Side-by-side dispatch flavors visible in the trace.

**Independent Test**: load `m07_7_static_vs_dyn.rs`, step through both calls, observe the distinct dispatch flows.

### Implementation

- [X] T037 [US4] Add 1 sample program pair: `tests/samples/m07_7_static_vs_dyn.rs` and `web/samples/m07_7_static_vs_dyn.rs`. Content:
  ```rust
  struct Point { x: i32, y: i32 }
  trait Show { fn show(&self) -> i32; }
  impl Show for Point { fn show(&self) -> i32 { self.x } }

  fn s<T: Show>(x: T) -> i32 {
      x.show()
  }

  fn d(x: &dyn Show) -> i32 {
      x.show()
  }

  fn main() {
      let p = Point { x: 1, y: 2 };
      let a = s(p);
      let b = d(&p);
  }
  ```

- [X] T038 [US4] In `src/pipeline.rs` `mod tests`, add unit tests:
  - `run_pipeline_static_vs_dyn`: asserts trace contains `FrameEnter { fn_name: "s::<Point>", .. }` AND `FrameEnter { fn_name: "d", .. }` (no `d::<Point>` mangling). Both inner frames are `<Point as Show>::show`. Distinct dispatch flavors visible.

- [X] T039 [US4] In `web/index.html`, add a dropdown `<option>` for `m07_7_static_vs_dyn.rs`. **Place LAST** in the M07.7 entries so learners see the foundational samples first.

**Checkpoint**: US4 fully functional. The ship-defining contrast is visible.

---

## Phase 7: Cross-cutting tests + default-method-through-dyn

**Purpose**: additional behavioral coverage not in US1-US4 happy paths.

- [X] T040 In `src/pipeline.rs` `mod tests`, add a default-method-through-dyn test:
  - `run_pipeline_dyn_default_method`: source with `trait Counter { fn count(&self) -> i32; fn double(&self) -> i32 { self.count() * 2 } } impl Counter for Point { fn count(&self) -> i32 { self.x } }` and `let d: &dyn Counter = &p; let v = d.double();` — asserts the call dispatches through the vtable to the trait's default body; the default body's `self.count()` re-dispatches to the impl's override; v = 2.

**Checkpoint**: ≥ 10 new M07.7 tests total (3 US1 + 3 US2 + 1 US3 + 1 US4 + 1 default-through-dyn + 1 vtable-interning = ≥ 10). Exceeds the SC floor.

---

## Phase 8: Polish & Cross-Cutting

**Purpose**: snapshot verify, bundle-size check, warnings, manual QA, doc updates.

- [X] T041 [P] Run `cargo test`. Verify M01/M02/M03 byte-identical (no existing sample constructs trait objects; additive variants don't surface in any pre-M07.7 trace).
- [X] T042 [P] Build WASM release and measure bundle size: `cd web && trunk build --release` (wasm-opt may fail per the pre-existing tooling issue; use the staged size at `dist/.stage/*.wasm`). Compare to M07.6 baseline (378,170 B). Acceptable if ≤ +25% (~473 KB). If over: candidate cuts per plan.md.
- [X] T043 [P] Run `RUSTFLAGS="-D warnings" cargo build --release` AND `cargo test`. Both clean. Fix any NEW warnings introduced by M07.7.
- [X] T044 [P] Run `cargo clippy --all-targets -- -D warnings`. Fix any NEW lints (pre-existing lints out of scope).
- [X] T045 Manual M07.7 QA per `specs/018-m07-7-trait-objects/quickstart.md` procedure. ~10 minutes (includes UX checkpoint validation after Phase 3, but here re-walks the full set of samples post-iteration). Verify error UX via live editing: bound not satisfied, inherent via dyn rejected.
- [X] T046 Verify `CLAUDE.md` "Recent Changes" footer includes M07.7 (the speckit update-agent-context script handles this; verify post-hoc).
- [X] T047 Final commit prep. MR note: "11th invocation of the closed-enum-with-revisions rule. First new MemEvent variant since M07.2 (`VtableAlloc`). New VTABLES panel + fat-pointer slot rendering + two-step dispatch arrows. Closes the Level 4 polymorphism story: M07.5 generics + M07.6 traits (static dispatch) + M07.7 trait objects (dynamic dispatch)."

---

## Dependencies

```text
Phase 1 (Setup)
  └─ T001 (verify baseline)

Phase 2 (Foundational) — blocks ALL user-story phases
  ├─ T002 (contract amendment, can run anytime)
  ├─ T003 [P] (lexer keywords)
  ├─ T004 [P] (token variants)
  ├─ T005 (AST surface — depends on T004)
  ├─ T006 (parser — depends on T005)
  ├─ T007 [P] (resolve — depends on T005)
  ├─ T008 (Ty::DynRef + BoxDyn — depends on T005)
  ├─ T009 (Value::DynRef + BoxDyn + VtableAddr + MemEvent::VtableAlloc — depends on T008)
  ├─ T010 (ty_from_ast extension — depends on T009)
  ├─ T011 (typecheck Expr::Cast — depends on T010)
  ├─ T012 (implicit fn-arg coercion — depends on T011)
  ├─ T013 (typecheck_method_call fourth layer — depends on T010)
  ├─ T014 (eval vtable interning — depends on T009)
  ├─ T015 (eval Expr::Cast — depends on T014)
  ├─ T016 (eval implicit coercion at fn-arg — depends on T014)
  ├─ T017 (eval method dispatch for DynRef — depends on T014)
  ├─ T018 (eval Box<dyn> construction — depends on T014)
  ├─ T019 [P] (ui scaffold — depends on T009)
  └─ T020 (apply_event extensions — depends on T019)

Phase 3 (US1) — depends on Phase 2 — 🎨 UX CHECKPOINT
  ├─ T021 (vtables panel HTML)
  ├─ T022 (renderVtables JS)
  ├─ T023 (renderDynView JS)
  ├─ T024 (vtables + fat-pointer CSS)
  ├─ T025 (dispatch arrow JS)
  ├─ T026 (dispatch arrow CSS)
  ├─ T027 (sample pair)
  ├─ T028 (3 unit tests)
  ├─ T029 (dropdown)
  └─ T030 (visual verification + 🎨 UX CHECKPOINT)

🎨 PAUSE for user review before Phase 4.

Phase 4 (US2) — depends on Phase 3 (UI approved)
  ├─ T031 (sample)
  ├─ T032 (3 unit tests)
  └─ T033 (dropdown)

Phase 5 (US3) — depends on Phase 4 (independent of US4)
  ├─ T034 (sample)
  ├─ T035 (1 unit test)
  └─ T036 (dropdown)

Phase 6 (US4) — depends on Phases 3-5 (the headline contrast needs all dispatch flavors working)
  ├─ T037 (sample)
  ├─ T038 (1 unit test)
  └─ T039 (dropdown — placed LAST in dropdown order)

Phase 7 (cross-cutting tests) — depends on Phase 2
  └─ T040 (default-method-through-dyn test)

Phase 8 (Polish) — depends on Phases 3-7
  └─ T041–T047 (snapshot/bundle/warnings/QA/docs/commit)
```

---

## Parallel execution opportunities

- **Phase 2**: T003 + T004 + T007 + T019 are file-disjoint [P]. T005 depends on T004; T008/T009/T010/T011/T012/T013 sequential in typeck.rs; T014/T015/T016/T017/T018 sequential in eval.rs.
- **Phases 4/5/6/7**: completely independent of each other after Phase 3 lands.
- **Phase 8**: T041/T042/T043/T044 all parallelizable [P].

---

## Implementation strategy

**MVP scope** = **US1 only** (basic `&dyn Trait` + dispatch + UX checkpoint). Lands the headline pedagogy (fat pointer + vtable + dispatch arrows). ~1000 LOC.

**Incremental delivery**:
1. **MVP (US1)**: Phases 1+2+3 (Setup + Foundational + US1). Headline pedagogy live; UI signed off.
2. **+US2 (`&dyn` param)**: Phase 4. Implicit coercion at fn-arg sites.
3. **+US3 (Box<dyn>)**: Phase 5. Heap-owning trait object.
4. **+US4 (static vs dyn)**: Phase 6. Ship-defining contrast.
5. **+Cross-cutting tests**: Phase 7. Default-method-through-dyn coverage.
6. **+Polish**: Phase 8. Snapshot/bundle/QA/docs.

**Recommended landing order**: ship all 4 user stories + cross-cutting test in one merge. The trait-object surface is cross-cutting (vtable interning + dispatch + UI all interact); splitting at user-story granularity would force three rounds of Phase 2 follow-ups. Single-merge matches M07.4/M07.5/M07.6 pattern.

**UX checkpoint is the only natural pause**: between Phase 3 (UI first cut) and Phase 4 (foundational visual decisions locked in). After approval, the remaining US stories don't add new UI surface — they just exercise existing rendering with different sample shapes.

**Sequence note**: M07.7 closes the Level 4 polymorphism story. After this milestone, the project has demonstrated every Rust polymorphism mechanism a learner needs.
