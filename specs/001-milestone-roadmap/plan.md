# Implementation Plan: Reliable Milestone Splitting Plan

**Branch**: `001-milestone-roadmap` | **Date**: 2026-05-20 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/001-milestone-roadmap/spec.md`

## Summary

Produce a single repo-level markdown document (`MILESTONES.md`) that partitions the rustviz scope documented in `CLAUDE.md` into a finite, ordered, DAG-shaped list of milestones. Each milestone carries goal, in/out scope, dependencies, entry/exit criteria, demo artifact, and a citation back to CLAUDE.md. The plan also delivers a milestone-entry schema (`contracts/milestone-schema.md`) and an audit procedure (`quickstart.md`) so the document can be re-verified mechanically when CLAUDE.md changes.

This feature has no runtime code. Its deliverables are: (1) the milestone roadmap document, (2) the entry schema acting as a contract on that document, and (3) the audit / re-audit procedure. The "tests" for this feature are an audit script and a manual coverage check, both described in the contract.

## Technical Context

**Language/Version**: N/A — deliverable is markdown documentation
**Primary Dependencies**: `CLAUDE.md` (authoritative scope source); `specs/001-milestone-roadmap/spec.md` (this feature's spec)
**Storage**: filesystem, version-controlled in git
**Testing**: manual audit of milestone coverage against CLAUDE.md scope bullets; optional shell script that greps CLAUDE.md scope markers and verifies each is referenced in `MILESTONES.md` (described in `contracts/milestone-schema.md`)
**Target Platform**: any markdown reader (editor, GitHub web, `cat`)
**Project Type**: documentation / planning artifact (no source code)
**Performance Goals**: a new reader can identify the active milestone and its exit criteria within 10 minutes (SC-001)
**Constraints**: 100% coverage of CLAUDE.md scope bullets (SC-002); every milestone rated S, M, or L per the complexity rubric — no XL (SC-005); dependency graph acyclic (SC-004). Implementation is by AI agents under maintainer direction; no human-week sizing.
**Scale/Scope**: 8 milestones (M01–M08) — 4 foundation (M01–M04: front-end, resolve/typeck, event model + L1 eval, UI shell) and 4 feature (M05–M08: live L1, references, heap, threads). Complexity mix: 1 S, 2 M, 5 L

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` is the unfilled speckit template (placeholders `[PRINCIPLE_1_NAME]` etc., no ratified principles). There are no concrete constitutional gates to evaluate.

**Decision**: PASS by vacuity. No principles defined → no violations possible. Re-check after Phase 1 is also vacuous.

**Note to maintainer**: when constitution principles are eventually filled in (likely things like "pedagogical readability beats optimization", "interpreter never writes to UI directly", "every level ships with at least one demo program"), this plan and future plans must re-evaluate. The current milestone roadmap deliverable is consistent with the principles CLAUDE.md already implies, so a future filled-in constitution is unlikely to invalidate it.

## Project Structure

### Documentation (this feature)

```text
specs/001-milestone-roadmap/
├── plan.md                 # This file
├── spec.md                 # Feature spec (already exists)
├── research.md             # Phase 0: milestone breakdown decisions
├── data-model.md           # Phase 1: Milestone entity schema
├── quickstart.md           # Phase 1: how to read / audit / revise MILESTONES.md
├── contracts/
│   └── milestone-schema.md # Phase 1: the format MILESTONES.md must follow
├── checklists/
│   └── requirements.md     # Already exists (from /speckit.specify)
└── tasks.md                # NOT created here — /speckit.tasks output
```

### Source Code (repository root)

This feature produces a documentation artifact at the repo root, not source code:

```text
MILESTONES.md               # The deliverable (created by /speckit.tasks → implementation)
CLAUDE.md                   # Source of truth for scope (already exists, untouched)
specs/001-milestone-roadmap/  # This feature's planning artifacts (see above)
```

**Structure Decision**: documentation-only feature. The single deliverable file is `MILESTONES.md` at the repository root (chosen over `specs/001-milestone-roadmap/milestones.md` so contributors find it without digging — see `research.md` Decision R-002). No `src/` or `tests/` changes are required for this feature. When milestone implementation begins, each milestone will get its own `specs/NNN-milestone-X/` directory via a fresh `/speckit.specify` run.

## Complexity Tracking

> No constitutional violations. Table omitted.
