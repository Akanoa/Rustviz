# Specification Quality Checklist: M03.2 — Scalar Lattice Expansion

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-05-22
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

- Validation pass 1 (2026-05-22): all items pass.
- **Second revision milestone in the project** (after M03.1). Same pattern: extends a previously-shipped milestone's closed types in a coordinated, additive way.
- **Three P1 user stories**:
  - **US1** — integers (the headline gap exposed by M05 QA: `u8`/`u32`/`i64` etc.).
  - **US2** — floats with NaN/Inf surfacing (pedagogically distinct from integer overflow).
  - **US3** — non-regression of M01–M05.
- **Integer overflow vs. float NaN/Inf asymmetry**: explicit, intentional. Integer overflow halts (matches the existing div-by-zero pattern in M03); float specials are valid Rust behavior surfaced via `Note { kind: Info }`. Pedagogically these are different lessons and should look different in the UI.
- **`Value::Eq` dropped**: floats force `PartialEq` only. Plan-phase audits any downstream usage. Documented in Assumptions.
- **`usize`/`isize` ≡ `u64`/`i64`**: forced platform-independent width for browser determinism. A future revision could make these dynamic if a pedagogical case calls for it.
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION because reasonable defaults exist):
  1. **`Value` representation** (per-type variants vs. unified `{kind, bits}`) — research recommends unified for compactness, but per-type is also valid.
  2. **Suffix rendering style** (`5_u8` vs. `5 : u8` vs. `5<u8>`) — settles during M03.2's QA.
  3. **Info-note emission cadence** (once per binding vs. once per producing expression vs. on every appearance) — spec picks "once per binding"; plan-phase refines.
