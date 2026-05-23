# Research — M07 Implementation Decisions

22 decisions across lexer, parser, AST, typeck, eval, heap state, animations, dangling-borrow detection, and protocol amendment.

## Lexer

### R-001 — Five new token variants

- **Decision**: add `TokenKind::Str(String)`, `ColonColon`, `Dot`, `LBracket`, `RBracket`. The lexer recognizes:
  - `"..."` with escapes `\n`, `\t`, `\\`, `\"`. No raw strings, no multi-line, no Unicode escapes.
  - `::` as a single two-char token (not two `Colon`s).
  - `.` as a single token (after the float-literal lexer arm has already consumed `digits.digits`).
  - `[` and `]` as single-char tokens.
- **Rationale**: `::` as one token avoids parser ambiguity with `let x: i32`. `.` as one token enables postfix method-call parsing without ad-hoc lookahead. The lexer's existing float-literal arm already consumes `.` greedily for `1.5` etc.; any remaining `.` is the postfix-dot token.
- **Alternatives considered**:
  - **Lex `::` as two `Colon` tokens, disambiguate in parser**: parser becomes more complex; rejected.
  - **Reuse `Str` as a generic literal**: M07 only needs raw string-content tokens; specialized is fine.

### R-002 — String literal escape handling

- **Decision**: scan byte-by-byte from the opening `"`. On `\`, peek the next byte: `n` → `\n`, `t` → `\t`, `\\` → `\\`, `"` → `"`. Any other escape sequence is a parse error (`invalid escape sequence`). Unterminated string (EOF before closing `"`) is also an error.
- **Rationale**: minimum useful set. Pedagogical samples won't need more. ASCII-only assumption per spec.

## Parser

### R-003 — `Expr::Path` for static fn references

- **Decision**: `Expr::Path { segments: Vec<String>, span }`. Parser sees `Ident :: Ident` and consumes greedy (any number of `::Ident` after the first). Single-segment Ident still parses as `Expr::Ident`; only multi-segment becomes `Expr::Path`.
- **Rationale**: keeps single-segment Ident's hot path simple. Multi-segment is a separate variant so typeck dispatch is unambiguous.
- **Alternatives considered**: store single-segment paths as `Expr::Path` too. Would change every existing Ident-handling site. Rejected.

### R-004 — `Expr::MethodCall` separate from `Expr::Call`

- **Decision**:
  - `Expr::MethodCall { receiver: Box<Expr>, name: String, args: Vec<Expr>, span }` for `expr.method(args)`.
  - `Expr::Call { callee: Box<Expr>, args, span }` (existing) for `name(args)` and `Path::name(args)` (callee becomes `Expr::Ident` or `Expr::Path`).
- **Rationale**: method calls' typeck dispatch is fundamentally different (resolve by receiver Ty + method name, not callee name). Separating the AST shapes makes both clearer.
- **Alternatives considered**:
  - **Unified `Expr::Call` with `callee: Expr` that can be a method-shaped form**: forces an artificial `Expr::Method(receiver, name)` callee. Rejected — adds an indirection for no gain.

### R-005 — `Expr::Index` for `v[i]`

- **Decision**: `Expr::Index { receiver: Box<Expr>, index: Box<Expr>, span }`. Postfix form, binds tighter than binary ops (same precedence as method call: bp ~90).
- **Rationale**: simplest natural shape. No multi-dimensional indexing (`a[i, j]`) — M07 scope is `Vec` only, single-element index.

### R-006 — Postfix parsing strategy

