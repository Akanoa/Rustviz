# Implementation Plan: M07.4 — Structs + `impl` blocks (named-field composite types with methods)

**Branch**: `015-m07-4-structs-impl` | **Date**: 2026-05-24 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/015-m07-4-structs-impl/spec.md`

## Summary

Introduce Rust's `struct` declarations + `impl` blocks (methods + associated functions). The first milestone in which a learner can **model their own data types**. Stack-allocated composite values; each field laid out contiguously at a known offset. Methods extend the M07 hardcoded dispatch table with a user-defined registry built during typeck.

**8th invocation of the closed-enum-with-revisions rule**: additive `Ty::Struct { name, fields }`, additive `Value::Struct { name, fields }`, and an **additive extension** of `Value::Ref` with a `field_path: Vec<String>` (empty = whole binding, non-empty = sub-field path). No new `MemEvent` variants. No new `Pointee` variants — field borrows reuse `Pointee::Slot(_)` (already produced by M03 borrows and M07.3 array slices).

**Meaty part flagged by the user**: the **struct visualization in the stack slot**. This plan locks down the data flow (`SlotRowView.struct_view: Option<StructView>`, JSON wire shape, hover plumbing) and presents a **recommended visual** with ASCII mockup in `research.md` (R-016). The visual itself is the explicit iterate-on-this piece — the user will review the proposal and we'll modify step by step.

Authority chain: `MILESTONES.md` › M07.4 → `spec.md` (this feature) → this plan.

## Technical Context

**Language/Version**: Rust 2024 edition (same toolchain as M01–M07.3). No new toolchain requirements.
**Primary Dependencies**: existing `indexmap`, `serde`, `serde_json`, `wasm-bindgen`, `js-sys`, `console_error_panic_hook`. **No new Rust deps**. **No JS deps changes**.
**Storage**: in-memory; no new files. Struct contents live in the eval-side slot's `LocalSlot.value` as a new `Value::Struct { name, fields: Vec<(String, Value)> }` variant. Struct schemas + impl registries are typeck-side `IndexMap`s built during phase 1 (2-pass typeck). M01/M02 snapshot tests stay byte-identical (no existing L1 sample constructs structs).
**Testing**: existing `cargo test --test m01 / m02 / m03` should pass byte-identical (additive variants only). New `cargo test --lib pipeline::tests` covering: struct decl + literal, field access, missing/extra/wrong-type-field errors, field-shorthand, field borrow + per-field metadata, field assignment (if scope permits — see deferral #2), method call, associated function, two methods in one impl, user-method vs built-in disambiguation, unknown-method error, two-pass forward reference. **≥ 10 new tests**. Manual M07.4 QA per the SC-008 procedure.
**Target Platform**: same as M01–M07.3 (host + `wasm32-unknown-unknown`).
**Project Type**: Rust library + companion UI. Touches ~6 source modules (parse/{ast,parser}, resolve, typeck, eval, ui) + JS struct rendering + CSS for struct slot layout. Sized XL.
**Performance Goals**: same pipeline latency budget. Struct operations are O(1) per field access; method dispatch is O(methods_per_struct) — bounded by the small impl-block size in practice.
**Constraints**: M01/M02 byte-identical; M03 snapshots byte-identical for existing samples (additive `Value::Struct` + `Ty::Struct` don't affect existing variants' Debug output; `Value::Ref` extension uses `#[serde(default, skip_serializing_if = "Vec::is_empty")]` for `field_path` so existing borrow snapshots stay identical); WASM bundle ≤ +25% vs M07.3 baseline (294,655 B → ≤ ~368 KB raw) per SC-009; zero warnings under `-D warnings` (SC-010); existing M01–M07.3 features preserved.
**Scale/Scope**: ~6 source modules + JS struct rendering + CSS additions + ≥ 4 sample pairs + ≥ 10 new unit tests. **Estimated ~1200–1500 LOC net change**. Sizing: **XL** per the rubric — comparable to M07 (heap milestone).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is still the unfilled speckit template. Same vacuous PASS as features 001–014.

