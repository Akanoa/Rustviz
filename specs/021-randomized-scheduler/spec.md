# Feature Specification: Randomized (seeded) thread scheduler

**Feature Branch**: `021-randomized-scheduler`
**Created**: 2026-05-29
**Status**: Draft
**Input**: User description: "The actual threading mechanism is deterministic giving the same result at each run. But a real threaded code is partially randomized. The thread T1 is picked the T2 parked, or T2 picked and T1 parked. This produces totally different shaping regarding events. To materialize this a random seed can be used which allow randomized but deterministic behaviors."

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Seeded scheduling produces one specific interleaving (Priority: P1)

A learner loads the `Arc<Mutex>` sample and steps through the trace. The two threads (main + spawned closure) run in some specific order: a particular thread acquires the lock first, the other waits, etc. The order looks plausible — not always "spawned thread runs to completion before main proceeds." Same seed across reloads gives the same trace, so the learner can re-walk the same execution as many times as needed.

**Why this priority**: This is the foundation — without a seeded scheduler, every other story is impossible. The learner's first impression of "what threads look like" hinges on the default trace not looking artificial.

**Independent Test**: Load the M08 `Arc<Mutex>` sample, observe the recorded interleaving. Reload the page and verify the same interleaving plays back identically. The interleaving should differ from M08 v1's strictly-deferred order (where every spawned thread runs only at `join()`).

**Acceptance Scenarios**:

1. **Given** the page just loaded with the default seed, **When** the learner picks a multi-threaded sample, **Then** the trace shows interleaved thread steps consistent with one plausible thread schedule (some closure work happens before main reaches `join()`).
2. **Given** the learner is at step N of a multi-threaded sample, **When** they reload the page and step to step N again, **Then** the visualization shows the exact same events at the exact same step indices (deterministic replay).
3. **Given** the closure depends on a Mutex held by the main thread, **When** the scheduler picks the closure first, **Then** the closure parks on the lock and the visualization shows it parked until the main thread releases.

---

### User Story 2 — Learner can change the seed to see a different interleaving (Priority: P1)

The learner enters a seed value (or clicks a control to set one) and the trace re-runs with a different but still deterministic schedule. The same code now produces visibly different events: maybe the spawned closure runs to completion BEFORE main reaches `join()`, or vice versa. The point lands: "the same Rust program has many valid executions."

**Why this priority**: This is the pedagogical payoff. Without user control over the seed, the learner sees ONE trace and might assume it's THE only execution. The whole reason to seed (rather than hard-code one ordering) is to let the learner explore alternatives.

**Independent Test**: Enter seed `1`, observe trace A. Enter seed `2`, observe trace B. Verify A ≠ B (events appear in a different order or at different step indices). Verify replay of seed `1` matches trace A.

**Acceptance Scenarios**:

1. **Given** a multi-threaded sample is loaded, **When** the learner sets a new seed value and triggers re-execution, **Then** the trace re-renders with potentially different event ordering across threads (while remaining a valid Rust execution of the same source).
2. **Given** the learner enters the same seed value twice, **When** the sample re-executes both times, **Then** the resulting traces are bytewise identical.
3. **Given** the learner enters seed `42`, **When** they share the seed with a peer who enters the same seed on the same sample, **Then** both see the identical trace.

---

### User Story 3 — Quick "new seed" affordance to explore variations (Priority: P2)

A button in the toolbar generates a fresh random seed and immediately re-runs the trace. The learner clicks it repeatedly to see many different valid schedules of the same code — building intuition that thread interleaving is one of many possibilities, not a fixed pattern.

**Why this priority**: Without this affordance the learner has to invent seed numbers manually. Most won't bother. The "re-roll" affords the exploration that makes the pedagogy click — but it's optional polish; US2's manual seed-entry covers the same ground.

