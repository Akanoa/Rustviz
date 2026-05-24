# Research — M07.3 Implementation Decisions

12 decisions across parser, AST, typeck, eval, UI inline-cell rendering, and protocol amendment.

## Parser

### R-001 — `Expr::ArrayLit` and `Type::Array` share no parser code; LBracket disambiguates by context

- **Decision**: in `parse_atom`, `LBracket` triggers array-literal parsing (`[e1, e2, ..., eN]`). In `parse_type`, `LBracket` triggers array-type parsing (`[T; N]`). The two grammar productions are completely separate; no shared code.
- **Postfix index vs. atom literal**: the existing Pratt parser already handles `[` as a postfix operator (for `expr[i]` and `expr[range]`) inside the postfix loop. The atom-level `[` for ArrayLit is checked BEFORE entering the postfix loop. No grammar conflict.
- **Rationale**: simplest disambiguation that fits the existing parser's structure.

### R-002 — Empty array literal `[]` requires type annotation

- **Decision**: `let t = [];` is a typeck error ("cannot infer element type for empty array literal — add a type annotation like `let t: [i32; 0] = [];`"). With annotation it works.
- **Rationale**: matches Rust's behavior. Empty literal has no element to infer T from.

### R-003 — Array size: integer literal only, no const expressions

- **Decision**: `Type::Array.size` is a `u64` parsed from a `TokenKind::Int(n, _)`. Negative values rejected at parse time. No `[T; N + 1]` or `[T; SOME_CONST]`.
- **Rationale**: const generics + const-eval are well out of M07's scope. Literal-only keeps the parser simple and matches the realistic pedagogical examples a learner would write.

## AST

### R-004 — `Expr::ArrayLit { elements: Vec<Expr>, span }`

- **Decision**: single variant carrying the element expressions. No size field — the size is `elements.len()`.
- **Rationale**: minimal AST footprint. Inferred-size annotations like `let t: [i32; _] = [1, 2, 3];` aren't supported (size must be a literal).

### R-005 — `Type::Array { inner: Box<Type>, size: u64, span }`

- **Decision**: separate variant from `Type::Slice` (M07.2 added — `&[T]`). Carries both element type and compile-time size.
- **Rationale**: structural parallelism with `Ty::Array(Box<Ty>, u64)` at the type-system layer. Distinct from `Type::Slice` because slices erase size.

## Typeck

### R-006 — `Ty::Array(Box<Ty>, u64)`

- **Decision**: new `Ty` variant. Element type + compile-time size. Distinct from `Ty::Vec(T)` (size unknown at compile time, heap-allocated) and `Ty::Slice(T)` (size erased, lives behind a borrow).
- **Rationale**: cleanest representation matching Rust's three distinct sequence-of-T types. `Ty::name()` renders `"[i32; 3]"`.
- **`is_copy()`**: returns true iff element type is Copy. M07.3 restricts to primitive elements (always Copy), so always true. Future M07.x with non-Copy elements would refine.

### R-007 — Array literal element-type unification

- **Decision**: typecheck the first element to get a baseline type T. For each subsequent element, attempt to coerce its type to T using existing `try_coerce_to` (handles literal-narrowing — `[1u8, 2]` → both `u8`). If coercion fails, typeck error: `"array elements must all have the same type, found `<other>` (expected `<first>`)"`.
- **Rationale**: matches Rust's element-type inference. The first element anchors the type; later elements coerce.

### R-008 — Type-annotation length check

- **Decision**: when both annotation `[T; N]` and literal are present, the literal's element count MUST equal N. Mismatch is a typeck error: `"array length mismatch: annotation specifies N elements, literal has M"`.
- **Rationale**: matches Rust. Defensive: catches the common mistake of `let t: [i32; 3] = [1, 2]`.

### R-009 — `Slice::len` extends to `Ty::Array`-receiver case

- **Decision**: extend method dispatch with `(Ty::Array(_, N), "len") -> Ty::Int(IntKind::U64)`. Eval returns `N` (= `elements.len()`).
- **Rationale**: arrays have a `.len()` method in Rust matching their compile-time size.

### R-010 — Slicing an array produces `Ty::Slice(T)`, not `Ty::Array(T, M)`

