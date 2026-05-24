# Specification Quality Checklist: M08 — threads, Arc, Mutex

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-05-24
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

- Validation pass 1 (2026-05-24): all items pass.
- **Closes Level 4** — M08 ships concurrency (threads + Arc + Mutex). After M08, the entire Level 4 surface (M07 heap, M07.1 slices, M07.2 strings, M07.3 arrays, M07.4 structs, M07.5 generics, M07.6 traits, M07.7 trait objects, M08 concurrency) is complete. CLAUDE.md doesn't define Levels 5+.
- **Four user stories** — US1+US2+US3 P1 (foundational: spawn/join + Arc clone + Mutex lock with parking); US4 P2 (canonical Arc<Mutex<T>> synthesis, the SHIP-DEFINING headline).
- **Headline pedagogy** — making shared state under exclusive lock visible: two stack columns, two dashed-purple Arc arrows pointing at one shared mutex heap block, parked-thread visual when contention happens. The full canonical "share T between threads safely" pattern with every visualization layer engaged simultaneously.
- **First multi-execution-context milestone** — the stacks panel grows from one column to N (one per live thread). Execution branches at `thread::spawn` and rejoins at `.join()`.
- **Pure-additive Value extensions** — `Value::Arc`, `Value::Mutex`, `Value::MutexGuard` added; `ThreadId` newtype added (analog of M07.7's `VtableAddr`). NO new `MemEvent` variants — the 7 thread+sync stubs (`ThreadSpawn`, `ThreadJoin`, `ThreadPark`, `ArcClone`, `ArcDrop`, `LockAcquire`, `LockRelease`) were pre-declared at M03 and M08 finally fills them in. Honors M03's "closed enum" promise: the variants existed all along; M08 emits them.
- **Meaty UI surface** — analog of M07.4's struct view and M07.7's VTABLES panel. Multi-column stacks (slide-in animation on spawn), parked-thread visual (column grey + dotted line to held mutex), dashed-purple Arc arrows, refcount display in heap blocks. **UX checkpoint expected** after first cut, per the M07.4/M07.7 precedent.
- **Sized L per MILESTONES.md but borderline-XL** — MILESTONES.md note: "Borderline-XL by per-event counting (9 atomic events across Threads + Sync categories) but L by event-category counting per the rubric. If sizing-axis tracking reveals XL during implementation, split into `M08a` (threads + multi-column stacks) and `M08b` (Arc / Mutex / sync + dashed purple overlay)." Plan phase decides whether to ship as one or split.
- **Deterministic scheduling** — per FR-013, multi-threaded programs produce byte-identical event streams across runs. Thread interleaving is fixed (e.g. round-robin per cursor step, or sequential by spawn order with explicit yield-points at `lock()`/`join()`). Plan-phase to confirm the exact rule.
- **No `Send`/`Sync` typeck** — per MILESTONES.md Deferred entry. The visualizer accepts programs that capture non-Send data; full auto-trait inference is rustc-grade work and out of scope.
- **Move-only closures** — `thread::spawn` accepts `move || { body }` only. Non-move closures (borrowing captures) require cross-thread borrow checking; out of scope.
- **Minimal closure surface** — closures appear ONLY as `thread::spawn` arguments in M08. No `Fn`/`FnMut`/`FnOnce` trait dispatch, no closure type inference beyond the spawn-arg shape.
- **Tight rejections kept tractable** — out of scope: poisoned mutex, `try_lock`, `RwLock`, `Condvar`, atomics, channels (`mpsc`), `async`/`await`, scoped threads (`thread::scope`), `Weak<T>`, nested `Mutex<Arc<T>>`, panic propagation across `join()`, fn-pointer `spawn` args.
- **Refcount display on heap block** — Arc's strong count rendered as `[refs: N]` suffix on the heap block's display string. No separate refcount UI panel. The display update is driven by `ArcClone`/`ArcDrop` events.
- **Mutex state on heap object** — `HeapObject::Mutex { value, holder: Option<ThreadId> }`. `LockAcquire`/`LockRelease` events mutate `holder`. `holder == None` means lock free; `Some(tid)` means held by thread `tid`.
- **MutexGuard as a stack slot** — `let g = m.lock();` produces a slot named `g` typed `MutexGuard<T>`. The guard's Drop is visualized as a `LockRelease` event when the slot goes out of scope.
- **Hover-only arrows continue** — per post-M07.7 `[[feedback-arrow-viz-rules]]` Rule 1, dashed-purple Arc arrows default to hover-only. Hovering a binding holding an Arc reveals its arrow to the shared heap block.
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION because reasonable defaults exist):
  1. **Thread scheduling rule** — round-robin per step vs sequential-by-spawn-order with yield-points. Recommendation: sequential-by-spawn-order with explicit yields at `lock()` (to surface contention) and `.join()` (to wait for child).
  2. **Multi-column stacks layout** — equal-width columns, horizontal scroll if N > some threshold; OR fluid columns shrinking to fit. Recommendation: equal-width with horizontal scroll past 3 columns (M08 samples target 2-3 threads).
  3. **Dashed-purple Arc arrow style** — iterative; UX checkpoint after first cut. Recommendation: dashed purple, same dash pattern as M07.7's orange dispatch arrows for visual family consistency.
- **WASM bundle target ≤ +25%** — substantial new surface (multi-column UI + Arc/Mutex/Guard value variants + parked-thread visual + refcount display + dashed-purple arrows). Comparable to M07.4 and M07.7 in UI investment.
- **Foundation completion** — after M08, Level 4 is complete. CLAUDE.md doesn't define Levels 5+; nothing is being deferred to a hypothetical M09 since no such scope claim exists.
