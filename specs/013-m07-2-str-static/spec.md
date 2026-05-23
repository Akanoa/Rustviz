# Feature Specification: M07.2 — `&str` + static memory

**Feature Branch**: `013-m07-2-str-static`
**Created**: 2026-05-23
**Status**: Draft
**Input**: User description: "M07.2"

**Authoritative scope source**: [`MILESTONES.md` › M07.2 — `&str` + static memory](../../MILESTONES.md). The In scope / Out of scope / Entry criteria / Exit criteria / Demo lines in that block are the contract for this feature. This spec elaborates user stories, requirements, and success criteria; it does not redefine scope.

## User Scenarios & Testing *(mandatory)*

M07 took a pragmatic shortcut: string literals (`"toto"`) typecheck as `String`, the same type as heap-allocated owned strings. This is **wrong** by Rust's semantics — `"toto"` is `&'static str`, a borrow into the binary's read-only data segment. It allocates nothing at runtime; the bytes are baked into the executable.

M07.2 fixes this by introducing the **static memory region** — a new visual area (alongside the stack and heap) holding read-only bytes for each unique string literal. Literals become `&'static str` slices pointing into this region. `String::from(s: &str)` is what actually heap-allocates and copies. The contrast between "this borrows from the binary" and "this owns a heap allocation" becomes tangible.

This builds directly on M07.1's slice infrastructure: `&str` IS a slice (`Ty::Slice(Ty::Int(U8))` — slice of bytes), reusing the same `Value::Slice` shape, the same `[len: N]` arrow annotation, the same byte-cell hover highlight. M07.2 adds a new `Pointee::Static` target variant and the visual region; everything else is reuse.

### User Story 1 - String literal is `&'static str` (Priority: P1)

A learner types `fn main() { let s = "toto"; }`. The stacks panel shows `s : &str` (NOT `s : String`). The static-memory region (a new visual area at the top of the page, or alongside the heap panel) displays a read-only block containing `"toto"` (4 bytes). **A blue slice arrow** connects `s`'s slot to that static block, annotated with `[len: 4]`. The trace contains zero `HeapAlloc` events — nothing was allocated, just borrowed.

**Why this priority**: this IS the headline pedagogy. Without it, the learner has the wrong mental model that `"hi"` is a heap-owned string. P1.

**Independent Test**: load `m07_2_str_literal.rs`, step to the binding, observe `s : &str` with the blue slice arrow pointing into the static region and the bytes `"toto"` visible there. No heap activity.

**Acceptance Scenarios**:

1. **Given** the source `let s = "toto";`, **When** the pipeline runs, **Then** typeck succeeds with `s : &str`; the trace contains zero `HeapAlloc` events; a static-memory block holding `"toto"` (4 bytes) is visible; a blue slice arrow connects `s` to the static block with `[len: 4]` annotation.
2. **Given** the slice's scope ends, **When** the cursor passes the closing `}`, **Then** the slice arrow disappears (BorrowEnd fires) but the static block persists (static memory is never freed).
3. **Given** two identical literals `let a = "hi"; let b = "hi";`, **When** the pipeline runs, **Then** the static region shows ONE block (deduplication — matches Rust's linker behavior of merging duplicate string constants).

---

### User Story 2 - `String::from` copies static bytes to heap (Priority: P1)

A learner types `fn main() { let s = String::from("hi"); }`. The static region shows a block with `"hi"`. The heap also shows a new block with `"hi"` (capacity 2). **A blue slice arrow** points from `String::from`'s argument site to the static block during the call; **a black owning arrow** then points from `s` to the heap block. The pedagogy: the bytes were COPIED from the binary's RO segment into a fresh heap allocation that `s` owns.

**Why this priority**: completes the literal-vs-owned-string story. Without seeing the copy, the static region looks decorative. P1.

**Independent Test**: load `m07_2_string_from.rs`, observe BOTH a static `"hi"` block AND a heap `"hi"` block side by side, with an owning arrow from `s` to the heap one.

**Acceptance Scenarios**:

1. **Given** `let s = String::from("hi");`, **When** the pipeline runs, **Then** the static region holds `"hi"` (2 bytes); the heap region holds a fresh `String` block with `"hi"` (2 bytes); `s`'s slot owns the heap block (black arrow).
2. **Given** the static block content and the heap block content, **When** observed at the same cursor step, **Then** both display the same bytes (`"hi"`) — making the copy explicit.
3. **Given** the slice's scope ends, **When** the cursor passes the closing `}`, **Then** the heap block disappears (HeapFree fires for the owned String), the static block stays (RO segment is never freed).

---

### User Story 3 - `push_str` takes `&str`, not the old transient (Priority: P2)

A learner types `fn main() { let mut s = String::from("hi"); s.push_str("!"); }`. The `"!"` argument is a `&str` — typeck accepts it as such, evaluation reads from the static region (no heap allocation for the argument), and the bytes are appended to `s`'s heap allocation. The static region holds two read-only blocks (`"hi"` and `"!"`); the heap shows `s`'s buffer (which may realloc if cap is exceeded).

**Why this priority**: removes the M07 shortcut where `push_str`'s argument was a "transient Value::Str". With M07.2, the argument flows through the same `&str` (slice) machinery as the literal binding from US1. Smaller pedagogy than US1/US2 but necessary for consistency. P2.

**Independent Test**: load `m07_2_push_str.rs`, observe two static blocks (`"hi"` for the original, `"!"` for the argument) AND one heap block (s's String, growing from `"hi"` to `"hi!"`).

**Acceptance Scenarios**:

1. **Given** `let mut s = String::from("hi"); s.push_str("!");`, **When** the pipeline runs, **Then** the static region contains both `"hi"` and `"!"` as distinct read-only blocks; the heap region contains exactly one block (s's String).
2. **Given** push_str succeeds, **When** the cursor advances past the call, **Then** s's heap block shows updated bytes `"hi!"` (with realloc if needed).

---

### Edge Cases

- **Empty string literal** `let s = "";` — valid; `&str` with len 0. Static region shows a 0-byte block (or omits it — plan-phase decides).
- **Repeated literals** `"hi"` appearing multiple times — dedupe in the static region (matches Rust's linker behavior; the same `.rodata` constant is reused). One block per unique literal content.
- **Literal escapes** `"a\nb"` — the static block contains 3 bytes (`a`, `\n`, `b`). Display format may render the escape sequence visibly (`a\nb`) or render the byte values; plan-phase confirms.
- **`String::from("")` of empty literal** — heap-allocates a 0-capacity (or initial-cap) String; the slice arrow into the static empty block is still drawn.
- **Static region location** — plan-phase decides whether to add a new third visual region (alongside stacks + heap) or annotate within the heap panel. Initial proposal: a thin horizontal band above the heap region, labeled "static memory (RO)".
- **Static block freeing** — static blocks NEVER fire `HeapFree`. They persist for the whole trace. Plan-phase confirms whether the JSON wire format uses a distinct event variant (e.g. `StaticAlloc`) or extends `HeapAlloc` with a flag — the spec defers this.
- **Slicing a `&str`** `let t = &s[1..3];` where `s: &str` — out of scope (slice-of-slice is deferred per M07.1).
- **`&str` as function parameter** `fn print(s: &str) { ... }` — supported via the existing M07.1 function-signature slice support (`&[T]` syntax already extends to `&str` once the parser accepts `&str` as a type annotation). Calling with a literal or a `String::from`-derived value works through the same machinery.
- **Mixing literals and `String` in `push_str`** — only literal args supported (M07's existing restriction). M07.2 doesn't generalize push_str to take arbitrary `&str` expressions yet — same scope as M07.
- **`String::from` arg restriction** — same as M07: arg must be `Expr::StrLit`. Generalizing to arbitrary `&str` is deferred.
- **Mutation through `&'static str`** — impossible (it's a shared borrow into RO memory). No special handling needed; M07.1 already enforces no mutation through `&` borrows.
- **String indexing `s[0]`** — Rust rejects this (can't index UTF-8 by byte safely); M07.2 follows.
- **`+` or `+=` on strings** — out of scope (would need owned-string concat semantics).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST type string literals (`"..."`) as `&str` (the M07.1 slice type with `Ty::Int(IntKind::U8)` element type), NOT as `Ty::String`. This is a behavioral change to typeck for `Expr::StrLit`.
- **FR-002**: System MUST introduce a static-memory region distinct from the stack and the heap. The region holds one read-only block per unique string-literal content (deduplicated by content).
- **FR-003**: System MUST extend the `Pointee` enum with a `Static(StaticAddr)` variant so `Value::Slice.target` can point at a static-region block.
- **FR-004**: System MUST emit an event (or extend an existing one — plan-phase) signaling static-block existence so the UI can render the static region.
- **FR-005**: System MUST evaluate string literals as `Value::Slice { borrow_id, target: Pointee::Static(addr), start: 0, len: bytes.len(), byte_offset: 0, byte_len: bytes.len(), mutable: false }`. The transient `Value::Str` MAY be removed entirely or kept as an internal-only construct (plan-phase confirms).
- **FR-006**: System MUST evaluate `String::from(literal)` such that (a) the literal evaluates to its static-region slice, (b) a fresh `HeapAlloc` fires for the new String's buffer, (c) the static bytes are copied into the heap allocation. Both blocks must be visible simultaneously.
- **FR-007**: System MUST evaluate `s.push_str(literal)` such that the literal's bytes flow from the static region into `s`'s heap buffer (via copy at the byte level, same as `String::from`'s copy step).
- **FR-008**: System MUST render the static region visually distinct from the heap region — different background, label, or position. Static blocks never display as freed (they persist for the trace's lifetime).
- **FR-009**: System MUST render the slice arrow from any `s: &str` binding to the corresponding static block, with the existing `[len: N]` annotation from M07.1. Hover highlights should work on static-block byte-cells using the same M07.1 mechanism.
- **FR-010**: System MUST ensure existing M07 samples (`m07_string.rs` etc.) keep working — `String::from("hi"); s.push_str("!")` produces the same end-state observable in `s`'s heap buffer, with the addition of static blocks for the literals.
- **FR-011**: System MUST ship at least 2 new reference programs (`tests/samples/m07_2_*.rs` + `web/samples/`) covering: string-literal-as-slice, and `String::from`-shows-both-blocks.

### Key Entities

- **Static memory region**: a new visual region in the UI holding one read-only block per unique string-literal content. Never freed. Visually distinct from heap.
- **Static block**: one read-only block in the static region. Carries `addr: StaticAddr`, `bytes: String`, `size: u32` (= bytes.len()). Persists for the whole trace.
- **`StaticAddr`**: a new newtype (or extension to `HeapAddr` — plan-phase) identifying a static block.
- **`Pointee::Static(StaticAddr)`**: a new `Pointee` variant; `Value::Slice.target` can be `Static(_)` for `&'static str` slices (in addition to M07.1's `Heap(_)`).
- **`&str` type**: in M07.2, `&str` is `Ty::Slice(Box::new(Ty::Int(IntKind::U8)))` — a slice of bytes. May get a `Ty::Str` sugar variant (plan-phase decides between sugar and direct-encoding).
- **String literal evaluation**: at first encounter of a literal value, eval allocates a `StaticAddr` (or reuses if content matches a prior literal — dedup); emits a static-block event; returns a `Value::Slice` pointing into that block.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After M07.2 ships, `let s = "toto";` produces a trace with `s : &str` (NOT `s : String`); the trace contains zero `HeapAlloc` events for the literal; the page renders a static-region block holding `"toto"` and a slice arrow from `s` to that block.
- **SC-002**: `String::from("hi")` produces a trace where both the static `"hi"` block AND a fresh heap `"hi"` block are visible at the same cursor step; the heap block carries the String's bytes (copied from static).
- **SC-003**: At end-of-scope for `s : String`, the heap block fires `HeapFree` and disappears; the static block stays visible.
- **SC-004**: Two identical literals (`let a = "hi"; let b = "hi";`) produce ONE static block (deduplication).
- **SC-005**: `s.push_str("!")` succeeds; the `"!"` literal's bytes flow into `s`'s heap buffer without allocating a separate heap block for the argument.
- **SC-006**: ≥ 2 new `m07_2_*.rs` reference programs ship.
- **SC-007**: Existing M01–M07.1 tests pass. M03 snapshots stay byte-identical (existing L1 samples don't construct string literals). M07's `m07_string.rs` may behave slightly differently (now has a static block for `"hi"` and `"!"`); the test assertion (`run_pipeline_string_from`) should be re-examined and updated if the alloc count changes.
- **SC-008**: WASM bundle growth ≤ +15% vs M07.1 baseline. Small additive surface — one `Pointee` variant, one new event (or extension), one visual region.
- **SC-009**: Zero warnings under `RUSTFLAGS="-D warnings" cargo build --release` AND `cargo test`. Both host and WASM targets clean.

## Assumptions

- **`&str` shape**: `Ty::Slice(Box::new(Ty::Int(IntKind::U8)))` (direct encoding as slice-of-bytes). Plan-phase may choose `Ty::Str` sugar for cleaner rendering. Either way, the underlying `Value::Slice` shape is reused.
- **Static-block dedup**: literals with identical byte content share one static block. Matches Rust's actual linker behavior (string constants are merged in `.rodata`).
- **Static region position**: a thin horizontal band labeled "static memory (RO)" above the heap region (between stacks and heap). Plan-phase decides exact layout; could also be a small section within the heap region with distinct styling.
- **`Value::Str` transient**: kept as an internal Rust-side `Value` variant for compatibility OR removed entirely (plan-phase decides). Pedagogically nothing references it from the UI after M07.2 — string literals are now `Value::Slice`.
- **`StaticAddr` representation**: new newtype `StaticAddr(u32)`, separate from `HeapAddr`. Cleaner because static blocks have different lifetime semantics (never freed).
- **Event shape**: plan-phase decides between (a) a new `MemEvent::StaticAlloc { addr, bytes, span }` variant, (b) extending `HeapAlloc` with a `static: bool` flag, or (c) emitting via a different mechanism. Option (a) is cleanest and matches the M03.1/M03.2 pattern of adding event variants in revision milestones. 6th invocation of the closed-enum-with-revisions rule if so.
- **`push_str` arg restriction**: M07.2 keeps M07's "arg must be `Expr::StrLit`" restriction. Generalizing to arbitrary `&str` expressions is deferred. The internal evaluation path uses the static-slice machinery — the restriction is at typeck only.
- **`String::from` arg restriction**: same as M07 — arg must be `Expr::StrLit`. Generalizing to arbitrary `&str` is deferred.
- **No `&str` slicing in M07.2**: `let t = &s[1..3];` where `s: &str` is out of scope (M07.1 deferred slice-of-slice). The slice infrastructure can technically support it (Vec-target machinery generalizes), but typeck rejects to keep scope tight.
- **No `format!`, `println!`, `write!`** — out of scope.
- **No string indexing** `s[0]` — Rust rejects this for `&str`; M07.2 follows.
- **No `+` or `+=` on strings** — out of scope.
- **Static blocks visualized at byte granularity** — same `byte-cell` rendering as heap blocks, possibly with a different color scheme (gray or a different blue) to convey "read-only / different region".
- **Bundle target ≤ +15%**: small additive surface. No restructure.
- **Sized M** per the rubric: ~3-4 source modules (event.rs adds Pointee variant + possibly new MemEvent variant, eval.rs adds static-region machinery, ui.rs adds StaticView + render path, web/js adds static-region rendering). Estimated ~400-600 LOC net change. Smaller than M07.1 because the slice machinery is fully reused.
- **Three plan-phase deferrals** (no NEEDS CLARIFICATION because reasonable defaults exist):
  1. **`Ty::Str` sugar vs direct `Ty::Slice(Ty::Int(U8))`** — sugar reads cleaner but adds a variant.
  2. **Event shape for static blocks** — new `StaticAlloc` variant vs. extending `HeapAlloc`. New variant is cleaner.
  3. **Static region visual placement** — separate panel band vs. annotated section of heap panel. Separate band is more visually distinct.
- **Foundation for future work**: `&str` slicing (`&"hello"[1..3]`), `&str` method receivers (`s.len()`), and integration with format-style APIs all build on this infrastructure.