- **No literal suffix parsing** (`5u8`, `2.5_f32`): deliberately deferred. Annotations-only keeps the lexer/parser changes minimal. Future revision can add suffixes.
- **No `as` casts**: deferred to a separate revision if needed.
- **No `f16` / `f128`**: not supported in current stable Rust; not in scope.
- **No platform-dependent width for `usize`/`isize`**: FR-011 pins these to 64-bit equivalents for determinism. Real Rust varies by target; this is a pedagogical simplification.
- **WASM bundle budget (+5% vs M05's 63,144 B)** generous: 14 enum variants + arithmetic dispatch should add < 5 KB raw. If exceeded, plan-phase decides whether to switch to a unified `Value` form to reclaim space.
- **No new MemEvent variants**: M03.2 reuses the existing `Note { kind: Info }` and `Note { kind: RuntimeError }` variants for the new pedagogical surfaces. Wire format only grows via `Value`'s shape change.

## Post-implementation audit (2026-05-22)

Following `/speckit-implement` execution of M03.2 (21 tasks T001–T021).

### Success-criteria results

| ID    | Description | Result |
|-------|-------------|--------|
| SC-001 | All 14 new types accepted as annotations; ≤ 1 s editor → trace | **CODE-VERIFIED**; browser perf deferred to maintainer QA |
| SC-002 | Integer overflow → RuntimeError halt for ≥ 3 types | PASS — covered by `run_pipeline_u8_overflow` and integer dispatch generalized over `IntKind` |
| SC-003 | Float `±Inf` / `NaN` → Info note without halt | PASS — covered by `run_pipeline_float_nan`, `_float_inf`, `_float_propagation_no_redundant_note` |
| SC-004 | Cross-type arithmetic → typeck error | PASS — covered by `run_pipeline_cross_type_error` |
| SC-005 | M01/M02/M03 byte-identical | **PARTIAL** — M01 ✓ byte-identical (8 tests). **M02 + M03 re-baselined** because `Ty`'s Debug format changed (`I32` → `Int(I32)`), affecting M02's TypeMap snapshots; `Value`'s Debug format also changed (`Int(5)` → `Int { kind: I32, bits: 5 }`), affecting M03 event snapshots. Plus one M02 snapshot drifted because typeck's binary-op error message was generalized from "requires both operands to be i32" to "requires both operands to be the same numeric type". All diffs are mechanical and predictable. **Spec SC-005 was overly optimistic** about M02 staying byte-identical — `Ty`'s Debug format is referenced in M02's TypeMap snapshots |
| SC-006 | ≥ 3 M03.2 reference samples | PASS — 3 shipped: `m03_2_basic_u8`, `m03_2_u8_overflow`, `m03_2_float_nan` |
| SC-007 | WASM bundle growth ≤ +5% vs M05 baseline (63,144 B) | **FAIL** — **84,007 B gzipped (+33%)** even with `[profile.release] lto=true, codegen-units=1, strip=true, opt-level="z"` tuning added in M03.2. Growth comes from genuinely new code: 14 new `IntKind`/`FloatKind` arms × multiple ops, type-name lookup expanded from 3 to 16 cases, float arithmetic dispatch. Still **well under M04's 2 MB SC-005 ceiling** (84 KB ≪ 2,000 KB). The +5% rolling budget was tight; +33% reflects real new functionality |
| SC-008 | Zero warnings under `-D warnings` | PASS — host build + full test suite clean, WASM target clean |

### Implementation findings

- **Unified `Value` representation paid off**: `Value::Int { kind, bits }` + `Value::Float { kind, value }` dispatches all 14 numeric types through one set of arithmetic match arms (with a `if a_k == b_k` guard) instead of 14 per-type cases per op. The codebase stays compact.

- **Literal coercion is bidirectional**: `let x: u8 = 250; let y: u8 = x + 10;` works because `try_coerce_to` recoerces `10` (default `I32`) to `U8` based on the binary-op context. Eval consults typeck's `expr_types` map to use the coerced kind. This sidesteps explicit `as` casts and makes the overflow demo work as intended.

- **Eval consults typeck's `expr_types`** for literals: a clean coupling that avoids passing context through eval. The `LitInt`/`LitFloat` arms look up the recorded type, falling back to the default kind if missing.

- **Float arithmetic's "de novo" Info note** works as designed: `run_pipeline_float_propagation_no_redundant_note` confirms only ONE Info note fires when NaN propagates through subsequent operations.

- **Two M02 snapshots re-baselined**: the `Ty` Debug-format change cascades through M02's TypeMap snapshots. SC-005 needs the M01-only guarantee — M02 byte-identical was a wrong assumption in the spec.

- **`[profile.release]` tuning** (LTO + codegen-units=1 + strip + opt-level=z) added in `Cargo.toml`. Without it the WASM was 95 KB gzipped; with it, 84 KB. The 11 KB savings is worth the slightly longer release builds.

- **Bundle-size budget exceeded**: SC-007's +5% budget was tight given that M03.2 adds 14 type variants with full arithmetic + range checking. The +33% growth (~21 KB gzipped raw) is real new functionality, not bloat. The visualizer's overall WASM remains well under the M04 SC-005 ceiling of 2 MB gzipped. The budget should be revisited or made non-binding for revision milestones with genuine variant additions.

- **Helper enum tests are exhaustive over signedness**: `is_signed_exhaustive` checks all 12 IntKind variants. Catches future variant additions that forget to set `is_signed` correctly.

### Test summary

```
$ RUSTFLAGS="-D warnings" cargo test
73 passed
  - m01: 8 (byte-identical snapshots)
  - m02: 16 (re-baselined — Ty Debug change + 1 error-message wording)
  - m03: 8 (re-baselined — Value Debug change)
  - lib: 41 (6 event smoke + 12 cursor + 17 pipeline + 6 IntKind)

$ cargo build --release --target wasm32-unknown-unknown
WASM: 204 KB raw / 84,007 B gzipped (M05 baseline 63,144 B; +33% — over the +5% SC-007 budget; documented above)
```

### Conclusion

M03.2 code-side complete. **Shipping for QA.** Maintainer:

1. Walks `quickstart.md` SC-008 procedure (~7 min) covering integer types, integer overflow, cross-type errors, float Inf/NaN, no regressions of existing samples.
2. The new dropdown options should each render with their type-tag suffix (`5_u8`, `+Inf_f64`, etc.) and the overflow / float-special demos should behave as designed.
3. If bundle-size growth is a concern, follow-up work could investigate `wasm-opt` post-processing or per-kind macro-generated arithmetic dispatch.
