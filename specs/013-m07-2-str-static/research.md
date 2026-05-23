# Research — M07.2 Implementation Decisions

12 decisions across event protocol, typeck (Ty::Str sugar), eval (static-region interning), UI (third visual region), and Value::Str deprecation.

## Protocol

### R-001 — New `StaticAddr(u32)` newtype

- **Decision**: add `StaticAddr(pub u32)` newtype in `src/event.rs`, parallel to `HeapAddr` and `SlotId`. Monotonic allocation, never reused.
- **Rationale**: static blocks have fundamentally different lifetime semantics from heap blocks (never freed). Using a distinct type at the API surface prevents accidental confusion ("can I free this addr?" — only meaningful for HeapAddr).
- **Alternatives considered**: reuse HeapAddr with a flag. Rejected — pollutes the heap APIs with "is this static?" checks and conflates two different mental models.

### R-002 — `Pointee::Static(StaticAddr)` variant

- **Decision**: extend the existing `Pointee` enum with `Static(StaticAddr)`. M03 declared `Pointee = Slot(SlotId) | Heap(HeapAddr)`; M07 started producing `Pointee::Heap(_)`; M07.1 declared `Value::Slice.target: Pointee`; M07.2 adds the `Static` case.
- **Rationale**: matches Rust's three memory regions (stack, heap, static rodata). The Pointee enum's design from M03 already anticipated this kind of extension. Slice arrows and borrows naturally generalize.
- **Alternatives considered**: a separate `Borrow::Static` mechanism distinct from `Pointee`. Rejected — would duplicate the slice and arrow infrastructure unnecessarily.

### R-003 — New `MemEvent::StaticAlloc { addr, bytes, span }` variant

- **Decision**: add a single new event variant for static-block allocation. Fired on the FIRST interning of a literal's content. Subsequent literals with identical content reuse the addr — no new event.
- **Rationale**: minimum payload — addr + bytes (the content) + span (where the literal first appeared lexically). The UI uses this to materialize a static block. Matches the M03.1/M03.2 pattern of adding additive event variants in revision milestones.
- **Alternatives considered**:
  - **Extend `HeapAlloc` with `static: bool` flag**. Rejected — conflates two different lifecycles. HeapAlloc fires per-allocation; StaticAlloc fires once-per-unique-content. The dedup semantics are different enough to warrant a distinct event.
  - **No explicit event; UI infers static blocks from `Pointee::Static` borrows**. Rejected — UI needs to know the bytes to render them, and the lazy-allocation-on-first-use model leaves the trace's ordering ambiguous.

## Typeck

### R-004 — `Ty::Str` sugar over `Ty::Slice(Box::new(Ty::Int(U8)))`

- **Decision**: add `Ty::Str` as a distinct variant. Semantically equivalent to `Ty::Slice(Box::new(Ty::Int(U8)))` for borrow-tracking, method dispatch, and aliasing-rule purposes. Rendered as `"&str"` (not `"&[u8]"`).
- **Rationale**: pedagogical clarity. Learners type `let s = "hi";` and expect `s : &str`. Showing `&[u8]` would be technically correct but confusing — the Rust language presents `&str` as a distinct type even though its representation is `&[u8]`-shaped.
- **Treatment**: typeck rules accept `Ty::Str` interchangeably with `Ty::Slice(Ty::Int(U8))` where the latter would work. A small `Ty::is_str_like()` predicate centralizes the check.
- **Alternatives considered**:
  - **Direct `Ty::Slice(Box::new(Ty::Int(U8)))`** with rendering peephole. Rejected — every `Ty::Slice` case-handling site would need to special-case "is this a byte slice? render &str" which is uglier than one variant.
  - **`Ty::Str` as a full standalone type with no equivalence to Slice**. Rejected — would duplicate every slice operation (len, hover-highlight, etc.).

### R-005 — `Expr::StrLit` typeck returns `Ty::Str`