**Independent Test**: Click the "new seed" button N times. Each click should produce a visibly different trace OR document that the seed was already producing a representative variant (depending on the program's branching surface).

**Acceptance Scenarios**:

1. **Given** a multi-threaded sample is loaded, **When** the learner clicks the "new seed" button, **Then** the seed updates to a fresh random value AND the trace re-runs with that seed AND the new seed is displayed somewhere the user can read it.
2. **Given** the learner has clicked "new seed" three times in a row, **When** they want to return to a previous schedule, **Then** they can manually enter the previously-displayed seed and reproduce the earlier trace.

---

### Edge Cases

- **Single-threaded program with a seed**: programs that don't spawn any threads must produce identical traces regardless of seed — there's no scheduling choice to randomize.
- **Seed `0` and other "edge" values**: must produce a valid deterministic trace (no crash on edge integers).
- **Very small programs with one scheduling point**: e.g., `thread::spawn(|| {}); h.join()`. There's only one ordering possible; different seeds must still produce identical traces (the seed is consulted but doesn't change the outcome).
- **Deadlock-forming schedules**: if a particular seed leads to a deadlock (both threads waiting on each other's locks), the visualization must surface this clearly rather than hang silently.
- **Replay determinism across reloads**: a saved trace re-rendered after the source code changes must NOT use the old seed silently — the trace must reflect the current source.
- **Trace length explosion**: a randomized scheduler may produce longer traces than the deterministic one (more thread switches). The step counter and player must scale.
- **Seed UI when no threads exist**: should still show seed input even if no thread::spawn is in the code (so the affordance is discoverable BEFORE the learner writes thread code), but the seed has no observable effect.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST accept a seed value (non-negative integer) that fully determines the thread scheduling decisions for a given source program.
- **FR-002**: Given the same source code and the same seed, the system MUST produce a bytewise-identical event trace on every execution (deterministic replay).
- **FR-003**: Given the same source code and a DIFFERENT seed, the system MUST be allowed to (and frequently will) produce a different event trace reflecting different valid thread interleavings.
- **FR-004**: The system MUST expose the current seed to the user in a readable form (input field, status text, or similar) and allow them to change it.
- **FR-005**: Changing the seed MUST trigger a fresh execution and re-render of the trace.
- **FR-006**: The seed input MUST accept any value in a reasonable integer range (e.g., 0 to 2^32-1) and produce a valid trace for any value in that range.
- **FR-007**: At every cooperative scheduling point (thread spawn, thread park, thread unpark, lock acquire, lock release, join), the scheduler MUST use the seeded random source to pick which ready thread advances next. The choice must be deterministic given the (seed, current scheduler state) pair.
- **FR-008**: Single-threaded programs MUST produce identical traces regardless of seed.
- **FR-009**: The system MUST provide a way to generate a fresh random seed quickly (e.g., a "re-roll" or "random seed" button) without requiring the learner to type a number.
- **FR-010**: When the user re-rolls or otherwise changes the seed, the new seed value MUST be visible BEFORE the trace finishes re-running (so the user knows which seed produced what they're about to see).
- **FR-011**: The scheduler MUST surface deadlocks: if a chosen schedule produces a state where no thread can advance and at least one thread is parked indefinitely, the trace MUST end with a clear "deadlock" marker rather than producing an empty or truncated trace.
- **FR-012**: The seed input MUST be reachable by keyboard (tab navigation) and screen reader (labeled control), matching the existing toolbar's accessibility model.

### Key Entities

- **Schedule seed**: a non-negative integer that fully determines the sequence of thread-selection choices made during the program's execution. Single owner per page load. Persists in-memory for the active session; whether it persists across reloads is documented in Assumptions.
- **Scheduling decision point**: any moment during evaluation where two or more threads could advance. The scheduler consults the seeded RNG at each such point to pick one. The set of decision points is fixed by the source code (deterministic across runs of the same seed).

## Assumptions

- **Default seed on first visit**: a fixed value (e.g., `0` or `1`) — so every learner's first impression of the sample is the same. Reproducibility is favored over surprise.
- **Seed persistence**: the seed is in-memory only for the active page session — does NOT survive page reloads. Reloading the page resets to the default seed. (Justification: the schedule belongs to "this run," not "this user's preference." Per-user persistence would obscure the per-trace mental model.)
- **Seed display format**: the seed is shown as a decimal integer. No fancy encodings (hex, base64). Learners read and type integers.
- **Re-roll affordance form**: a button in the toolbar near the play/step controls. Single-click action. The new seed is generated client-side from a non-cryptographic source (UX, not security).
- **Scope of randomization for v1**: cooperative scheduling points only — thread spawn, thread park / unpark, mutex lock acquire / release, join. NOT randomized: instruction-level interleaving within a single thread's basic block (Rust threads don't preempt mid-statement in real programs either, modulo memory ordering).
- **Single-threaded programs unaffected**: a program with no `thread::spawn` produces the same trace regardless of seed. (FR-008.)
- **Deadlock surfacing UX**: a final event with kind `Deadlock` is emitted, the status bar shows "deadlock: threads X, Y waiting on each other," and the player stops at that step. (FR-011.)
- **Maximum trace length**: a randomized scheduler MAY produce traces several times longer than the deterministic one (more thread switches). The existing player's step counter handles arbitrary lengths; no new cap needed.
- **Backwards compatibility**: existing samples will trace differently than M08 v1. This is a deliberate behavior change; the M08 v1 "everything runs at join" pattern was a pedagogical compromise and is replaced.
- **Pre-requisite milestone**: M08.1 (real Mutex parking + contention handling) is a natural prerequisite. Without parking, the randomization has fewer scheduling points to exploit. This feature ASSUMES M08.1's parking model is in place; if 021 lands first, the value is reduced (only thread spawn / join order is randomized).
- **Random source**: a non-cryptographic PRNG seeded with the user's seed. Specific algorithm is an implementation detail.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For at least 80% of the existing M08 sample programs, two different seeds produce two different traces (measured by event-stream non-equality).
- **SC-002**: For 100% of single-threaded sample programs, all seeds produce identical traces (measured by event-stream equality).
- **SC-003**: Same-seed replay across reloads produces bytewise-identical event streams in 100% of runs.
- **SC-004**: A learner who has never seen the feature can locate the seed control within 30 seconds of being told "see if this code runs the same every time" (qualitative usability bar).
- **SC-005**: The re-roll button changes the visible trace in ≥ 50% of clicks on the M08 Arc<Mutex> sample (some clicks may produce the same observable trace if seeds happen to make identical choices at every decision point — acceptable, but most clicks should produce visible change).
- **SC-006**: Seed change to trace re-render completes in under 1 second for all existing samples on a mid-tier laptop.
- **SC-007**: A trace ending in deadlock surfaces a clear "deadlock" status within the same step that the deadlock is detected (no silent hang, no infinite loop).
- **SC-008**: Bundle size impact: WASM bundle grows by ≤ 5% versus the pre-021 baseline (small PRNG state + scheduler logic, no large dependencies).
- **SC-009**: Zero Rust compile warnings introduced by the feature.

## Scope & Out of Scope

### In scope

- Seeded PRNG-driven scheduler in the evaluator.
- Seed input + display + re-roll UI affordances.
- Deterministic replay given (source, seed) pair.
- Deadlock detection + surfacing.
- Documentation of pedagogical intent in the README / docs.

### Out of scope

- **Memory-ordering modeling**: Rust threads with `Relaxed` / `Acquire-Release` / `SeqCst` atomic operations produce results that depend on the underlying CPU's memory model. This feature does NOT model memory ordering — only thread-interleaving at scheduling-point granularity.
- **Instruction-level interleaving**: real CPUs may preempt threads mid-instruction. This feature only randomizes at cooperative scheduling points.
- **Loom-style exhaustive interleaving search**: this feature picks ONE interleaving per (source, seed). It does not enumerate all valid interleavings or find data races automatically.
- **Cross-session seed persistence**: the seed does NOT persist across page reloads.
- **Cross-device seed sharing via URL**: the seed is not encoded in the URL in v1. (Possible v2 enhancement.)
- **Replacing M08 v1's existing scheduling for non-threaded samples**: single-threaded samples are unaffected.

## Dependencies

- Implicit dependency on M08.1 (real Mutex parking + contention handling). Without M08.1, the randomization has few decision points to operate on. M08.1 is documented as pending in the project memory.

## Pedagogical notes

- A randomized scheduler is HALF of the "threads are scary because they're non-deterministic" lesson. The other half is the WHY — showing learners that the same code can produce different outputs (e.g., a `Mutex<Counter>` where the counter ends up at different values depending on which thread's `+= 1` saw the most-recent value). For this milestone, the goal is the scheduler. The "result varies with the schedule" payoff lands in a follow-up sample-set, not in scheduler infrastructure itself.
- This feature directly serves the README's stated pedagogical goal: "give a newcomer concrete intuition for Rust's memory mechanics … threads with `Arc`/`Mutex`."
