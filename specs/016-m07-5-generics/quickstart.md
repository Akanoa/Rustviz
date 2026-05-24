# Quickstart — M07.5 development + verification

Audience: maintainer + contributors working on M07.5 or extending it.

## Run the page

```bash
cd web && trunk serve --open
```

After M07.5 ships, the dropdown gains 3 entries: `Generic id fn`, `Generic struct`, `Turbofish`. Selecting `Generic id fn` shows the headline pedagogy — two distinct frames `id::<i32>` and `id::<bool>` for the same source fn called twice.

## Run all tests

```bash
cargo test                            # full suite

cargo test --lib pipeline::tests::run_pipeline_generic_id_fn         # two distinct frames
cargo test --lib pipeline::tests::run_pipeline_generic_struct        # Wrapper<i32> rendering
cargo test --lib pipeline::tests::run_pipeline_turbofish             # explicit type arg
cargo test --lib pipeline::tests::run_pipeline_generic_inference_mismatch  # cannot infer T error
cargo test --lib pipeline::tests::run_pipeline_turbofish_type_mismatch     # expected/found error
cargo test --lib pipeline::tests::run_pipeline_generic_multi_param_rejected
cargo test --lib pipeline::tests::run_pipeline_generic_bound_rejected
cargo test --lib pipeline::tests::run_pipeline_generic_nested_call_rejected
```

M01 may re-baseline once for the `type_params` field on `FnDecl`/`StructDecl` (depends on serializer format vs JSON-skip behavior). M02 and M03 snapshots should stay byte-identical (they serialize resolution + types + events, not raw AST).

## Manual QA procedure

~6 minutes. Walk in this order:

1. **Page loads** with the default sample. No console errors. Existing M01–M07.4 samples render unchanged.

2. **US1 — Generic id fn (the headline)**:
   - Select `Generic id fn (M07.5)`. Editor shows:
     ```rust
     fn id<T>(x: T) -> T { x }
     fn main() {
         let a = id(5);
         let b = id(true);
     }
     ```
   - Step through. Observe:
     - First call: frame card slides in labeled **`id::<i32>`**; param row `x : i32 = 5_i32`.
     - `ReturnValue → 5_i32` flashes; frame grays out.
     - Back in `main`: `a : i32 = 5_i32`.
     - Second call: NEW frame card slides in labeled **`id::<bool>`** (NOT a reuse of the first frame); param row `x : bool = true`.
     - `ReturnValue → true` flashes; frame grays out.
     - Back in `main`: `b : bool = true`.
   - **The two frames are clearly distinct** — different fn names in the header, different param types. That's the monomorphization pedagogy.

3. **US2 — Generic struct**:
   - Select `Generic struct (M07.5)`. Editor shows `struct Wrapper<T> { v: T } let w = Wrapper { v: 5 }; let a = w.v;`.
   - Step past `let w = ...`. Observe `w : Wrapper<i32>` in the slot's type column (NOT `Wrapper<T>`). The struct view renders one field row `v: i32 = 5_i32`.
   - Step past `let a = w.v;`. Observe `a : i32 = 5_i32`.

4. **US3 — Turbofish**:
   - Select `Turbofish (M07.5)`. Editor shows `fn id<T>(x: T) -> T { x } let v = id::<bool>(false);`.
   - Step past `let v = ...`. Observe frame labeled `id::<bool>`; param `x : bool = false`; return `false`; `v : bool = false`.

5. **Error UX** — live editing:
   - In the id-fn sample, change to `let bad = pair(5, true);` after adding `fn pair<T>(a: T, b: T) -> T { a }`. Observe typeck error "cannot infer T from conflicting args: i32 vs bool".
   - Try `let v = id::<bool>(5);`. Observe typeck error "argument 1: expected `bool`, found `i32`".
   - Try `fn pair<T, U>(a: T, b: U) -> T { a }`. Observe typeck error citing the M07.5 single-param restriction.
   - Try `fn id<T: Foo>(x: T) -> T { x }`. Observe typeck error pointing at M07.6.

