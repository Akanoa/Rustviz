# Quickstart — M07.4 development + verification

Audience: maintainer + contributors working on M07.4 or extending it.

## Run the page

```bash
cd web && trunk serve --open
```

After M07.4 ships, the dropdown gains 4 entries: `Struct basic`, `Field borrow`, `Method`, `Associated function`. The first three show **zero heap activity** for the entire trace — the heap panel stays empty. The associated-function sample also stays heap-free since `Point::new` just constructs and returns a struct.

## Run all tests

```bash
cargo test                            # full suite

cargo test --lib pipeline::tests::run_pipeline_struct_basic           # struct decl + literal + field access
cargo test --lib pipeline::tests::run_pipeline_struct_shorthand       # field-shorthand `Point { x, y }`
cargo test --lib pipeline::tests::run_pipeline_struct_missing_field   # typeck error
cargo test --lib pipeline::tests::run_pipeline_struct_extra_field     # typeck error
cargo test --lib pipeline::tests::run_pipeline_struct_wrong_type      # typeck error
cargo test --lib pipeline::tests::run_pipeline_field_borrow           # &p.x with field_path metadata
cargo test --lib pipeline::tests::run_pipeline_field_borrow_unknown   # typeck error "no field z"
cargo test --lib pipeline::tests::run_pipeline_method                 # p.method() dispatches
cargo test --lib pipeline::tests::run_pipeline_method_self_field      # self.x inside &self method
cargo test --lib pipeline::tests::run_pipeline_method_two_methods     # two methods in one impl
cargo test --lib pipeline::tests::run_pipeline_method_unknown         # typeck error "no method bogus"
cargo test --lib pipeline::tests::run_pipeline_assoc_fn               # Point::new(1, 2)
cargo test --lib pipeline::tests::run_pipeline_assoc_fn_mixed         # Vec::new + Point::new dispatch
cargo test --lib pipeline::tests::run_pipeline_struct_forward_ref     # impl Point BEFORE struct Point
cargo test --lib pipeline::tests::run_pipeline_struct_no_heap         # zero heap events
```

M01, M02 should stay byte-identical (no existing sample constructs structs). M03 snapshots should stay byte-identical (the `Value::Ref` extension uses serde-default-empty for `field_path`).

## Manual QA procedure (SC-008)

~8 minutes. Walk in this order:

1. **Page loads** with the default sample. No console errors. Existing M01–M07.3 samples render unchanged.

2. **US1 — Struct basic (the headline)**:
   - Select `Struct basic (M07.4)`. Editor shows:
     ```rust
     struct Point { x: i32, y: i32 }
     fn main() {
         let p = Point { x: 1, y: 2 };
         let a = p.x;
     }
     ```
   - Step through. At `let p = Point { x: 1, y: 2 }`:
     - `p` row appears in `main`'s frame with type `Point`.
     - **Struct view** in `p`'s value area: per research R-016 Proposal A, two labeled field rows (`x: i32` with 4 byte-cells + `= 1_i32`; `y: i32` with 4 byte-cells + `= 2_i32`).
     - **Heap panel stays empty** — no allocation event fires.
   - Step to `let a = p.x`. Observe `a: i32 = 1_i32`. `p` stays usable.

3. **US2 — Field borrow with per-field hover**:
   - Select `Field borrow (M07.4)`. Editor shows `let p = ...; let r = &p.x;`.
   - Step past `let r = &p.x`:
     - `r` row appears with type `&i32`.
     - **A blue field-borrow arrow** connects `r`'s slot to `p`'s slot.
     - **The arrow carries a `.x` annotation** at its midpoint.
   - Hover the arrow: **only the `x` row** in `p`'s struct view lights up yellow — NOT the `y` row. This is the structural payoff.

4. **US3 — Method dispatch**:
   - Select `Method (M07.4)`. Editor shows:
     ```rust
     struct Point { x: i32, y: i32 }
     impl Point { fn x(&self) -> i32 { self.x } }
     fn main() {
         let p = Point { x: 1, y: 2 };
         let v = p.x();
     }
     ```
   - Step through `let v = p.x()`:
     - A new stack frame card slides in for `Point::x`.
     - Inside the frame, `self : &Point` row appears, with a blue arrow to caller's `p`.
     - Cursor advances inside the body: `self.x` evaluated → `1`.
     - `ReturnValue { 1_i32 }` step shows `→ 1` annotation on the method frame.
     - `FrameLeave`: method frame grays out.
     - `v: i32 = 1_i32` row appears in `main`'s frame.

5. **US4 — Associated function**:
   - Select `Associated function (M07.4)`. Editor shows `let p = Point::new(1, 2);` after the impl.
   - Step through: `Point::new` frame opens, `x=1` and `y=2` params bound, body constructs `Point { x, y }` via shorthand, `ReturnValue` shows `→ Point { x: 1, y: 2 }`, frame closes, `p` lands the struct.

6. **Live editing** — error UX:
   - In any sample, edit the code to `let p = Point { x: 1 };` (missing field). Observe typeck error "missing field `y` in struct literal `Point`" pointing at the literal.
   - Try `let p = Point { x: 1, y: 2, z: 3 };` (extra field). Observe "no field `z` on struct `Point`".
   - Try `let p = Point { x: true, y: 2 };`. Observe "expected i32, found bool" on `true`.
   - Try `let v = p.bogus();`. Observe "no method `bogus` on type `Point`".
   - Try `let r = &p.z;`. Observe "no field `z` on struct `Point`".

