# Implementation Plan: M07.5 — Generics (`fn foo<T>(...)`, `struct Wrapper<T>`, monomorphization-visible frames)

**Branch**: `016-m07-5-generics` | **Date**: 2026-05-24 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/016-m07-5-generics/spec.md`

## Summary

Introduce Rust's type-parameter surface end to end: `fn id<T>(x: T) -> T { x }`, `struct Wrapper<T> { v: T }`, `let v = id::<bool>(false);` (turbofish), `let w = Wrapper { v: 5 };` (inferred). **Headline pedagogy**: monomorphization is visible — each concrete substitution produces a distinct `FrameEnter.fn_name` (`id::<i32>` vs `id::<bool>`). M07.5 is the foundation that M07.6 (traits) builds on; without M07.5, `fn print<T: Show>(x: T)` is unreachable.

**9th invocation of the closed-enum-with-revisions rule**: additive `Ty::Param(String)` for substitution, plus additive `type_args: Vec<Ty>` field on `Ty::Struct` (serde-default-empty so existing M03+ snapshots stay byte-identical). No new MemEvent variants, no new Pointee variants, no new Value variants. AST gains `type_params: Vec<TypeParam>` on `FnDecl` and `StructDecl` plus type-arg fields on path-shaped expressions for turbofish / generic-struct-lit syntax.

Authority chain: `MILESTONES.md` › M07.5 → `spec.md` (this feature) → this plan.

## Technical Context

**Language/Version**: Rust 2024 edition (same toolchain as M01–M07.4). No new toolchain requirements.
**Primary Dependencies**: existing `indexmap`, `serde`, `serde_json`, `wasm-bindgen`, `js-sys`, `console_error_panic_hook`. **No new Rust deps**. **No JS deps changes**.
**Storage**: in-memory; no new files. Substitution state lives in typeck's `Typechecker` (a `subst: Vec<HashMap<String, Ty>>` stack mirroring the body-typecheck depth). M01/M02/M03 snapshot tests should stay byte-identical — additive AST fields use `#[serde(default, skip_serializing_if = "Vec::is_empty")]` to keep existing serialized AST unchanged. M01's `parses_full_l1.snap` may re-baseline once if the snapshot picks up the new field even when empty (depends on serde's Debug format vs JSON format).
**Testing**: existing `cargo test --test m01 / m02 / m03` should pass byte-identical (M01 candidate re-baseline; M02/M03 should not). New `cargo test --lib pipeline::tests` covering: generic id fn with two substitutions, generic struct, turbofish, mismatched-arg-inference error, turbofish type mismatch, multi-type-param rejection, bound-on-generic rejection, generic-call-inside-generic-fn rejection. **≥ 8 new tests**. Manual M07.5 QA per the SC procedure.
**Target Platform**: same as M01–M07.4 (host + `wasm32-unknown-unknown`).
**Project Type**: Rust library + companion UI. Touches ~5 source modules (parse/{ast,parser}, typeck, eval, ui). Sized XL but smaller than M07.4 — no new UI rendering surface.
**Performance Goals**: same pipeline latency budget. Substitution is O(N) per call site (N = number of generic-typed params); inference is single-pass.
**Constraints**: M01/M02/M03 byte-identical (or M01 re-baseline once for `type_params` field); WASM bundle ≤ +20% vs M07.4 baseline (310,880 B → ≤ ~373 KB raw) per SC-011; zero warnings under `-D warnings` (SC-012); existing M01–M07.4 features preserved.
**Scale/Scope**: ~5 source modules + 3 sample pairs + ≥ 8 new unit tests. **Estimated ~800-1100 LOC net change**. Sizing: **XL** per the rubric — smaller than M07.4 (no new UI rendering surface beyond type-label substitution).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is still the unfilled speckit template. Same vacuous PASS as features 001–015.

**Post-design re-check**: still vacuous, still PASS.

## Project Structure

### Documentation (this feature)