6. **No regressions**:
   - Cycle through M01–M07.4 samples. Each renders correctly. M07.4 struct view, field borrows, method dispatch, associated functions all unchanged.

## Developer notes

### Why does eval not see `Ty::Param`?

Typeck applies substitution BEFORE recording any concrete type that eval consults:
- `binding_types[param_id]` stores the SUBSTITUTED type (e.g. `Ty::Int(I32)` for `x: T` in `id::<i32>(5)`).
- `expr_types[span]` stores the substituted type for every value-producing expr in the generic body.
- `call_substs[call_span]` is a typeck-side side table that eval reads ONCE to build the mangled fn name; it's not consulted again during the body walk.

So `Ty::Param(_)` only ever appears in the typeck's `FnSig.params`/`ret` (the pre-substitution signature) and the typeck-internal substitution stack. Eval never sees it. If it leaks, ui's `render_ty(Ty::Param(name))` renders `<T>` as a fallback that makes the leak visually obvious during development.

### Why is `Ty::Param.is_copy()` false?

Without bounds (M07.6's payoff), we can't know whether `T` is Copy. So `Ty::Param(_).is_copy() == false`. In M07.5 this is fine because at the call site, the substituted concrete type (e.g. `Ty::Int(I32)` which IS Copy) replaces the param before any is_copy check applies. The "false" answer only matters inside the generic body's typecheck — and since M07.5 doesn't permit operations that REQUIRE Copy on `T` (move-only ops like `x` return are fine; field access on T would require knowing T's shape), this never blocks valid M07.5 programs.

### Mangled fn name format

`source_name::<sub_ty_name>` for single-T; multi-T extension (when supported) is `source_name::<ty1, ty2>`. Built at eval time from typeck's `call_substs` lookup; consumed as `display_name` parameter to `call_decl` (already added in M07.4 for method dispatch).

### Each call entry is a fresh frame

No event-level memoization. `id(5); id(5);` produces TWO `FrameEnter { fn_name: "id::<i32>" }` events with distinct `frame_id`s. Matches the cost model: each call site opens its own frame; the monomorphization just shares the source body. This is consistent with non-generic fns — `let r1 = add(2, 3); let r2 = add(4, 5);` also produces two `FrameEnter { fn_name: "add" }` events.

### Why no UI changes for the headline?

The two distinct frame labels (`id::<i32>` and `id::<bool>`) come straight from `FrameEnter.fn_name` which the existing frame-card renderer already displays. M07.4's renderer treats `fn_name` as opaque text — it doesn't care that the new strings have `::<>` brackets. No JS change needed.

The generic-struct type label (`Wrapper<i32>`) comes from `Ty::name()` rendering — extended in M07.5 to include `type_args` when non-empty. The slot's `ty` field already shows `Ty::name()` output verbatim; no JS / CSS change needed.

## When extending in M07.6

M07.6 (traits) layers on top:
- Trait declarations + impls → register in a new `TraitRegistry` + `TraitImplRegistry`.
- Trait bounds on generics (`fn print<T: Show>(x: T)`) → typeck records the bounds on the generic; method-call dispatch in the body searches the trait impls for the substituted type. The substitution machinery from M07.5 stays unchanged; M07.6 adds the bound-checking + trait-dispatch fall-through.

## What this milestone does NOT add

- **Trait bounds** — M07.6.
- **Multiple type parameters per fn/struct** — deferred.
- **Const generics / lifetime generics / where clauses / default type params** — deferred.
- **Generic methods** (method-level type params) — deferred.
- **Specific-instantiation impls** (`impl Wrapper<i32>` separately from `impl<T> Wrapper<T>`) — deferred.
- **Nested generic calls** — deferred.
- **Inference from type annotations** (`let x: i32 = id();`) — deferred. Workaround: turbofish (`let x = id::<i32>();`).
