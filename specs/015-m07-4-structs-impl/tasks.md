---

description: "Task list for M07.4 — Structs + `impl` blocks (named-field composite types with methods)"
---

# Tasks: M07.4 — Structs + `impl` blocks (named-field composite types with methods)

**Input**: Design documents from `/specs/015-m07-4-structs-impl/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/m07-4-protocol-delta.md ✓, quickstart.md ✓

**Tests**: M01/M02 should stay byte-identical (no L1 sample constructs structs). M03 snapshots should stay byte-identical (`Value::Ref.field_path` uses serde-default-empty; existing borrow snapshots unaffected). New `cargo test --lib pipeline::tests` covering: struct decl + literal + field access, shorthand, missing/extra/wrong-type-field errors, field borrow with `field_path` metadata, unknown-field-borrow error, method dispatch, two-method impl, unknown-method error, associated function, mixed user/builtin path dispatch, forward reference, zero-heap-events. **≥ 12 new tests**. Manual M07.4 QA per the SC-008 procedure.

**Organization**: 4 user stories (US1 + US2 + US3 P1; US4 P2). Sized XL. ~6 source files modified + 1 lexer keyword extension + JS struct rendering + CSS for struct slot layout + 4 sample pairs.

**EXPLICIT UX CHECKPOINT**: research R-016 (struct viz visual) is the iterate-on-this proposal. Phase 4 lands the first cut of Proposal A (vertical labeled rows); Phase 4-end UX checkpoint pauses for user review before proceeding to Phase 5 (per-field hover plumbing).

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: parallelizable (different files, no incomplete-task dependencies)
- **[Story]**: US1/US2/US3/US4 tag, mandatory on user-story phases only
- File paths relative to repo root

## Path Conventions

~6 existing source files modified + lexer keyword additions + JS/CSS additions in `web/` + 4 sample pairs. See `specs/015-m07-4-structs-impl/plan.md` Project Structure.

---

## Phase 1: Setup

**Purpose**: pre-flight — confirm starting state.

- [X] T001 Verify pre-conditions: branch `015-m07-4-structs-impl` checked out; `cargo test` from `main` passes (baseline confirmed: 126 tests). Note current test count for post-merge delta.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: parser additions + AST nodes + `Ty::Struct` + `Value::Struct` + `Value::Ref` extension + UI scaffolding. Required by all four user stories. Larger than M07.3's Phase 2 — first milestone with multiple new top-level Item variants AND a value-variant extension.

- [X] T002 Amend M03's contract in `specs/004-m03-event-eval/contracts/m03-api.md` — append an entry under the closed-enum-with-revisions section noting M07.4 as the 8th invocation (additive `Ty::Struct`, additive `Value::Struct`, additive `Value::Ref.field_path` extension via serde-default-empty). Reference `specs/015-m07-4-structs-impl/contracts/m07-4-protocol-delta.md`.

- [X] T003 [P] In `src/parse/lexer.rs`, extend the KEYWORDS table with three new entries: `"struct"` → `TokenKind::Struct`, `"impl"` → `TokenKind::Impl`, `"self"` → `TokenKind::SelfKw`. No new punctuation; reuse existing `Amp`/`AmpMut` for `&self`/`&mut self`.

- [X] T004 [P] In `src/parse/token.rs`, add three new variants to `TokenKind`: `Struct`, `Impl`, `SelfKw`. Update `TokenKind::describe()` to return `"`struct`"`, `"`impl`"`, `"`self`"` respectively.

- [X] T005 In `src/parse/ast.rs`, add new AST surface:
  - Two new `Item` variants: `Item::Struct(StructDecl)` and `Item::Impl(ImplBlock)`.
  - `StructDecl { name: String, fields: Vec<StructField>, span: Span }`.
  - `StructField { name: String, ty: Type, span: Span }`.
  - `ImplBlock { ty_name: String, items: Vec<FnDecl>, span: Span }`.
  - Two new `Expr` variants: `Expr::StructLit { path: Vec<String>, fields: Vec<StructLitField>, span }` and `Expr::FieldAccess { receiver: Box<Expr>, name: String, span }`.
  - `StructLitField { name: String, value: Option<Expr>, span: Span }` (None = shorthand).
  - New enum `ParamKind { Normal, SelfOwned, SelfShared, SelfMut }`.
  - Extend `Param` with `kind: ParamKind` field — initialize to `Normal` at every existing construction site in `parser.rs` (for free-fn params).
  - Update `Expr::span()` to cover `StructLit { span, .. } => *span` and `FieldAccess { span, .. } => *span`.
  - Update `item_span()` in `parser.rs` (or equivalent) to handle the two new `Item` variants.

- [X] T006 In `src/parse/parser.rs`, extend parsing (incl. cond-position struct-literal restriction flag to disambiguate `if c { 1 }` from `c { 1 }` struct lit):
  - **`parse_item`**: peek leading keyword and dispatch — `Fn` → existing `parse_fn_decl`; `Struct` → new `parse_struct_decl`; `Impl` → new `parse_impl_block`; else error "expected an item (`fn`, `struct`, or `impl`)".
  - **`parse_struct_decl`**: consume `struct`, expect ident (name), expect `{`, parse comma-separated `name: Type` field list (at least 1 — empty struct rejected with "structs in M07.4 must have at least one field"; trailing comma allowed), expect `}`. Build `Item::Struct(StructDecl { name, fields, span })`.
  - **`parse_impl_block`**: consume `impl`, expect ident (ty_name), expect `{`, parse zero or more `FnDecl`s via `parse_fn_decl`, expect `}`. Build `Item::Impl(ImplBlock { ty_name, items, span })`.
  - **`parse_param`** extension: at param index 0 ONLY, peek for self-receiver patterns BEFORE the existing `name: Type` path. Cases: `SelfKw` (no `&`) → `ParamKind::SelfOwned`; `Amp SelfKw` → `ParamKind::SelfShared`; `AmpMut SelfKw` → `ParamKind::SelfMut`. Build `Param { name: "self", ty: <placeholder>, kind, span }`. The ty placeholder is `Type::Path { segments: ["__SelfPlaceholder"], span }`; phase-1 typeck swaps it for the real type. For non-self params at index 0, fall through. At param index ≥ 1, any self-pattern is a `ParseError { message: "`self` parameter must be the first parameter", .. }`.
  - **`parse_atom`** extension: after parsing an `Ident` (or a `Path` via `::`), peek for `LBrace`. If present, parse a struct literal: consume `{`, parse comma-separated `StructLitField`s (each is `name: expr` OR shorthand `name`; trailing comma allowed), expect `}`. Build `Expr::StructLit { path: [<ident>], fields, span }`. **Important**: this peek runs at parse_atom (atom-level) position, not inside any cond context. Document the `if cond { ... }` ambiguity in a code comment per research R-003.
  - **`parse_expr` postfix loop** extension: the existing `Dot Ident LParen ... RParen` → `MethodCall` arm gets a sibling. When `Dot Ident` is followed by anything OTHER than `LParen` (typically `Dot`, `Semi`, `RBrace`, binary op, etc.), produce `Expr::FieldAccess { receiver, name, span }`. The `LParen` lookahead disambiguates the two.

- [X] T007 [P] In `src/resolve.rs`, add traversal for the new AST surface:
  - `Expr::StructLit { fields, .. }`: recurse on each `field.value` if `Some`. For shorthand (`value: None`), do nothing (typeck handles the local lookup).
  - `Expr::FieldAccess { receiver, .. }`: recurse on receiver only (`name` is symbol-lookup-deferred-to-typeck).
  - `Item::Struct { name, .. }`: register the struct's name as a top-level type binding (so future `&p.x` can resolve `p`'s type via its binding).
  - `Item::Impl { .. }`: no new bindings; types resolved at typeck phase 1.

- [X] T008 In `src/typeck.rs`, add `Ty::Struct { name: String, fields: Vec<(String, Ty)> }` variant. Update:
  - `Ty::name(&self)` → return the bare `name` (`"Point"`).
  - `Ty::is_copy(&self)` → return `true` for `Struct` (M07.4 fields are primitive-only, all Copy).
  - **Defer `ty_from_ast` arm** to T011 (depends on `StructRegistry` being populated).

- [X] T009 In `src/event.rs`:
  - Add new variant `Value::Struct { name: String, fields: Vec<(String, Value)> }`.
  - **Extend** existing `Value::Ref` struct-variant with new field `field_path: Vec<String>` AFTER the existing fields. Annotate with `#[serde(default, skip_serializing_if = "Vec::is_empty")]` so existing M06+ borrow snapshots stay byte-identical.
  - Update `Value::type_name()` arm for `Struct` returning `"{}"` (short tag).
  - Update every existing `Value::Ref { .. }` construction site (search `src/eval.rs`, `src/ui.rs`, `src/typeck.rs`) to pass `field_path: Vec::new()` as the new field's default value. Compiler will flag missing-field errors; fix exhaustively.

