# Research — Milestone Splitting Decisions

This file records the concrete design decisions that the milestone roadmap document (`MILESTONES.md`) will encode. The spec deliberately did not commit to a milestone count or content; this phase resolves those. Each decision is recorded as Decision / Rationale / Alternatives.

## Document-level decisions

### R-001 — `MILESTONES.md` location

- **Decision**: Place the deliverable at the repository root as `MILESTONES.md`.
- **Rationale**: FR-001 requires a "stable, known location". Root-level `MILESTONES.md` is the convention contributors check first (next to `README.md`, `CLAUDE.md`). It is the shortest path to "find without asking" (User Story 1).
- **Alternatives considered**:
  - `specs/001-milestone-roadmap/milestones.md` — buried, requires the contributor to know the speckit naming convention. Rejected.
  - `docs/MILESTONES.md` — no `docs/` directory exists yet; creating one for a single file is over-structured. Rejected.
  - `README.md` "Roadmap" section — couples the public README to internal planning that changes often. Rejected.

### R-002 — Milestone identifier format

- **Decision**: `M01`, `M02`, …, `M99`. Two-digit zero-padded. Never reused.
- **Rationale**: FR-012 requires machine-greppable IDs. Two digits give >5× capacity over CLAUDE.md's currently-planned ~8 milestones without sorting weirdness ("M10" sorting before "M2" textually). Zero-padding makes `grep -E '\bM[0-9]{2}\b'` precise.
- **Alternatives considered**:
  - Single digit (`M1`–`M9`) — collides with `M10+` in grep. Rejected.
  - `MS-001` / `MILESTONE-1` — verbose, less common in commit messages. Rejected.
  - Semantic names (`m-parser`, `m-events`) — non-stable when milestones split (does `m-parser` become `m-parser-lex` and `m-parser-syntax`?). Rejected.

### R-003 — Handling splits, merges, deferrals, absorption

- **Decision**: Numbers are stable forever. A milestone is `active | closed | absorbed | deferred`. Splits append a letter (`M06a`, `M06b`) and leave the parent entry as `absorbed → M06a, M06b` with a one-line note. Merges leave one survivor; the absorbed milestone retains its number with `absorbed → M0x`.
- **Rationale**: FR-011 + edge cases require in-place revision without renumbering. Stable numbers make commits/PRs/issues survive plan edits.
- **Alternatives considered**:
  - Renumber on every revision — breaks every external reference. Rejected.
  - Use UUIDs — unreadable in PR titles. Rejected.

### R-004 — Pre-UI demoability artifact format

- **Decision**: For milestones M01–M03, "demo artifact" = a snapshot test under `tests/snapshots/` (one `.rs` input + expected `.snap` output via `insta` or hand-written assertion). For M04 onward (UI exists), demo artifact = a sample `.rs` program plus an expected browser interaction described as a short list of steps (e.g. "click play, observe slot `x` highlight at step 3").
- **Rationale**: FR-003 and edge case "pre-UI milestones have no visual demo" require accepting non-visual artifacts before the UI lands. Snapshot tests are the leanest way to make pre-UI progress demoable and regression-resistant.
- **Alternatives considered**:
  - Require browser visuals from M01 — impossible, no UI yet. Rejected.
  - Use plain `assert_eq!` tests instead of snapshots — works but loses the "look at the output" quality that snapshots give a reader scanning a milestone closure. Snapshots win on demo readability.

### R-005 — CLAUDE.md citation format inside `MILESTONES.md`

- **Decision**: Each milestone's `**Authority**:` line cites CLAUDE.md by section heading + a short quoted phrase, not by line number. Example: `**Authority**: CLAUDE.md › Supported Rust subset › "Level 1: primitives, let/let mut, functions, scopes, moves…"`.
- **Rationale**: FR-009 requires citation. Line numbers in CLAUDE.md churn on every edit and would silently rot. Section + quote survives reformatting.
- **Alternatives considered**:
  - Line numbers — brittle. Rejected.
  - Just section names — loses precision when a section bullets several distinct items. Rejected.

## Milestone breakdown

### R-006 — Foundation vs feature milestones

- **Decision**: Four foundation milestones (M01–M04) deliver the cross-cutting machinery; four feature milestones (M05–M08) deliver visible value to learners by exercising one architectural layer or one Rust level at a time. Edge case "cross-cutting architectural work" (FR-007) is handled by making the foundations explicit milestones rather than dissolving them into feature milestones.
- **Rationale**: CLAUDE.md describes three architectural layers (interpreter, event stream, UI). Each layer needs at least one foundation milestone before any feature milestone can demo it. Lumping foundation work into feature milestones violates SC-005 sizing.
- **Alternatives considered**:
  - One mega-foundation (M01: "everything below the UI") — busts every axis of the complexity rubric (8+ new modules, 8+ scope bullets, multiple boundaries → XL). Rejected.
  - No foundations; build everything per-level — moves architectural work to L1 silently, makes L1 4× larger than L4. Rejected.