**Post-design re-check**: still vacuous, still PASS.

## Project Structure

### Documentation (this feature)

```text
specs/015-m07-4-structs-impl/
├── plan.md                          # This file
├── spec.md                          # Feature spec
├── research.md                      # Phase 0: 17 design decisions (R-016 = UI struct viz PROPOSAL)
├── data-model.md                    # Phase 1: 2 new AST items, 2 new Expr variants, Param self-receiver, 1 new Ty variant, 1 new Value variant, 1 Value::Ref extension, SlotRowView+ArrowView extensions
├── quickstart.md                    # Phase 1: dev workflow + manual QA procedure
├── contracts/
│   └── m07-4-protocol-delta.md      # Phase 1: 8th closed-enum invocation; field_path additive extension
└── checklists/
    └── requirements.md              # From /speckit-specify
```

### Source Code (repository root) — files M07.4 touches

```text
src/
├── parse/
│   ├── token.rs                # Unchanged — `Struct`, `Impl`, `SelfKw`, `Comma`, `Colon`, `Dot`, `LBrace`, `RBrace`, `Amp`, `AmpMut` all exist or are simple keyword additions. Add `TokenKind::Struct`, `TokenKind::Impl`, `TokenKind::SelfKw` if not already lexed (lexer.rs `KEYWORDS` table extension).
│   ├── lexer.rs                # MODIFIED — extend KEYWORDS with `"struct"` → `TokenKind::Struct`, `"impl"` → `TokenKind::Impl`, `"self"` → `TokenKind::SelfKw`. Add minimal new tokens (3 keyword recognitions; no new punctuation).
│   ├── ast.rs                  # MODIFIED — add `Item::Struct { name, fields: Vec<StructField>, span }` and `Item::Impl { ty_name: String, items: Vec<FnDecl>, span }`. Add `StructField { name, ty: Type, span }`. Add `Expr::StructLit { path: Vec<String>, fields: Vec<StructLitField>, span }` with `StructLitField { name, value: Option<Expr>, span }` (None = shorthand). Add `Expr::FieldAccess { receiver: Box<Expr>, name: String, span }`. Extend `Param` with `kind: ParamKind { Normal, SelfOwned, SelfShared, SelfMut }` (default Normal — back-compat for all existing free fns).
│   └── parser.rs               # MODIFIED — `parse_item` dispatches on `Struct`/`Impl` keywords in addition to `Fn`. New `parse_struct_decl`, `parse_impl_block`. `parse_atom`: after `Ident` (and optionally `Path` via `::`), if next token is `LBrace` at expression-atom position, parse struct literal (with field-shorthand). `parse_expr` postfix loop: `Dot` followed by ident NOT followed by `LParen` → `Expr::FieldAccess`; followed by `LParen` → `Expr::MethodCall` (existing). `parse_param`: when seeing `Amp`/`AmpMut`/`SelfKw` at param index 0, parse a self-receiver. Document the **`if cond { ... }`-style ambiguity** (M07.4 follows Rust's rule: struct literals disallowed in cond positions for `if`/`while` — practical mitigation: typeck rejects non-bool cond, which catches the same cases).
├── resolve.rs                  # MODIFIED — `resolve_expr` adds arms for `Expr::StructLit` (resolve each field-init expr; for shorthand, resolve the bare ident the same way) and `Expr::FieldAccess` (recurse on receiver only — `name` is a symbol lookup deferred to typeck). `resolve_item` adds arms for `Item::Struct` (register the struct's name as a type binding) and `Item::Impl` (no new bindings; types resolved at typeck phase 1).
├── typeck.rs                   # MODIFIED — add `Ty::Struct { name: String, fields: Vec<(String, Ty)> }`. Update `Ty::name()` (returns the bare name `"Point"`), `Ty::is_copy()` (always false in M07.4 — primitive-only restriction means the bytes ARE copyable but we model structs as non-Copy by default to leave room for future non-Copy fields; reconsider if it costs anything for the headline pedagogy. **Plan-phase decision: STRUCTS ARE COPY in M07.4** — matches the primitive-only restriction's natural consequence and avoids "moved" pedagogy that would conflict with field access). Add `Ty::Struct` arm to `ty_from_ast` for `Type::Path { segments: ["Point"], .. }` resolution against the new struct registry. **Two-pass typeck**: phase 1 collects every `Item::Struct` into `StructRegistry { schemas: IndexMap<String, Vec<(String, Ty)>> }` AND every `Item::Impl` into `ImplRegistry { methods: IndexMap<(String, String), FnSig>, assoc_fns: IndexMap<Vec<String>, FnSig> }`. Phase 2 typechecks fn bodies with both registries visible. Extend `typecheck_method_call` to consult `ImplRegistry.methods` after the M07 hardcoded built-ins (M07 wins on tie). Extend `typecheck_path_call` to consult `ImplRegistry.assoc_fns` after the M07 hardcoded paths. Add `typecheck_struct_lit` (verify every declared field present, no extras, each field's expr coerces to declared field ty, support shorthand). Add `typecheck_field_access` (receiver must be `Ty::Struct(_)` OR `Ty::Ref { inner: Ty::Struct(_), .. }` for auto-deref; field name must exist; result = field's ty). Extend `typecheck_borrow` to accept `Expr::FieldAccess` as a place expression (result: `Ty::Ref { inner: field_ty, mutable }`). Extend `typecheck_assign` to accept `Expr::FieldAccess { receiver: Expr::Ident, .. }` as lhs (if receiver binding is `mut` AND the resolved struct is Copy — see deferral #2).
├── event.rs                    # MODIFIED — add `Value::Struct { name: String, fields: Vec<(String, Value)> }` variant. **Extend `Value::Ref`** with `field_path: Vec<String>` field (empty = whole binding, non-empty = sub-field navigation path — single segment in M07.4 since nested structs are out of scope). `#[serde(default, skip_serializing_if = "Vec::is_empty")]` on `field_path` keeps existing M03/M06 snapshots byte-identical (deserialization assumes empty when missing). Update `Value::type_name()` (returns `"{}"` for Struct — short tag; full name from Ty layer). No new `MemEvent` variants. No new `Pointee` variants.
└── eval.rs                     # MODIFIED — `Expr::StructLit` arm in `eval_expr`: eval each field (resolving shorthand via local binding lookup), build `Value::Struct { name, fields }` in declaration order. `Expr::FieldAccess` arm: eval receiver. If `Value::Struct`, find field by name → clone. If `Value::Ref` to a Slot holding a Struct, auto-deref read (look up slot, find field). If `Value::Ref` with non-empty `field_path` (a `&p.x` borrow re-used), navigate the path. `Expr::Borrow` arm: when inner is `Expr::FieldAccess { receiver: Expr::Ident, name }`, register the borrow on the receiver's binding (existing borrow_tracker call); construct `Value::Ref { target: Pointee::Slot(receiver_slot), field_path: vec![name], .. }` — skip `BorrowShared`/`BorrowEnd` events (slot-target field borrows follow M07.3's lazy materialization pattern). Method dispatch: when typeck resolved the call to a user-defined method, eval enters a new frame for the method (`FrameEnter` + `SlotAlloc`/`SlotWrite` for params, including `self` for instance methods), executes the body, emits `ReturnValue` + `FrameLeave`. Associated function: same flow without `self`. `Stmt::Assign` extension for `Expr::FieldAccess` lhs (if deferral #2 lands): clone the receiver slot's `Value::Struct`, mutate the named field, emit `SlotWrite` with the new struct value.

