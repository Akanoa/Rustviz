# CLAUDE.md Scope-Bullet Inventory

**Source**: `CLAUDE.md` as of 2026-05-20.
**Purpose**: canonical list of every scope-bearing bullet in CLAUDE.md, used by the T015 coverage audit to verify every bullet is claimed by exactly one milestone in `MILESTONES.md` or appears under `## Deferred`.

**Sections inventoried** (per `quickstart.md` step 1):
- `## Architecture` — layers + core principles
- `## Event model` — event categories + structural commitments
- `## The three panels` — one bullet per panel
- `## Supported Rust subset (by levels)` — one bullet per level
- `## Immediate roadmap` — one bullet per numbered step

Sections **not** inventoried (not scope-bearing per the quickstart definition): `## Pedagogical goal`, `## Planned code layout`, `## Locked-in decisions`, `## Notes for Claude`. Milestone `Authority` lines may still cite them as context.

## Inventory

| ID      | Section                                  | Quote (verbatim short phrase)                                                                              | Primary owner | Notes |
|---------|------------------------------------------|------------------------------------------------------------------------------------------------------------|---------------|-------|
| SB-001  | Architecture                             | "UI (web, WASM bindings)"                                                                                  | M04           |       |
| SB-002  | Architecture                             | "Event stream (MemEvent[])"                                                                                | M03           |       |
| SB-003  | Architecture                             | "Interpreter (Rust → WASM)"                                                                                | M01           | Extended in M02, M03 |
| SB-004  | Architecture                             | "the interpreter never writes to the UI directly. It emits a typed event stream"                           | M03           | Secondarily referenced by M05 (live round-trip); primary in M03 where the boundary is implemented |
| SB-005  | Architecture                             | "The UI replays the stream with a cursor (play / pause / step / rewind)"                                   | M04           |       |
| SB-006  | Event model                              | "MemEvent is the centerpiece"                                                                              | M03           |       |
| SB-007  | Event model                              | "Threads: ThreadSpawn, ThreadJoin, ThreadPark"                                                             | M08           |       |
| SB-008  | Event model                              | "Frames: FrameEnter, FrameLeave"                                                                           | M03           |       |
| SB-009  | Event model                              | "Stack slots (bindings): SlotAlloc, SlotWrite, SlotMove, SlotDrop"                                         | M03           |       |
| SB-010  | Event model                              | "Heap: HeapAlloc, HeapRealloc, HeapFree"                                                                   | M07           |       |
| SB-011  | Event model                              | "Borrows: BorrowShared, BorrowMut, BorrowEnd"                                                              | M06           |       |
| SB-012  | Event model                              | "Synchronization: LockAcquire, LockRelease, ArcClone, ArcDrop"                                             | M08           |       |
| SB-013  | Event model                              | "Pedagogy: Note { kind, message, span }"                                                                   | M03           | Infrastructure lands here; later milestones emit Notes |
| SB-014  | Event model                              | "Every event carries a SourceSpan"                                                                         | M03           |       |
| SB-015  | Event model                              | "Pointee is an enum Slot(SlotId) | Heap(HeapAddr)"                                                         | M03           |       |
| SB-016  | Event model                              | "SlotMove is intentionally distinct from SlotDrop"                                                         | M03           |       |
| SB-017  | The three panels                         | "Editor (Monaco or CodeMirror)"                                                                            | M04           |       |
| SB-018  | The three panels                         | "Stacks: one column per thread"                                                                            | M04           | Single-column in M04; multi-column extension in M08 |
| SB-019  | The three panels                         | "Heap: free-form area where each HeapAlloc creates a box"                                                  | M07           |       |
| SB-020  | The three panels                         | "Pointers: SVG overlay across the panels"                                                                  | M06           | Blue/red in M06; black in M07; dashed purple in M08 |
| SB-021  | Supported Rust subset                    | "Level 1: primitives, let/let mut, functions, scopes, moves of non-Copy types"                             | M03           | Demoable end-to-end in M05 |
| SB-022  | Supported Rust subset                    | "Level 2: & and &mut, aliasing rules, scope-level lifetimes"                                               | M06           |       |
| SB-023  | Supported Rust subset                    | "Level 3: Box, Vec (with visible realloc), String"                                                         | M07           |       |
| SB-024  | Supported Rust subset                    | "Level 4: thread::spawn, Arc, Mutex, Send/Sync"                                                            | M08           | Full Send/Sync inference deferred — see `## Deferred` in MILESTONES.md |
| SB-025  | Immediate roadmap                        | "Integrate the parse/ skeleton"                                                                            | M01           |       |
| SB-026  | Immediate roadmap                        | "Name resolver: Ident → BindingId"                                                                         | M02           |       |
| SB-027  | Immediate roadmap                        | "Lightweight typeck: validate annotations, propagate obvious types"                                        | M02           |       |
| SB-028  | Immediate roadmap                        | "Define MemEvent and write the level-1 evaluator"                                                          | M03           |       |
| SB-029  | Immediate roadmap                        | "First UI prototype: single stack panel, static replay of a pre-recorded trace"                            | M04           |       |

**Total**: 29 scope bullets.

## Distribution check

| Owner | Count |
|-------|-------|
| M01   | 2     |
| M02   | 2     |
| M03   | 11    |
| M04   | 5     |
| M05   | 0 primary (secondary cite of SB-004 and SB-021) |
| M06   | 3     |
| M07   | 3     |
| M08   | 3     |
| Deferred | 0 (partial: SB-024's `Send`/`Sync` inference) |
| **Total** | 29 |

M03 carries the most bullets because the event model defines 11 of CLAUDE.md's 29 scope bullets. This is expected — the event stream is the architectural pivot, and CLAUDE.md describes it in the most detail.

M05 has zero primary citations. This is intentional and not a violation: M05 is a pure-integration milestone that re-uses M03 (Level 1 evaluator) and M04 (UI shell) to produce the project's first live demo. Per the data-model.md `Milestone.authority` rule (VR ≥ 1), M05's milestone block must still cite at least one CLAUDE.md bullet — it does so by citing SB-004 (the core round-trip principle) and SB-021 (Level 1) as secondary references. The audit log records this dual-citation as expected behavior, not as a coverage violation.