- **Decision**: in `parse_expr`, after parsing the atom, loop: peek next token. If `.` → parse method call (consume `.`, ident, `(`, args, `)`). If `[` → parse index (consume `[`, expr, `]`). Repeat. This handles chained postfix like `v.push(x).foo()[0]` naturally (though M07 doesn't need chaining).
- **Rationale**: standard Pratt-style postfix at the atom level.

### R-007 — Type-position parsing for `Box<T>`, `Vec<T>`, `String`

- **Decision**: extend `parse_type` to handle generic type paths. After parsing a path-like type name, peek for `<`. If present, parse generic args (a comma-separated list of types, then `>`). The AST `Type::Path { segments }` (existing) gains a sibling `Type::Generic { segments, args: Vec<Type>, span }` OR the existing `Type::Path` grows an optional `generic_args` field. Plan-phase confirms.
- **Decision (concrete)**: add new variant `Type::Generic { segments: Vec<String>, args: Vec<Type>, span }` instead of mutating Type::Path's fields. Keeps existing M01–M06 type parsing untouched.
- **Rationale**: additive. Type::Path stays simple for primitive type names.

## AST

### R-008 — String literal expression: `Expr::StrLit(String, Span)`

- **Decision**: simple variant carrying the unescaped string bytes. Typeck assigns this expression a transient string type used only by `String::from(...)`. No `Ty::StrLit` first-class type — typeck handles it as `&str-like` for the one method that consumes it.
- **Rationale**: string literals only appear as arguments to `String::from` / `String::push_str` in M07's hardcoded method/path table. Doesn't need a fully-typed `str` type.

## Typeck

### R-009 — `Ty` extension: `Box`, `Vec`, `String`

- **Decision**:
  ```rust
  Ty::Box(Box<Ty>)         // owns the inner type on the heap
  Ty::Vec(Box<Ty>)         // owns a sequence of the inner type
  Ty::String               // owns a UTF-8 byte sequence
  ```
- **Rationale**: Box and Vec are generic in their element type, recursive via `Box<Ty>`. String is monomorphic.
- **`is_copy()`**: returns `false` for all three (extends the M06 Ref non-Copy precedent — Box/Vec/String have destructors that fire on scope exit, distinguishing them from L1 Copy types).

### R-010 — Method-call dispatch table

- **Decision**: hardcoded `(receiver_ty, method_name) → signature` table in typeck:

  | Receiver | Method | Signature |
  |---|---|---|
  | `Vec<T>` | `push` | `(self, T) -> ()` |
  | `Vec<T>` | `len` | `(&self) -> u64` |
  | `String` | `push_str` | `(&mut self, &str-like) -> ()` |

  Receivers for `push`/`push_str` are `&mut self` semantically — typeck rejects if the binding isn't `mut`. For `len`, receiver is `&self` — works on any Vec binding (mut or not).
- **Rationale**: minimum to make Vec realloc + String demo work. Structural dispatch (no traits, no impl blocks).
- **Note**: `Vec::push`'s `&mut self` matches Rust's actual signature, but M07's simplification: methods don't go through the typeck borrow tracker (the implicit `&mut self` doesn't need to be tracked — it's released at the end of the method call). Documented as a simplification.

### R-011 — Path-fn dispatch table

- **Decision**: hardcoded `Vec<String> → (signature, kind)` table:

  | Path | Signature | Kind |
  |---|---|---|
  | `Box::new` | `(T) -> Box<T>` | Allocates |
  | `Vec::new` | `() -> Vec<T>` (T inferred from let-annotation) | No allocation |
  | `String::from` | `(StrLit) -> String` | Allocates |

- **Rationale**: minimum useful set. `Vec::new()` requires type annotation on the let-binding (e.g. `let v: Vec<i32> = Vec::new();`) — typeck errors if T can't be inferred.

### R-012 — Indexing typeck

- **Decision**: `receiver[index]` requires receiver type `Ty::Vec(T)` and index type to be any `Ty::Int(_)`. Result type is `T` (a copy). Rejects:
  - Non-Vec receiver: "cannot index non-Vec value".
  - Non-integer index: "expected integer index".
- **Rationale**: M07 indexing is rvalue-only on Vec, returning a copy of the element. Matches Rust semantics for `Vec<T> where T: Copy`.

### R-013 — `Ty::Vec(Box<Ty>)` type inference for `Vec::new()`

- **Decision**: `Vec::new()` returns `Ty::Vec(T)` where T is "to-be-inferred". Typeck collects this as a type variable; if the surrounding let-binding has annotation `Vec<U>`, T = U. Otherwise typeck error: "type annotation needed for `Vec::new()` — `let v: Vec<...> = Vec::new();`".
- **Rationale**: matches Rust's behavior (Rust infers from subsequent `push` calls too, but M07 requires explicit annotation for simplicity).

## Eval

### R-014 — Heap state model

- **Decision**:
  ```rust
  struct HeapState {
      next_addr: u32,
      objects: IndexMap<HeapAddr, HeapObject>,
  }
  enum HeapObject {
      Box(Value),
      Vec { elements: Vec<Value>, capacity: usize, elem_ty: Ty },
      Str { bytes: String, capacity: usize },  // Rust's `String` field name conflicts; use Str
  }
  ```
- **Rationale**: per-allocation state. `capacity` tracks Vec/String's underlying buffer size; `elements.len()` gives the logical length.

### R-015 — Vec growth policy

- **Decision**: doubling. Initial capacity 0; first push grows to 1; second to 2; third to 4; fifth to 8; ninth to 16; etc. (Cap = power of 2.) Each capacity increase emits a `HeapRealloc` event with `from = old_addr`, `to = new_addr`, `new_size = new_capacity * elem_size`.
- **Rationale**: deterministic and visible. Matches Rust's actual default growth (modulo allocator alignment hacks Rust does — irrelevant for pedagogy). The doubling makes reallocs visible across multiple pushes (1, 2, 3, 5, 9, ...).

### R-016 — String growth policy

- **Decision**: same as Vec. Initial capacity 0 for `String::new()` (not in scope for M07), or = len of source for `String::from("...")`. `push_str(suffix)` grows by doubling when needed.
- **Rationale**: consistent with Vec.

### R-017 — Dangling-borrow detection

- **Decision**: on `HeapRealloc { from: old, to: new, .. }`, the evaluator scans its active borrow registry. Any borrow whose target is `Pointee::Heap(old)` is dangling. For each, emit `MemEvent::Note { kind: NoteKind::RuntimeError, message: "dangling reference: \`r\` was borrowed here but the underlying memory has been reallocated", span: <original borrow's span> }`.
- **Note**: M07 does NOT halt evaluation at dangling borrows. The trace continues. The Note is informational + an editor highlight at the original borrow site. (Real Rust would not even compile this code; M07 simulates the runtime consequence for pedagogy.)
- **Rationale**: visible without breaking the trace.

### R-018 — Heap-borrow tracking in eval

- **Decision**: in M06.1, `Value::Ref` carried `target_slot: SlotId`. **M07 restructures** to `Value::Ref { target: Pointee, mutable, borrow_id }` where `Pointee` is the existing M03 enum `Slot(SlotId) | Heap(HeapAddr)`. The evaluator's borrow-tracking similarly uses `Pointee`.
- **Rationale**: necessary for heap borrows. Per R-022 below, this is the 4th invocation of the closed-enum-with-revisions rule.

### R-019 — Method-call eval

- **Decision**:
  - `Vec::push(v, x)`: append x to v's elements. If len + 1 > capacity, allocate a new HeapAddr with double capacity, copy elements, emit `HeapRealloc { from: old, to: new, .. }`, free old (implicit in the realloc — old addr is invalidated). Update v's `Value::Vec.addr` to new.
  - `Vec::len(v)`: return `Value::Int { kind: U64, bits: v.elements.len() as i128 }`.
  - `String::push_str(s, suffix_literal)`: append suffix bytes to s. Same realloc logic.
  - `Vec::new`: returns `Value::Vec { addr: <fresh, with empty contents and capacity 0> }`. **Per R-015, no actual HeapAlloc event fires for empty Vec.**
  - `Box::new(v)`: emit `HeapAlloc { addr, size: size_of(T), ty_name, span }`, store `HeapObject::Box(v)` at addr, return `Value::Box { addr }`.
  - `String::from(literal)`: emit `HeapAlloc { addr, size: literal.len(), ty_name: "String", span }`, store `HeapObject::Str { bytes: literal.clone(), capacity: literal.len() }`, return `Value::String { addr }`.

### R-020 — HeapFree at scope exit

- **Decision**: extend `drop_current_scope` to emit `HeapFree { addr, span }` for each Value::Box/Vec/String in the scope's locals (before the existing SlotDrop). Non-Copy types still emit SlotDrop after HeapFree. Order: HeapFree first (deallocate), SlotDrop second (slot's bytes invalidated by reuse).
- **Rationale**: matches Rust's drop semantics. Heap memory is released before the stack slot.