- **Decision**: change `Expr::StrLit` arm in `typecheck_expr_inner` from `Ok(Ty::String)` to `Ok(Ty::Str)`. This is the headline behavioral change.
- **Rationale**: matches Rust's actual semantics — `"hi"` is `&'static str`, not `String`.
- **Cascade**: the existing M07 `String::from`/`String::push_str` arg typeck checks `matches!(&args[0], ast::Expr::StrLit(_, _))` which is unaffected by the type change. The arg's TYPE was `Ty::String` and is now `Ty::Str` — but the AST-shape check is what gates the call, not the type.

### R-006 — `Slice::len` method dispatch extends to `Ty::Str`

- **Decision**: extend the method dispatch table with `(Ty::Str, "len") -> u64`. Existing `(Ty::Slice(_), "len")` from M07.1 stays.
- **Rationale**: `&str` has `len()` returning `usize` in real Rust. M07.2 follows.
- **Eval**: same as `Slice::len` — extract the slice's `len` field from `Value::Slice` and return as `Value::Int { U64, .. }`.

## Eval

### R-007 — `StaticState` with content-deduplicated interning

- **Decision**:
  ```rust
  struct StaticState {
      next_addr: u32,
      blocks: IndexMap<StaticAddr, StaticBlock>,
      by_content: HashMap<String, StaticAddr>,
  }
  struct StaticBlock {
      bytes: String,
  }
  ```
  `intern_static(bytes, span) -> StaticAddr`: if `by_content` has the bytes, return the existing addr (no event). Otherwise allocate fresh addr, insert into both maps, emit `MemEvent::StaticAlloc { addr, bytes, span }`, return new addr.
- **Rationale**: matches Rust linker behavior (`.rodata` merges duplicate string constants). Dedup is content-keyed (`HashMap<String, StaticAddr>`) for O(1) lookup; the IndexMap preserves insertion order for deterministic rendering.

### R-008 — `Expr::StrLit` eval emits BorrowShared + returns Value::Slice

- **Decision**: change the `Expr::StrLit(s, span)` arm in `eval_expr`:
  1. `let addr = self.intern_static(s.clone(), *span);` — emit StaticAlloc if first.
  2. Allocate a `borrow_id`.
  3. Emit `MemEvent::BorrowShared { borrow_id, target: Pointee::Static(addr), span }`.
  4. Register the borrow in the active-scope's borrows (so BorrowEnd fires at scope exit).
  5. Return `Value::Slice { borrow_id, target: Pointee::Static(addr), start: 0, len: s.len() as u64, mutable: false, byte_offset: 0, byte_len: s.len() as u64 }`.
- **Rationale**: parallel to M07.1's `eval_slice_borrow` but with the static-region target. Each literal occurrence creates its OWN borrow (with its own borrow_id), even when sharing the same static block — Rust similarly conceptualizes each use of `"hi"` as a separate `&'static str` slice (which happen to be equal).
- **Note on borrow_id allocation**: every literal occurrence gets a fresh borrow_id. The static block dedup is at the content level (byte-storage), NOT at the borrow level. Two `let a = "hi"; let b = "hi";` lines produce two borrows pointing at the same static addr.

### R-009 — `Value::Str` removed (cleanup)

- **Decision**: remove the `Value::Str(String)` variant from `event.rs`. Update `String::from` eval to extract bytes from the new `Value::Slice` via static-region lookup. Update `string_push_str` similarly.
- **Rationale**: with literals now becoming `Value::Slice`, nothing constructs `Value::Str` anymore. Keeping a dead variant is clutter.
- **Cascade**: `Value::type_name()`, `value_size_bytes()`, `render_value()`, `render_value_for_note()` all lose their `Value::Str` arms — match exhaustiveness will flag every site. Mechanical.

### R-010 — `String::from(literal)` eval reads bytes from static region

- **Decision**: in the `["String", "from"]` arm of `eval_path_call`:
  1. Evaluate the arg → `Value::Slice { target: Pointee::Static(addr), .. }`.
  2. Look up the static block by addr → get the bytes.
  3. Allocate a fresh heap addr; insert `HeapObject::Str { bytes: copied, capacity: bytes.len() }`; emit HeapAlloc.
  4. Return `Value::String { addr: heap_addr }`.
- **Rationale**: same end-state as M07's path but the byte-source is now explicit (static region). The arrow visualization at the call site shows the slice arrow into static plus the owning arrow into heap, making the copy visible.

## UI

### R-011 — `StaticView` + new visual region

- **Decision**: `StaticView { addr, bytes, size, display }` struct in `src/ui.rs`. `World.static_region: Vec<StaticView>`. `StateSnapshot.static_region: Vec<StaticView>` (serde-skip-if-empty for backwards-compat of the wire format).
- **Apply-event**: `MemEvent::StaticAlloc { addr, bytes, span }` appends a StaticView to world.static_region (never removes — static blocks persist).
- **Visual region**: a new `<section id="static">` between stacks and heap in the page grid. CSS: gray-ish background with "static memory (RO)" label, byte-cells in a muted color (gray fill) to distinguish from heap's blue.
- **Rendering**: similar to renderHeap — maintain `staticElements: Map<addr, HTMLElement>`; each block carries `data-static-addr="..."` and renders byte-cells per byte. No "freed" state.
- **Rationale**: distinct region matches Rust's mental model (three memory areas). Reuses the byte-cell rendering primitive from M07. No element-span highlight (static blocks are raw bytes, no Vec-style element segmentation — could add later if pedagogically useful for UTF-8 visualization).

### R-012 — `ArrowTarget::Static(u32)` + renderer extension

- **Decision**: extend the `ArrowTarget` enum with `Static(u32)`. The `apply_event` SlotWrite arm dispatching on `Value::Slice.target` adds a `Pointee::Static(addr)` arm producing `ArrowTarget::Static(addr.0)`. JS `renderArrows` resolves `arrow.target.Static` via `[data-static-addr="..."]` (mirrors the existing Heap and Slot resolvers). Hover-highlight follows the same byte-cell path; element-span highlight skipped (no segmented display).
- **Rationale**: minimum invasive extension. The renderer already dispatches on target type for arrow routing; adding a third arm is mechanical.
- **Arrow routing**: static blocks are likely above/right of the heap (depending on layout). The existing "enter-from-above" routing for heap targets generalizes — static targets use the same routing strategy. Plan-phase confirms after seeing the visual.
