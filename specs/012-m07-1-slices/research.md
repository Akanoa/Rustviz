# Research — M07.1 Implementation Decisions

14 decisions across lexer, parser, AST, typeck, eval, UI annotation, and protocol amendment.

## Lexer

### R-001 — New token `DotDot` for `..`

- **Decision**: add `TokenKind::DotDot`. Lexer emits it on two consecutive `.` chars. Two-char lookahead — same pattern as `==`, `!=`, `::`, `->`.
- **Disambiguation from `Dot`**: the existing float-literal arm already greedily consumes `digit.digit` patterns, so `1.5` lexes as `Float(1.5)` not `Int(1)` + `Dot` + `Int(5)`. A bare `..` after a number-end (e.g. in `1..3` where `1` is an Int) is unambiguous — after lexing `Int(1)`, the next `.` is not a fractional digit (no digit follows immediately in the float arm's lookahead because there's another `.`), so the lexer reads `..` as `DotDot`.
- **Edge case** `1.0..3.0`: lexes as `Float(1.0)`, `DotDot`, `Float(3.0)`. Correct because the float arm consumes `1.0`; the next chars are `..` (no leading digit), so the float arm doesn't fire; the dot-dot arm sees `..` and emits `DotDot`.
- **Edge case** `1..3`: lexes as `Int(1)`, `DotDot`, `Int(3)`. The float arm requires `digit.digit` (digit on both sides); `1.` followed by `.` (not digit) doesn't match.
- **Rationale**: standard two-char tokenization. Reuses M07's lookahead patterns.
- **Alternatives considered**: lex `..` as two `Dot` tokens, disambiguate in parser. Rejected — adds parser complexity, and `Dot` is used for method calls (`v.push`), so two `Dot` tokens would be misleading.

## Parser

### R-002 — Single `Expr::Range` variant for all four range forms

- **Decision**: one AST variant `Expr::Range { start: Option<Box<Expr>>, end: Option<Box<Expr>>, span }`. Each `Option` is `None` when the corresponding bound is absent. The four forms map as:
  - `a..b` → `Range { start: Some(a), end: Some(b), .. }`
  - `..b` → `Range { start: None, end: Some(b), .. }`
  - `a..` → `Range { start: Some(a), end: None, .. }`
  - `..` → `Range { start: None, end: None, .. }`
