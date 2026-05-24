# Quickstart — M07.7 development + verification

Audience: maintainer + contributors working on M07.7 or extending it.

## Run the page

```bash
cd web && trunk serve --open
```

After M07.7 ships, the dropdown gains 4 entries: `Dyn basic`, `Dyn parameter`, `Box<dyn>`, `Static vs dyn`. The layout grows a new **VTABLES panel** between HEAP and STATIC MEMORY (or wherever positioned at the UX checkpoint).

## Run all tests

```bash
cargo test                            # full suite

cargo test --lib pipeline::tests::run_pipeline_dyn_basic           # &dyn cast + dispatch
cargo test --lib pipeline::tests::run_pipeline_dyn_param           # &dyn parameter
cargo test --lib pipeline::tests::run_pipeline_dyn_param_two_types # same `print` called with Point + Q
cargo test --lib pipeline::tests::run_pipeline_box_dyn             # Box<dyn Trait>
cargo test --lib pipeline::tests::run_pipeline_dyn_default_method  # default-method dispatch through dyn
cargo test --lib pipeline::tests::run_pipeline_dyn_vtable_interned # one VtableAlloc per (trait, type) pair
cargo test --lib pipeline::tests::run_pipeline_dyn_coercion_error  # &i32 to &dyn Show rejected
cargo test --lib pipeline::tests::run_pipeline_dyn_inherent_rejected   # inherent method via dyn rejected
cargo test --lib pipeline::tests::run_pipeline_static_vs_dyn       # paired-comparison sample
```

M01/M02/M03 should stay byte-identical (no existing sample constructs trait objects).

## Manual QA procedure (with UX checkpoint)

~10 minutes. UX checkpoint after step 2.

1. **Page loads** with the default sample. No console errors. Existing M01–M07.6 samples render unchanged. Verify the new VTABLES panel is present but empty (no trait objects in default sample).

2. **🎨 UX CHECKPOINT — US1 first cut**:
   - Select `Dyn basic (M07.7)`. Editor shows:
     ```rust
     trait Show { fn show(&self) -> i32; }
     impl Show for Point { fn show(&self) -> i32 { self.x } }
     fn main() {
         let p = Point { x: 1, y: 2 };
         let d: &dyn Show = &p;
         let s = d.show();
     }
     ```
   - Step past `let d: &dyn Show = &p`:
     - **d's slot** renders as a fat-pointer with TWO labeled cells: `data: → p` and `vtable: → <Point as Show>`.
     - **VTABLES panel** shows a new box: `<Point as Show>` with one method row: `show → <Point as Show>::show`.
   - Step past `d.show()`:
     - **Two dispatch arrows** light up: data → p, vtable_ptr → vtable box.
     - New frame `<Point as Show>::show` opens.
     - `s = 1_i32`.
   - **PAUSE.** Present the visualization. Discuss tweaks: arrow color, panel positioning, fat-pointer cell layout, vtable-box typography. Iterate.

3. **US2 — &dyn parameter**:
   - Select `Dyn parameter (M07.7)`. Editor shows `fn print(x: &dyn Show) { x.show() } let r = print(&p);`.
   - Step past `print(&p)`: ONE `print` frame opens (no monomorphization). Inside, the body's `x.show()` dispatches via the vtable; nested `<Point as Show>::show` frame.
   - (Optional, for two-type test): if the sample includes a second call `print(&q)` for a Q that impls Show, observe both calls go through the SAME `print` frame (no new mangled frame).

4. **US3 — Box<dyn Trait>**:
   - Select `Box<dyn> (M07.7)`. Editor shows `let b: Box<dyn Show> = Box::new(p); let s = b.show();`.
   - Step past `Box::new(p)`: heap allocation visible; b's slot renders as fat pointer with data = heap addr label, vtable label.
   - Step past `b.show()`: dispatch arrows visible (data → heap, vtable → vtable box → method).
   - At scope exit: Box dropped; heap freed (existing M07 Drop machinery); vtable persists.

5. **US4 — Static vs dynamic (the headline contrast)**:
   - Select `Static vs dyn (M07.7)`. Editor shows two paired calls: one to `fn s<T: Show>(x: T)`, one to `fn d(x: &dyn Show)`.
   - Step through both calls. Observe:
     - Static call: outer frame `s::<Point>` (M07.5 monomorphization); inner `<Point as Show>::show` opens via direct dispatch (no vtable arrow).
     - Dynamic call: outer frame `d` (one-frame); inner `<Point as Show>::show` opens via vtable dispatch (two-step arrow visible).
   - The contrast is the ship-defining learning moment.