src/ui.rs                       # MODIFIED — `SlotRowView` gains optional `struct_view: Option<StructView>` field (mutually exclusive with both `value` and `inline_cells`). New `StructView { name: String, fields: Vec<StructFieldView> }` with `StructFieldView { name: String, ty_label: String, size: u32, display: String }`. `apply_event` SlotWrite arm: when value is `Value::Struct { name, fields }`, build the StructView; set `value = ""` and `inline_cells = None`. `apply_event` SlotWrite arm for `Value::Ref { field_path, .. }`: when `field_path` is non-empty AND no prior BorrowShared/BorrowMut fired (lazy materialization for slot-target field borrows), insert an `ActiveBorrowState` entry with `field_path` recorded. `ArrowView` gains `field_label: Option<String>` populated from the borrow's `field_path` (joined with `.` and prefixed — e.g. `[".x"]` → `".x"`).

tests/
├── m01.rs / m02.rs / m03.rs        # Unchanged. Snapshots byte-identical (no existing sample constructs `Ty::Struct`, `Value::Struct`, or `Value::Ref` with non-empty `field_path`).
└── samples/
    ├── (existing)                  # Unchanged.
    └── m07_4_*.rs                  # NEW (4 files): m07_4_struct_basic, m07_4_field_borrow, m07_4_method, m07_4_associated_fn.