### R-021 — Updating `Value::Ref` consumers

- **Decision**: all sites currently reading `target_slot: SlotId` from `Value::Ref` need updating to `target: Pointee` and the appropriate `Slot(SlotId)` extraction. Cascade affects:
  - `eval.rs::lookup_slot_value` (used for deref-read of Slot-targets)
  - `eval.rs::update_slot_value` (used for deref-write of Slot-targets)
  - `eval.rs::Stmt::Assign` Deref(Ident) lhs branch
  - `eval.rs::Expr::Deref` eval arm
  - `ui.rs::apply_event` SlotWrite arm (binds source_slot to borrow)
  - `ui.rs::render_value` Value::Ref arm
  - New paths for Heap-target borrows in each of the above.
- **Rationale**: necessary cascade. Estimated 10-15 sites; mechanical.

## Protocol

### R-022 — 4th invocation of closed-enum-with-revisions rule

- **Decision**: amend M03's contract to note M07 as the 4th invocation. M07's changes:
  - **Additive variants on `Ty`**: `Box(Box<Ty>)`, `Vec(Box<Ty>)`, `String`.
  - **Additive variants on `Value`**: `Box`, `Vec`, `String` (each carrying `addr: HeapAddr`). Also `Str(String)` for transient string-literal values.
  - **Restructure of `Value::Ref`**: `target_slot: SlotId` → `target: Pointee`. With maintainer consent per the rule's wording (which already allows restructure since M03.2 — see M03 contract).
