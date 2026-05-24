# Implementation Plan: M07.7 — Trait objects (`&dyn Trait`, vtables, dynamic dispatch)

**Branch**: `018-m07-7-trait-objects` | **Date**: 2026-05-24 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/018-m07-7-trait-objects/spec.md`

## Summary

Introduce trait objects (`&dyn Trait`, `&mut dyn Trait`, `Box<dyn Trait>`) — Rust's dynamic-dispatch mechanism. M07.6 shipped static dispatch via generic bounds (`fn print<T: Show>(x: T)`); M07.7 ships dynamic (`fn print(x: &dyn Show)`). **Headline pedagogy**: fat pointer + vtable + two-step dispatch arrow. A `&dyn Show` value is 16 bytes (data ptr + vtable ptr); a new **VTABLES panel** (analog of M07.2's STATIC MEMORY) holds one vtable box per `(type, trait)` pair; method dispatch goes through a runtime indirection visible as a two-step arrow at the call site.

**11th invocation of the closed-enum-with-revisions rule**: additive `Type::DynTrait` (AST), `Ty::DynRef` (typeck), `Value::DynRef` (runtime fat pointer), `VtableAddr` (new addressing namespace), `MemEvent::VtableAlloc` (analog of `StaticAlloc` from M07.2). First new MemEvent variant since M07.2. No new `Pointee` variants — vtables ride on a separate `vtable: VtableAddr` field on `Value::DynRef` rather than a new Pointee variant (vtables aren't borrow targets in the Slot/Heap/Static sense).

**UX checkpoint expected** — fat-pointer slot rendering + VTABLES panel + two-step dispatch arrows are iterative UI pieces like M07.4's struct view. Plan stages a first cut, then pauses for visual review before refining.

Authority chain: `MILESTONES.md` › M07.7 → `spec.md` (this feature) → this plan.

## Technical Context

**Language/Version**: Rust 2024 edition (same toolchain as M01–M07.6). No new toolchain requirements.
**Primary Dependencies**: existing `indexmap`, `serde`, `serde_json`, `wasm-bindgen`, `js-sys`, `console_error_panic_hook`. **No new Rust deps**. **No JS deps changes**.
**Storage**: in-memory; no new files. Vtable registry lives in typeck's `Typechecker` (`VtableRegistry { vtables: IndexMap<(String, String), VtableAddr> }`, content-deduplicated by `(trait, type)`); eval-side mirror in `Evaluator.vtable_addrs` for runtime dispatch. M01/M02/M03 snapshot tests should stay byte-identical (additive variants + serde-default-empty on new fields preserves wire shape for non-trait-object programs).
**Testing**: existing `cargo test --test m01 / m02 / m03` should pass byte-identical. New `cargo test --lib pipeline::tests` covering: basic &dyn cast + dispatch, &dyn parameter (with multiple types), Box<dyn Trait>, default-method dispatch through dyn, vtable interning (one VtableAlloc per (trait, type) pair across multiple borrows), `i32: Show` coercion-error, inherent-method-via-dyn rejection, static-vs-dyn side-by-side. **≥ 10 new tests**. Manual M07.7 QA per the quickstart procedure with UX checkpoint after first UI cut.
**Target Platform**: same as M01–M07.6 (host + `wasm32-unknown-unknown`).
**Project Type**: Rust library + companion UI. Touches ~6 source modules (parse/{ast,parser,lexer for `dyn`}, typeck, eval, ui) + JS for VTABLES panel + dispatch arrows + CSS. Sized XL — comparable to M07.4 (struct view milestone).
**Performance Goals**: same pipeline latency budget. Vtable dispatch is O(1) lookup; vtable interning is O(1) per call site via IndexMap.
**Constraints**: M03 byte-identical; M01/M02 may re-baseline if new AST fields surface in Debug snapshots (likely byte-identical via serde-default-empty); WASM bundle ≤ +25% vs M07.6 baseline (378,170 B → ≤ ~473 KB raw) per SC-014; zero warnings under `-D warnings` (SC-015); existing M01–M07.6 features preserved.
**Scale/Scope**: ~6 source modules + 4 sample pairs + ≥ 10 new unit tests + new VTABLES panel HTML/CSS/JS. **Estimated ~1500-1800 LOC net change**. Sizing: **XL** per the rubric — comparable to M07.4 (the new UI panel + arrows + fat-pointer rendering push the upper bound).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is still the unfilled speckit template. Same vacuous PASS as features 001–017.

**Post-design re-check**: still vacuous, still PASS.

## Project Structure

### Documentation (this feature)

```text
specs/018-m07-7-trait-objects/
├── plan.md                          # This file
├── spec.md                          # Feature spec
├── research.md                      # Phase 0: 18 design decisions (R-014 = UX-checkpoint visual proposal)
├── data-model.md                    # Phase 1: Type::DynTrait, Ty::DynRef, Value::DynRef, VtableAddr, MemEvent::VtableAlloc, DynView, VtableView
├── quickstart.md                    # Phase 1: dev workflow + manual QA procedure with UX checkpoint
├── contracts/
│   └── m07-7-protocol-delta.md      # Phase 1: 11th closed-enum invocation
└── checklists/
    └── requirements.md              # From /speckit-specify (16/16 PASS)
