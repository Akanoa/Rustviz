# Data Model — Milestone Document Entities

The "data" of this feature is the structured content of `MILESTONES.md`. This file enumerates the entities that document represents, their fields, and the validation rules each must satisfy. The wire format (markdown headings, fields) is in `contracts/milestone-schema.md`.

## Entity: `Milestone`

A single ordered unit of work.

### Fields

| Field             | Type                                                          | Required | Notes                                                                          |
|-------------------|---------------------------------------------------------------|----------|--------------------------------------------------------------------------------|
| `id`              | string matching `^M[0-9]{2}(\.[0-9]+)?[a-z]?$`                | yes      | `M01`–`M99`; optional `.N` revision suffix (e.g. `M03.1` — a protocol revision patching an earlier shipped milestone); optional `a`/`b`/… split-suffix letter. See R-002, R-003.       |
| `title`           | string, ≤ 60 chars                                            | yes      | Human-readable name.                                                          |
| `kind`            | enum `foundation \| feature`                                  | yes      | Foundation = cross-cutting machinery; feature = visible level/capability.     |
| `status`          | enum `planned \| active \| closed \| absorbed \| deferred`    | yes      | Lifecycle. Numbers persist across status changes.                             |
| `goal`            | string, single sentence                                        | yes      | One-sentence outcome statement.                                                |
| `in_scope`        | list of strings                                                | yes      | Bullet list of what this milestone delivers.                                  |
| `out_of_scope`    | list of strings                                                | yes      | Bullet list of what this milestone explicitly does **not** deliver.            |
| `depends_on`      | list of milestone ids                                          | yes      | May be empty (M01). Must reference milestones with smaller id or split suffix.|
| `entry_criteria`  | list of testable statements                                    | yes      | What must already be true before starting.                                    |
| `exit_criteria`   | list of testable statements                                    | yes      | What must be true to call it done.                                            |
| `demo`            | `DemoArtifact`                                                 | yes      | At least one. See entity below.                                               |
| `authority`       | list of `ClaudeMdCitation`                                     | yes      | ≥1. Where in CLAUDE.md this milestone's scope is authorized.                  |
| `complexity`      | enum `S \| M \| L`                                             | yes      | Per the rubric in `research.md` R-007. Never `XL`; XL milestones must split before commit. |
| `sizing_axes`     | object `{ new_modules: int, scope_bullets: int, boundaries: int }` | yes  | The three counts that justify the `complexity` bucket. Lets a reader audit the rating without re-deriving it. |
| `notes`           | free-text                                                      | no       | For open decisions (e.g. editor choice in M04, see Research R-011 open Q).   |

### Validation rules

- **VR-1**: `id` is unique across the document. Once assigned, never reused even if `status == absorbed`.
- **VR-2**: If `status == absorbed`, body MUST contain `absorbed → <id>` redirect and MAY omit all fields except `id`, `status`, redirect.
- **VR-3**: For every id in `depends_on`, the corresponding milestone exists and is `closed | active | planned` (not `absorbed | deferred` — depending on those is a bug).
- **VR-4**: The graph induced by `depends_on` across all `planned | active | closed` milestones is acyclic (SC-004).
- **VR-5**: At least one of `entry_criteria` is non-trivial (i.e. not just "previous milestone closed") for foundation milestones, to defend against scope leak.
- **VR-6**: `exit_criteria` includes at least one statement that mentions a runnable artifact (a test file, a sample `.rs`, or a browser interaction). Pure prose like "code is clean" is not acceptable.
- **VR-7**: `demo` is reachable from `main` at the milestone's closing commit (SC-003, SC-006).
- **VR-8**: `authority` cites only CLAUDE.md (not other docs, not the spec). Citations use section heading + short quoted phrase (R-005).
- **VR-9**: `complexity` is `S`, `M`, or `L` and matches the bucket implied by `sizing_axes` per the R-007 rubric (`S`: 1 module, ≤2 bullets, 0–1 boundaries; `M`: 2–3 modules, 3–5 bullets, 1–2 boundaries; `L`: 3–4 modules, 5–8 bullets, 2+ boundaries). Any milestone whose `sizing_axes` exceed `L` on any axis (XL) MUST be split before the document is committed.
- **VR-10**: `kind == foundation` MUST appear earlier in the topological order than any `kind == feature` milestone that depends on it.