- **Decision**: `&t[1..3]` on `t: [i32; 4]` produces `&[i32]` (slice — size erased), NOT `&[i32; 2]`. Same shape as slicing a Vec or `&str`.
- **Rationale**: matches Rust. Size info is lost when you take a borrow with a range index — Rust's `&[T; N]` arrays-of-references aren't constructible this way.

## Eval

### R-011 — `Value::Array { elements: Vec<Value>, elem_ty: Ty }`

- **Decision**: new Value variant holding the element values inline. `elem_ty` is the element type (used for sizing computations and as parent to `Ty::Slice(elem_ty)` when sliced).
- **Rationale**: minimal new state. Arrays don't allocate heap memory, so no `HeapAddr` field. The `Vec<Value>` is an implementation detail of the value, not an event-visible heap allocation.
- **`type_name()`**: returns `"[]"` (short tag; full type name comes from the `Ty` layer).
- **No SlotMove on assignment**: since arrays are Copy in M07.3, `let t2 = t1;` clones the Value::Array and both t1 and t2 remain usable. Matches Rust's semantics for `[T; N] where T: Copy`.

### R-012 — Slot-target slice borrow: receiver's slot as the source

- **Decision**: when `eval_slice_borrow` is called with a Value::Array receiver, the source slot is the receiver's `Expr::Ident` slot (looked up via `lookup_local_slot`). Plan path:
  1. Detect receiver is `Expr::Ident(_)` (M07.3 only supports slicing directly-bound arrays — `&arr_literal[1..3]` isn't in scope).
  2. Resolve `binding_id` → `slot_id` via `lookup_local_slot`.
  3. Eval the receiver to confirm it's `Value::Array { elements, elem_ty }`.
  4. Bounds-check the range against `elements.len()`.
  5. Construct `Value::Slice { target: Pointee::Slot(slot_id), byte_offset, byte_len, ... }`.
  6. Skip BorrowShared/BorrowEnd events (Slot targets use lazy materialization per M07.2's pattern).
- **Rationale**: simplest mapping. Slicing temporaries (`&[1,2,3][1..2]`) is out of scope.

## UI

### R-013 — `SlotRowView` gains optional `inline_cells: Option<InlineCellsView>`

- **Decision**: parallel to `value: Option<String>` — when the slot holds a Value::Array, populate `inline_cells` with `{ size, used, elements }` and leave `value` empty. Otherwise, `value` is populated and `inline_cells` stays None.
- **Wire format**: `#[serde(default, skip_serializing_if = "Option::is_none")]` keeps existing slot-row JSON unchanged when no array is in the slot.
- **Rationale**: clean separation. JS renders one or the other based on which field is present.

### R-014 — Distinct CSS class for stack inline cells

- **Decision**: `.stack-inline-cells` (separate from `.heap-cells` and `.static-cells`). Gray-tinted byte-cell background to convey "stack memory". Filled cells use the same `byte-used` modifier as heap; visual difference is in the base background color.
- **Hover highlights**: the existing `.byte-slice-highlighted` + `.elem-slice-highlighted` classes apply regardless of parent — the M07.2 broadening (`.elem-cell.elem-slice-highlighted`) covers stack-inline-cells too. Slice-hover queries extend to include `.stack-inline-cells .byte-cell` alongside the existing heap/static queries.
- **Rationale**: minimal CSS additions. The hover-highlight machinery generalizes cleanly.

## Protocol

### R-015 — 7th invocation of the closed-enum-with-revisions rule

- **Decision**: amend M03's contract to note M07.3 as the 7th invocation. M07.3's changes:
  - **Additive variant on `Ty`**: `Array(Box<Ty>, u64)`.
  - **Additive variant on `Value`**: `Array { elements: Vec<Value>, elem_ty: Ty }`.
  - **No new MemEvent variants**. Existing `SlotAlloc` + `SlotWrite` events carry array values.
  - **No new Pointee variants**. `Pointee::Slot(_)` (declared in M03, used by M06 for `&x`-style borrows) is now ALSO produced by `Value::Slice` for slot-targeted slice borrows.
- **Rationale**: precedent chain M03.1 → M03.2 → M06 → M07 → M07.1 → M07.2 → M07.3.
- **Pure additive**: no restructure of any existing variant. M03 snapshot tests stay byte-identical because no existing sample constructs `Ty::Array` or `Value::Array`.