```

### Source Code (repository root) — files M07.7 touches

```text
src/
├── parse/
│   ├── token.rs                # MODIFIED — add `TokenKind::Dyn` for the `dyn` contextual keyword + `TokenKind::As` (verify if absent; needed for `as &dyn Show` cast). `for` and `trait` already exist from M07.6.
│   ├── lexer.rs                # MODIFIED — extend KEYWORDS with `"dyn"` → `TokenKind::Dyn`. Verify `"as"` — if absent, add `"as"` → `TokenKind::As`.
│   ├── ast.rs                  # MODIFIED — add `Type::DynTrait { trait_name: String, span: Span }` AST shape (the `&` and `&mut` wrap is handled by `Type::Ref { inner: Type::DynTrait, .. }` — same M06+ ref pattern). Add `Expr::Cast { inner: Box<Expr>, target_ty: Type, span: Span }` for `&p as &dyn Show` (the `as` cast expression). The `Box<dyn Show>` form parses via existing `Type::Generic` with `Type::DynTrait` as the arg.
│   └── parser.rs               # MODIFIED — `parse_type` extension: when seeing `Amp`/`AmpMut` followed by `Dyn` keyword followed by an ident, parse `Type::Ref { inner: Type::DynTrait { trait_name, span }, mutable, span }`. `parse_expr` postfix loop: after any expression, if `As` token follows, parse `Type`, build `Expr::Cast { inner, target_ty, span }`. `parse_type` ALSO handles bare `dyn TraitName` (when inside `Box<dyn Show>` etc.) — bare `dyn` produces `Type::DynTrait` without the Ref wrap (the `Box<T>` machinery wraps it).
├── resolve.rs                  # MODIFIED — `resolve_expr` adds an `Expr::Cast { inner, .. }` arm recursing on `inner` only. `Type::DynTrait` doesn't resolve trait names (typeck does).
├── typeck.rs                   # MODIFIED — **major surface**. Add `Ty::DynRef { trait_name: String, mutable: bool }` variant. Update `Ty::name()` to return `"&dyn Show"` / `"&mut dyn Show"`. Update `Ty::is_copy()` — `false` for `Ty::DynRef { mutable: true, .. }` (matches Rust's `&mut` non-Copy rule); `true` for `Ty::DynRef { mutable: false, .. }` (shared refs are Copy, including dyn refs). Add `VtableRegistry { vtables: IndexMap<(String, String), VtableAddr>, next_addr: u32 }`. Add `lookup_or_intern_vtable(&mut self, trait_name, type_name)` helper. Extend `ty_from_ast_resolving_structs` to handle `Type::DynTrait` → `Ty::DynRef { .. }` when wrapped in `Type::Ref`; handle bare `Type::DynTrait` (inside Box) by lowering to a placeholder Ty (or extending `Ty::Box(Box<Ty>)` to accept a DynTrait-shaped inner — see R-007). Extend `typecheck_expr` with an `Expr::Cast` arm: typecheck `inner`, verify the cast is valid (`&T` → `&dyn Trait` where `T: Trait`), produce the substituted Ty. Extend `typecheck_call`: when a fn param is `Ty::DynRef`, the corresponding arg accepts both an explicit `&p as &dyn Show` AND an implicit `&p` coercion (auto-coerce shared `&T` to `&dyn Trait` when `T: Trait`). Extend `typecheck_method_call` with a fourth layer (after the M07.6 three-layer): when receiver is `Ty::DynRef { trait_name, .. }`, look up the trait's methods (required + default) via existing `TraitRegistry`; the dispatch is dynamic at eval time but typecheck statically. Reject inherent-method calls through dyn.
├── event.rs                    # MODIFIED — add `Value::DynRef { borrow_id, target: Pointee, vtable: VtableAddr, mutable, trait_name }` variant. Add `VtableAddr(u32)` newtype. Add `MemEvent::VtableAlloc { addr, trait_name, type_name, methods: Vec<String>, span }`. The `Ty` enum gets `DynRef { trait_name, mutable }` (mirrors typeck-side `Ty`).
└── eval.rs                     # MODIFIED — extend Evaluator with: `vtable_addrs: HashMap<(String, String), VtableAddr>` (content-dedup interning of vtables — analog of static_region's by_content map). Add `intern_vtable(&mut self, trait_name, type_name) → VtableAddr` (emits `VtableAlloc` lazily on first use). `Expr::Cast` arm in `eval_expr`: for `&p as &dyn Show`, eval inner to get `Value::Ref { target, .. }`, intern the vtable for `(Show, type_of(target))`, construct `Value::DynRef { borrow_id, target, vtable, mutable, trait_name }`. Implicit coercion at fn-arg sites: similar handling at the call site (auto-cast `Value::Ref` → `Value::DynRef` when the param type is `Ty::DynRef`). Extend `eval_method_call` with `Value::DynRef` receiver case: resolve the concrete type from the target (look up the slot/heap's Value::Struct), dispatch via `trait_impl_bodies` (M07.6) with `<ConcreteType as Trait>::method` mangled name.

