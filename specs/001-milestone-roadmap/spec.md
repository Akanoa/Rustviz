# Feature Specification: Reliable Milestone Splitting Plan

**Feature Branch**: `001-milestone-roadmap`
**Created**: 2026-05-20
**Status**: Draft
**Input**: User description: "from Claude.md create a reliable plannification for milestone splitting"

## User Scenarios & Testing *(mandatory)*

The "users" of this feature are the people who interact with the rustviz project itself: the maintainer planning the next chunk of work, contributors picking up a slice without a verbal handoff, and outside observers (potential employers, students, blog readers) trying to understand the project's trajectory. The milestone plan is the artifact that serves all three.

### User Story 1 - Maintainer plans the next slice of work without rederivation (Priority: P1)

The maintainer opens the milestone plan and, without re-reading CLAUDE.md end-to-end or making new design decisions, knows exactly which milestone is next, what is in and out of scope, what files/areas are touched, and what concrete artifact proves it is done. They can start coding within minutes.

**Why this priority**: This is the entire point of the plan. Without it, the maintainer pays the planning cost every session, drifts, and ships nothing. Everything else in this spec is in service of this story.

**Independent Test**: Hand the plan to someone unfamiliar with the project, ask "what is the next thing to build and how will you know when it is done?". They should answer correctly using only the plan and CLAUDE.md, with no further questions.

**Acceptance Scenarios**:

1. **Given** the plan exists in the repo, **When** the maintainer opens it cold after a week away, **Then** they can identify the active milestone and its exit criteria without re-reading CLAUDE.md.
2. **Given** a milestone is "in progress", **When** the maintainer wants to know whether a proposed change belongs to it, **Then** the plan's scope boundaries answer yes/no unambiguously.
3. **Given** a milestone is "done", **When** asked to prove it, **Then** the plan points to a specific demo program / replay trace / test that exercises the milestone end-to-end.

---

### User Story 2 - Each milestone is independently shippable and demoable (Priority: P1)

When a milestone closes, the project is in a coherent demoable state: a beginner can sit in front of rustviz, run the sample programs for that milestone, and learn something concrete about Rust memory — even if every later milestone is still untouched. No milestone leaves the project half-finished or non-functional.

**Why this priority**: A pedagogical visualizer is only valuable when it actually visualizes something. Milestones that end mid-feature ("parser done but no eval yet, nothing to show") break the maintainer's motivation loop and produce no public artifact. Demoability per milestone is what makes the project shippable in slices.

**Independent Test**: For any single milestone in the plan, verify it lists at least one runnable demo (sample `.rs` program + expected replay/output) that exercises the milestone's new capabilities end-to-end. The demo must run on `main` immediately after the milestone closes, without code from later milestones.

**Acceptance Scenarios**:

1. **Given** milestone N has closed, **When** a newcomer clones the repo at that commit, **Then** they can run at least one sample program and see the visualization (or, for pre-UI milestones, the event-stream output) for the features introduced in N.
2. **Given** two consecutive milestones N and N+1, **When** N+1 is removed from the plan entirely, **Then** N still ships and still teaches something useful — N does not depend on unreleased work from N+1.
3. **Given** a milestone introduces a new Rust subset feature (e.g. `&mut`), **When** that milestone closes, **Then** at least one demo program in the repo uses that feature and replays correctly.

---

### User Story 3 - Plan reflects CLAUDE.md scope exhaustively and faithfully (Priority: P2)

Every scope item documented in CLAUDE.md — the 4 Rust subset levels, the 3 architectural layers, the 3 UI panels, the 5-step immediate roadmap — is mapped to exactly one milestone (or explicitly deferred as out-of-scope for v1 with a stated reason). Nothing in CLAUDE.md falls through the cracks; nothing in the plan invents scope CLAUDE.md does not authorize.

**Why this priority**: CLAUDE.md is the source of truth for what rustviz is. A plan that quietly drops `Arc`/`Mutex` or invents a fifth level is no longer a faithful split of the project — it becomes a competing design doc. P2 because the maintainer can catch this manually if P1 and User Story 2 are met, but automating the coverage check prevents drift.

**Independent Test**: Take every scope-bearing bullet in CLAUDE.md (Rust subset items, panel responsibilities, event categories, architectural layers, roadmap steps). For each, locate the milestone in the plan that owns it. Coverage must be 100% (owned or explicitly deferred-with-reason); no orphans on either side.

**Acceptance Scenarios**:

1. **Given** the plan and CLAUDE.md side by side, **When** auditing every scope bullet in CLAUDE.md, **Then** each bullet maps to exactly one milestone or one "deferred / out-of-scope" entry with a stated reason.
2. **Given** a milestone in the plan, **When** asking "where does CLAUDE.md authorize this work?", **Then** at least one CLAUDE.md section / bullet is cited.
3. **Given** CLAUDE.md is updated (e.g. a new level added), **When** the plan is reviewed, **Then** the discrepancy is detectable as an unmapped bullet.

---

### User Story 4 - Dependencies between milestones are explicit and acyclic (Priority: P3)

The plan states, for each milestone, which earlier milestones it strictly depends on (e.g. "Level 2 references depend on lexer/parser support for `&` introduced in Foundation milestone"). The dependency graph is a DAG — no cycles, no surprise prerequisites discovered mid-milestone.

**Why this priority**: Without explicit dependencies, the maintainer picks "Level 3 heap" thinking it is ready, then discovers mid-implementation it needs the borrow-checker scaffolding from Level 2. Stated dependencies prevent these stalls. P3 because if milestones are small and ordered, dependencies are mostly implicit in the order — but stating them defends against re-ordering mistakes.

**Independent Test**: Construct the dependency graph from the plan's stated dependencies. Verify it is acyclic and that the proposed milestone order is a valid topological sort.

**Acceptance Scenarios**:

1. **Given** the plan, **When** drawing the dependency graph, **Then** no cycles exist.
2. **Given** a proposed reordering of two milestones, **When** checking the plan, **Then** the plan immediately reveals whether the swap violates a stated dependency.

---

### Edge Cases

- **Cross-cutting architectural work** (e.g. defining `MemEvent`, building the event replay cursor, the SVG pointer overlay) does not fit cleanly into a single Rust-subset level. The plan must designate "foundation" milestones for this work, ordered before the levels that consume them, rather than smearing the architecture across feature milestones.
- **A milestone turns out too big to ship** (its sizing axes drift to XL during implementation — too many new modules, too many scope bullets, or too many integration boundaries): the plan must allow splitting it in place without renumbering downstream milestones (e.g. "M3a / M3b"). The plan format must not assume milestone count is frozen.
- **A milestone turns out unnecessary or already covered**: the plan must allow marking it "absorbed into M_x" without leaving a dangling number, so the historical record stays readable.
- **CLAUDE.md changes mid-project** (e.g. Level 5 added, a panel responsibility shifts): the plan must be re-auditable against the new CLAUDE.md with the User Story 3 procedure; out-of-date plan entries must be detectable, not silent.
- **Pre-UI milestones have no visual demo**: the demoability requirement (User Story 2) must accept non-visual artifacts (CLI replay of event stream, snapshot test, recorded trace) for milestones before the UI layer lands, with a stated commitment to visual demos from the UI milestone onward.
- **A milestone scope leaks during implementation** ("while I was in there I also added X"): the plan does not need to prevent this in real time, but the next time the plan is reviewed, leaked scope must be either retroactively assigned to the milestone or moved to a follow-up.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The plan MUST exist as a single markdown document in the repository at a stable, known location (e.g. `specs/001-milestone-roadmap/plan.md` or `MILESTONES.md`) so contributors can find it without asking.
- **FR-002**: The plan MUST partition the full scope documented in CLAUDE.md into a finite, ordered list of named milestones, with no scope item left unassigned and no milestone scope unauthorized by CLAUDE.md.
- **FR-003**: Each milestone MUST state, in plain language: a one-sentence goal, in-scope items, out-of-scope items, entry criteria (what must already be true to start), exit criteria (what must be true to call it done), and at least one demo artifact (sample program + expected replay/output, or for pre-UI milestones a CLI/snapshot equivalent).
- **FR-004**: Each milestone MUST be independently shippable: closing milestone N MUST leave the repository in a coherent demoable state without depending on any unreleased code from milestones > N.
- **FR-005**: Each milestone MUST state its dependencies on prior milestones explicitly, by milestone identifier, and the set of dependencies across all milestones MUST form a DAG.
- **FR-006**: The plan MUST preserve the level progression documented in CLAUDE.md (Level 1 → 2 → 3 → 4): no later level's exit criteria may be required before an earlier level closes.
- **FR-007**: The plan MUST include explicit "foundation" milestones for cross-cutting architectural work (event model, interpreter skeleton, replay cursor, UI shell) that does not belong to a single Rust-subset level, ordered before the milestones that consume them.
- **FR-008**: Each milestone MUST be tagged with a complexity bucket S, M, or L, defined by three axes — number of new modules, number of CLAUDE.md scope bullets covered, and number of integration boundaries crossed. Any milestone that would exceed L on any axis (XL) MUST be split before the plan is considered ready. The rubric is recorded in the implementation plan.
- **FR-009**: The plan MUST cite, for each milestone, at least one CLAUDE.md section or bullet that authorizes its scope, so the mapping back to the source of truth is auditable.
- **FR-010**: The plan MUST list explicitly any CLAUDE.md scope item deferred or out-of-scope for the current roadmap, with a one-sentence reason (e.g. "Send/Sync analysis deferred — Level 4 ships with `Arc`/`Mutex` only").
- **FR-011**: The plan MUST support in-place revision (renaming, splitting, merging, marking absorbed) without renumbering downstream milestones, so the historical sequence remains stable across plan edits.
- **FR-012**: The plan MUST be machine-greppable for milestone identifiers (e.g. `M01`, `M02`, …) so contributors and tooling can reference them unambiguously in commits, PRs, and issues.