- [ ] T010 [P] In `src/ui.rs`, scaffold the struct-view UI types (DEFERRED to Phase 3 T016 — folded into apply_event SlotWrite arm for Value::Struct):
  - Add `pub struct StructView { pub name: String, pub fields: Vec<StructFieldView> }`.
  - Add `pub struct StructFieldView { pub name: String, pub ty_label: String, pub size: u32, pub display: String }`.
  - Add `pub struct_view: Option<StructView>` field to `SlotRowView` with `#[serde(default, skip_serializing_if = "Option::is_none")]`.
  - Add `pub field_path: Vec<String>` field to `LiveSlot` (or wherever the slot-side tracking happens — needs to follow `Value::Ref.field_path` through the apply_event arm). **Decision deferred**: simpler is to add `pub field_label: Option<String>` to `ActiveBorrowState` and `ArrowView` instead of carrying the full path; M07.4 only renders single-segment paths so `Option<String>` suffices.
  - Add `pub field_label: Option<String>` to `ArrowView` with serde default + skip-when-none.
  - All variants of `apply_event`'s SlotWrite arm that destructure `Value::Ref { .. }` need to add `field_path` to the pattern — fix exhaustively.

**Checkpoint**: `cargo build` should compile cleanly. Match-exhaustiveness will flag any `Ty` or `Value` sites that need `Struct` arms — fix them as warnings appear (typically `ty_size_bytes`, `value_size_bytes`, `render_value`, `render_value_for_note`). `cargo test` passes: M01/M02 byte-identical; M03 byte-identical (verify with `cargo insta test` — `field_path` is omitted from JSON when empty). The AST/parser/Ty/Value scaffolding is in place; no user-facing struct behavior yet.

---

## Phase 3: User Story 1 — Struct declaration, literal, and field access (Priority: P1) 🎯 MVP

**Goal**: `struct Point { x: i32, y: i32 } let p = Point { x: 1, y: 2 }; let a = p.x;` typechecks with `p: Point`, `a: i32 = 1_i32`; emits a `Value::Struct` SlotWrite for `p`; the trace contains **zero** `HeapAlloc`/`HeapRealloc`/`HeapFree` events.

