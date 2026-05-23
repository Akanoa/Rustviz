# Specification Quality Checklist: M06.1 — Mutation

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
- **Third revision milestone in the project** (after M03.1 and M03.2). Same pattern: closes a pedagogical gap discovered in QA of the prior milestone.
- **Three P1 user stories**: direct assignment (US1), deref-read (US2), deref-write (US3). All are coequal in priority — without any one, the mutation pedagogy is incomplete.
- **No protocol changes**: this milestone adds AST nodes and typeck rules but no new `MemEvent` variants, no new `Ty`/`Value` variants. The existing `SlotWrite` event variant carries the mutation. No M03 contract amendment needed.
- **`let mut` finally gains meaning**: from M03 through M06, the `mut` keyword on `let` was parsed but never tested with mutation. M06.1 makes it actually do something. Documented in the user-stories framing.
- **Visualization is "free"**: the stacks panel already animates `SlotWrite` value changes (used by `let x = init;`). Same animation handles `x = v;` and `*r = v;`. No new UI work in M06.1.
- **`SlotWrite` event reuse** (FR-008): the same variant carries both let-init writes and assignment-mutation writes. The visualization treats them identically. This was the design payoff of M03's choice to emit `SlotAlloc` separate from `SlotWrite` — assignment doesn't allocate, only writes.
- **Borrow tracker integration** (FR-007): direct assignment checks the tracker for active borrows. `*r = v` doesn't take a new borrow — the existing `&mut` permits it. Documented in the FR text.
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION because reasonable defaults exist):
  1. **AST shape**: `Stmt::Assign { lhs, rhs }` vs `Expr::Assign` — statement form preferred (smaller surface, matches M06.1's expression-statement-position restriction).
  2. **`*r` precedence in the parser**: prefix unary at the same level as `&`/`-`/`!`. Plan-phase confirms.
  3. **Assignment span**: covers whole `lhs = rhs` for the SlotWrite; alternative is span on the `=` token only. Plan-phase decides.
- **No literal type suffix interactions**: M03.2's literal suffixes (`5u8`) work the same way in assignment rhs — `let mut x: u8 = 5; x = 250_u8;` is valid.
- **No NLL** — the borrow tracker's scope-level lifetimes don't change. The fact that "the borrow isn't actively *read* anymore" doesn't end its lifetime. Documented in Assumptions.
- **No re-borrows / no `&*r`**: explicitly deferred. M06.1 stays focused on the deref + assignment surface.
- **Sized M** per the rubric: 3 source modules + 3 sample pairs + new tests. ~250 LOC net.

## Post-implementation audit (2026-05-22)

Following `/speckit-implement` execution of M06.1 (19 tasks T001–T019).

### Success-criteria results

| ID    | Description | Result |
|-------|-------------|--------|
| SC-001 | Live page accepts `x = v;`, `let y = *r;`, `*r = v;` within budget; arrows persist through mutations | **CODE-VERIFIED**; browser visual QA deferred |
| SC-002 | Immutable-binding assignment → typeck error | PASS — covered by `run_pipeline_assign_immutable_rejected` |
| SC-003 | Deref-write through `&T` → typeck error | PASS — covered by `run_pipeline_deref_write_through_shared_rejected` |
| SC-004 | `*r = v` emits exactly one SlotWrite targeting `target_slot`; no premature BorrowEnd | PASS — covered by `run_pipeline_deref_write_basic` |
| SC-005 | ≥ 3 M06.1 reference samples ship | PASS — 3 shipped: `m06_1_assign_basic`, `m06_1_deref_read`, `m06_1_deref_write` |
| SC-006 | M01–M06 tests stay byte-identical | **PASS** — m01: 8 byte-identical, m02: 16 byte-identical, m03: 8 byte-identical. Truly no snapshot drift since no existing samples construct deref or assign |
| SC-007 | WASM bundle ≤ +20% vs M06 baseline (87,354 B → ≤ 104,825 B) | PASS — **88,841 B gzipped (+1.7%)**, dramatically under budget. Reusing `SlotWrite` (no new event variant) keeps the binary lean |
| SC-008 | Zero warnings under `-D warnings` | PASS — host build + full test suite clean, WASM target clean |

### Implementation findings

- **Smallest milestone since M03.1** — sized M, 19 tasks, ~250 LOC net change as estimated. Tight cycle.

- **No protocol changes paid off** in test count: M01/M02/M03 stayed byte-identical (zero re-baselines), confirming that adding AST variants without touching `MemEvent`, `Ty`, or `Value` shapes leaves existing snapshots untouched.

- **`Stmt::Assign` typeck is dual-purpose** — single function (`typecheck_assign`) handles both US1 (direct Ident assign) and US3 (Deref(Ident) assign) via match on lhs. Clean abstraction; the place-expression check is at the top level.

- **`is_borrowed(BindingId) -> bool`** added to `BorrowTracker` for the assignment-into-borrowed-binding check. Minimal addition, one method.

- **Eval helpers `update_slot_value` and `lookup_slot_value`** symmetrically walk the call stack to find a LocalSlot by id. Used for assignment (write) and deref-read (read). The mutate-and-emit pattern in `Stmt::Assign` eval keeps in-memory state and event stream in sync.

- **No new MemEvent variants** — `SlotWrite` carries all M06.1 mutations. The M03 design choice (allocate vs write separation) paid off: reassignment is just another `SlotWrite` with no new infrastructure.

- **Zero JS/CSS changes** — visualization is "free" via the existing SlotWrite animation in the stacks panel. The dropdown HTML only grew by 3 entries.

- **Bundle growth was +1.7%**, vastly under the +20% budget (and under any sane interpretation of "minimal"). The lattice didn't grow; only AST/typeck/eval added pattern arms.

- **`let mut r = &x; r = &y;` edge case** (ref reassignment) — not exercised in the shipped samples; if a maintainer triggers it during QA, the old borrow's BorrowEnd won't fire until scope close per the documented limitation. Worth following up if it surfaces.

### Test summary

```
$ RUSTFLAGS="-D warnings" cargo test
94 passed
  - m01: 8 (byte-identical from M06)
  - m02: 16 (byte-identical from M06)
  - m03: 8 (byte-identical from M06)
  - lib: 51 (+7 new mutation tests: 2 assignment + 2 deref-read + 3 deref-write/aliasing)
  - intkind_tests: 5
  - misc: 6

$ cargo build --release --target wasm32-unknown-unknown
WASM: 217 KB raw / 88,841 B gzipped (M06 baseline 87,354 B; +1.7%)
```

### Conclusion

M06.1 code-side complete. **Shipping for QA.** Maintainer walks `specs/010-m06-1-mutation/quickstart.md` SC-008 procedure focused on:

1. **Direct assignment**: load `Direct assignment (M06.1)`, step, observe `x` animate `0_i32 → 7_i32`.
2. **Deref-read**: load `Deref read (M06.1)`, step, observe `y = 42_i32`, blue arrow persists.
3. **Deref-write (headline)**: load `Deref write (M06.1)`, step, observe `x` animate `5_i32 → 10_i32` WHILE the red arrow stays anchored.

If any of the typeck-error cases (immutable-assign / through-`&T`-write / assign-to-borrowed) surface a UX issue (error span misplaced, message unclear), those are tuning concerns for the maintainer's QA pass.