7. **No regressions**:
   - Cycle through M01–M07.3 samples. Each renders correctly. M07's Vec realloc + heap pedagogy unchanged. M07.3's array inline cells unchanged.

## Developer notes

### The struct viz proposal (research R-016) — iterative

The user has explicitly flagged the struct visualization as the step-by-step iterate-on-this part of M07.4. Implementation order:

1. Land all the non-UI plumbing first (AST, typeck, eval, protocol shape).
2. Implement the **first cut** of the JS struct rendering per research R-016 Proposal A (vertical labeled rows).
3. **UX checkpoint**: load `Struct basic` in the page, present to user, discuss tweaks (typography, spacing, color, label position, field-name dot prefix, etc.).
4. Iterate until happy.
5. Add per-field hover plumbing (research R-016 — the hover handler queries `.struct-field[data-field-name=...]`).
6. Add field-borrow arrow label (`field_label` on `ArrowView` → `.x` text rendered at arrow midpoint).
7. Ship US2/US3/US4 samples.

DO NOT skip the checkpoint between step 2 and step 3.

### Why is `Value::Struct` a Value variant instead of a heap object?

Same reason as M07.3's `Value::Array`: structs are stack-allocated. The slot's value IS the struct's content. No heap allocation, no destructor event in M07.4.

### Why extend `Value::Ref` instead of adding `Value::FieldRef`?

A new variant would split every `Value::Ref` consumer in eval, typeck, and UI into a two-arm match where the field-ref arm just delegates back to the regular ref arm with extra metadata. The extension uses `#[serde(default, skip_serializing_if = "Vec::is_empty")]` so existing M06+ borrow snapshots stay byte-identical; new field-borrow snapshots get the `field_path` field appended.

### Why "two-pass typeck"?

Phase 1 collects struct schemas + impl signatures into `StructRegistry` + `ImplRegistry` BEFORE any function body is typechecked. This lets `impl Point { .. }` reference `struct Point` even when the impl appears earlier in the file. Phase 2 typechecks bodies with both registries visible.

### Method dispatch flow

1. Parser produces `Expr::MethodCall { receiver, name, args }`.
2. Typeck evaluates the receiver's type, then tries (a) hardcoded built-ins (`Vec::push`, `Slice::len`, etc.), then (b) `ImplRegistry.methods[(receiver_struct_name, name)]`. First match wins.
3. Eval enters a new frame via `FrameEnter`, binds `self` (for instance methods) + each arg as `SlotAlloc + SlotWrite`, executes the body, emits `ReturnValue` + `FrameLeave`.

### Per-field hover highlighting

When a borrow's `field_path` is non-empty, the resulting `ArrowView` gets `field_label: Some(".x")`. On arrow hover (existing M07.1+ handler), in addition to the standard arrow-pulse class, the JS queries `[data-slot-id=<target>] .struct-field[data-field-name="x"]` and toggles `.field-borrow-highlighted` to light up just that field's row in the target slot.

### Forward references

`impl Point { .. }` can appear BEFORE `struct Point { .. }` in the source. Phase 1 typeck collects struct schemas + impl signatures in source order; phase 2 type checks bodies with both registries fully populated. Cyclic dependencies aren't possible in M07.4 (no struct-typed fields, so a struct can't reference itself or another struct).

### Auto-deref for `self.x`

`self` has type `&Self` inside `&self` methods. `self.x` reads the `x` field through the reference. Typeck rule (not parser sugar): `typecheck_field_access` accepts both `Ty::Struct(_)` and `Ty::Ref { Ty::Struct(_), .. }` receivers. Eval mirrors: `Expr::FieldAccess` on a `Value::Ref { target: Pointee::Slot(_), .. }` looks up the target slot and reads the field.

### What about `impl Vec { fn foo(...) }`?

Typeck-rejected at phase 1: "impl block references unknown user-defined type `Vec` (built-in types not impl-able in M07.4)". The hardcoded built-ins are part of "the stdlib" pedagogically; user impls extend the language, not the stdlib.

## When extending in M08

M08 (threads) doesn't depend on M07.4. Both are siblings depending on M07.1+. M08 introduces `thread::spawn`, `Arc<T>`, `Mutex<T>`. M07.4's struct machinery is unaffected. A future combined milestone could let learners spawn threads with struct-payload closures (`thread::spawn(move || { let p = Point { .. }; ... })`).

## What this milestone does NOT add

- **Non-Copy field types** (Vec, String, Box, Slice, Array, &str, other Structs). Future M07.x.
- **Nested structs**, **multi-level field access** (`p.x.y`).
- **Generics** (`Point<T>`).
- **Traits** + **trait impls** + **trait objects**.
- **Derive macros** (`#[derive(Debug, Clone)]`).
- **Struct update syntax** (`Point { x: 10, ..p }`).
- **Tuple structs**, **unit structs**, **empty structs**.
- **Pattern matching** (`let Point { x, y } = p;`).
- **Recursive structs**.
- **Multiple impl blocks per type**.
- **`Drop` impls** + per-field drop pedagogy (since fields are Copy, no destructors fire).
- **Inherent associated constants**.