**Independent Test**: load `m07_4_struct_basic.rs`, step past `let p = Point { x: 1, y: 2 }`, observe `p`'s slot with type `Point` and the new struct view (rendering deferred to Phase 4). Step past `let a = p.x`, observe `a = 1_i32` and `p` still usable. Zero heap events.

### Implementation

- [X] T011 [US1] In `src/typeck.rs`, build the **two-pass typeck** infrastructure:
  - Add `pub struct StructRegistry { schemas: IndexMap<String, Vec<(String, Ty)>> }` AND `pub struct ImplRegistry { methods: IndexMap<(String, String), FnSig>, assoc_fns: IndexMap<Vec<String>, FnSig> }`. Both `#[derive(Default)]`.
  - Extend `typeck()` entry function with phase 1: iterate `program.items` and for each `Item::Struct(decl)`, build `Vec<(String, Ty)>` by lowering each field's `Type` via a NEW helper `ty_from_ast_with_struct_registry(ty, &StructRegistry)`. Insert into `StructRegistry.schemas`. Duplicate-name detection: error "struct `<name>` already defined at <span>" if `schemas` already contains the name.
  - For each `Item::Impl(block)`: verify `block.ty_name` is in `StructRegistry.schemas` (error "impl block references unknown type `<name>`" if not). M07.4 also rejects `impl Vec`, `impl String`, `impl Box`, etc. — these names ARE NOT registered as user structs, so the same error fires.
  - For each fn item in the impl block: compute its `FnSig` (resolving the `ParamKind::SelfXxx` placeholder type to the real `Ty::Struct(_)` or `Ty::Ref { Ty::Struct(_), .. }`). Insert into `ImplRegistry.methods` if first param has self-receiver kind ≠ Normal; into `ImplRegistry.assoc_fns` otherwise. Reject duplicates per VR-36.
  - Extend `ty_from_ast` (used in phase 2) with a `Type::Path { segments: [name], .. }` arm that consults `StructRegistry.schemas`: if found, return `Ty::Struct { name, fields }`. Existing primitive resolution (`i32`, `bool`, etc.) wins; struct lookup is the fallback.
  - Phase 2 typechecks bodies with both registries visible (pass them to `Typechecker::new`).

- [X] T012 [US1] In `src/typeck.rs`, add `typecheck_struct_lit` (new method on `Typechecker`):
  - Resolve `path[0]` against `StructRegistry.schemas`. Error if not found ("unknown struct `<name>`").
  - For each declared field in the schema: find the matching `StructLitField` in `lit.fields` (match by name). Missing → "missing field `<name>` in struct literal `<struct>`". For shorthand (`value: None`), look up a local binding of `name` in the current scope; missing → "no local named `<name>` for field-shorthand". Otherwise typecheck the value expression and coerce to the declared field type via `try_coerce_to`. Mismatch → "expected `<declared>`, found `<found>`" pointing at the value expr.
  - For each `StructLitField` not matched against any schema field: "no field `<name>` on struct `<struct>`".
  - Result type: `Ty::Struct { name, fields }`.

- [X] T013 [US1] In `src/typeck.rs`, add `typecheck_field_access` (new method):
  - Typecheck `receiver`. Accept `Ty::Struct(_)` directly. Accept `Ty::Ref { inner: Ty::Struct(_), mutable: _, .. }` via auto-deref (R-017) — the result field type comes from the inner struct's schema.
  - Reject anything else: "field access requires a struct receiver, found `<ty>`".
  - Find `name` in the resolved struct's schema. Missing → "no field `<name>` on struct `<struct>`".
  - Result type: the field's type (cloned).
  - **Reject multi-level**: if `receiver` is itself an `Expr::FieldAccess`, error "nested struct fields not supported in M07.4 — use intermediate let bindings".
  - Wire into the typecheck dispatch: `Expr::FieldAccess { .. }` arm in `typecheck_expr` calls this.

- [X] T014 [US1] In `src/eval.rs`, evaluate `Expr::StructLit`:
  - Look up the struct's schema (M07.4 typeck already verified existence). For each declared field in order, find the matching `StructLitField`. For shorthand (`value: None`), look up the local binding via `lookup_local_slot` + `read_slot_value`. Otherwise eval the value expression.
  - Build `Value::Struct { name: path[0].clone(), fields: ordered_pairs }` where `ordered_pairs.len() == schema.len()` and field order = declaration order.

- [X] T015 [US1] In `src/eval.rs`, evaluate `Expr::FieldAccess`:
  - Eval receiver. If `Value::Struct { fields, .. }`, find the named field and clone its value. Result.
  - If `Value::Ref { target: Pointee::Slot(slot), field_path, .. }`: if `field_path` is non-empty, return error (multi-level access — typeck should have rejected; defensive). If empty, read the target slot's `Value::Struct`, find the named field, clone its value. Result.
  - Else: panic (typeck should have rejected non-struct receivers; defensive).

- [X] T016 [P] [US1] In `src/ui.rs`, extend `apply_event`'s `SlotWrite` arm:
  - When `value` is `Value::Struct { name, fields }`, build a `StructView { name, fields: <map fields to StructFieldView> }`. Each `StructFieldView { name, ty_label: <Ty::name() of field value>, size: <ty_size_bytes_ui of field value>, display: <render_value of field value> }`.
  - Set the slot's `value = String::new()` AND `inline_cells = None` AND `struct_view = Some(view)`.
  - Snapshot conversion: in the `StateSnapshot` builder, pass `struct_view: slot.struct_view.clone()` into `SlotRowView`.

