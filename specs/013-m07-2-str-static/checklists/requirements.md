# Specification Quality Checklist: M07.2 — `&str` + static memory

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-05-23
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

- Validation pass 1 (2026-05-23): all items pass.
- **Closes M07's known type-incorrectness**: string literals shouldn't be `String`. The static-memory region is a new visual concept worth its own pedagogy.
- **Two P1 user stories** (string-literal-as-&str AND String::from-copies-from-static). Both are essential headlines. US3 (push_str takes &str) is P2 — necessary for consistency but smaller pedagogy.
- **Builds directly on M07.1's slice infrastructure**: `&str` is `Ty::Slice(Box::new(Ty::Int(U8)))` — slice of bytes. Reuses Value::Slice shape, `[len: N]` arrow annotation, byte-cell hover highlight. M07.2 only adds `Pointee::Static` target variant + visual region.
- **6th invocation of closed-enum-with-revisions rule** (if new `MemEvent::StaticAlloc` event variant is chosen at plan-phase). Pure additive: new Pointee variant + new event variant (or extended HeapAlloc).
- **No restructure** of existing variants. M03 snapshots stay byte-identical (existing L1 samples don't construct string literals). M07's m07_string test may need re-baseline.
- **Static-block dedup** matches Rust linker behavior — `.rodata` merges duplicate string constants. Important to document so behavior is predictable.
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION because reasonable defaults exist):
  1. **`Ty::Str` sugar vs direct `Ty::Slice(Ty::Int(U8))`** — both work; sugar is cosmetic.
  2. **Event shape for static blocks** — new `StaticAlloc` variant (cleanest) vs. extending `HeapAlloc` with a flag.
  3. **Static region visual placement** — separate band vs. annotated section of heap panel.
- **Out-of-scope items explicitly listed**: &str slicing, format!/println!, string indexing, +/+= on strings, generalized &str expressions (push_str/String::from still restricted to literal args). Tight scope.
- **Sized M** per the rubric: ~3-4 source modules, ~400-600 LOC. Smaller than M07.1 because the slice machinery is fully reused.
- **No new Rust deps, no new JS deps**.
- **Independent of M07.3** (arrays) — both are siblings depending only on M07.1.

## Post-implementation audit (2026-05-23)

| Success criterion | Status | Evidence |
|---|---|---|
| SC-001 (`"toto"` is `&str` + static block + slice arrow + `[len: 4]`) | code ✓ / visual ⏳ | `run_pipeline_str_literal` test ✓; visual deferred to maintainer QA |
| SC-002 (`String::from("hi")` shows both static + heap blocks) | code ✓ / visual ⏳ | `run_pipeline_string_from_static_visible` test ✓; visual deferred |
| SC-003 (heap String freed at scope exit; static block stays) | ✓ | `run_pipeline_string_from_static_visible` asserts `free_count == 1` |
| SC-004 (literal dedup — two `"hi"` → one StaticAlloc) | ✓ | `run_pipeline_literal_dedup` |
| SC-005 (`s.push_str("!")` argument from static; no separate heap alloc) | ✓ | `run_pipeline_push_str_static` (static_count=2, heap_count=1) |
| SC-006 (≥ 2 new `m07_2_*.rs` samples) | ✓ | `m07_2_str_literal.rs`, `m07_2_string_from.rs`, `m07_2_push_str.rs` — actually 3 samples in `tests/samples/` + `web/samples/` |
| SC-007 (existing M01–M07.1 byte-identical; M07 string tests re-baselined) | ✓ | Starting tests: 110 passing; after M07.2: 115 passing (+5 new tests; M07's `run_pipeline_string_from` updated to count both StaticAlloc + HeapAlloc; existing tests pass) |
| SC-008 (WASM bundle ≤ +15% vs M07.1) | ✓ | Raw release WASM: 280,519 B (vs M07.1 baseline 274,947 B = +2.0% growth; well under +15% ceiling) |
| SC-009 (zero warnings under -D warnings, host + WASM) | ✓ | `RUSTFLAGS="-D warnings" cargo build/test --release` clean; `cargo build --release --target wasm32-unknown-unknown` clean; verified after `cargo clean` |

### Implementation notes

- **`Value::Str` removal cascade** (T003 deferred to T008): 4 sites needed the arm dropped — `Value::type_name()` (event.rs), `value_size_bytes()` + `render_value_for_note()` (eval.rs), `render_value()` (ui.rs). The eval-site for `String::from` and `push_str` switched from `match self.eval_expr(arg) { Value::Str(s) => s }` to `match self.eval_expr(arg) { Value::Slice { target: Pointee::Static(addr), .. } => self.get_static_bytes(addr).to_owned() }`. Atomic with T008's literal-as-slice rewrite.
- **`Pointee` cascade**: 4 ui.rs match sites + 2 event_span match sites (ui.rs + tests/m03.rs) needed the new `Pointee::Static(_)` / `MemEvent::StaticAlloc { .. }` arm. All mechanical.
- **No existing `BorrowShared` apply-event modification needed** — the borrow-tracking machinery is shape-agnostic on the Pointee variant; adding the `Static` arm in the BorrowTarget conversion was sufficient.
- **No M03 snapshot drift** — confirmed: 110 starting tests all still pass.
- **No new Rust deps, no new JS deps, no `Cargo.toml` changes**.
- **Static-block dedup works as designed**: `let a = "hi"; let b = "hi";` produces 1 StaticAlloc + 2 BorrowShared (verified by `run_pipeline_literal_dedup`).
- **Pedagogical pattern**: the existing M07 `m07_string.rs` sample now renders BOTH the heap String AND the static `"hi"` / `"!"` blocks — pedagogy strengthens for free.
- **3 samples shipped** (vs spec's required 2): `m07_2_str_literal`, `m07_2_string_from`, `m07_2_push_str`. Each demonstrates a distinct aspect of the static-region pedagogy.