```text
specs/016-m07-5-generics/
├── plan.md                          # This file
├── spec.md                          # Feature spec
├── research.md                      # Phase 0: 14 design decisions
├── data-model.md                    # Phase 1: TypeParam, Ty::Param, AST extensions
├── quickstart.md                    # Phase 1: dev workflow + manual QA procedure
├── contracts/
│   └── m07-5-protocol-delta.md      # Phase 1: 9th closed-enum invocation
└── checklists/
    └── requirements.md              # From /speckit-specify (16/16 PASS)
```

### Source Code (repository root) — files M07.5 touches

```text
src/
├── parse/
│   ├── token.rs                # Unchanged — `Lt`, `Gt`, `Comma`, `ColonColon` all exist.
│   ├── lexer.rs                # Unchanged — no new keywords.
│   ├── ast.rs                  # MODIFIED — extend `FnDecl` with `type_params: Vec<TypeParam>`; extend `StructDecl` with `type_params: Vec<TypeParam>`. Add `TypeParam { name: String, span: Span }`. Extend `Type::Path` with `type_args: Vec<Type>` (for `Wrapper<i32>` annotations). Extend `Expr::Path` with `type_args: Vec<Type>` (for turbofish `id::<bool>` and `Wrapper::<i32>`). Extend `Expr::StructLit` with `type_args: Vec<Type>` (for the struct-literal turbofish path). All new fields default-empty via `Vec::new()` + `#[serde(default, skip_serializing_if = "Vec::is_empty")]`.
│   └── parser.rs               # MODIFIED — `parse_fn_decl` accepts optional `< T >` after the fn name. `parse_struct_decl` same after the struct name. `parse_type` extended: after a path identifier, if `<` follows (and we're at a type-context position), parse comma-separated type-args until `>`. `parse_atom` extended: after a `Path` (multi-segment), if `::<` follows, parse type-args (note: lexer doesn't have a `::<` 3-char token — parser sees `ColonColon` then `Lt`). Struct-literal `Wrapper::<i32> { v: 5 }` is path-then-type-args-then-`{`. Multi-type-param decls (`<T, U>`) parse-accepted but typeck-rejected. Bound syntax (`<T: Foo>`) parse-accepted but typeck-rejected per the M07.5 scope rules.
├── resolve.rs                  # MODIFIED — `resolve_fn` pushes a type-param scope before walking the body so type-param idents are resolvable as types. The type-param scope just stores the names (no BindingIds — type params aren't value-level bindings). `resolve_expr` arms for the new `Expr::Path { type_args }` and `Expr::StructLit { type_args }` don't need recursion in M07.5 (type-args restricted to concrete primitives — no nested type-args).
├── typeck.rs                   # MODIFIED — **major surface**. Add `Ty::Param(String)` variant. Update `Ty::name()` to return the param name (`"T"`). Update `Ty::is_copy()` — for `Ty::Param(_)`, treat as `false` (no bounds in M07.5 means we can't assume Copy; substituted types at call sites are concrete and have their own is_copy answer). Extend `Ty::Struct` with `type_args: Vec<Ty>` field (empty for non-generic structs). `Ty::name()` for `Struct { name, type_args: [] }` returns `"Point"`; for `Struct { name, type_args: [Int(I32)] }` returns `"Wrapper<i32>"`. Extend `register_struct` to read `decl.type_params` into the schema (store as field-name-to-Ty alongside the existing fields). Extend `register_impl_fn`/`build_fn_sig` to read `decl.type_params` so the resulting `FnSig` knows which params are generic. Add `Typechecker.subst: Vec<HashMap<String, Ty>>` — the substitution stack, pushed per call site, popped on return. Add `apply_subst(&self, ty: &Ty) -> Ty` helper that recursively substitutes `Ty::Param(name)` → the looked-up concrete type (returns the param unchanged if outside any substitution context). Extend `typecheck_call` + `typecheck_method_call` + `typecheck_path_call`: when the resolved sig has generic params, infer substitution from args (single-arg case: `T = arg_ty`; multi-arg same-T: all must agree), apply substitution to params + ret, typecheck args against the substituted param types. Extend `typecheck_struct_lit` similarly for generic structs. Reject multi-type-param decls (`fn pair<T, U>`), bounds (`T: Foo`), const-generics, lifetime-generics, generic-call-inside-generic-fn at the appropriate sites with M07.5-specific error messages.
├── event.rs                    # MODIFIED — same `Ty` enum is shared between typeck and event; the new `Ty::Param(String)` variant + the `Ty::Struct.type_args` extension live here. No new `Value` variants, no new `MemEvent` variants, no new `Pointee` variants.
└── eval.rs                     # MODIFIED — `call_decl` already accepts a `display_name` (added in M07.4 for `Point::x` method-frame naming). M07.5 builds the mangled name at the call site: source name + `::<substituted_ty_name>` for each type-param. Need a way for eval to know the substitution per call site — simplest: typeck records the substituted Ty for each generic call's span in a NEW `TypeMap.call_substs: IndexMap<Span, Vec<(String, Ty)>>` side table. Eval looks up `call_substs[call_span]` to build the mangled name + pass into the substitution-aware body walk. `Ty::Param(_)` should be unreachable at eval time (typeck substituted before recording any expr_types entries eval consults); if it leaks, panic with a clear "M07.5 invariant: Ty::Param escaped typeck substitution at <span>".