### R-007 — Complexity rubric

Implementation is performed by AI agents, so sizing is in complexity buckets rather than human-weeks. The rubric is:

| Bucket | New modules | CLAUDE.md scope bullets | Integration boundaries |
|--------|-------------|--------------------------|------------------------|
| **S**  | 1           | ≤ 2                      | 0–1                    |
| **M**  | 2–3         | 3–5                      | 1–2                    |
| **L**  | 3–4         | 5–8                      | 2+                     |
| **XL** | exceeds L on any axis — MUST split |       |                        |

Counting conventions:
- A "new module" is a new top-level Rust source file or a new UI component file. Extending an existing module does not count.
- A "scope bullet" is a discrete bullet in CLAUDE.md (a level's bullet, a panel's bullet, an event-category bullet, a roadmap step). Event-category bullets are counted as one unit even though each lists multiple events.
- An "integration boundary" is a place where this milestone's output is consumed by, or consumes, another component (e.g. eval → event stream, WASM → DOM, lexer → parser).

### R-007.1 — Proposed milestone list (concrete)

The roadmap will encode the following eight milestones plus a deferred bucket.

| ID  | Title                                | Kind       | Complexity | Demo                                                         |
|-----|--------------------------------------|------------|------------|--------------------------------------------------------------|
| M01 | Frontend skeleton (lexer + parser)   | Foundation | L          | Snapshot tests of AST for 3 sample L1 programs               |
| M02 | Name resolution + lightweight typeck | Foundation | M          | Snapshot tests + error-message tests                         |
| M03 | Event model + Level 1 evaluator      | Foundation | M          | CLI dump of `MemEvent` stream for L1 programs                |
| M04 | UI shell + replay cursor             | Foundation | L          | Browser replay of a pre-recorded L1 trace                    |
| M05 | Live Level 1 (edit → run → watch)    | Feature    | S          | Live editing of L1 program with live stack visualization     |
| M06 | Level 2 — references and borrows     | Feature    | L          | Visual blue/red borrow arrows; aliasing violations flagged   |
| M07 | Level 3 — heap (Box, Vec, String)    | Feature    | L          | `&v[0]`-after-`push` shows arrow snap / UB note              |
| M08 | Level 4 — threads (Arc, Mutex)       | Feature    | L          | Two-thread `Arc<Mutex<T>>` with contention visible           |

Complexity distribution: 1 S, 2 M, 5 L, 0 XL. No splits required at planning time.

- **Rationale**:
  - The 4 levels in CLAUDE.md map naturally to 4 feature milestones (M05–M08). M05 is L1 because L1 is the smallest level (no references) and needs to follow M04 to be demoable.
  - L1 is split across M01–M03 because the interpreter front-end work is foundation, not L1-specific. Lexer/parser/resolver/typeck will be extended in every later milestone too; they do not "belong" to L1.
  - M04 is its own milestone because UI shell work (WASM bindings, editor embed, replay cursor) crosses two integration boundaries (Rust↔WASM, WASM↔DOM) and shares nothing with interpreter milestones.
  - M05 is intentionally S (pure glue) because it is the milestone where the project becomes publicly demoable for the first time. Small + early is good for momentum and gives an early checkpoint before the larger level milestones.
  - M01 is L (4 new modules: span, lexer, ast, parser) even though each module is individually small — the 4-module count alone pushes it into L per the rubric. Considered splitting into "span+lexer" / "ast+parser" but the parser is only meaningful when ast exists and the lexer is only useful when there's a parser to consume it; splitting would create non-shippable halves.
  - M08 looks like a borderline XL by event-event counting (3 Thread events + 4 Sync events + L4 level + multi-thread Stacks = 9 atoms), but the rubric counts event categories as single bullets — giving Threads(1) + Sync(1) + L4(1) + Stacks-multi-col(1) = 4 bullets, comfortably L.