- **Rationale**: precedent chain M03.1 → M03.2 → M06 → M07. Each documented in M03's contract.

## Animation + heap panel layout

### R-023 — Heap panel layout via flexbox

- **Decision**: `#heap` becomes `display: flex; flex-wrap: wrap; gap: 0.5rem; padding: 1rem; align-content: flex-start;`. Each `.heap-box` is a flex item with `position: relative`. Boxes added on HeapAlloc go to the end; on HeapFree disappear (CSS transition on opacity for graceful exit).
- **Rationale**: simplest layout that handles dynamic add/remove without manual position math.

### R-024 — Realloc animation: stable DOM element + flex reflow

- **Decision**: when HeapRealloc fires, the JS renderer KEEPS the existing DOM element for `from`, just relabels it as `to` (updates `data-heap-addr`) and updates its content. Flex reflow happens automatically if other boxes were added/removed. CSS transition on `.heap-box { transition: transform 300ms ease-out; }` smooths the position shift.
- **Note**: the position shift is implicit from the flex layout — when other boxes change, this box may end up at a new x/y; CSS animates the move. For demos with a single heap allocation (which is most M07 samples), the box stays put visually unless content changes shift the layout.
- **Caveat**: with only one heap allocation and no other boxes, the realloc visually amounts to "the contents update in place." The pedagogy still works (`r` was pointing at old contents that are now invalidated), but the box doesn't move. For more dramatic visual: the renderer could deliberately animate a "blink" on the realloc (border flash to red and back). Plan-phase decides; default is just the implicit flex reflow.
- **Decision (final)**: implement implicit flex reflow only for M07's first cut. If maintainer QA shows the demo isn't dramatic enough, add a border-flash effect as polish.

## Owning arrows (black)

### R-025 — Owning arrow rendering

- **Decision**: extend the existing `#arrow-overlay` SVG. Add a third arrowhead `<marker>` (`arrow-head-owning`, black fill). Add CSS class `.arrow-owning { stroke: black; fill: none; stroke-width: 1.5; marker-end: url(#arrow-head-owning); }`. The `BorrowView` (M06's per-arrow info) is reused for owning arrows too — extend it with a kind field: `BorrowView { source_slot, target: TargetRef, kind: ArrowKind { Shared, Mut, Owning } }` where `TargetRef = SlotTarget(u32) | HeapTarget(u32)`. The renderer queries `data-slot-id` or `data-heap-addr` accordingly.
- **Rationale**: unified arrow rendering pipeline. Owning arrows are just "another kind" with a different color.

### R-026 — Source-slot tracking for owning bindings

- **Decision**: the existing M06 `World.borrows` Vec stores active borrows. Extend to also track "owning relationships" — when a `SlotWrite` lands a `Value::Box/Vec/String { addr }` into a slot, register that slot as the owning source for that heap addr. On `HeapFree`, remove. The StateSnapshot's `borrows: Vec<BorrowView>` (renamed in M07 to `arrows: Vec<ArrowView>` per R-027) includes both borrows and owning relationships.

### R-027 — Rename `BorrowView` → `ArrowView` (small protocol bikeshed)

- **Decision**: rename `BorrowView` → `ArrowView` since it now represents both borrows and ownership. Field updates: `mutable: bool` → `kind: ArrowKind`. Same renderer with conditional class selection.
- **Note**: this is JSON wire format change. The M04 contract permits additive view changes; this is more like a restructure. Document.
- **Alternative considered**: keep `BorrowView` and add a sibling `OwningView`. Cleaner separation but doubles the renderer's per-arrow work. Rejected.

## Constitution

### R-028 — Same vacuous PASS

- **Decision**: `.specify/memory/constitution.md` still unfilled. No gates apply.
