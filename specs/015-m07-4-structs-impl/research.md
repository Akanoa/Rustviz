# Research — M07.4 Implementation Decisions

17 decisions across parser, AST, typeck, eval, **UI struct visualization (R-016 — the explicit iterate-on-this proposal)**, and protocol amendment.

## Parser

### R-001 — `Item::Struct` parsed by `parse_struct_decl` after `Struct` keyword

- **Decision**: `parse_item` peeks the leading keyword. `Fn` → `parse_fn_decl` (existing); `Struct` → `parse_struct_decl`; `Impl` → `parse_impl_block`.
- **`parse_struct_decl`** consumes `struct`, expects an identifier (the type name), `{`, comma-separated `name: Type` fields (≥ 1 required), `}`. Trailing comma allowed.
- **Empty struct rejected at parse time**: `struct Empty {}` returns a `ParseError { message: "structs in M07.4 must have at least one field", .. }`. Cleaner error than typeck's, and matches the M07.4 scope.
- **Rationale**: minimal additions to the existing item-dispatch path; failure mode is local.

### R-002 — `Item::Impl` parsed by `parse_impl_block` after `Impl` keyword

- **Decision**: `parse_impl_block` consumes `impl`, expects an identifier (the receiver type's name), `{`, zero or more `FnDecl`s, `}`. The fn decls are parsed via the existing `parse_fn_decl` extended to handle self-receivers (R-005).
- **Single-path impl only**: `impl Point` works; `impl path::Point` doesn't (matches M07's struct-path restriction). `impl Trait for Type` — completely out of scope (no traits in M07.4).
- **One impl block per type**: enforced at typeck phase 1, not at parse time. The parser accepts multiple `impl Point { .. }` blocks but typeck rejects the second with a clear error (so the error message can name the prior block's location).
- **Rationale**: parser stays simple; the multi-impl-block restriction is a typeck-level scope concern.

### R-003 — Struct literal disambiguation via `Ident { ... }` at expression-atom position

- **Decision**: in `parse_atom`, after parsing an `Ident` (or a multi-segment `Path` — typeck rejects multi-segment paths for structs in M07.4 but the parser accepts), peek for `LBrace`. If present, parse comma-separated `StructLitField`s.
- **Field syntax**: each `StructLitField` is either `name: expr` (full form) OR `name` (shorthand — `value` is `None`). Shorthand resolves at typeck/eval to the local of the same name.
- **Trailing comma allowed**: `Point { x: 1, y: 2, }` parses.
- **Classic `if cond { ... }` ambiguity**: M07.4 inherits Rust's rule — struct literals are disallowed in cond positions for `if`/`while`. Practical mitigation: typeck rejects non-bool cond expressions (existing M01 behavior), which catches the same cases. **No parser-level cond-position tracking** — keeps the parser simple; the typeck error is just as clear ("expected `bool`, found `Point`").
- **Rationale**: simplest disambiguation. Matches Rust's grammar at the level a learner cares about. Test program `if Point { x: 1, y: 2 }.x > 0 { .. }` doesn't appear in samples — out of scope by happy coincidence.

### R-004 — `Expr::FieldAccess` parsed in postfix loop alongside `MethodCall`

- **Decision**: `parse_expr` postfix loop currently handles `Dot Ident LParen ... RParen` → `MethodCall`. Extend: when `Dot Ident` is NOT followed by `LParen`, produce `Expr::FieldAccess { receiver, name }`.
- **Chained access**: `p.x.y` parses left-to-right as `FieldAccess(FieldAccess(p, "x"), "y")`. Typeck rejects multi-level access in M07.4 (single-level only — see deferral on nested structs).
- **Method call after field access**: `p.x()` is a MethodCall (M07's existing behavior); `p.x.foo()` is FieldAccess then MethodCall.
- **Rationale**: the smallest change to the postfix loop. The disambiguation is local (one-token lookahead).

### R-005 — Self-receiver parsing inside `parse_param` (only at param index 0)

- **Decision**: extend `parse_param`. When the parameter list is non-empty AND `self.cursor` is at param index 0, peek for: `Amp` `SelfKw` → `ParamKind::SelfShared`; `AmpMut` `SelfKw` → `ParamKind::SelfMut`; `SelfKw` (no leading `&`) → `ParamKind::SelfOwned`. Otherwise fall through to the existing `name: Type` path.
- **Resulting `Param`**: name = `"self"`, ty = `Type::Path { segments: [<impl_block_ty_name>], .. }` (synthesized — the actual type comes from the enclosing impl block; the parser uses a placeholder span at the self-receiver's token), kind = the matched `ParamKind`. A later typeck phase 1 step swaps the placeholder type for the real `Ty::Struct(_)` or `Ty::Ref { Ty::Struct(_), .. }`.
- **Out-of-position self rejected**: `fn foo(x: i32, self)` is a `ParseError` ("`self` parameter must be the first parameter").
- **Rationale**: synthesizing a regular `Param` shape keeps the eval-side machinery uniform — method-call frame entry treats `self` as just another param-binding event. The `ParamKind` enum carries the borrow-mode info.

### R-006 — Field-shorthand `Point { x, y }` parsed when `:` is absent

- **Decision**: in `parse_struct_lit_field`, after consuming the field name, peek the next token. `Colon` → full form: parse `value` as the field's expression. Otherwise → shorthand: `value: None`. Eval/typeck later resolve shorthand by looking up a local binding of the same name.
- **Type-check failure for missing local**: a shorthand `Point { x, y }` where `y` is not a bound local fails typeck with "no local named `y` in scope for field-shorthand".
- **Rationale**: small parser concession, big ergonomic payoff. Matches Rust 1.17+ behavior.

### R-007 — `&self` parses as `Amp SelfKw` (NOT as a regular borrow expression)

- **Decision**: at param index 0, the parser disambiguates `Amp` based on what follows. If `SelfKw` → self-receiver. If anything else → fall through (which produces a `ParseError` since regular params start with an ident, not a borrow expression). Mut variant uses the existing `AmpMut` token (which is the special `&mut` 2-char token, NOT `Amp` + `Mut`).
- **Lexer assist**: the existing `AmpMut` token (added in M06.1 for parser disambiguation between borrow expr and `&mut` type annotation) is reused here. No new tokens.
- **Rationale**: zero new lexer surface.

## AST

### R-008 — `Item::Struct { name, fields: Vec<StructField>, span }`

- **Decision**: new `Item` variant + new `StructField { name: String, ty: Type, span: Span }` carrier. Field order is declaration order (drives byte layout AND drop order — drop order is academic in M07.4 since fields are Copy primitives).
- **Rationale**: minimal AST footprint matching the lexical structure.

### R-009 — `Item::Impl { ty_name: String, items: Vec<FnDecl>, span }`

- **Decision**: new `Item` variant carrying the type name + the impl block's fn decls. Each `FnDecl` may have a self-receiver as its first `Param`.
- **No trait field**: traits are completely out of scope; the impl is always an inherent impl.
- **Rationale**: matches the lexical structure of inherent impls.

### R-010 — `Expr::StructLit { path, fields: Vec<StructLitField>, span }`

- **Decision**: `path: Vec<String>` (single segment in M07.4 — `Point`; multi-segment paths typeck-rejected). `fields: Vec<StructLitField>` where `StructLitField { name: String, value: Option<Expr>, span: Span }`. `value: None` indicates field-shorthand.
- **Rationale**: keeps the literal's shape uniform; shorthand encoded by absence of `value` rather than a separate variant.

### R-011 — `Expr::FieldAccess { receiver: Box<Expr>, name: String, span: Span }`

- **Decision**: minimal shape mirroring `MethodCall` minus the `args` list.
- **Rationale**: parallel structure with `MethodCall` keeps the postfix-loop code paths symmetric.

### R-012 — `Param` extended with `kind: ParamKind`

```rust
pub enum ParamKind {
    Normal,
    SelfOwned,    // `self`        (param.ty = Ty::Struct)
    SelfShared,   // `&self`       (param.ty = Ty::Ref { Ty::Struct, mutable: false })
    SelfMut,      // `&mut self`   (param.ty = Ty::Ref { Ty::Struct, mutable: true })
}

pub struct Param {
    pub name: String,
    pub ty: Type,
    pub kind: ParamKind,  // NEW; defaults to Normal for existing free fns
    pub span: Span,
}
```

- **Decision**: kind drives self-receiver detection in typeck (which substitutes the placeholder type with the impl block's real type during phase 1) AND in eval (which binds `self` to a borrow vs an owned value).
- **Back-compat**: existing free fns always have `kind: ParamKind::Normal` — no Debug-format change to existing M01–M07.3 snapshots since the field is new (added to all branches of the parser).
- **Snapshot impact**: M03 snapshot tests may re-baseline minimally if any existing test serializes a `Param` directly. Spot-check: search for `Param {` in snapshot files. **Mitigation**: snapshot tests serialize `MemEvent`s, not AST nodes; expected zero impact.

## Typeck

### R-013 — `Ty::Struct { name: String, fields: Vec<(String, Ty)> }`

- **Decision**: new `Ty` variant. Nominal typing: two `Ty::Struct` are equal iff their `name` matches (the `fields` are redundant for equality — they're carried for convenience so callers don't always need a registry lookup).
- **`Ty::name()`**: returns the bare name (`"Point"`).
- **`Ty::is_copy()`**: returns true (every field is restricted to primitives in M07.4, all Copy; the struct is Copy). Future milestones with non-Copy field types will refine to "Copy iff every field is Copy".
- **Rationale**: simplest representation matching Rust's nominal struct typing. Carrying the fields inline avoids registry round-trips in eval.

### R-014 — `Value::Struct { name: String, fields: Vec<(String, Value)> }`

- **Decision**: new `Value` variant. Fields stored in declaration order (so byte-offset computations are positional, not by-name). Name is carried for the `value_size_bytes` / `render_value` paths and for the UI's struct name label.
- **`Value::type_name()`**: returns `"{}"` (short tag; full `"Point"` rendering comes from the `Ty` layer).
- **Cloning**: deep-clone the `Vec<(String, Value)>` (each field's `Value` cloned). Used for Copy-style assignment `let p2 = p` (since structs are Copy in M07.4).
- **Rationale**: parallel structure with `Value::Array { elements, elem_ty }` but field-indexed instead of element-indexed. Minimal new state.

### R-015 — `Value::Ref` extended with `field_path: Vec<String>` (NOT a new variant)

- **Decision**: extend the existing `Value::Ref` struct-variant with a new `field_path: Vec<String>` field. Empty = the ref points at the whole binding (existing M06/M07/M07.1/M07.2/M07.3 semantics). Non-empty = the ref points at a sub-field of the binding's struct value.
- **Single-segment paths in M07.4**: only `vec!["x"]` shapes — multi-level access (`p.x.y`) is out of scope. `field_path` is a `Vec` to leave room for nested structs (future milestone) without another protocol revision.
- **Serde**: `#[serde(default, skip_serializing_if = "Vec::is_empty")]`. Existing M06/M07/M07.1/M07.2/M07.3 borrow snapshots stay byte-identical (empty `field_path` is omitted from JSON; deserialization defaults to empty).
- **Rendering**: when `field_path` is non-empty, `render_value` produces `"&p.x"` instead of `"&p"`. The slot's target name is the binding name; the field path is appended with `.`.
- **Rationale**: extension over new variant matches the existing pattern (cf. `MemEvent::HeapAlloc.split_remainder` added in M07.2). Avoids splitting `Value::Ref` consumers across two variants.

### R-016 — UI struct visualization in the stack slot (THE ITERATE-ON-THIS PROPOSAL)

This is the meaty piece the user has flagged for step-by-step iteration. The data flow is **locked in** — what's iterative is the **visual rendering**.

#### Locked-in data shape

```rust
// In src/ui.rs — extension of SlotRowView (already touched by M07.3's inline_cells)
pub struct SlotRowView {
    pub slot_id: u32,
    pub name: String,
    pub ty: String,
    pub value: Option<String>,
    pub inline_cells: Option<InlineCellsView>,
    /// **M07.4**: present when the slot holds a `Value::Struct`. Drives the
    /// per-field byte-cell strip + field-name labels in the slot's value
    /// area. Mutually exclusive with `value` and `inline_cells`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub struct_view: Option<StructView>,
}

pub struct StructView {
    /// Struct type name (`"Point"`).
    pub name: String,
    /// Fields in declaration order. Each entry drives one row in the rendered
    /// struct (per the recommended visual below).
    pub fields: Vec<StructFieldView>,
}

pub struct StructFieldView {
    /// Field name (`"x"`, `"y"`).
    pub name: String,
    /// Field type label (`"i32"`, `"bool"`). Drives the per-field type column.
    pub ty_label: String,
    /// Byte size of this field. Drives the per-field byte-cell count.
    pub size: u32,
    /// Rendered field value (`"1_i32"`, `"true"`).
    pub display: String,
}
```

#### Recommended visual — Proposal A: vertical labeled rows (PRIMARY)

```
┌──────────────────────────────────────────────────┐
│ p : Point                                        │
│ ┌────────────────────────────────────────────┐   │
│ │ x: i32                                     │   │
│ │   ▦▦▦▦                          = 1_i32    │   │
│ │ y: i32                                     │   │
│ │   ▦▦▦▦                          = 2_i32    │   │
│ └────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────┘
```

CSS sketch (kept additive over M07.3's `.stack-inline-cells`):

```css
.struct-view { display: flex; flex-direction: column; gap: 2px;
               border: 1px solid #999; padding: 4px; max-width: 240px; }
.struct-field { display: grid; grid-template-columns: auto 1fr auto;
                gap: 6px; align-items: center; font-size: 11px; }
.struct-field-label { font-family: ui-monospace, monospace; color: var(--muted); }
.struct-field-cells { display: flex; gap: 1px; }
.struct-field-cells .byte-cell { width: 8px; height: 8px; border: 1px solid #999;
                                 background: #c8c8c6; box-sizing: border-box; }
.struct-field-value { font-family: ui-monospace, monospace; color: inherit; }
.struct-field.field-borrow-highlighted { background: #fffde7; outline: 2px solid #fbc02d; }
```

**Why this as the primary**:
- Clearest visual hierarchy — field name + type label sit on a dedicated line above the byte-cells; impossible to confuse with adjacent fields.
- Per-field hover highlight is trivial: query `[data-slot-id=X] .struct-field[data-field-name="x"]` and toggle a class. The whole row (label + cells + value) lights up as one.
- Extends naturally to nested structs (future): a `Point` field could render inline as a sub-`.struct-view` inside its row.
- Vertical growth is acceptable since M07.4 structs are bounded to ≤ 5 fields by the primitive-only restriction (no nested structs).

**Trade-off**: vertical real estate. A 5-field struct takes 5 rows; the stack panel scrolls more.

#### Alternative — Proposal B: compact horizontal segments

```
p : Point
  ┌──x──┐  ┌──y──┐
  ▦▦▦▦   ▦▦▦▦
   1_i32  2_i32
```

CSS sketch: per-field segments laid out with `display: flex` horizontally; labels above, cells in middle, values below.

**When it would win**: structs are short (≤ 3 fields), all primitives, want to mirror the actual memory layout (contiguous bytes left-to-right). Pedagogically: emphasizes "memory order = declaration order" stronger than Proposal A.

**Trade-off**: field name labels can clash with value labels for narrow fields (i32 = 4 cells wide; "field_name = 1_i32" might wrap into the next field's column). Per-field hover requires byte-index bookkeeping (the highlighted column needs to span "label + cells + value" — the JS gets fiddlier).

#### Alternative — Proposal C: single byte strip with field-name brackets

```
p : Point
  ┃─ x ─┃─ y ─┃              ← field-name brackets above
  ▦▦▦▦▦▦▦▦                   ← single contiguous byte strip
    1_i32   2_i32             ← per-field values below
```

**When it would win**: hyper-faithful to Rust's memory layout — emphasizes that a struct IS a contiguous byte sequence. Most "pedagogically pure" of the three.

**Trade-off**: brackets are CSS-hard (have to render with positioned pseudo-elements OR a separate table-like layout); per-field hover requires byte-index ranges (highlight bytes [0..4] for `x`, bytes [4..8] for `y`); doesn't scale past ~4 fields before the brackets visually clutter.

#### Recommendation summary

| | Pedagogical clarity | Per-field hover impl | Layout cost | Scales to >5 fields |
|--|--|--|--|--|
| **A: Vertical rows** | High (field-by-field) | Trivial | Tall | Yes |
| **B: Horizontal segments** | Medium | Medium (column-span) | Wide | No (>3 fields) |
| **C: Bracketed strip** | High (memory-faithful) | Hard (byte ranges) | Compact | No (>4 fields) |

**Plan-phase pick: Proposal A** as the starting implementation target. User explicitly flagged this as the iterate-on-this part — implementation halts after rendering the first sample for a UX checkpoint before refining or switching proposals.

### R-017 — Auto-deref for `self.x` inside method bodies

- **Decision**: typeck rule (NOT parser-level sugar). `typecheck_field_access` accepts both `Ty::Struct(_)` AND `Ty::Ref { inner: Ty::Struct(_), .. }` receivers, reading the field type uniformly.
- **Eval mirror**: `Expr::FieldAccess` on a `Value::Ref { target: Pointee::Slot(_), .. }` looks up the target slot's `Value::Struct` and reads the field by name. For `Value::Ref { field_path: vec!["x"], .. }` (a chained `&p.x` ref re-derefed), navigate the path then read.
- **Rationale**: typeck-rule approach is more flexible than parser sugar — works uniformly for explicit deref-then-access (`(*r).x` where `r: &Point`) AND for the method-body sugar (`self.x` where `self: &Self`).

### R-018 — Method dispatch tie-breaker: hardcoded built-ins win

- **Decision**: in `typecheck_method_call`, dispatch order is (1) hardcoded M07 built-ins (`Vec::push`, `Vec::len`, `Slice::len`, `String::push_str`, etc.), then (2) user-defined methods from `ImplRegistry.methods.get((struct_name, name))`. If neither matches, error "no method `<name>` on type `<ty>`".
- **Rationale**: hardcoded built-ins are pedagogically "stdlib" — user impls extend the language, they shouldn't shadow stdlib. If a learner writes `impl Vec { fn len(&self) -> i32 { ... } }`, the built-in `Vec::len` still wins (and the user's `len` is unreachable). Edge case: M07.4 typeck rejects `impl Vec { .. }` outright since `Vec` is not a user-defined struct name (it's a typeck-builtin) — same for `impl String`, `impl Box`, etc. So this tie-breaker is more theoretical than practical, but the rule is documented.

## Eval

### R-019 — Method-call frame entry (uniform with free-fn calls)

- **Decision**: when typeck resolved a method call to a user-defined `ImplRegistry.methods` entry, eval enters a new frame:
  1. `FrameEnter { frame_id: <new>, fn_name: "Point::x" /* mangled */, span: <call_site> }`.
  2. `SlotAlloc { name: "self", ty: <Ty::Ref or Ty::Struct>, .. }` then `SlotWrite` binding the receiver.
     - For `&self` methods: receiver evaluated, `Value::Ref { target: Pointee::Slot(receiver_slot), .. }` written.
     - For `&mut self` methods: same but `mutable: true`.
     - For `self` methods (owned receiver): the receiver value moved into the slot.
  3. For each explicit param: `SlotAlloc` + `SlotWrite` with the arg's value.
  4. Execute the method body via `eval_block`.
  5. `ReturnValue` + `FrameLeave`.
- **Rationale**: reuses the existing fn-call event flow verbatim. Method dispatch is just a different lookup mechanism — once the right `FnDecl` is found, eval is identical to a free-fn call.

### R-020 — Associated function call: same as method call, no `self`

- **Decision**: `Expr::Call` with `callee = Expr::Path { segments: ["Point", "new"], .. }` that resolved to `ImplRegistry.assoc_fns` enters a frame the same way as a free-fn call: `FrameEnter`, then `SlotAlloc`/`SlotWrite` for each explicit param (no `self`), execute body, `ReturnValue` + `FrameLeave`.
- **Rationale**: associated fns are free fns nested inside an impl block; eval treats them as exactly that.

### R-021 — Field borrow eval emits `Value::Ref` with `field_path = vec![name]`

- **Decision**: when `Expr::Borrow { inner: Expr::FieldAccess { receiver: Expr::Ident(name, _), name: field_name, .. }, .. }`:
  1. Resolve `receiver` to its slot via `lookup_local_slot`.
  2. Take the borrow on the receiver's binding via `borrow_tracker.try_take_shared` / `try_take_mut`.
  3. Construct `Value::Ref { borrow_id, target: Pointee::Slot(receiver_slot), mutable, field_path: vec![field_name.clone()] }`.
  4. **Skip `BorrowShared`/`BorrowEnd` events** — slot-target borrows use M07.3's lazy-materialization pattern (the UI's `apply_event` SlotWrite arm materializes the borrow when it sees the `Value::Ref` with field_path).
- **Multi-level access rejected**: `&p.x.y` typeck-rejected ("nested field borrows not supported in M07.4 — use intermediate let bindings").
- **Rationale**: minimal extension to existing borrow eval; the field_path bookkeeping happens entirely at the value layer.

## Protocol

### R-022 — 8th invocation of the closed-enum-with-revisions rule

- **Decision**: amend M03's contract to note M07.4 as the 8th invocation. M07.4's changes:
  - **Additive variant on `Ty`**: `Struct { name, fields }`.
  - **Additive variant on `Value`**: `Struct { name, fields }`.
  - **Extension of existing `Value::Ref`**: new `field_path: Vec<String>` field with serde default + skip-when-empty.
  - **No new MemEvent variants**.
  - **No new Pointee variants**. `Pointee::Slot(_)` (M03; M07.3) carries field borrows.
  - **Param shape extension**: `kind: ParamKind` field added. Pure AST-side; no protocol/snapshot impact since `Param` isn't serialized in event traces.
- **Precedent chain**: M03.1 → M03.2 → M06 → M07 → M07.1 → M07.2 → M07.3 → **M07.4**.
- **Snapshot byte-identity**: M01/M02 stay clean (no struct types in samples). M03 snapshots stay byte-identical because (a) no M03 sample constructs `Value::Struct`, (b) `Value::Ref`'s `field_path` is serde-default-empty (skipped in JSON when empty), so existing borrow snapshots are unchanged. Verify with `cargo insta test` before merging.