src/ui.rs                       # MODIFIED — `render_ty` / `render_value` exhaustive matches need a `Ty::Param(_)` arm. Should be unreachable in practice (typeck substitutes before serializing), but the match arm needs to exist for compile-time exhaustiveness. Render as `format!("<{}>", name)` (e.g. `<T>`) as a defensive fallback so any leak surfaces visibly. SlotRowView's `ty` field already accepts arbitrary strings (`render_ty` output), so generic-struct labels like `"Wrapper<i32>"` "just work" once `Ty::name()` for `Struct { name, type_args }` renders them. No new SlotRowView fields, no JS changes.

tests/
├── m01.rs / m02.rs / m03.rs        # M01 may re-baseline once for the `type_params` field on FnDecl/StructDecl. M02 + M03 should stay byte-identical (they serialize resolution + types + events, not raw AST).
└── samples/
    ├── (existing)                  # Unchanged.
    └── m07_5_*.rs                  # NEW (3 files): m07_5_id_fn, m07_5_generic_struct, m07_5_turbofish.

web/
├── samples/                    # MODIFIED — add 3 m07_5_*.rs mirrors.
├── index.html                  # MODIFIED — dropdown grows 3 entries.
├── index.js                    # Unchanged — type-label substitution is rendered server-side via `Ty::name()`; the JS just displays whatever string `slot.ty` carries.
├── style.css                   # Unchanged — no new UI elements.
└── Trunk.toml                  # Unchanged.

