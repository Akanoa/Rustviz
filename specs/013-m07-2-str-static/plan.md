# Implementation Plan: M07.2 — `&str` + static memory

**Branch**: `013-m07-2-str-static` | **Date**: 2026-05-23 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/013-m07-2-str-static/spec.md`

## Summary

Make string literals typecheck correctly as `&str` (slice of u8 into static memory) instead of M07's incorrect `Ty::String`. Introduces a static-memory region (third visual area alongside stacks + heap) holding read-only byte blocks for each unique string literal — content-deduplicated to match Rust's linker behavior. `String::from(literal)` now copies bytes from the static region into a fresh heap allocation; both blocks are visible simultaneously, making the copy explicit.

**6th invocation of the closed-enum-with-revisions rule**: `Pointee` gains `Static(StaticAddr)`; `MemEvent` gains `StaticAlloc { addr, bytes, span }`; `Ty` gains `Str` (sugar over `Ty::Slice(Ty::Int(U8))` for cleaner rendering). All purely additive.

Authority chain: `MILESTONES.md` › M07.2 → `spec.md` (this feature) → this plan.

## Technical Context

**Language/Version**: Rust 2024 edition (same toolchain as M01–M07.1). No new toolchain requirements.
**Primary Dependencies**: existing `indexmap`, `serde`, `serde_json`, `wasm-bindgen`, `js-sys`, `console_error_panic_hook`. **No new Rust deps**. **No JS deps changes**.
**Storage**: in-memory; no new files. M01/M02/M03 snapshot tests stay byte-identical (existing L1 samples don't construct string literals). M07's `m07_string` test re-baselines: alloc-count for `String::from("hi")` now stays 1 (heap) but the trace gains a `StaticAlloc` event for the `"hi"` literal that occurs BEFORE the `String::from` heap alloc.
**Testing**: existing `cargo test --test m01 / m02 / m03` should pass byte-identical. New `cargo test --lib pipeline::tests` covering: string-literal-as-slice (no heap event), `String::from`-emits-both-events (one StaticAlloc + one HeapAlloc), literal dedup (`"hi"; "hi";` → one StaticAlloc), `push_str` with both literals visible. ≥ 5 new tests. M07's `run_pipeline_string_from` updated to check for the StaticAlloc event presence + heap event still firing. Manual M07.2 QA per the SC-008 procedure.
**Target Platform**: same as M01–M07.1 (host + `wasm32-unknown-unknown`).
**Project Type**: Rust library + companion UI. Touches ~4 source modules + new web-side static-region rendering. Sized M.
**Performance Goals**: same pipeline latency budget. Static-block dedup is O(N) per literal (linear scan of existing blocks); acceptable for pedagogical traces.
**Constraints**: M01/M02/M03 byte-identical; WASM bundle ≤ +15% vs M07.1 baseline (905,170 B → ≤ 1,040,946 B uncompressed) per SC-008; zero warnings under `-D warnings` (SC-009); existing M06/M06.1/M07/M07.1 features preserved.
**Scale/Scope**: ~4 source modules + minor JS additions + 3 sample pairs + ≥ 5 new unit tests. **Estimated ~400-600 LOC net change**. Sizing: **M** per the rubric.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is still the unfilled speckit template. Same vacuous PASS as features 001–012.

**Post-design re-check**: still vacuous, still PASS.

## Project Structure

### Documentation (this feature)

```text
specs/013-m07-2-str-static/
├── plan.md                 # This file
├── spec.md                 # Feature spec
├── research.md             # Phase 0: 12 design decisions
├── data-model.md           # Phase 1: 1 new Pointee variant, 1 new Ty variant, 1 new MemEvent variant, StaticState eval-side, StaticView UI-side
├── quickstart.md           # Phase 1: dev workflow + manual QA procedure (3 sample walkthroughs)
├── contracts/
│   └── m07-2-protocol-delta.md  # Phase 1: 6th closed-enum invocation; static-region protocol
└── checklists/
    └── requirements.md     # From /speckit-specify
```

### Source Code (repository root) — files M07.2 touches

```text
src/
├── event.rs                # MODIFIED — add `StaticAddr(u32)` newtype, `Pointee::Static(StaticAddr)` variant, `MemEvent::StaticAlloc { addr, bytes, span }` variant. Value::Slice's `target: Pointee` automatically supports the new variant (no Value-shape change).
├── typeck.rs               # MODIFIED — add `Ty::Str` sugar variant (semantically equivalent to `Ty::Slice(Box::new(Ty::Int(U8)))`; rendered as `"&str"`). Update `is_copy()` for Ty::Str (false — same as Slice). Update `name()` to render `"&str"`. Change `Expr::StrLit` arm to return `Ty::Str` (NOT `Ty::String`). Update method dispatch: add `(Ty::Str, "len") -> u64`. Keep `String::push_str` / `String::from` arg-typecheck restriction to `Expr::StrLit` (unchanged).
├── eval.rs                 # MODIFIED — add `static_region: StaticState` field to Evaluator (parallel to `heap: HeapState`). Helpers: `intern_static(bytes, span) -> StaticAddr` (dedup by content; emit StaticAlloc on first occurrence). Change `Expr::StrLit` eval: intern bytes, allocate a borrow_id, emit `BorrowShared { target: Pointee::Static(addr), .. }`, return `Value::Slice { target: Pointee::Static(addr), start: 0, len, byte_offset: 0, byte_len: len, mutable: false, borrow_id }`. The transient `Value::Str` variant becomes unused for literals — removed for cleanliness (plan-phase decision). Update `String::from` to accept the new Value::Slice (extract bytes via the static region lookup). Update `string_push_str` similarly.
└── ui.rs                   # MODIFIED — add `StaticView { addr, bytes, size, display }` struct mirroring `HeapView` with no `freed` flag. Add `static_region: Vec<StaticView>` field to `World` + `StateSnapshot`. apply_event for `StaticAlloc` inserts into world.static_region. apply_event for `SlotWrite` with `Value::Slice { target: Pointee::Static(_), .. }` populates the ArrowView with a new `ArrowTarget::Static(u32)` variant. Existing `ArrowTarget` gains `Static(u32)` arm. The dangling-detection scan in realloc_heap is unaffected (static targets never go stale).