- [X] T017 [P] [US1] In `src/ui.rs`, update `render_value` and any sibling helpers to handle `Value::Struct`:
  - `render_value(Value::Struct { name, .. })` returns `format!("{} {{ ... }}", name)` (a short fallback for places that aren't using `struct_view` — e.g. the `return_value` annotation). The full per-field rendering goes through `struct_view`.
  - `render_value_for_note` similar: short summary form for note messages.

- [X] T018 [US1] Add 1 sample program pair: `tests/samples/m07_4_struct_basic.rs` and `web/samples/m07_4_struct_basic.rs`. Content:
  ```rust
  struct Point { x: i32, y: i32 }
  fn main() {
      let p = Point { x: 1, y: 2 };
      let a = p.x;
  }
  ```

- [X] T019 [US1] In `src/pipeline.rs` `mod tests`, add unit tests:
  - `run_pipeline_struct_basic`: asserts the trace contains exactly one `SlotWrite` with a `Value::Struct { name: "Point", fields: [("x", Int{I32, 1}), ("y", Int{I32, 2})] }`, plus a subsequent `SlotWrite` for `a` carrying `Value::Int { I32, 1 }`. Asserts zero `HeapAlloc`/`HeapRealloc`/`HeapFree` events.
  - `run_pipeline_struct_shorthand`: source `let x = 1; let y = 2; let p = Point { x, y };` — asserts identical `Value::Struct` constructed.
  - `run_pipeline_struct_missing_field`: source `let p = Point { x: 1 };` — asserts the pipeline returns a `ParseError` with "missing field `y`" in the message.
  - `run_pipeline_struct_extra_field`: source `let p = Point { x: 1, y: 2, z: 3 };` — asserts the error mentions "no field `z`".
  - `run_pipeline_struct_wrong_type`: source `let p = Point { x: true, y: 2 };` — asserts the error mentions "expected `i32`, found `bool`".

- [X] T020 [US1] In `web/index.html`, add a dropdown `<option>` for `m07_4_struct_basic.rs`.

**Checkpoint**: at this point US1 is fully functional at the pipeline + state-snapshot level. `cargo test` passes including the 5 new tests. The web page can load `Struct basic` and step through it; the stack shows `p : Point` but the value-area visualization is NOT yet rendered (deferred to Phase 4). **User Story 1 deliverable**: structurally working struct decl + literal + field access through the entire pipeline.

---

## Phase 4: UI struct visualization (cross-cutting; SERVES US1 first) 🎨 UX CHECKPOINT

**Goal**: implement the struct slot visualization per research R-016 Proposal A (vertical labeled rows with byte-cells + per-field name/type/value). After this phase, US1's sample renders with the full struct view in the page.

**This phase is the explicit iterate-on-this part** flagged by the user. After T024 lands, **PAUSE** for a UX checkpoint before proceeding to Phase 5 (per-field hover, US2's payoff). The user may request tweaks (typography, spacing, color, label layout, switching to Proposal B/C).

### Implementation

- [X] T021 In `web/index.js`, extend `renderStacks` (the per-slot rendering loop, around line 170 where `inline_cells` is handled): when `slot.struct_view` is present, call a NEW helper `renderStructView(slot.struct_view, slot.slot_id)` and append the resulting element into the `valueEl`. The struct view replaces both the `slot.value` text path AND the `inline_cells` path (mutually exclusive). Skip the existing text/cells rendering when `struct_view` is set.
- [X] T022 In `web/index.js`, implement `renderStructView(struct_view, slot_id)`:
  - Returns a `<div class="struct-view" data-struct-name="Point">` containing one child `<div class="struct-field" data-field-name="x">` per field.
  - Each `.struct-field` contains: `<span class="struct-field-label">x: i32</span>`, `<div class="struct-field-cells">` (N byte-cells via `<span class="byte-cell byte-used">` × `size`), `<span class="struct-field-value">= 1_i32</span>`.
  - The `data-field-name` attribute on `.struct-field` drives the per-field hover query in Phase 5.
- [X] T023 In `web/style.css`, add the struct-view CSS per research R-016 Proposal A:
  ```css
  .slot-row .slot-value:has(.struct-view) { display: inline-flex; flex-direction: column; align-items: flex-start; }
  .struct-view { display: flex; flex-direction: column; gap: 2px; border: 1px solid #999;
                 padding: 4px; max-width: 240px; background: #fafafa; border-radius: 3px; }
  .struct-field { display: grid; grid-template-columns: auto 1fr auto;
                  gap: 6px; align-items: center; font-size: 11px; padding: 2px 4px;
                  border-radius: 2px; }
  .struct-field-label { font-family: ui-monospace, monospace; color: var(--muted); }
  .struct-field-cells { display: flex; gap: 1px; }
  .struct-field-cells .byte-cell { width: 8px; height: 8px; border: 1px solid #999;
                                   background: #c8c8c6; box-sizing: border-box; }
  .struct-field-value { font-family: ui-monospace, monospace; color: inherit; }
  .struct-field.field-borrow-highlighted { background: #fffde7; outline: 2px solid #fbc02d; }
  ```
- [X] T024 Verify US1 sample renders cleanly: `cd web && trunk serve`, load `Struct basic`, step past `let p = Point { x: 1, y: 2 }`, confirm the per-field rows render as designed. Take screenshot for the UX checkpoint.

- [X] T024b **Bugfix surfaced by the UX checkpoint** (user reported `struct Point { x: i32, y: f64 } ... Point { x: 1, y: 2 }` rendering `y: i32` instead of `y: f64`): `eval_expr`'s `Expr::LitInt` arm previously only honored `Ty::Int(_)` records from typeck and fell through to default `i32` when typeck had coerced the int literal to a float-typed context. Pre-existing latent bug surfaced for the first time by M07.4's struct field-type coercion. Fix: `LitInt` arm now checks `Ty::Float(_)` first and emits `Value::Float` with the same narrowing semantics as `LitFloat`. New regression test `run_pipeline_struct_int_to_float_coercion`.

**🎨 UX CHECKPOINT**: pause and present the rendered visualization to the user. Discuss tweaks. **Do not proceed to Phase 5 until the user signs off on Proposal A (or explicitly requests switching to Proposal B/C)**. If switching: revise T022/T023 with the new mockup before resuming.

---

## Phase 5: User Story 2 — Field borrow `&p.x` with per-field hover (Priority: P1)

**Goal**: `let r = &p.x;` typechecks with `r: &i32`; emits a `Value::Ref { field_path: vec!["x"], target: Pointee::Slot(p_slot), .. }`; the page renders a blue arrow from `r` to `p` with a `.x` annotation; hover on the arrow lights up ONLY the `x` field's row in `p`'s struct view.

**Independent Test**: load `m07_4_field_borrow.rs`, step past `let r = &p.x`, observe arrow with `.x` annotation; hover lights up just the `x` row in `p`.

### Implementation

- [X] T025 [US2] In `src/typeck.rs`, extend `typecheck_borrow` (M06 helper) to accept `Expr::FieldAccess { receiver: Expr::Ident, name }` as a place expression:
  - Resolve the receiver's binding. Verify its type is `Ty::Struct(_)` (multi-level rejected: nested struct fields not supported in M07.4).
  - Find the field's type in the struct's schema. Missing → "no field `<name>` on struct `<struct>`".
  - Take the borrow via `borrow_tracker.try_take_shared` / `try_take_mut` (on the receiver's binding — field-level borrow tracking deferred).
  - Result type: `Ty::Ref { inner: Box::new(field_ty), mutable }`.
  - Reject deeper place shapes: `&p.x.y`, `&(...).x`, etc. with clear error.

- [X] T026 [US2] In `src/eval.rs`, extend `Expr::Borrow` evaluation. When inner is `Expr::FieldAccess { receiver: Expr::Ident(_, _), name }`:
  - Resolve the receiver to its slot via `lookup_local_slot`.
  - Take the borrow (allocate a new `BorrowId`).
  - **Skip emitting `BorrowShared`/`BorrowEnd` events** — slot-target field borrows use M07.3's lazy-materialization pattern.
  - Construct `Value::Ref { borrow_id, target: Pointee::Slot(receiver_slot), mutable, field_path: vec![name.clone()] }`.
  - Whoever evaluates the enclosing `let r = &p.x` will subsequently emit a `SlotWrite { value: Value::Ref { .. field_path = ["x"] } }` for `r`'s slot — this is what triggers the UI's lazy materialization.

- [X] T027 [US2] In `src/ui.rs`, extend `apply_event`'s SlotWrite arm for `Value::Ref`:
  - When `field_path` is non-empty AND no matching `ActiveBorrowState` exists in `world.borrows` for the borrow_id (lazy-materialization case): insert a new `ActiveBorrowState` entry with `source_slot: Some(slot_id)`, `target: BorrowTarget::Slot(target_slot_id)`, `mutable`, AND `field_label: Some(format!(".{}", field_path[0]))`.
  - When a matching `ActiveBorrowState` already exists (BorrowShared was emitted — shouldn't happen for field borrows in M07.4, but defensive): just update `field_label` on the existing entry.
  - `ArrowView` builder in `state_snapshot`: copy `field_label` from each `ActiveBorrowState` into the resulting `ArrowView`.

- [X] T028 [P] [US2] In `web/index.js`, extend `renderArrows`: when `arrow.field_label` is set, render a small text annotation at the arrow midpoint (analogous to the existing `[len: N]` slice annotation). Use a new CSS class `.arrow-field-label`. Style: small monospace text in muted color, similar to the slice label but with a `.x` prefix.
- [X] T029 [P] [US2] In `web/index.js`, extend the arrow-hover handler (existing `findHoverTargets` / hover plumbing): when the hovered arrow has `field_label`, query `[data-slot-id=<target>] .struct-field[data-field-name="<field_name>"]` (strip the leading `.` from `field_label`) and toggle `.field-borrow-highlighted` on the matched element. Clear on hover-leave.
- [X] T030 [P] [US2] In `web/style.css`, add `.arrow-field-label` styling — small text, monospace, muted color, positioned near the arrow midpoint. The `.struct-field.field-borrow-highlighted` rule was already added in T023.

- [X] T031 [US2] Add 1 sample program pair: `tests/samples/m07_4_field_borrow.rs` and `web/samples/m07_4_field_borrow.rs`. Content:
  ```rust
  struct Point { x: i32, y: i32 }
  fn main() {
      let p = Point { x: 1, y: 2 };
      let r = &p.x;
  }
  ```

- [X] T032 [US2] In `src/pipeline.rs` `mod tests`, add unit tests:
  - `run_pipeline_field_borrow`: asserts the trace contains a `SlotWrite { value: Value::Ref { borrow_id, target: Pointee::Slot(p_slot_id), mutable: false, field_path: vec!["x"] } }` for `r`. Asserts NO `BorrowShared` event with the same borrow_id (lazy materialization).
  - `run_pipeline_field_borrow_unknown`: source `let r = &p.z;` — asserts pipeline returns ParseError with "no field `z`".

- [X] T033 [US2] In `web/index.html`, add a dropdown `<option>` for `m07_4_field_borrow.rs`.

**Checkpoint**: US2 fully functional. `cargo test` passes. Page renders the field borrow with arrow + `.x` annotation; hover lights up only the `x` row.

---

## Phase 6: User Story 3 — Method definition + dispatch (Priority: P1)

**Goal**: `impl Point { fn x(&self) -> i32 { self.x } } let v = p.x();` typechecks; the method dispatches via `ImplRegistry.methods`; eval enters a new frame for the method body with `self` bound to `&p`; returns `1_i32`; `v` lands `1_i32`.

**Independent Test**: load `m07_4_method.rs`, step through `let v = p.x()`, observe the method frame card slide in with `self : &Point` row; observe a borrow arrow from `self` to caller's `p`; step the body to see `self.x → 1`; observe `ReturnValue` then `FrameLeave`; observe `v = 1_i32` land in main's frame.

### Implementation

- [X] T034 [US3] In `src/typeck.rs`, extend `typecheck_method_call` (M07 helper) — at the END of its existing match arms (so hardcoded built-ins win per R-018), add a fallback that consults `ImplRegistry.methods`:
  - Compute `receiver_struct_name`: from `Ty::Struct { name, .. }` directly, or via auto-deref from `Ty::Ref { Ty::Struct { name, .. }, .. }`.
  - Look up `(receiver_struct_name, name)` in `ImplRegistry.methods`. If missing → existing "no method" error (now widened to mention user methods too).
  - Typecheck each arg against the method's `FnSig.params` (skipping the implicit self slot if applicable; the FnSig's `params` is the EXPLICIT-only param list, since self is recorded separately on the `ParamKind`). Mismatch → "expected `<ty>`, found `<ty>`".
  - Result type: `FnSig.ret`.

- [X] T035 [US3] In `src/typeck.rs`, extend `typecheck_field_access` (T013) for the auto-deref case used by `self.x` inside method bodies — this should already work from T013's `Ty::Ref { inner: Ty::Struct, .. }` arm; verify and add an explicit test for it.

- [X] T036 [US3] In `src/eval.rs`, extend method-call evaluation. When typeck resolved the call to a user-defined `ImplRegistry.methods` entry (the typeck-side resolution is recoverable from the method-call expression's metadata — verify how the existing built-in dispatch records this and mirror):
  - Allocate a new `FrameId` and emit `FrameEnter { frame_id, fn_name: format!("{}::{}", struct_name, method_name), span: <call_site> }`.
  - For self-receiver methods (`SelfShared` / `SelfMut`): eval the receiver expression to get a slot id (must be `Expr::Ident` in M07.4 for borrow tracker hookup). Emit `SlotAlloc { name: "self", ty: Ty::Ref { Ty::Struct(receiver_ty), mutable: <SelfMut?> }, .. }` then `SlotWrite { value: Value::Ref { target: Pointee::Slot(receiver_slot), mutable, field_path: vec![] } }`. Take a borrow via `borrow_tracker`.
  - For each explicit param: eval the corresponding arg, emit `SlotAlloc` + `SlotWrite`.
  - Execute the method body via `eval_block` against the impl's `FnDecl.body`.
  - Emit `ReturnValue { frame_id, value: <body result>, span: <body tail span> }` (skip for `Value::Unit` from entry-frame implicit-unit returns per M07.2's existing rule — but for methods we DO emit since the method isn't an entry frame).
  - Emit `FrameLeave { frame_id, return_value, span }`.
  - The borrow taken for self is released here (BorrowEnd if needed — actually, slot-target borrows skip BorrowEnd too, so just remove from borrow_tracker without an event).

- [X] T037 [US3] In `src/eval.rs`, verify `Expr::FieldAccess` arm handles the `self.x` case in method bodies. When receiver eval produces `Value::Ref { target: Pointee::Slot(_), field_path: [], .. }` (a `&self` ref), read the target slot's `Value::Struct` and look up `name`. Should fall out of T015's existing code; verify with a method-body test.

- [X] T038 [US3] Add 1 sample program pair: `tests/samples/m07_4_method.rs` and `web/samples/m07_4_method.rs`. Content:
  ```rust
  struct Point { x: i32, y: i32 }
  impl Point { fn x(&self) -> i32 { self.x } }
  fn main() {
      let p = Point { x: 1, y: 2 };
      let v = p.x();
  }
  ```

- [X] T039 [US3] In `src/pipeline.rs` `mod tests`, add unit tests:
  - `run_pipeline_method`: asserts the trace contains a `FrameEnter` for `Point::x`, a `SlotWrite` for `self` carrying `Value::Ref { target: Pointee::Slot(p_slot), field_path: vec![] }`, a `ReturnValue` carrying `Value::Int { I32, 1 }`, then a `FrameLeave`, then a `SlotWrite` for `v` carrying `Value::Int { I32, 1 }`.
  - `run_pipeline_method_self_field`: same source — asserts `self.x` inside the body produces the correct value (read-through-ref + field-access).
  - `run_pipeline_method_two_methods`: source `impl Point { fn x(&self) -> i32 { self.x } fn dist(&self) -> i32 { self.x } } let v = p.dist();` — asserts dispatch correctly resolves to `dist`.
  - `run_pipeline_method_unknown`: source `let v = p.bogus();` — asserts ParseError "no method `bogus`".

- [X] T040 [US3] In `web/index.html`, add a dropdown `<option>` for `m07_4_method.rs`.

**Checkpoint**: US3 fully functional. `cargo test` passes. Page renders the method call with the new method frame, self-borrow arrow, ReturnValue annotation.

---

## Phase 7: User Story 4 — Associated function (no `self`) (Priority: P2)

**Goal**: `impl Point { fn new(x: i32, y: i32) -> Point { Point { x, y } } } let p = Point::new(1, 2);` typechecks; the path call dispatches via `ImplRegistry.assoc_fns`; eval enters a new frame for `new`, binds `x=1` and `y=2`, constructs the struct via field-shorthand, returns it; `p` lands the new struct.

**Independent Test**: load `m07_4_associated_fn.rs`, step through `let p = Point::new(1, 2)`, observe the `new` frame, its params, the constructed struct in the return, then `p` landing the struct.

### Implementation

- [X] T041 [US4] In `src/typeck.rs`, extend `typecheck_path_call` (M07 helper) — at the END of its existing match arms (so hardcoded built-ins win), add a fallback that consults `ImplRegistry.assoc_fns`:
  - Look up `segments.to_vec()` in `ImplRegistry.assoc_fns`. If missing → existing "unknown path" error (now widened to mention user assoc fns too).
  - Typecheck each arg against the assoc fn's `FnSig.params` with coercion. Mismatch → clear error.
  - Result type: `FnSig.ret`.

- [X] T042 [US4] In `src/eval.rs`, extend path-call evaluation. When typeck resolved the path call to a user-defined `ImplRegistry.assoc_fns` entry:
  - Allocate `FrameId`, emit `FrameEnter { frame_id, fn_name: <segments.join("::")>, span: <call_site> }`.
  - For each param: eval the arg, emit `SlotAlloc` + `SlotWrite`. NO `self` slot.
  - Execute the fn body. Emit `ReturnValue` + `FrameLeave`.
  - Same flow as method-call (T036) without the self-receiver setup. Worth extracting a shared helper if both paths look similar.

- [X] T043 [US4] Add 1 sample program pair: `tests/samples/m07_4_associated_fn.rs` and `web/samples/m07_4_associated_fn.rs`. Content:
  ```rust
  struct Point { x: i32, y: i32 }
  impl Point { fn new(x: i32, y: i32) -> Point { Point { x, y } } }
  fn main() {
      let p = Point::new(1, 2);
  }
  ```

- [X] T044 [US4] In `src/pipeline.rs` `mod tests`, add unit tests:
  - `run_pipeline_assoc_fn`: asserts the trace contains a `FrameEnter` for `Point::new`, two param SlotWrites (`x=1`, `y=2`), a `ReturnValue` carrying `Value::Struct { name: "Point", fields: [("x", Int{I32,1}), ("y", Int{I32,2})] }`, a `FrameLeave`, then a `SlotWrite` for `p` carrying that struct.
  - `run_pipeline_assoc_fn_mixed`: source mixing `Vec::new` (builtin) and `Point::new` (user) — asserts both dispatch correctly.
  - `run_pipeline_struct_forward_ref`: source where `impl Point { fn ... }` precedes `struct Point { ... }` — asserts the program typechecks (proves two-pass typeck works).
  - `run_pipeline_struct_no_heap`: any struct-only sample — asserts the trace contains zero `HeapAlloc`/`HeapRealloc`/`HeapFree` events (echoing M07.3's structural test).

- [X] T045 [US4] In `web/index.html`, add a dropdown `<option>` for `m07_4_associated_fn.rs`.

**Checkpoint**: US4 fully functional. `cargo test` passes. Page renders the associated function call with its frame.

---

## Phase 8: Polish & Cross-Cutting

**Purpose**: snapshot re-baselines, bundle-size check, sample integration verification, warnings check, doc updates.

- [X] T046 [P] Run `cargo insta test`. Verify M01/M02 snapshots are byte-identical. Verify M03 snapshots are byte-identical (the `field_path` extension should be invisible in JSON when empty). If any M03 snapshot accidentally changed, investigate — it should not.
- [X] T047 [P] Build WASM release and measure bundle size: `cd web && trunk build --release && wasm-strip dist/*.wasm | true && ls -lh dist/*.wasm`. Compare to M07.3 baseline (294,655 B). Acceptable if ≤ +25% (~368 KB). If over: candidate cuts per plan.md Complexity Tracking (drop US4 to M07.5, drop field-assignment, inline less rendering scaffolding).
- [X] T048 [P] Run `RUSTFLAGS="-D warnings" cargo build --release` AND `cargo test`. Both should be clean (zero warnings). Fix any new warnings introduced.
- [X] T049 [P] Run `cargo clippy --all-targets -- -D warnings`. Fix any new lints.
- [X] T050 Manual QA per `specs/015-m07-4-structs-impl/quickstart.md` SC-008 procedure. ~8 minutes. Step through US1–US4 in the page; verify error UX via live editing; cycle through M01–M07.3 samples to confirm no regressions.
- [X] T051 Update `CLAUDE.md` "Recent Changes" footer to note 015-m07-4-structs-impl added (if the speckit update-agent-context didn't already).
- [X] T052 (Optional but recommended) **Implement field assignment** (deferral #2 from spec/plan): extend M06.1's place-expression set in `typecheck_assign` and `eval` to accept `Expr::FieldAccess { receiver: Expr::Ident(_, _), name }` when the receiver is a `mut` binding. Eval: clone the receiver slot's `Value::Struct`, mutate the named field, emit a `SlotWrite` with the new struct value. Add a test `run_pipeline_struct_field_assign` and a sample `m07_4_struct_field_assign.rs`. Skip if it lands cleanly; defer to M07.5 if it costs >100 LOC.
- [X] T053 Final commit messages + tag. Push branch. Note in the eventual merge MR: "8th invocation of the closed-enum-with-revisions rule. Extends `Value::Ref` with serde-default-empty `field_path`. Two-pass typeck for forward references."

---

## Dependencies

```text
Phase 1 (Setup)
  └─ T001 (verify baseline)

Phase 2 (Foundational) — blocks ALL user-story phases
  ├─ T002 (contract amendment, can run anytime)
  ├─ T003 [P] (lexer keywords)
  ├─ T004 [P] (token variants)
  ├─ T005 (AST surface — uses tokens; depends on T004)
  ├─ T006 (parser — uses AST + tokens; depends on T004, T005)
  ├─ T007 [P] (resolve — uses AST; depends on T005)
  ├─ T008 (typeck Ty::Struct — uses AST; depends on T005)
  ├─ T009 (event Value::Struct + Ref extension — depends on T008 for Ty)
  └─ T010 [P] (ui scaffolding — depends on T009 for Value::Ref shape)

Phase 3 (US1) — depends on Phase 2
  ├─ T011 (two-pass typeck infra)
  ├─ T012 (struct_lit typeck — depends on T011)
  ├─ T013 (field_access typeck — depends on T011)
  ├─ T014 (struct_lit eval — depends on T012)
  ├─ T015 (field_access eval — depends on T013)
  ├─ T016 [P] (apply_event SlotWrite arm for Value::Struct)
  ├─ T017 [P] (render_value for Value::Struct)
  ├─ T018 (sample pair)
  ├─ T019 (5 unit tests — depends on T011–T017)
  └─ T020 (dropdown entry)

Phase 4 (UI viz — Proposal A) — depends on Phase 3 (US1 must work at pipeline level)
  ├─ T021 (renderStacks extension)
  ├─ T022 (renderStructView helper)
  ├─ T023 (CSS)
  └─ T024 (visual verification + 🎨 UX CHECKPOINT)

🎨 PAUSE for user review before Phase 5.

Phase 5 (US2) — depends on Phase 4 (visual must be approved)
  ├─ T025 (typeck_borrow extension for FieldAccess)
  ├─ T026 (eval Expr::Borrow extension for FieldAccess)
  ├─ T027 (apply_event for Value::Ref with field_path)
  ├─ T028 [P] (arrow field label rendering)
  ├─ T029 [P] (hover handler for field highlights)
  ├─ T030 [P] (CSS for arrow label)
  ├─ T031 (sample pair)
  ├─ T032 (2 unit tests)
  └─ T033 (dropdown entry)

Phase 6 (US3) — depends on Phase 5 (independent of US4)
  ├─ T034 (method-call typeck dispatch)
  ├─ T035 (auto-deref verification)
  ├─ T036 (method-call eval frame entry)
  ├─ T037 (self.x verification)
  ├─ T038 (sample pair)
  ├─ T039 (4 unit tests)
  └─ T040 (dropdown entry)

Phase 7 (US4) — depends on Phase 6 (assoc-fn reuses method-call frame machinery)
  ├─ T041 (path-call typeck dispatch)
  ├─ T042 (path-call eval frame entry)
  ├─ T043 (sample pair)
  ├─ T044 (4 unit tests)
  └─ T045 (dropdown entry)

Phase 8 (Polish) — depends on Phase 7
  └─ T046–T053 (snapshot/bundle/warnings/QA/docs)
```

---

## Parallel execution opportunities

- **Phase 2**: T003, T004, T007, T010 all parallelizable [P]. T005 depends on T004; T006 depends on T004+T005; T008 depends on T005; T009 depends on T008.
- **Phase 3**: T016 and T017 parallelizable [P] (different concerns in `src/ui.rs` — apply_event arm vs render helpers).
- **Phase 5**: T028, T029, T030 all parallelizable [P] (different files: JS arrow rendering, JS hover handler, CSS).
- **Phase 8**: T046, T047, T048, T049 all parallelizable [P].

---

## Implementation strategy

**MVP scope** = **US1 only** (struct decl + literal + field access). Lands the byte-layout pedagogy and the new struct view. Even without US2/US3/US4, the page demonstrates "you can model your own data type". Estimated incremental size: ~400 LOC.

**Incremental delivery**:
1. **MVP (US1)**: Phases 1–4 (Setup + Foundational + US1 + UI viz). Ship the struct visualization as a standalone milestone if needed; field borrows + methods can land in a follow-up.
2. **+US2 (field borrows)**: Phase 5. Adds the per-field hover pedagogy.
3. **+US3 (methods)**: Phase 6. Adds method dispatch.
4. **+US4 (assoc fns)**: Phase 7. Adds the constructor pattern.
5. **+Polish**: Phase 8. Snapshot/bundle/QA/docs.

**Recommended landing order**: ship all 4 user stories in one merge (the AST/typeck/eval surface is too cross-cutting to split cleanly mid-flight; the UI viz checkpoint is the only natural pause). If implementation slips past XL → bail US4 to M07.5 and ship M07.4 as US1+US2+US3.

**Field assignment (T052)**: deferral #2. Include if it falls out cleanly (< 100 LOC); defer to M07.5 otherwise.
