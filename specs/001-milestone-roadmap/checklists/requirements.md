# Specification Quality Checklist: Reliable Milestone Splitting Plan

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

- Items marked incomplete require spec updates before `/speckit.clarify` or `/speckit.plan`
- Validation pass 1 (2026-05-20): all items pass. Spec is a meta-deliverable (a planning document), so "implementation details" was scrutinized carefully — references to "markdown document", "milestone identifier format", and "DAG" are properties of the deliverable artifact, not implementation choices for software. Citing CLAUDE.md is a content requirement, not a technology choice.
- The "users" framing is unusual for speckit (maintainer + contributors + outside observers, not end-users of rustviz). This is intentional: the deliverable is a planning artifact whose audience is the project's own contributors. The end-users of rustviz (Rust learners) appear only as the indirect beneficiaries of User Story 2's demoability requirement.

## Post-implementation audit (2026-05-20)

Following completion of `/speckit.implement` tasks T001–T021. `MILESTONES.md` was produced at the repo root with 8 milestone blocks (M01–M08), a dependency graph, and a `## Deferred` bucket. The supporting `scope-inventory.md` was produced under `specs/001-milestone-roadmap/`. Conformance checks against `contracts/milestone-schema.md`:

| Check | Description | Result |
|-------|-------------|--------|
| C-1   | Top-level section order matches contract | PASS |
| C-2   | Unique milestone IDs (M01–M08) | PASS |
| C-3   | Field order Kind → Status → Complexity → Depends on → Authority → Goal → In scope → Out of scope → Entry criteria → Exit criteria → Demo → Notes | PASS in all 8 blocks |
| C-4   | Field values within declared types; Complexity ∈ {S, M, L} with non-XL axes (1 S, 2 M, 5 L, 0 XL) | PASS |
| C-5   | Dependency DAG acyclic; document order is a valid topological sort | PASS (after T016 fix) |
| C-6   | 100% of 29 SB-NNN scope bullets owned by exactly one milestone or deferred | PASS (M03 owns 11 primary; M05 owns 0 primary by design; see scope-inventory.md notes) |
| C-7   | Authority citations point to existing CLAUDE.md sections + verbatim phrases | PASS with normalization note (see below) |
| C-8   | Closed milestones have runnable demos | VACUOUS (all milestones are `planned`; nothing to verify yet) |

### Per-task results (T013–T018)

- **T013 (Demo conformance)** — PASS. M01–M03 use `Format: snapshot`; M04–M08 use `Format: browser`; all browser-format milestones have M04 in transitive closure of `Depends on`; all `Command` lines are runnable from repo root (assuming each milestone's own entry criteria are met — e.g. `cargo test --test m01` requires `Cargo.toml` to exist, which is in M01's entry criteria).
- **T014 (Exit criteria scope-leak check)** — PASS. No milestone's exit criteria references code, behavior, or events outside its `Depends on` transitive closure. M07 explicitly references the M06-introduced blue arrow ("step to `&v[0]`, observe blue arrow into the heap box") — acceptable because M06 is in M07's transitive dependency closure.
- **T015 (CLAUDE.md coverage)** — PASS. All 29 scope bullets in `scope-inventory.md` are owned. M05 has zero primary citations by design (pure-integration milestone); its block cites SB-004 and SB-021 as secondary references, which is acceptable per VR-1 (`authority` requires ≥ 1 citation, not necessarily owned). One typo found and fixed: the inventory's distribution table reported M03=12, actual count is 11.
- **T016 (DAG)** — PASS after fix. Initial draft of `## Dependency graph` only showed the M03→M05 branch from `research.md` while the `Depends on:` data declared M03 as direct dep of M06 and M07 too (each extends MemEvent enum payloads). Graph updated to show M03→{M05, M06, M07, M08} and M08's `Depends on:` updated from `M07` to `M07, M03` for consistency. Acyclic; M01→M02→M03→M04→M05→M06→M07→M08 is a valid topological sort.
- **T017 (Structural C-1/2/3/4)** — PASS. All 8 milestone blocks have all 12 required fields in the contract-specified order. No XL complexity ratings.
- **T018 (Citation validity C-7)** — PASS with normalization rule. All cited section headings exist in CLAUDE.md. Quoted phrases match CLAUDE.md content under the following normalization: code-span backticks (e.g. `` `MemEvent` ``) are dropped from quotes for prose readability. Strict-VR-15 (verbatim substring without backtick normalization) would flag some quotes; the normalization is recorded here as an interpretation, not a bug. If the audit script described in `contracts/milestone-schema.md` is later built, it should normalize backticks.

### Two-meanings clarification ("scope bullet")

The term "scope bullet" appears in two places with different definitions:
- `quickstart.md` audit procedure (T015 input): only bullets under `## Architecture` (layers), `## Event model`, `## The three panels`, `## Supported Rust subset (by levels)`, `## Immediate roadmap`. Used for coverage. Inventoried in `scope-inventory.md` (29 bullets).
- `research.md` R-007 sizing rubric (Complexity axes): any discrete CLAUDE.md bullet, including `## Planned code layout` and `## Locked-in decisions`. Used to derive S/M/L per milestone.

Both definitions are internally consistent for their purpose. Documented here to prevent future confusion. If unified later, prefer the quickstart definition for coverage and a separate "cited-bullet count" term for sizing.

### Conclusion

`MILESTONES.md` conforms to the contract and is ready to commit. Not yet committed — commit is the maintainer's explicit action per project policy. See `git status` and the T021 staging summary.
