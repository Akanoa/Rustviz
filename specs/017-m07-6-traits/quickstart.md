# Quickstart — M07.6 development + verification

Audience: maintainer + contributors working on M07.6 or extending it.

## Run the page

```bash
cd web && trunk serve --open
```

After M07.6 ships, the dropdown gains 4 entries: `Trait basic`, `Default method`, `Generic bound`, `Multi-bound`. Selecting `Generic bound` shows the headline pedagogy — a generic fn `fn print<T: Show>(x: T) { x.show(); }` whose body calls a method on `T` thanks to the bound.

## Run all tests

```bash
cargo test                            # full suite

cargo test --lib pipeline::tests::run_pipeline_trait_basic        # trait decl + impl + dispatch
cargo test --lib pipeline::tests::run_pipeline_default_method     # default method dispatch
cargo test --lib pipeline::tests::run_pipeline_generic_bound      # T: Show bound proves method
cargo test --lib pipeline::tests::run_pipeline_multi_bound        # T: Show + Counter
cargo test --lib pipeline::tests::run_pipeline_trait_missing_method        # impl missing required
cargo test --lib pipeline::tests::run_pipeline_trait_extra_method          # impl has extra method
cargo test --lib pipeline::tests::run_pipeline_trait_inherent_wins         # inherent > trait
cargo test --lib pipeline::tests::run_pipeline_trait_bound_unsatisfied     # T: Show, called with i32
cargo test --lib pipeline::tests::run_pipeline_trait_method_ambiguous      # T: A + B, both have name
cargo test --lib pipeline::tests::run_pipeline_trait_impl_for_builtin      # impl Show for i32
```

M03 should stay byte-identical (no event-shape changes). M01/M02 may re-baseline once for `TypeParam.bound` → `bounds` promotion.

## Manual QA procedure

~8 minutes. Walk in this order:

1. **Page loads** with the default sample. No console errors. Existing M01–M07.5 samples render unchanged.

2. **US1 — Trait basic (the foundation)**:
   - Select `Trait basic (M07.6)`. Editor shows:
     ```rust
     trait Show { fn show(&self) -> i32; }
     impl Show for Point { fn show(&self) -> i32 { self.x } }
     fn main() {
         let p = Point { x: 1, y: 2 };
         let s = p.show();
     }
     ```
   - Step through `let s = p.show()`:
     - New frame card opens labeled **`<Point as Show>::show`** (the `<as>` syntax distinguishes trait dispatch from inherent).
     - Self bound to `&p`.
     - Body runs: returns `1_i32`.
     - `s : i32 = 1_i32` in main.

3. **US2 — Default method**:
   - Select `Default method (M07.6)`. Editor shows:
     ```rust
     trait Counter {
         fn count(&self) -> i32;
         fn double(&self) -> i32 { self.count() * 2 }
     }
     impl Counter for Point { fn count(&self) -> i32 { self.x } }
     ...
     let v = p.double();
     ```
   - Step through `let v = p.double()`:
     - Outer frame opens for **`<Point as Counter>::double`** — executes the trait's default body.
     - The `self.count()` call inside opens a nested frame **`<Point as Counter>::count`** — executes the impl's override (since Point provides count).
     - Inner returns `p.x = 1_i32`; multiplication yields 2.
     - `v : i32 = 2_i32`.

4. **US3 — Generic bound (the headline)**:
   - Select `Generic bound (M07.6)`. Editor shows:
     ```rust
     trait Show { fn show(&self) -> i32; }
     impl Show for Point { fn show(&self) -> i32 { self.x } }
     fn print<T: Show>(x: T) -> i32 { x.show() }
     ...
     let r = print(p);
     ```
   - Step through `let r = print(p)`:
     - Outer frame opens for **`print::<Point>`** (M07.5 monomorphization, substituted Point).
     - Inside: `x.show()` dispatches via the bound. Nested frame for **`<Point as Show>::show`**.
     - Returns `p.x`; `r : i32 = 1_i32`.

5. **US4 — Multi-bound**:
   - Select `Multi-bound (M07.6)`. `fn show_n_count<T: Show + Counter>(x: T) -> i32 { x.show() + x.count() }` called with Point that impls both.
   - Step through; observe TWO nested frames `<Point as Show>::show` and `<Point as Counter>::count` inside the outer `show_n_count::<Point>`.

