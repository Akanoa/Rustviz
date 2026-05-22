# Research — M06 Implementation Decisions

Decision / Rationale / Alternatives for the references + borrows milestone.

## Type system

### R-001 — `Ty::Ref { inner: Box<Ty>, mutable: bool }` (unified, non-Copy)

- **Decision**: extend `Ty` with one new variant: `Ref { inner: Box<Ty>, mutable: bool }`. Adopting `Box<Ty>` means `Ty` can no longer derive `Copy` — this cascades through every method that took `Ty` by value (refactor to `&self`).
- **Rationale**:
  - **Unified form** matches the M03.2 lesson: `Ty::Ref { inner, mutable }` has one match arm in every consumer; the alternative `Ty::SharedRef(Box<Ty>) | Ty::MutRef(Box<Ty>)` doubles dispatch.
  - **`Box<Ty>` over `TyId`-arena**: simpler. The arena approach would be more cache-friendly but adds an indirection layer (every Ty deref goes through the arena). For a pedagogical tool, simplicity wins. If perf later matters, an arena migration is a contained refactor.
  - **Dropping `Copy`** is a one-time cost. Estimated ~50 sites taking `Ty` by value need to be audited. Most are method receivers (change to `&self`); a few `match self` arms need `.clone()` on a leaf branch.
- **Alternatives considered**:
  - **`Ty::SharedRef(Box<Ty>) | Ty::MutRef(Box<Ty>)`** (per-variant): twice the match arms in every consumer. Rejected.
  - **Flat `Ty::Ref { inner_scalar: ScalarKind, mutable: bool }`** (preserves Copy by disallowing nested refs): would duplicate ScalarKind = Ty's leaf variants. Rejected for the code duplication and the implicit "no nested refs" rule that might confuse later milestones.
  - **`Ty::Ref(TyId, bool)` + arena**: maybe in M07+ if perf bites. Rejected for M06.

### R-002 — `Value::Ref { borrow_id, target_slot, mutable }`

- **Decision**: extend `Value` with one new variant: `Ref { borrow_id: BorrowId, target_slot: SlotId, mutable: bool }`.
- **Rationale**:
  - **`borrow_id`** uniquely identifies the borrow this reference represents — matches the BorrowShared/BorrowMut event's id, lets the renderer correlate ref-in-slot to active-borrow-event.
  - **`target_slot`** denormalizes what's reachable from `borrow_id` + the event stream, but makes the StateSnapshot self-contained: the JS renderer doesn't need to walk events to know where an arrow points.
  - **`mutable`** ditto — denormalizes for renderer convenience.
  - Snapshot self-containment is worth the small duplication: avoids JS-side event walking on every render.
- **Alternatives considered**:
  - **`Value::Ref(BorrowId)`** only (lookup target via events): forces the renderer to walk events. Rejected.
  - **`Value::Ref { pointee: Pointee, ... }`** using the M03 `Pointee::Slot | Heap` enum: future-proofed for M07. Accepted as the M07 evolution path, but for M06 just `SlotId` is enough.

### R-003 — `Ref` is non-Copy; `Value` continues without `Eq`

- **Decision**: `Value::Ref` carries `Copy` field types (BorrowId, SlotId, bool — all u32-or-bool). `Value::Ref` itself is `Copy`-friendly, BUT `Value` as a whole already gave up `Copy` in M03 (its `Float` field's `f64` is Copy, but `Bool(bool)` already wasn't a problem; `Int { kind, bits: i128 }` is also Copy). So Value continues to derive `Clone + Debug + PartialEq + Serialize + Deserialize` — same as M03.2.
- **Rationale**: no change to Value's derive set. Borrow values are addressable by id; PartialEq compares by id (two `Value::Ref` are equal iff their `borrow_id` matches). NaN-style asymmetry doesn't apply.

## Event protocol

### R-004 — `BorrowShared`/`BorrowMut`/`BorrowEnd` variants already exist

- **Decision**: no `MemEvent` enum changes. The three variants were declared in M03 with their payloads typed (`BorrowId`, `Pointee`, `Span`) but no evaluator code emitted them. M06 just fills the emitter side.
- **Rationale**:
  - Confirmed by inspecting `src/event.rs`: the three variants and `BorrowId`, `Pointee` types are already in place.
  - This is exactly why M03's protocol was structured this way — to let M06 land without protocol additions.
- **Alternatives considered**:
  - **Add new variants** if the existing ones don't fit: not needed; the existing payload shape (BorrowId + Pointee + Span) is sufficient.