# M03's contract amended for the 9th closed-enum invocation:
specs/004-m03-event-eval/contracts/m03-api.md   # MODIFIED — note M07.5 as the 9th invocation. Adds `Ty::Param(String)`; extends `Ty::Struct` with `type_args: Vec<Ty>` (empty for non-generic structs; serde-default-empty). Pure additive. No event-variant changes; no Pointee changes; no Value changes.
```

**Structure Decision**: smaller surface than M07.4. Substitution machinery is mostly typeck-side; mangled fn names flow through the existing `call_decl` `display_name` parameter (added in M07.4). The UI is unchanged except for the type-label rendering (which falls out of `Ty::Struct` gaining `type_args` and `Ty::name()` extending its output). No new sample-rendering CSS, no new JS handlers — the headline pedagogy (distinct frame names per substitution) is a frame-card label change, fully driven by the existing `FrameEnter.fn_name` rendering.

## Complexity Tracking

> No constitutional violations. Table omitted.

### Notable non-trivial complexity

- **Substitution stack + scope**: typeck needs to know "which type-param names are bound to which concrete types right now". The substitution lives per fn-body typecheck — push at body entry (built from the call site's inference / turbofish), pop at exit. The stack mirrors the call-typecheck depth. For M07.5 with no nested generic calls, the max depth is 1; but the abstraction is built correctly so a future milestone can lift the restriction.
- **Inference algorithm**: simple direct-match. At a call site, walk param types; for each `Ty::Param("T")`, the corresponding arg's type IS the substitution. If `T` appears multiple times (e.g. `fn pair<T>(a: T, b: T)`), all occurrences must agree. No HM unification, no constraint solving. Trade-off: rejects some Rust programs that full HM would accept (e.g. `fn id<T>() -> T` called from a type-annotated context — `let x: i32 = id();` — would fail in M07.5 because no arg gives T; the workaround is turbofish `id::<i32>()`).
- **Turbofish vs comparison ambiguity**: `id::<bool>(false)` works because `::<` is a distinguishing two-token sequence (existing `ColonColon` token followed by `Lt`). The parser sees `Path` → `ColonColon` → `Lt` and commits to turbofish. Plain `<` after an expression remains a less-than comparison. No grammar conflict.
- **Generic-struct literal `Wrapper::<i32> { v: 5 }`**: the turbofish prefix `Wrapper::<i32>` parses as `Expr::Path { segments: ["Wrapper"], type_args: [Type::Path { segments: ["i32"], type_args: [] }] }`. The following `{ v: 5 }` is the struct-literal body. Parser disambiguates by peeking what follows the closing `>`: `(` → call, `{` → struct lit (when not in cond-position per M07.4's restriction).
- **`Ty::Param` representability at event-stream level**: in principle, `Ty::Param` could leak into `SlotAlloc { ty: Ty::Param("T") }` if a generic-fn's body opens a slot for `x : T` before substitution is applied. Plan: typeck substitutes BEFORE any `SlotAlloc`-bound type is recorded — the typeck's `subst` stack is consulted at every `apply_subst()` call inside a generic-fn body. By the time eval reads `binding_types[param_id]`, the type is concrete. `Ty::Param` only appears in the GENERIC fn's signature (unreachable from eval) and in the typeck's `Ty::Struct.type_args` of GENERIC struct schemas (substituted before lookup). Defensive: ui's `render_ty(Ty::Param(name))` returns `"<{name}>"` as a fallback that would be visibly wrong if it ever leaked.
- **`Ty::Struct.type_args` field**: extending `Ty::Struct` is a SERDE-shape change (new field). The existing pattern from M07.4 — serde-default-empty — keeps the JSON wire byte-identical for non-generic structs. Existing M03 snapshots stay byte-identical.
- **Mangled name format `id::<i32>`**: just `format!("{}::<{}>", source_name, ty.name())` for the single-T case. Multi-T would be `format!("{}::<{}>", source_name, types.iter().map(|t| t.name()).collect::<Vec<_>>().join(", "))` — written this way from the start so M07.5+'s multi-T lift is just relaxing the parser rejection.
- **Bundle growth ≤ +20%**: estimated +30–50 KB from typeck substitution + name mangling. No new UI surface. Verify post-merge with `ls -la web/dist/.stage/*.wasm`. If miss: candidate cuts are (a) skip US3 turbofish (rare in practice, -10 KB), (b) skip generic structs (US2, -15 KB).
- **No new MemEvent variants**: existing `FrameEnter` carries the mangled fn name (`id::<i32>`); existing `SlotAlloc` / `SlotWrite` carry concrete (post-substitution) types. The cost model "each substitution opens a fresh frame" is a labeling change, not a protocol change.
- **`Ty::Struct` extension carries serde-impact risk**: this is the SECOND time we've added a field to an existing variant (first was `Value::Ref.field_path` in M07.4). Same pattern — `#[serde(default, skip_serializing_if = "Vec::is_empty")]` keeps existing snapshots byte-identical. Verify with `cargo insta test` before merging.