- **Alternatives considered**:
  - Merge M05 into M04 — M04 grows to ~6 new modules + 3 boundaries, becomes XL on the module axis. Rejected.
  - Skip M05; demo L1 inside M04 directly — M04 then requires interpreter integration, which couples UI delivery to interpreter completeness. Rejected.
  - Split M03 into "event enum" + "L1 evaluator" — each half becomes S (1 module, ≤2 bullets, 0–1 boundary) and the event-enum half has no useful demo on its own (no events get emitted). Rejected.
  - Split M06 into "borrow tracking" + "pointer overlay" — would work; leave as an in-place split option (M06a / M06b) if mid-implementation it bloats past L on any axis.

### R-008 — Pointer overlay introduction strategy

- **Decision**: Introduce the SVG pointer overlay infrastructure in M06 (when references first need arrows). Extend its color vocabulary in M07 (add black for owning `Box`/`Vec`/`String`) and M08 (add dashed purple for `Arc`/`Rc`). The overlay is never its own milestone.
- **Rationale**: An overlay with no pointers to draw has no demoable value, so it cannot stand alone (violates SC-006). Tying it to M06 means M06's demo doubles as the overlay's first demo.
- **Alternatives considered**:
  - Standalone "pointer overlay" milestone — no demoable artifact without something to point at. Rejected.

### R-009 — Realloc animation belongs in M07

- **Decision**: The `HeapRealloc` animation is part of M07's exit criteria, not split out.
- **Rationale**: It is *the* pedagogical wow moment for Level 3 (`&v[0]` after `push`). Splitting it weakens M07's demo to "Vec exists but nothing exciting happens". Independent shippability (User Story 2) demands M07 ship with the realloc visual.
- **Alternatives considered**:
  - Ship M07 without realloc animation, add later — M07 demo becomes flat. Rejected.

### R-010 — Pedagogy / `Note` events placement

- **Decision**: Introduce the `Note { kind, message, span }` event variant in M03 (with the rest of the event enum). Each later milestone can emit Notes from its own eval paths. There is no dedicated "pedagogy" milestone.
- **Rationale**: Notes are infrastructure; they need to exist before any milestone can emit them. Each level needs its own Notes (move-after-use, dangling-borrow, realloc-invalidates, lock-poisoned). Centralizing in M03 keeps the event enum coherent.
- **Alternatives considered**:
  - Dedicated "pedagogy polish" milestone after M08 — defers Notes for the whole project; learners get a worse experience in the meantime. Rejected.

### R-011 — Deferred / out-of-scope items

These CLAUDE.md-adjacent topics are explicitly deferred. They will appear in `MILESTONES.md` under a `## Deferred` heading with a one-sentence reason each, satisfying FR-010.

- **Detailed `Send`/`Sync` inference** — M08 ships `Arc<Mutex<T>>` happy-path only; full auto-trait inference and error messages are deferred. Reason: full inference is rustc-grade work, out of scope for a pedagogical visualizer.
- **Parser error recovery** — CLAUDE.md decision is "stop at first parse error". Continue stopping; recovery deferred. Reason: enough for a live editor, smaller scope.
- **Multi-file support** — spans already carry `FileId` (CLAUDE.md), but the level milestones target single-file programs. Multi-file UI deferred. Reason: complicates Editor panel without proportional pedagogical gain.
- **Lifetime visualization beyond scope-level** — CLAUDE.md L2 is "scope-level lifetimes". Generic/named lifetimes (`<'a>`) deferred. Reason: out of L2 scope.
- **Other levels beyond L4** — closures, trait objects, `unsafe`, `async`. None in CLAUDE.md; not introduced here.

## Dependency graph

```
M01 ──► M02 ──► M03 ──► M04 ──► M05 ──► M06 ──► M07 ──► M08
                  │              ▲
                  └──────────────┘   (M05 also depends on M03 directly)
```

Acyclic ✓. The drawn order is one valid topological sort. Verified manually:

- M01 has no dependencies.
- M02 depends on M01 (resolver works on AST).
- M03 depends on M02 (evaluator needs resolved + typed AST) and transitively M01.
- M04 depends on M03 (UI replays the trace format defined in M03).
- M05 depends on M04 (UI exists) and M03 (interpreter exists).
- M06 depends on M05 (need live demoable system to extend) and M03 (event enum extensions land in M06 too).
- M07 depends on M06 (pointer overlay) and M03 (heap events).
- M08 depends on M07 (full pointer overlay, including arrows into shared heap state).

Satisfies SC-004 (acyclic) and User Story 4 (explicit dependencies).

## Open question — not blocking

- **Editor choice**: Monaco vs CodeMirror (CLAUDE.md says "Monaco or CodeMirror"). This is internal to M04 and does not affect milestone splitting. The milestone document will record it as an open decision to be made when M04 starts; not surfaced as a NEEDS CLARIFICATION because it does not change milestone boundaries.