### R-005 — `BorrowId` allocation

- **Decision**: monotonic counter in the evaluator (`next_borrow_id: u32`). Allocated at `Expr::Borrow` eval time. Same pattern as `next_frame_id` / `next_slot_id`.
- **Rationale**: matches existing id-allocation conventions. Deterministic, replayable, debuggable.

## Lexer / parser

### R-006 — Lex `&` and `&mut` (one or two tokens?)

- **Decision**: lex as **two distinct tokens**: `TokenKind::Amp` and `TokenKind::AmpMut`. `&mut` requires no whitespace between `&` and `mut`. `&` followed by whitespace + `mut` lexes as `Amp + Ident("mut")` and produces a parse error downstream (since `mut` as a bare keyword isn't a valid expression).
- **Rationale**:
  - **Two tokens** simplifies the parser (it sees the borrow flavor directly from the token type). Rust's grammar treats `&mut` as a single token-like form in expression position.
  - **No whitespace** matches what learners type. The slightly-more-permissive `& mut x` is less common in practice and the error message ("unexpected `mut`") is informative.
- **Alternatives considered**:
  - **Single `Amp` token always**: parser inspects the next token for `mut`. Cleaner from a token-set standpoint; slightly more work in the parser. Both approaches work. Going with two tokens for parser simplicity.

### R-007 — Lexer of `&` as standalone token

- **Decision**: replace M01's lexer rejection of `&` with a normal Amp/AmpMut path. The rejection (which emitted a "L1 doesn't support borrows" error) is now removed. Verify any M01-style tests that asserted the rejection are updated (likely a single negative test in the m01 suite).
- **Rationale**: M01's outright rejection was always documented as "until M06 lands."

### R-008 — Parser: `Expr::Borrow` and `Type::Ref`

- **Decision**:
  - In `parse_atom` / prefix-expr position: see `Amp` → consume, parse the inner expression at the appropriate precedence level (place-expr-strict — only identifier paths and parenthesized place exprs accepted), return `Expr::Borrow { inner, mutable: false, span }`. Same for `AmpMut` with `mutable: true`.
  - In `parse_type`: see `Amp` → consume, parse inner type, return `Type::Ref { inner, mutable: false, span }`. Same for `AmpMut`.
- **Rationale**:
  - **Place-expression check at parser level (loose) + typeck (strict)**: parser accepts `&expr` for any expr that COULD be a place; typeck rejects `&(1 + 2)` etc. as "expected place expression."
  - For L2: place expressions are just identifier paths (`&x`, `&y`). No fields, no array indexing yet. Stricter check is fine.

### R-009 — Place-expression check at typeck

- **Decision**: typeck rejects `&expr` when `expr` is not a place. For L2, place expressions are limited to `Expr::Ident(_, _)` (identifier paths). Anything else (`&5`, `&(2 + 3)`, `&f()`) is a typeck error: "expected place expression for borrow."
- **Rationale**: matches Rust's actual rule. M07+ may extend to allow `&field` etc. when struct fields land. M06 keeps it minimal.

## Typeck — borrow tracker

### R-010 — `BorrowTracker` data structure

- **Decision**: in `src/typeck.rs`, add an inline `mod borrow_tracker` with:

  ```rust
  pub struct BorrowTracker {
      // For each binding ever borrowed, the stack of currently-active borrows.
      // When a borrow goes out of scope, it's popped.
      active: IndexMap<BindingId, Vec<ActiveBorrow>>,
  }

  pub struct ActiveBorrow {
      kind: BorrowKind,        // Shared or Mut
      scope_depth: u32,        // for scope-level lifetime tracking
      borrow_span: Span,       // for error messages pointing at the conflicting borrow
  }

  pub enum BorrowKind { Shared, Mut }

  impl BorrowTracker {
      pub fn try_take_shared(&mut self, b: BindingId, depth: u32, span: Span) -> Result<(), AliasConflict>;
      pub fn try_take_mut(&mut self, b: BindingId, depth: u32, span: Span) -> Result<(), AliasConflict>;
      pub fn pop_scope(&mut self, leaving_depth: u32);  // remove borrows >= depth
  }
  ```

- **Rationale**:
  - **Per-binding stack** simplifies the rule: when checking new borrow, look at `active[binding]`. If empty, succeed. If non-empty, apply Rust's rule.
  - **`scope_depth`** lets us drop borrows precisely at scope exit. The typechecker tracks current scope depth as it walks; on scope exit, drop all borrows in `active` whose depth >= the exiting depth.
  - **`AliasConflict`** is a simple struct carrying the existing-borrow span — used to build clear error messages.
- **Alternatives considered**:
  - **Per-binding count of (sh, mut)**: loses the span of the conflicting borrow, which weakens error messages. Rejected.
  - **Free-form borrow graph**: overkill for L2. Rejected.

### R-011 — Aliasing rule check

- **Decision**: standard Rust rules:
  - `try_take_shared(b)`: succeeds if `active[b]` has no `Mut` entries. Adds a Shared entry.
  - `try_take_mut(b)`: succeeds only if `active[b]` is empty. Adds a Mut entry.
  - On failure, return `AliasConflict { existing_kind, existing_span }`. typeck builds the error message.

### R-012 — Borrow ending at scope exit

- **Decision**: `BorrowTracker::pop_scope(depth)` removes all `ActiveBorrow` entries where `scope_depth >= depth`. Called when typecheck_block exits.
- **Rationale**: scope-level lifetimes per FR / MILESTONES.md. NLL is out of scope.

### R-013 — Tracking which borrows belong to which scope at eval time

- **Decision**: each `Scope` struct in the evaluator gains a `borrows: Vec<BorrowId>` field. When `Expr::Borrow` evaluates, allocate the BorrowId, push to the current scope's borrows, emit BorrowShared/Mut. When the scope exits, before popping locals, iterate `borrows` in reverse and emit BorrowEnd for each.
- **Rationale**: mirrors typeck's scope-level tracking. Symmetric with how SlotDrop fires at scope exit (M03.1).

## UI / SVG overlay

### R-014 — SVG arrow overlay layer

- **Decision**: a single `<svg id="arrow-overlay">` element placed absolutely over the `main` content area, with `pointer-events: none` so clicks pass through to the underlying panels. Drawn via SVG `<path>` elements with `<marker>`-defined arrowheads.
- **Rationale**:
  - **SVG over HTML+CSS**: arrows are the natural domain of SVG (curves, arrowhead markers, stroke widths).
  - **Single overlay**: one DOM element to query/render, not many. Updated on every state render + window resize.
  - **`pointer-events: none`**: the overlay must not interfere with editor / dropdown / button interactions.
- **Alternatives considered**:
  - **Canvas**: arbitrary drawing but loses CSS-style styling (color via CSS classes). Rejected.
  - **CSS-only arrows via transformed divs**: clunky for non-axis-aligned arrows; doesn't scale visually. Rejected.

### R-015 — Arrow positioning algorithm

- **Decision**: at every state render, for each `BorrowView`:
  1. Query `document.querySelector('[data-slot-id="N"]')` for the source slot and target slot.
  2. `getBoundingClientRect()` each. Compute their centers.
  3. Compute start and end points (source slot's right edge, target slot's left edge — or top/bottom depending on relative position).
  4. Compute control points for a curved path (quadratic Bezier) or straight line.
  5. Append/update an `<path>` element in the SVG.
- **Rationale**:
  - **`data-slot-id` attribute** on slot card DOM elements is needed; add at slot-card creation time in `render`. Cheap addition.
  - **Curved arrows** read better when source and target are close in space (avoids a horizontal line right over the cards). Quadratic Bezier with one mid-control-point at a perpendicular offset is sufficient.
- **Alternatives considered**:
  - **Straight lines only**: simpler but visually noisier when arrows cross. Rejected for the curved approach.
  - **Pre-computed positions via flexbox layout**: brittle. Rejected.

### R-016 — Re-render on window resize

- **Decision**: `window.addEventListener('resize', () => render(currentState))`. Throttled is unnecessary at the scale of L1+L2 programs (≤ 10 arrows typically).
- **Rationale**: window resize changes slot positions; arrows would drift without re-rendering. Cheap.

## M03 contract amendment

### R-017 — Document M06 as third invocation of the closed-enum-with-revisions rule

- **Decision**: amend `specs/004-m03-event-eval/contracts/m03-api.md` to note M06 as the third invocation of the closed-enum-with-revisions rule:
  - M03.1 added `MemEvent::ReturnValue`.
  - M03.2 restructured `Ty` and `Value` (the rule was generalized from `MemEvent` to all event-protocol types).
  - **M06 adds variants** to `Ty` (`Ref`) and `Value` (`Ref`). Pure additive growth — the rule covers this without further relaxation.
- **Rationale**: documents the actual policy in force; future reviewers see the precedent chain.

## Constitution

### R-018 — Same vacuous PASS

- **Decision**: `.specify/memory/constitution.md` still unfilled. No gates apply.