- **Rationale**: minimal variant footprint. Span covers the whole range expression including bounds.
- **Alternatives considered**:
  - **Four explicit variants** (`RangeFull`, `RangeFrom`, `RangeTo`, `RangeFromTo`). Cleaner but quadruples the match arms. Rejected — Option-pair is idiomatic Rust (matches Rust's own `Range`/`RangeFrom`/etc. design at the type level while keeping AST flat).
  - **Range as a sugar inside `Expr::Index`** (no first-class Range AST). Tighter scope but makes future "for i in 1..10" support require a refactor. Rejected — additive Range variant is forward-compatible.

### R-003 — Parse `..` only inside `[ ]` brackets in M07.1

- **Decision**: `parse_expr` does NOT recognize `..` at expression-start or as an infix operator. The dedicated `parse_index_inner` function — called between `[` and `]` — is the only entry point that accepts `..`. Standalone `..` outside `[ ]` produces a parse error: "range expressions are only valid inside index brackets".
- **Rationale**: tight scope. Avoids precedence questions about `..` (is `1 + 2..3` parsed as `1 + (2..3)` or `(1+2)..3`?). When standalone ranges become useful (M07.x or later for `for` loops), the parser extends to handle them with explicit precedence rules.
- **Alternatives considered**: parse `..` as a low-precedence binary operator in `parse_expr`. Would require deciding bp now without a real consumer. Rejected.

### R-004 — `parse_index_inner` accepts four entry shapes

- **Decision**: after the `[`, the parser looks at the next token:
  1. `DotDot` immediately → `start = None`. Next: if `RBracket`, `end = None` (full range `..`). Otherwise parse `end` expression then expect `RBracket`. → `Range { start: None, end: Some(e), .. }` or `Range { start: None, end: None, .. }`.
  2. Anything else → parse a primary expression (the potential `start`). Then peek:
     - If `RBracket` → scalar index path, return `Expr::Index { index: start, .. }` (existing M07 behavior).
     - If `DotDot` → consume it. Then peek: `RBracket` → `Range { start: Some(s), end: None, .. }`. Otherwise parse `end` expression then expect `RBracket` → `Range { start: Some(s), end: Some(e), .. }`.
- **Rationale**: handles all four range forms + the existing scalar-index form in one parsing routine. The disambiguation between "scalar index" and "range" happens on the token after the first expression (or `[`).
- **Edge case**: an arbitrary expression as range bound (e.g. `&v[a + b..2 * c]`) works because `start` and `end` are parsed via the full expression parser. Recursion is fine because range expressions themselves don't appear inside `[ ]` of an index in M07.1 — only one-level range parsing.

### R-005 — Slice type annotation syntax `&[T]`

- **Decision**: extend `parse_type`. After seeing `&` (or `&mut`), peek the next token. If `LBracket`, this is a slice type — parse the inner type, expect `RBracket`, produce `Type::Slice { inner: Box<Type>, mutable: bool, span }` (NEW AST type variant). Otherwise the existing reference-type rule fires (`Type::Ref { inner, mutable }`).
- **Type:: vs Ty:: distinction**: `Type::Slice` is the AST annotation node; `Ty::Slice` is the typeck representation. The annotation->Ty lowering maps `Type::Slice { inner, mutable }` → `Ty::Slice(Box::new(inner_ty))` (mutable: false enforced in M07.1; mutable=true → typeck error "mutable slices are out of scope in M07.1").
- **Rationale**: matches Rust's syntax for slice references. Required for `fn takes(s: &[i32]) { .. }` function signatures.

## Typeck

### R-006 — Slice typing — `&v[a..b]` is `Ty::Slice(T)`, NOT `Ty::Ref<Ty::Slice(T)>`

- **Decision**: when typecheck-ing `Expr::Borrow { inner: Expr::Index { index: Expr::Range(..), receiver, .. }, mutable: false }`:
  1. Typeck the receiver. Must be `Ty::Vec(elem)` (or `Ty::Slice(elem)` for slice-of-slice — but that's out of scope per spec, so reject).
  2. Typeck the range bounds. Each present bound must be `Ty::Int(_)` (any integer kind).
  3. Result type: `Ty::Slice(elem.clone())`. NOT wrapped in `Ty::Ref` — the leading `&` is absorbed into the slice type.
  4. Borrow tracker: register a shared borrow with `target: Pointee::Heap(receiver_addr)`. Slice and single-element borrows share the same borrow-registry machinery.
- **Rationale**: matches Rust's actual type semantics — `&[i32]` is the slice type itself, not a reference TO a slice. In Rust the `[i32]` unsized type only ever appears behind a reference, so the language treats `&[T]` as the canonical slice form.
- **Asymmetry note**: `Expr::Borrow` normally promotes inner type `T` to `Ty::Ref { inner: T, mutable }`. For the range-index special case, it promotes to `Ty::Slice(T)` directly. This is a peephole rule in the typeck — explicitly documented with a comment in the code.
- **Alternatives considered**:
  - **Two-step promotion: `&v[..]` → `&Ty::Slice(T)` → unify-down to `Ty::Slice(T)`**. Cleaner formal model but adds intermediate types that don't exist anywhere else. Rejected.
  - **Make `Ty::Slice` a reference-type variant** (e.g. `Ty::Ref { inner, mutable, slice_len: Option<...> }`). Tried — bloats the Ref variant for a corner case. Rejected.

### R-007 — `Slice::len` method dispatch

- **Decision**: add one row to the M07 method dispatch table:
  | Receiver | Method | Signature |
  |---|---|---|
  | `Ty::Slice(_)` | `len` | `(&self) -> u64` (return `Ty::Int(IntKind::U64)`) |
- **Rationale**: matches Rust's actual `<[T]>::len() -> usize` signature, with M07's existing convention of returning `u64` (Rust's `usize` is `u64` on 64-bit; M07's `Vec::len` already returns `u64`).
- **Evaluation**: extract the `len` field from `Value::Slice` and return `Value::Int { kind: U64, bits: len as i128 }`.

### R-008 — Range-bound typeck

- **Decision**: each present bound (`start`, `end`) of an `Expr::Range` must typecheck to `Ty::Int(_)`. Any integer kind is accepted (`u8`, `i32`, `usize`, etc.) — bounds are implicitly converted to `u64` (or compared as `i128`) for the OOB check at eval time. Cross-kind bounds (e.g. `&v[a..b]` where `a: u8` and `b: u32`) are accepted at typeck (typeck doesn't unify range bounds — Rust requires them to be the same `usize` but M07.1's pedagogy doesn't depend on this detail).
- **Negative bound**: `&v[(-1)..3]` is accepted at typeck if `-1`'s inferred type allows it (e.g. `i32`); the OOB check at eval emits a RuntimeError ("slice start out of bounds: negative or > vec len").
- **Rationale**: pragmatic. Real Rust enforces `usize`; M07.1 accepts any integer to keep the syntax surface small.

### R-009 — Mutable slice rejected at typeck

- **Decision**: `Expr::Borrow { mutable: true, inner: Expr::Index { index: Range, .. }, .. }` produces typeck error: `"mutable slices are out of scope in M07.1 — only &[T] (shared) is supported"`. Error span is the `&mut` token through end of index.
- **Rationale**: prevents silent wrong behavior. When M07.x adds mutable slices, this rule is removed.

### R-010 — Standalone Range expression rejected at typeck

- **Decision**: an `Expr::Range` outside an `Expr::Index.index` position is a typeck error: `"range expressions are only valid inside index brackets in M07.1"`. The error happens at typeck (not parse) because the parser allows the AST node — but typeck's tree-walk flags any Range node not reached via the Index → Range path.
- **Implementation**: typeck tracks "am I currently typeck-ing an `Expr::Index.index`?" via a flag passed to recursive calls. Range nodes outside this context immediately error.
- **Rationale**: forward-compatible. When standalone ranges become useful, this flag goes away.

## Eval

### R-011 — Range-indexing eval

- **Decision**: evaluate `Expr::Index { receiver, index: Expr::Range(start_opt, end_opt), span }`:
  1. Evaluate `receiver` → `Value::Vec { addr }`. Look up the heap object; extract `len = elements.len() as u64`.
  2. Evaluate `start` (default 0) and `end` (default len). Both as `i128` (then check non-negative and ≤ len).
  3. Bounds check:
     - If `start < 0` or `start > len` → emit `Note { RuntimeError, message: "slice start out of bounds: start is {start}, vec len is {len}", span }`. Halt evaluation.
     - If `end < 0` or `end > len` → emit `Note { RuntimeError, message: "slice end out of bounds: end is {end}, vec len is {len}", span }`. Halt.
     - If `start > end` → emit `Note { RuntimeError, message: "slice start > end: start is {start}, end is {end}", span }`. Halt.
  4. Allocate a fresh `BorrowId`. Emit `MemEvent::BorrowShared { borrow_id, target: Pointee::Heap(addr), span }`.
  5. Register the borrow in `world.borrows` with `target: Pointee::Heap(addr)` so the dangling-detection scan catches it on later realloc.
  6. Return `Value::Slice { borrow_id, target: Pointee::Heap(addr), len: end - start, mutable: false }`.
- **Note**: M07.1 doesn't track which sub-range of the Vec the slice covers — only the length. The slice's `target` is the whole Vec's heap addr (not "Vec's heap addr + offset"). For pedagogy this is sufficient: the slice's relationship to the Vec's allocation is what matters; the byte-offset within the allocation isn't visualized. Future improvement could add `start_offset: u64` if pedagogy demands it.

### R-012 — Slice value is rendered as empty string (arrow IS the visualization)

- **Decision**: `render_value(Value::Slice { .. })` returns `""` (empty string). The slice's presence is communicated entirely by its blue arrow with `[len: N]` annotation. Rendering text alongside (like `"Slice→heap[1]"`) duplicates the arrow's information and clutters the stacks panel.
- **Precedent**: M07 made the same call for `Value::Box/Vec/String` — the owning arrow is the visualization; no slot text needed.

## UI

### R-013 — Length annotation on `ArrowView` and rendering

- **Decision**: `ArrowView` gains `len: Option<u64>` field. None for non-slice arrows (default). Some(n) for slice arrows. JSON shape: `arrow.len` is either omitted or present as a number.
- **Rendering**: in `renderArrows()`, after drawing the SVG `<path>` for an arrow, if `arrow.len` is present, append a `<text class="arrow-len-label">[len: N]</text>` element. Position: at the arrow's midpoint along its length, offset perpendicular by ~6-8px to avoid overlapping the arrow line. For arrows routed above the heap row (typical for slice/borrow arrows targeting heap), the label sits in the lane just above the arrow.
- **Styling**: `.arrow-len-label { font-size: 10px; fill: #4d8fcd; font-family: monospace; user-select: none; pointer-events: none; }`. Subtle blue matches the borrow arrow color.
- **Apply-event**: `World.apply_event(SlotWrite { value: Value::Slice { borrow_id, target, len, mutable }, .. })` constructs an ArrowView with `kind: Shared` (M07.1 only has shared slices), `target: ArrowTarget::Heap(addr)` (extracted from `target`), and `len: Some(len)`.
- **Rationale**: minimal SVG addition. The label inherits the arrow's routing.

## Protocol

### R-014 — 5th invocation of closed-enum-with-revisions rule

- **Decision**: amend M03's contract to note M07.1 as the 5th invocation. M07.1's changes:
  - **Additive variant on `Ty`**: `Slice(Box<Ty>)`.
  - **Additive variant on `Value`**: `Slice { borrow_id, target, len, mutable }`.
  - **No event-variant changes**. Slices use existing `MemEvent::BorrowShared` / `BorrowEnd` events with `Pointee::Heap(addr)` targets (M07 already started producing these).
- **Rationale**: precedent chain M03.1 → M03.2 → M06 → M07 → M07.1. Each documented in M03's contract.
- **Pure additive**: no restructure of any existing variant. M03 snapshot tests stay byte-identical because no existing sample constructs `Ty::Slice` or `Value::Slice`.