tests/
├── m01.rs / m02.rs / m03.rs  # Unchanged. Snapshots byte-identical (no L1 sample constructs string literals).
└── samples/
    ├── (existing)            # Unchanged.
    └── m07_2_*.rs            # NEW (3 files): m07_2_str_literal, m07_2_string_from, m07_2_push_str.

web/
├── samples/                # MODIFIED — add 3 m07_2_*.rs mirrors.
├── index.html              # MODIFIED — add a new `<section id="static" aria-label="static memory">` between stacks and heap. Dropdown grows 3 entries.
├── index.js                # MODIFIED — `renderStaticRegion(state.static_region)`: maintains a `staticElements: Map<addr, HTMLElement>` (similar to heapElements). For each StaticView, create/update a `<div class="static-block" data-static-addr="...">` with byte-cells. `renderArrows`: extend the target resolver to handle `arrow.target.Static` lookups via `[data-static-addr]`. Hover-highlight extends: if target is Static, look up `[data-static-addr]` then its byte-cells; no element-span highlight (static blocks render raw bytes, no Vec-style element segmentation in M07.2).
├── style.css               # MODIFIED — `#static` styling (different background from heap; "static memory (RO)" label; gray-ish byte-cells to convey read-only).
└── Trunk.toml              # Unchanged.

# M03's contract amended for the 6th closed-enum invocation:
specs/004-m03-event-eval/contracts/m03-api.md   # MODIFIED — note M07.2 as the 6th invocation. Adds `Pointee::Static(StaticAddr)`, `Ty::Str`, `MemEvent::StaticAlloc`. Pure additive.
```

**Structure Decision**: smaller than M07.1 but with a new third visual region. The slice infrastructure from M07.1 is fully reused — `&str` IS a slice (`Ty::Str` is a sugar over `Ty::Slice(Ty::Int(U8))`). The headline new code is (a) the static-region eval-side state with content-dedup, (b) the StaticAlloc event variant, (c) the new visual region rendering.

## Complexity Tracking

> No constitutional violations. Table omitted.

### Notable non-trivial complexity

- **Three-target slice machinery**: M07.1's `Value::Slice` already supports `Pointee::Heap`. M07.3 will add `Pointee::Slot`. M07.2 adds `Pointee::Static` — extending the existing dispatcher in `apply_event` and `renderArrows`. The slice abstraction is now genuinely three-way, which is the right shape for Rust's three memory regions (stack, heap, static).
- **Content-deduplicated static interning**: `intern_static(bytes)` scans existing blocks for matching content; reuses the addr on match. Matches Rust's linker behavior. O(N) per literal; fine for pedagogy.
- **`Ty::Str` sugar vs direct `Ty::Slice(Ty::Int(U8))`**: chose sugar (`Ty::Str`) so the rendered type is `"&str"` not `"&[u8]"` — pedagogically clearer and matches what Rust developers see. Internally `Ty::Str` and `Ty::Slice(Box::new(Ty::Int(U8)))` are treated equivalently for borrow-tracking, method dispatch, and aliasing. Documented as a peephole in typeck.
- **`Value::Str` deprecation**: M07's transient `Value::Str(String)` was used for `Expr::StrLit` → `String::from(arg)` plumbing. With M07.2 the literal becomes `Value::Slice` from the start, so `Value::Str` is no longer constructed. The variant is removed for cleanliness; the eval site for `String::from` extracts bytes via the static-region lookup instead.
- **M07 String test re-baseline**: `run_pipeline_string_from` currently counts `HeapAlloc` events (expects exactly 1). After M07.2 the count stays 1 (heap), but the trace also gains a `StaticAlloc` for the literal. Test assertion updated to count both event variants separately. `run_pipeline_string_push_str_realloc` similarly.
- **Static region position**: a new `<section id="static">` between stacks and heap in the page layout. Width-wise narrow (static blocks are typically small — string literals); height-wise expands as more literals are interned. CSS-side: distinct background (subtle gray gradient) and a "static memory (RO)" label to differentiate from the heap.
- **Hover highlight on static blocks**: works the same way as M07.1's heap-block hover — but without element-span highlights (static blocks are raw bytes, not Vec-style structured displays). The byte-cell yellow outline path generalizes cleanly to static addresses by targeting `[data-static-addr]` instead of `[data-heap-addr]`.
- **`&str` arg in push_str/String::from**: typeck still requires `Expr::StrLit` as the arg (M07's restriction). The internal byte-extraction path changes — instead of looking at a transient `Value::Str`, we look up the static block and copy its bytes into the heap allocation. Pedagogically the copy from static to heap is visible at the `String::from` step.