6. **Error UX** — live editing:
   - Try `let d: &dyn Show = &5;` → typeck error "the type `i32` cannot be coerced to `&dyn Show` because it does not implement `Show`".
   - Try calling an inherent method through dyn (`impl Point { fn extra(&self) ... }` and then `d.extra()` on a `d: &dyn Show`) → typeck error "method `extra` is not in trait `Show`".

7. **No regressions**:
   - Cycle through M01–M07.6 samples. Each renders correctly. VTABLES panel remains empty for non-trait-object programs.

## Developer notes

### Why the fat-pointer rendering?

A learner sees a regular `&T` as a single 8-byte pointer. M07.7's `&dyn Trait` is 16 bytes — two pointers side by side. Rendering it as two labeled cells in the slot's value area makes the doubled size tangible. Without this, dynamic dispatch is "method call that works somehow"; with it, the data + vtable split becomes intuitive.

### Why a separate VTABLES panel?

Vtables aren't on the stack (they're not local data) and aren't on the heap (the program doesn't allocate them — the compiler emits them). They live in a separate read-only data segment in real Rust binaries; pedagogically we mirror this by putting them in their own panel (analog of M07.2's STATIC MEMORY).

One vtable per `(type, trait)` pair, content-deduplicated — multiple `&dyn Show` borrows of Point all point at the same vtable. This matches Rust's actual linker behavior.

### Why two-step dispatch arrows?

At the call site, the runtime indirection has two steps:
1. The value's vtable_ptr field points at the vtable box (in the VTABLES panel).
2. The vtable's method slot points at the resolved method's frame (when it opens).

Rendering both arrows simultaneously at the call step makes the indirection cost tangible. Without it, the method call would look like a direct call (same as inherent or static-dispatch trait methods).

### Why same `<TypeName as TraitName>::method` format as M07.6?

The dispatch PATH differs (static vs dynamic), but the dispatch DESTINATION is the same — the impl's body for that concrete type. Using the same UFCS-style mangled name for both lets the learner see that "the destination is the same; the route is different". The contrast is visible in the OUTER frame name (`s::<Point>` for static, `print` for dynamic), not the inner one.

### Why no `Pointee::Vtable` variant?

Vtables aren't borrow targets in the M07.4-7 sense. You can't take `&vtable` of one; you can't slice into one. They're a function-pointer table, not a memory region you'd hold a borrow into. Keeping them on a dedicated `vtable: VtableAddr` field on `Value::DynRef` / `Value::BoxDyn` keeps the `Pointee` enum tight.

### Implicit vs explicit coercion

```rust
let d: &dyn Show = &p;            // implicit — Rust auto-coerces &T to &dyn Trait
let d: &dyn Show = &p as &dyn Show;  // explicit
fn print(x: &dyn Show) { ... } print(&p);  // implicit at fn-arg site
```

All three should work. Typeck handles the coercion at type-mismatch sites (let annotation, fn arg, return type). Eval performs the actual Value::Ref → Value::DynRef construction inline at the coercion point.

### Vtable interning timing

Lazy. The first `&dyn Show` value targeting a Point construction triggers `intern_vtable("Show", "Point")` → emits `MemEvent::VtableAlloc` ONCE → subsequent constructions return the cached addr without re-emitting. Matches M07.2's static-memory interning pattern.

## When extending in M08

M08 (threads) adds `Arc<T>`, `Mutex<T>`. Auto-traits (`Send`, `Sync`) — which gate thread-safe types — are trait-like concepts that benefit from M07.7's trait-object machinery (if M08 chooses to visualize `Box<dyn Send>` etc.). M07.7's `TraitRegistry` + dispatch foundations are the base; M08 layers auto-trait inference + thread events.

## What this milestone does NOT add

- **Multi-trait objects** (`&dyn A + B`) — deferred.
- **Upcasting** (`&dyn Child` → `&dyn Parent`) — requires supertraits.
- **`?Sized`** and custom DSTs — only `dyn Trait` behind borrow/Box.
- **`impl Trait`** sugar in argument/return position — deferred.
- **`Vec<Box<dyn Trait>>`** heterogeneous collection — explicit out-of-scope.
- **`fn` pointers** as values — unrelated.
- **Auto-traits** (`Send`, `Sync`) — deferred to M08.
- **Trait-object methods returning Self / generic methods** — M07.6 already restricts; M07.7 inherits.