## Entity: `DemoArtifact`

Proof that a milestone closes successfully.

### Fields

| Field        | Type                              | Required | Notes                                                            |
|--------------|-----------------------------------|----------|------------------------------------------------------------------|
| `format`     | enum `snapshot \| browser`        | yes      | `snapshot` = pre-UI test artifact (R-004). `browser` = M04+.    |
| `inputs`     | list of file paths                | yes      | The `.rs` sources / inputs that drive the demo.                  |
| `outputs`    | list of file paths or descriptions| yes      | Expected `.snap` files, or for browser: list of expected steps.  |
| `command`    | string                            | yes      | What a contributor runs to reproduce (e.g. `cargo test M03`).    |

### Validation rules

- **VR-11**: `format == snapshot` only valid for `M01`, `M02`, `M03` (the pre-UI milestones — R-004).
- **VR-12**: `format == browser` requires `M04` or later in `depends_on` transitive closure.
- **VR-13**: `command` is executable from repo root with no extra setup (or any setup is itself a one-liner in `entry_criteria`).

## Entity: `ClaudeMdCitation`

A reference back to the authoritative scope source.

### Fields

| Field      | Type   | Required | Notes                                                           |
|------------|--------|----------|-----------------------------------------------------------------|
| `section`  | string | yes      | The CLAUDE.md heading path, e.g. `Supported Rust subset (by levels)`. |
| `quote`    | string | yes      | A short verbatim phrase from that section, ≤ 80 chars.          |

### Validation rules

- **VR-14**: `section` exists as a heading in the current `CLAUDE.md`.
- **VR-15**: `quote` appears verbatim somewhere under `section` in the current `CLAUDE.md` (substring match; whitespace normalized).
- **VR-16**: If VR-14 or VR-15 fails on a re-audit, the milestone is flagged in the audit report; fix is either to update the citation or to acknowledge that CLAUDE.md changed scope (and possibly add/remove milestones — SC-007).

## Entity: `DependencyEdge`

Implicit. An edge `B depends_on A` exists wherever `A ∈ B.depends_on`. Edges are not stored as separate records; they are derived. The DAG check (VR-4) walks `depends_on` lists.

## Entity: `DeferredItem`

Scope acknowledged in CLAUDE.md (or adjacent to it) that the current roadmap will not deliver.

### Fields

| Field      | Type   | Required | Notes                                                           |
|------------|--------|----------|-----------------------------------------------------------------|
| `title`    | string | yes      | What is deferred.                                                |
| `reason`   | string | yes      | One sentence: why this is not in v1.                            |
| `authority`| `ClaudeMdCitation` | no | If deferral relates to a specific CLAUDE.md section.            |

### Validation rules

- **VR-17**: A deferred item MUST NOT be referenced by any active milestone's `exit_criteria`.

## State machine: `Milestone.status`

```
                         (initial)
                            │
                            ▼
                       ┌────────┐
                       │planned │
                       └───┬────┘
                           │ start
                           ▼
                       ┌────────┐
            ┌──────────┤ active ├──────────┐
            │          └───┬────┘          │
            │              │ exit criteria │ split / merge
            │              │ all true      │
            │              ▼               ▼
            │          ┌────────┐     ┌─────────┐
            │          │ closed │     │absorbed │
            │          └────────┘     └─────────┘
            │ deferred during planning
            ▼
       ┌─────────┐
       │ deferred│
       └─────────┘
```

- `planned → active`: when work begins.
- `active → closed`: when all `exit_criteria` are demonstrably met (demo runs, tests pass, etc.).
- `active → absorbed` or `planned → absorbed`: when scope is folded into another milestone. Body becomes a redirect.
- `planned → deferred`: when scope is acknowledged but cut from the current roadmap.
- `closed` is terminal. `deferred` and `absorbed` are terminal in their lane (can be revived only by adding a new milestone elsewhere).
