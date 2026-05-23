# Data Model — M07.2 Entities

Static-memory-focused expansion: 1 new newtype (`StaticAddr`), 1 new `Pointee` variant (`Static`), 1 new `Ty` variant (`Str`), 1 new `MemEvent` variant (`StaticAlloc`), `Value::Str` removed, `ArrowTarget` extended with `Static(u32)`, new `StaticView` for the UI.

All additive variants. `Value::Str` is the only removal (cleanup of a now-dead transient).

## New (newtype): `StaticAddr`

```rust
// In src/event.rs

/// **M07.2**: identifier for a block in the static-memory region (read-only
/// data segment). Distinct from `HeapAddr` because static blocks have
/// different lifetime semantics — they are never freed.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct StaticAddr(pub u32);
```

### Validation rules

- **VR-1**: `StaticAddr.0` is monotonic across the trace (`next_addr` counter; never reused).
- **VR-2**: Static blocks at a `StaticAddr` persist for the full trace (no `StaticFree` event).

## Modified: `Pointee` — adds `Static`

```rust
pub enum Pointee {
    Slot(SlotId),
    Heap(HeapAddr),
    /// **M07.2**: points into the static-memory region (read-only data
    /// segment). Used by string-literal slices (`&'static str`).
    Static(StaticAddr),
}
```

### Validation rules

- **VR-3**: `Pointee::Static(addr)` corresponds to a `StaticAlloc { addr, .. }` event earlier in the trace.
- **VR-4**: `Value::Slice.target = Pointee::Static(_)` is the only way to construct a Static-target slice in M07.2 (created by `Expr::StrLit` eval).

## Modified: `Ty` — adds `Str` sugar

```rust
pub enum Ty {
    // ... existing variants
    /// **M07.2**: `&str` — semantically equivalent to `Ty::Slice(Box::new(
    /// Ty::Int(IntKind::U8)))`. Kept as a distinct variant for cleaner
    /// rendering (`"&str"` instead of `"&[u8]"`) and matching Rust's
    /// user-facing presentation.
    Str,
}
```

### Validation rules

- **VR-5**: `Ty::Str` and `Ty::Slice(Box::new(Ty::Int(IntKind::U8)))` are treated equivalently for borrow-tracking, method dispatch, and aliasing purposes. The distinction is only in rendering.
- **VR-6**: `Ty::is_copy()` returns `false` for `Ty::Str` (same as Slice).
- **VR-7**: `Ty::name()` renders `Ty::Str` as `"&str"`.
- **VR-8**: Method dispatch table treats `(Ty::Str, "len")` the same as `(Ty::Slice(_), "len")` — returns `Ty::Int(IntKind::U64)`.

## Modified: `Value` — removes `Str`

```rust
pub enum Value {
    Int { kind, bits },
    Float { kind, value },
    Bool(bool),
    Unit,
    Ref { borrow_id, target, mutable },
    Box { addr },
    Vec { addr },
    String { addr },
    // REMOVED in M07.2: Str(String) — string literals now become Value::Slice
    Slice { borrow_id, target, start, len, mutable, byte_offset, byte_len },
}
```

### Validation rules

- **VR-9**: `Value::Str` is removed; no code path constructs it after M07.2 ships. Match-exhaustiveness will flag every site that needs to drop its `Value::Str` arm.
- **VR-10**: String literal evaluation returns `Value::Slice { target: Pointee::Static(_), .. }`.

## Modified: `MemEvent` — adds `StaticAlloc`

```rust
pub enum MemEvent {
    // ... existing variants
    /// **M07.2**: a new static-memory block was allocated (i.e., a string
    /// literal whose byte content was not seen before). Fires ONCE per
    /// unique literal content. Subsequent literals with identical content
    /// reuse the existing `addr` without emitting a new event.
    StaticAlloc {
        /// Identifier of the static block.
        addr: StaticAddr,
        /// The block's byte content (already-processed string after escape
        /// resolution).
        bytes: String,
        /// Source location of the literal that first interned this content.
        span: Span,
    },
}
```

### Validation rules

- **VR-11**: `StaticAlloc` fires exactly once per unique byte-content string. Repeated literals with identical content share the addr.
- **VR-12**: No `StaticFree` event exists. Static blocks persist for the trace's lifetime.
- **VR-13**: `addr` is monotonic; `bytes.len()` is the block's size in bytes.

## New: `StaticState` (Evaluator side)

```rust
// In src/eval.rs (private)

struct StaticState {
    next_addr: u32,
    blocks: IndexMap<StaticAddr, StaticBlock>,
    by_content: HashMap<String, StaticAddr>,
}

struct StaticBlock {
    bytes: String,
}
```

### Validation rules

- **VR-14**: `intern_static(bytes, span)` returns `by_content[bytes]` if present (no event); otherwise allocates fresh `StaticAddr`, inserts into both maps, emits `StaticAlloc`, returns new addr.
- **VR-15**: `blocks` (IndexMap) preserves insertion order for deterministic rendering.
- **VR-16**: `next_addr` is monotonic.

## New: `StaticView` (UI side)

```rust
// In src/ui.rs

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StaticView {
    pub addr: u32,
    pub bytes: String,
    pub size: u32,
    pub display: String,
}

pub struct StateSnapshot {
    // ... existing fields
    /// **M07.2**: live static-memory blocks. Never shrinks — static blocks
    /// persist for the trace's lifetime.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub static_region: Vec<StaticView>,
}
```

### Validation rules

- **VR-17**: `static_region` entries appear in `addr` order.
- **VR-18**: `display` is renderer-ready (e.g. `"\"hi\""` with surrounding quotes for visual clarity).
- **VR-19**: `serde(default, skip_serializing_if = "Vec::is_empty")` keeps the wire format backwards-compatible — traces without static blocks omit the field.

## Modified: `ArrowTarget` — adds `Static`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ArrowTarget {
    Slot(u32),
    Heap(u32),
    /// **M07.2**: static-memory block (StaticAddr.0).
    Static(u32),
}
```

### Validation rules

- **VR-20**: `ArrowTarget::Static(_)` arrows have `kind: ArrowKind::Shared` (no mutable static borrows — static memory is read-only).
- **VR-21**: JS resolves `arrow.target.Static` via `[data-static-addr="..."]` lookups (mirrors the existing Heap and Slot resolvers).

## New: M07.2 reference samples

| File | Content | Pedagogy |
|---|---|---|
| `tests/samples/m07_2_str_literal.rs` | `fn main() { let s = "toto"; }` | String literal as `&str`; static block holding `"toto"`; slice arrow with `[len: 4]`; no heap event. |
| `web/samples/m07_2_str_literal.rs` | Mirror. | |
| `tests/samples/m07_2_string_from.rs` | `fn main() { let s = String::from("hi"); }` | Both static `"hi"` block AND heap `String` block visible; owning arrow from `s` to heap; bytes copied from static. |
| `web/samples/m07_2_string_from.rs` | Mirror. | |
| `tests/samples/m07_2_push_str.rs` | `fn main() { let mut s = String::from("hi"); s.push_str("!"); }` | Two static blocks (`"hi"`, `"!"`) plus heap String; push_str copies `"!"` bytes from static into heap buffer. |
| `web/samples/m07_2_push_str.rs` | Mirror. | |