6. **Error UX** — live editing:
   - In trait_basic sample, delete the `impl Show for Point { ... }` block. Live-edit error: "no method `show` on type `Point`".
   - In generic_bound sample, change `let r = print(p);` to `let r = print(5);`. Error: "the trait bound `i32: Show` is not satisfied".
   - In an impl block missing a required method: error "missing implementation of trait method `<name>` for type `<Ty>`".
   - In an impl block with an extra method: error "method `<name>` is not on trait `<Trait>`".
   - Add a second impl block for the same `(trait, type)` pair: error "duplicate impl `Show for Point`".

7. **No regressions**:
   - Cycle through M01–M07.5 samples. Each renders correctly. M07.4 struct view, M07.5 monomorphization-distinct frames all unchanged.

## Developer notes

### Why the `<Point as Show>::show` frame name?

To distinguish trait dispatch from inherent dispatch in the trace. M07.4 introduced `Point::show` for inherent methods; M07.6 adds trait methods and would conflate them under the same `Point::show` label unless we use a different format. The `<as>` syntax is Rust's UFCS-style — familiar to learners who've encountered it elsewhere, and unambiguous about the dispatch path taken.

### Why does `print(5)` fail typeck when `print<T: Show>` is called?

The bound `T: Show` requires that whatever concrete type `T` resolves to must have an `impl Show for <T>`. When the inferred substitution is `T = i32`, typeck looks up `TraitImplRegistry[("Show", "i32")]`. If absent → error. Rust's standard "trait bound not satisfied" phrasing.

### Why does the default method body run when the impl overrides nothing?

Dispatch order: `TraitImplRegistry.impls[(trait, type)].overrides[method]` first (the impl's explicit body); fall through to `TraitRegistry.schemas[trait].default_methods[method]` (the trait's default). When the impl provides no override for a default method, the trait's body runs — with `self` bound to the receiver. Any `self.other()` calls inside re-dispatch through standard machinery, picking up impl overrides for those methods.

### Why inherent-wins-over-trait?

Established by M07.4: `Point::show` (inherent) wins over `<Point as Show>::show` (trait) if both exist with the same method name. Pedagogically clean — inherent methods are "the type's own behavior", traits are "extension via shared abstractions". When you write `p.show()` on a Point that has both, Rust picks the inherent (and the trait method is reachable only via UFCS, which is out of scope here).

### What if a generic-bound method call inside a generic body needs to know the concrete type?

It doesn't — typeck resolves the call against the BOUND (Show's schema), not the substituted concrete type. Eval substitutes T → Point at frame entry (M07.5 machinery); the body's `x.show()` dispatches through standard method-call logic using the substituted Ty. The result is a normal trait-method dispatch on the concrete type.

### Why no `&dyn Trait` (trait objects)?

Requires vtable + indirection machinery. M07.6's scope is static dispatch via generic bounds — the form `fn print<T: Show>(x: T)`. Trait objects (`fn print(x: &dyn Show)`) are a future milestone that would add vtable visualization (which would be a meaty UI piece on its own).

### Why no UFCS, even though the ambiguity error suggests it?

UFCS (`Show::show(&p)`) requires the parser to recognize a path expression in the same position as a method call. Out of scope for M07.6 to keep parser surface tight. The ambiguity error suggests UFCS because that's the Rust-standard escape hatch — the learner needs to know a path forward exists, even if they can't take it in M07.6.

## When extending in M08

M08 (threads) doesn't depend on M07.6 directly, but auto-traits (`Send`, `Sync`) — which gate thread-safe types — are a trait concept that M08 would build on. M07.6's `TraitRegistry` + bound-checking machinery is the foundation; M08 adds the special auto-trait inference.

## What this milestone does NOT add

- **Trait objects** (`&dyn Trait`) — deferred to a future dynamic-dispatch milestone.
- **Associated types**, **supertraits**, **blanket impls**, **derive macros**, **where clauses** — all deferred.
- **Generic trait methods** (method-level type-params on trait methods) — deferred.
- **`Self` return type** — deferred.
- **UFCS** — deferred. Method-call dot syntax only.
- **Multi-segment trait paths** (`mod::Show`) — deferred. Single-segment only.
- **Auto-traits** — deferred to M08 (threads).
