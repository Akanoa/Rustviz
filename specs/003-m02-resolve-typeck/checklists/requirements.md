# Specification Quality Checklist: M02 — Name Resolution + Lightweight Typeck

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-05-20
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

- Items marked incomplete require spec updates before `/speckit.clarify` or `/speckit.plan`.
- Validation pass 1 (2026-05-20): all items pass.
- **Authority chain**: same pattern as M01 — this spec defers all scope decisions to `MILESTONES.md` › M02. The "users" framing is again internal (M03 consumer + contributor) since M02 has no end-user-facing surface.
- **Type inference rules in FR-007 are concrete**: this is a borderline case for "no implementation details" — listing the operator typing rules is technically *how* typeck works. The rules were kept in the spec because (a) they are testable (each rule is an acceptance criterion), (b) the L1 grammar from CLAUDE.md is small enough that the rules are essentially scope statements, and (c) they're language semantics, not framework/library choices. Future milestones with bigger type lattices may need to externalize a separate typing-rules doc.
- **Snapshot output format deferred**: spec doesn't pin whether snapshots show a combined "annotated AST" view or two side tables. Plan phase will decide based on readability of actual sample output.

## Post-implementation audit (2026-05-20)

Following `/speckit-implement` of M02. All 24 tasks (T001–T025, skipping T007) executed; 16 snapshot tests pass.

### Success-criteria results

| ID    | Description | Result |
|-------|-------------|--------|
| SC-001 | `cargo test --test m02` runs ≥ 5 snapshot tests | PASS — **16 tests** (4 happy + 3 resolver-error + 9 typeck-error) |
| SC-002 | 100% of identifier use sites carry a `BindingId` | PASS — visual review of all happy-path snapshots; `Resolution.uses` has an entry for every `Expr::Ident` |
| SC-003 | 100% of expression nodes carry an inferred `Ty` | PASS — `TypeMap.expr_types` has entries for every value-producing Expr; callee Idents are intentionally absent per VR-11 |
| SC-004 | Single error per failing input | PASS — `errors_on_first_undeclared` confirms only the first undeclared ident is reported |
| SC-005 | Deterministic snapshots | PASS — re-running `cargo test --test m02` produces no `.snap.new` files |
| SC-006 | ≤ ~1500 LOC across `src/resolve.rs` + `src/typeck.rs` | PASS — **758 LOC** (resolve 254, typeck 504) |
| SC-007 | Zero warnings under `-D warnings` | PASS — `cargo build --release` and `cargo test --test m02` both clean |
| SC-008 | M01 tests still pass | PASS — `cargo test --test m01` exits 0, 8 tests green, no snapshot drift |

### Implementation findings

- **T013 and T016 absorbed into T008 and T009**: tasks.md mandated implementing happy-path resolver/typechecker first (T008/T009) with `unimplemented!()` for error paths, then filling in errors later (T013/T016). In practice, error paths are interleaved with happy paths (the resolver's `lookup` returns either `Some(id)` or the undeclared-variable error inline; the typechecker's operator rules return either the inferred type or the mismatch error inline). Splitting would have produced ugly partial code. Implemented all error paths in T008/T009; T013/T016 were marked done with no implementation work (only the corresponding test additions in T015/T018, which were carried out as planned).
- **`name_span == decl_span`**: M01's AST stores only an overall span on `LetStmt`, `Param`, and `FnDecl` — no separate span for just the binding name. M02's `BindingDecl` retains both fields (`decl_span`, `name_span`) per data-model.md, but they hold equal values for now. Future diagnostic polish could add `name_span` fields to M01's AST without changing M02's data model. Noted as a soft diagnostic compromise; doesn't affect correctness.
- **Top-level fn name shadowing is permissive**: research.md R-007 catalog didn't include "duplicate top-level function" because the spec edge cases don't mention it. M02 follows the lightweight stance — a duplicate fn at the program level silently shadows. Rust would reject; M02 is pedagogical and tolerates this. Add to deferred bucket if later milestones need stricter behavior.
- **`tests/snapshots/resolves_and_types_shadow.snap`** confirms the key shadowing requirement (spec FR-005, B-3 from contract): two distinct `BindingId`s for the two `let x` statements, with the use of `x` in `let z = x` resolving to the second `x` (Bool), not the first (I32).
- **`indexmap` worth its weight**: first regular dep on the crate (per "deps when needed" feedback). Compile time bumped by ~1s for the initial build of dependencies (indexmap + hashbrown + equivalent). Iteration order in snapshots reads naturally top-down, validating the choice over BTreeMap.
- **Test count expanded beyond tasks.md**: tasks T011/T015/T018 specified 4 + 3 + 9 = 16 test cases. Actual count is 16. No additions during implementation.

### Test summary

```
$ RUSTFLAGS="-D warnings" cargo test --test m02
running 16 tests
test errors_on_annotation_mismatch ... ok
test errors_on_arg_mismatch ... ok
test errors_on_call_arity ... ok
test errors_on_duplicate_param ... ok
test errors_on_first_undeclared ... ok
test errors_on_if_branch_mismatch ... ok
test errors_on_if_cond ... ok
test errors_on_non_fn_call ... ok
test errors_on_non_ident_callee ... ok
test errors_on_op_mismatch ... ok
test errors_on_return_mismatch ... ok
test errors_on_undeclared ... ok
test resolves_and_types_fn_params ... ok
test resolves_and_types_if_expr ... ok
test resolves_and_types_shadow ... ok
test resolves_and_types_simple ... ok

test result: ok. 16 passed; 0 failed; 0 ignored

$ cargo test --test m01
test result: ok. 8 passed; 0 failed; 0 ignored
```

### Conclusion

M02 exit criteria met. Ready to commit. M03 (event model + Level 1 evaluator) can begin once committed.