### Key Entities

- **Milestone**: A named, ordered unit of work with identifier, goal, scope (in/out), dependencies on earlier milestones, entry criteria, exit criteria, demo artifact, and CLAUDE.md citation.
- **Foundation Milestone**: A milestone whose scope is architectural / cross-cutting rather than a Rust-subset feature, ordered before the feature milestones that consume it.
- **Demo Artifact**: A sample Rust program (with expected event stream / replay / visual output) or pre-UI equivalent (snapshot test, CLI trace) that proves a milestone's exit criteria.
- **Scope Bullet**: A discrete claim in CLAUDE.md about what rustviz includes (a level item, a panel responsibility, an event category, an architectural layer, a roadmap step). Each must map to exactly one milestone or one deferred entry.
- **Dependency Edge**: A "milestone B depends on milestone A" relationship; the set of edges forms a DAG.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A contributor unfamiliar with the project can read the milestone plan and CLAUDE.md and correctly identify the next milestone, its scope, and its exit criteria within 10 minutes, with zero follow-up questions to the maintainer.
- **SC-002**: 100% of scope bullets in CLAUDE.md (Rust subset items per level, panel responsibilities, event categories, architectural layers, immediate-roadmap steps) are mapped to exactly one milestone or one explicit "deferred / out-of-scope" entry. Verified by a side-by-side audit.
- **SC-003**: Every milestone in the plan ships with at least one demo artifact runnable on `main` at the milestone's closing commit, exercising the features introduced by that milestone end-to-end.
- **SC-004**: The dependency graph extracted from the plan is acyclic, and the listed milestone order is a valid topological sort of that graph. Verified by inspection on initial drafting and on every plan revision.
- **SC-005**: Every milestone in the plan has a complexity rating of S, M, or L. No milestone is rated XL (defined as exceeding L on any of: new-module count, scope-bullet count, integration-boundary count); milestones that would be XL are split before the plan is approved.
- **SC-006**: Closing any single milestone leaves the project in a coherent demoable state — verified by checking out the closing commit and running that milestone's demo without code from later milestones present.
- **SC-007**: When CLAUDE.md is updated, a re-audit of the plan against the new CLAUDE.md detects every unmapped or newly-orphaned scope bullet in under 15 minutes of manual review.

## Assumptions

- The plan does not commit to wall-clock dates or human-week estimates. Implementation is performed by AI agents under maintainer direction, so sizing is expressed as complexity (S/M/L) rather than time.
- CLAUDE.md, as it stands today, is the authoritative scope source. If CLAUDE.md is silent on a topic, the plan does not invent scope for it — it either defers the topic or proposes a CLAUDE.md amendment first.
- The level progression (Level 1 → 4) documented in CLAUDE.md is correct as a pedagogical sequence and is preserved by the plan rather than re-derived.
- Demoability for pre-UI milestones is satisfied by non-visual artifacts (CLI replay of `MemEvent` stream, snapshot tests, recorded traces). Visual demoability is required from the first UI milestone onward.
- Milestones may in principle be parallelizable wherever the dependency DAG allows it; the plan does not assume strictly serial execution. Order in the document reflects topological order, not required sequence.
- The milestone identifier format (e.g. `M01`, `M02`) is stable and machine-greppable; identifiers are never reused even when a milestone is absorbed or removed.
- This planning artifact does not replace later `/speckit.plan` runs for individual milestones — each milestone, when started, may receive its own implementation plan; the roadmap plan is the index, not the implementation detail.