src/ui.rs                       # MODIFIED — **meaty UI surface**. Add `pub struct VtableView { addr: u32, trait_name: String, type_name: String, methods: Vec<(String, String)> /* (name, target_label) */ }`. Add a `vtables: Vec<VtableView>` field to `StateSnapshot`. Extend `apply_event` with `MemEvent::VtableAlloc` arm: push a VtableView entry to `world.vtables`. Add `pub struct DynView { data_label: String, vtable_label: String, vtable_addr: u32 }`. Extend `SlotRowView` with `dyn_view: Option<DynView>` (mutually exclusive with `value` / `inline_cells` / `struct_view`). Extend `apply_event`'s SlotWrite arm for `Value::DynRef`: build DynView (data_label = looked-up slot/heap binding name, vtable_label = `<Type as Trait>` form). Mangled fn name for trait-object dispatch reuses M07.6's `<Point as Show>::show` format via call_decl's display_name.

tests/
├── m01.rs / m02.rs / m03.rs        # Should stay byte-identical (no existing sample constructs trait objects).
└── samples/
    ├── (existing)                  # Unchanged.
    └── m07_7_*.rs                  # NEW (4 files): m07_7_dyn_basic, m07_7_dyn_param, m07_7_box_dyn, m07_7_static_vs_dyn.

web/
├── samples/                    # MODIFIED — add 4 m07_7_*.rs mirrors.
├── index.html                  # MODIFIED — dropdown grows 4 entries. Add a new `<section id="vtables">` panel between the stacks and the static-memory regions (or wherever pedagogically appropriate — plan-phase decides positioning).
├── index.js                    # MODIFIED — `renderVtables(state.vtables)` populates the new VTABLES panel (one box per vtable, listing methods). `renderStacks` extends to render `dyn_view` in the slot's value area when present (fat-pointer with `data: → label` and `vtable: → label`). `renderArrows` extends with TWO-STEP dispatch arrows for trait-object method calls — visible during the call's frame-entry step, fading on subsequent steps (similar to M07.2's transient BytesCopy arrow).
├── style.css                   # MODIFIED — `.vtable-panel` (the VTABLES section), `.vtable-box` (one per vtable), `.vtable-method` (per-method row inside a box). `.dyn-fat-pointer` (the two-cell slot rendering). `.arrow-vtable-dispatch` (the two-step dispatch arrow class — distinct style; recommendation: dashed orange or muted purple, plan-phase decides at UX checkpoint).
└── Trunk.toml                  # Unchanged.