web/
├── samples/                    # MODIFIED — add 4 m07_4_*.rs mirrors.
├── index.html                  # MODIFIED — dropdown grows 4 entries.
├── index.js                    # MODIFIED — `renderStacks()` extends: when `slot.struct_view` present, render the struct via `renderStructView(slot.struct_view, slotId)` (defined per research R-016 mockup — vertical labeled rows). Per-field hover highlighting: query `[data-slot-id=X] .struct-field[data-field-name="x"]` on borrow-arrow hover. `renderArrows`: when `arrow.field_label` present, render it as a small text label on the arrow midpoint (analogous to the existing `[len: N]` slice annotation but field-name flavored). `findHoverTargets` extends to include field-borrow path.
├── style.css                   # MODIFIED — `.struct-view` (container inside `.slot-value`), `.struct-field` (one row per field), `.struct-field-label`, `.struct-field-cells`, `.struct-field-value`. Hover state `.struct-field.field-borrow-highlighted` (yellow ring around the field's row when a field-borrow arrow is hovered). Arrow label class `.arrow-field-label`.
└── Trunk.toml                  # Unchanged.

# M03's contract amended for the 8th closed-enum invocation:
specs/004-m03-event-eval/contracts/m03-api.md   # MODIFIED — note M07.4 as the 8th invocation. Adds `Ty::Struct`, `Value::Struct`, extends `Value::Ref` with `field_path`. Pure additive (Ref extension uses serde default). No event-variant changes; no Pointee changes.
```

**Structure Decision**: substantially larger than M07.3 (one new milestone introducing user-defined types unlocks parser + AST + typeck + eval + UI surface all at once). Locked down: data flow + protocol shape + dispatch architecture. **Iterative**: the struct-slot visualization (research R-016) — recommended visual is **vertical labeled rows** with byte-cell strips per field, but the user explicitly asked to iterate step-by-step on this part; treat the mockup as a starting proposal and expect ≥ 1 design-review iteration before implementation.

## Complexity Tracking

> No constitutional violations. Table omitted.

### Notable non-trivial complexity

- **Two-pass typeck**: M01–M07.3 typeck did a single pass over fns. M07.4 needs phase 1 (collect struct schemas + impl signatures so forward references work — `impl Point` can appear before `struct Point` in the file) AND phase 2 (typecheck bodies with both registries visible). The phase split is small but cuts across the existing `typeck()` entry function; risk: subtle ordering bugs (struct declared inside an impl block, etc.). Mitigation: explicit registry types (`StructRegistry`, `ImplRegistry`) built in phase 1, passed to `Typechecker` in phase 2; integration test specifically for forward-reference cases (R-009).
- **Struct-literal parser ambiguity with `if cond { ... }`**: Rust's solution is to forbid struct literals in cond positions. M07.4 follows the same rule; practical mitigation = typeck rejects non-bool cond. Document in `parser.rs`; sample tests must NOT trigger the ambiguity (no `if Point { x: 1, y: 2 }.y > 0 { ... }`).
- **Field-borrow as place expression**: M06's place-expression rule was "Ident only". M07.4 extends it to `Expr::FieldAccess { receiver: Expr::Ident, .. }` (single-level access on a binding). Multi-level (`p.x.y`) is out of scope. Mitigation: explicit AST-shape match in `typecheck_borrow`; reject deeper shapes with clear error.
- **`Value::Ref` extension with `field_path`**: adding a field to an existing struct-variant requires `#[serde(default, skip_serializing_if = "Vec::is_empty")]` to preserve byte-identical M06/M07/M07.1/M07.2/M07.3 borrow snapshots. Verify with `cargo insta test` before merging.
- **Method dispatch + frame entry**: when a method call dispatches to a user impl, eval must (1) enter a new frame via `FrameEnter`, (2) allocate `self` slot + bind it to the receiver borrow (for `&self`/`&mut self`) or the moved value (for `self`), (3) bind each explicit param slot, (4) execute the body, (5) emit `ReturnValue` + `FrameLeave`. This is the standard fn-call flow; M07.4 just needs to plumb the new dispatch table into the existing call-frame machinery. Risk: subtle off-by-one in self-receiver vs explicit-param indexing. Mitigation: `ParamKind` enum on `Param` makes the self-receiver case lexically distinct in the parser → eval path.
- **Auto-deref for `self.x` inside method bodies**: `self` has type `&Self` for `&self` methods. `self.x` must work as if it were `(*self).x`. Plan: typeck rule (not parser sugar) — `typecheck_field_access` accepts both `Ty::Struct(_)` AND `Ty::Ref { inner: Ty::Struct(_), .. }` receivers, transparently reading the field type. Eval mirrors: `Expr::FieldAccess` on a `Value::Ref` reads through the ref to find the slot, then looks up the field.
- **Field assignment `p.x = 5` (deferral #2)**: extending M06.1's place-expression set. Plan: support if it falls out cleanly (re-use `Stmt::Assign` path); the modification is "clone the slot's Value::Struct, mutate the named field, emit SlotWrite with the new value". Recommendation: **include** — costs ~30 LOC, big pedagogical payoff (mutability extends to structs). Skip if implementation slips. Same machinery applies to `self.x = v;` inside `&mut self` methods (where the receiver is a Ref — needs an extra resolution step).
- **Struct viz in the stack slot (research R-016)**: locked-in data shape (`SlotRowView.struct_view: Option<StructView>`); recommended visual = vertical labeled rows. **EXPLICITLY ITERATIVE**: the user has flagged this as the meaty step-by-step piece. Plan stages a primary mockup + 2 alternatives in R-016 (`Vertical labeled rows`, `Compact horizontal segments`, `Single byte strip with field-name brackets`). Implementation cannot proceed past the JS rendering step without a UX checkpoint.
- **Bundle growth ≤ +25% per SC-009**: substantial new surface (AST + typeck registries + eval method-call frame entry + UI per-field rendering). Estimated +60–80 KB. Verify post-merge with `wasm-strip target/wasm32-*/release/*.wasm | wc -c`. If miss: candidate cuts are (a) drop associated functions to M07.5 (-15 KB est.), (b) drop field assignment to M07.5 (-10 KB est.), (c) inline less rendering scaffolding (-5 KB est.).
- **No new MemEvent variants**: existing `SlotAlloc` + `SlotWrite` + `FrameEnter` + `FrameLeave` + `ReturnValue` carry everything. Method calls reuse the standard fn-call event flow. Field borrows reuse `Pointee::Slot(_)` + the M07.3 lazy-materialization pattern (no `BorrowShared`/`BorrowEnd` for slot-target borrows; the UI materializes the arrow at the `SlotWrite` that lands the `Value::Ref`).