# M03's contract amended for the 11th closed-enum invocation:
specs/004-m03-event-eval/contracts/m03-api.md   # MODIFIED — note M07.7 as the 11th invocation. Adds `Type::DynTrait` (AST), `Ty::DynRef`, `Value::DynRef`, `VtableAddr`, `MemEvent::VtableAlloc`. First new MemEvent variant since M07.2. Pure additive.
```

**Structure Decision**: substantially XL surface. Meaty UI piece (VTABLES panel + fat-pointer rendering + two-step dispatch arrows) means a UX checkpoint after the first cut, mirroring M07.4's struct-view workflow. Typeck + eval extensions reuse M07.6's `TraitRegistry` + `TraitImplRegistry` foundations — no new dispatch logic, just routing through the dyn fat pointer.

## Complexity Tracking

> No constitutional violations. Table omitted.

### Notable non-trivial complexity

- **Fat-pointer Value shape**: `Value::DynRef` is the SECOND fat-pointer Value in the project (`Value::Slice` from M07.1 is the first — its second field is `len`). M07.7's second field is `vtable: VtableAddr`. The serialization shape grows but the pattern is established.
- **Vtable interning (analog of M07.2 static-memory)**: each `(trait, type)` pair gets exactly one vtable address; multiple `&dyn Show` borrows of the same Point all share the same vtable. Implementation: `HashMap<(String, String), VtableAddr>` on Evaluator; lazy emission of `VtableAlloc` on first use per pair.
- **Implicit coercion at fn-arg sites**: `fn print(x: &dyn Show); print(&p);` should work without explicit `as`. Implementation: in `typecheck_call`'s arg-checking loop, when the param type is `Ty::DynRef { trait_name, .. }` AND the arg type is `Ty::Ref { Ty::Struct(_), .. }`, treat as auto-coercion (verify the inner struct impls the trait via TraitImplRegistry). Same for `eval_call` — convert the Value::Ref to Value::DynRef at the call site before binding to the param slot.
- **Explicit `as` coercion**: `&p as &dyn Show` — new `Expr::Cast { inner, target_ty }` AST node. Typeck verifies the cast's validity and returns the target type. Eval performs the actual Value::Ref → Value::DynRef construction (interning the vtable).
- **`Box<dyn Trait>` support**: the existing M07 `Box<T>` machinery already wraps any T. M07.7 makes `Box::new(p)` produce a `Value::Box { addr }` whose underlying heap contents include both the Point's data AND a vtable pointer — actually no, `Box<dyn Show>` IS a fat pointer (heap_addr + vtable_addr); the Box itself is unsized on the stack? Actually in Rust, `Box<dyn Show>` IS a fat pointer (16 bytes — heap_data_ptr + vtable_ptr). So Box of dyn = fat-pointer Value too. Plan-phase decision: extend `Value::Box` with optional vtable field OR introduce `Value::BoxDyn { addr, vtable, trait_name }`. Recommendation: separate `Value::BoxDyn` variant to keep `Value::Box` shape unchanged for the regular case.
- **VTABLES panel UI**: a new panel alongside STATIC MEMORY. CSS layout needs adjusting; the existing 3-panel layout (stacks/heap/static-memory) becomes 4-panel. Plan-phase decides panel order at the UX checkpoint.
- **Two-step dispatch arrows**: at a call step, draw TWO arrows: (a) data → receiver location (existing borrow-arrow path), (b) vtable_ptr → vtable box → method body. The second is unique — needs a fresh arrow class (`.arrow-vtable-dispatch`) and a fresh rendering helper. Visually distinct from existing arrows (which are solid color); recommendation: dashed orange or muted purple to convey "indirection" without competing with the M06-7 color palette.
- **Method-name lookup at vtable-dispatch time**: the vtable doesn't store method names explicitly in the runtime model — it stores ordered pointers to FnDecls. M07.7 simplifies by keying lookup on method name + trait + type at eval time (using `trait_impl_bodies` + `trait_default_bodies` from M07.6). The vtable is conceptually `Vec<(method_name, FnDecl_ref)>` for visualization purposes; runtime dispatch goes through the lookup map.
- **`Value::Ref` to `Value::DynRef` coercion at fn-arg site**: the simplest implementation re-evaluates the arg expression when the param type is dyn — but that's wasteful. Better: at call_decl, when binding a param whose type is `Ty::DynRef`, take the arg Value (which is `Value::Ref`) and construct `Value::DynRef` inline before SlotWrite. Plan-phase confirms.
- **No new Pointee variants**: vtables don't ride on Pointee. They're a separate addressing namespace via `VtableAddr` on `Value::DynRef`. Keeps the Pointee enum tight (Slot/Heap/Static — three places where data can live).
- **UX checkpoint expected**: after the first cut of VTABLES panel + fat-pointer rendering + dispatch arrows, halt for visual review. Mirrors M07.4's workflow (struct view iteration).
- **Bundle growth ≤ +25%**: estimated +60-100 KB from AST/Value/MemEvent additions + new VTABLES panel + fat-pointer rendering + dispatch-arrow CSS/JS. Verify post-merge.
