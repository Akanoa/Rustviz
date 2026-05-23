# Contract — M07.2 Protocol Delta

6th invocation of the closed-enum-with-revisions rule. M07.2 adds variants and removes one transient — all changes are clean.

## Closed-enum rule — sixth invocation

| Milestone | Change |
|---|---|
| M03.1 | Added `MemEvent::ReturnValue`; removed `FrameEnter.params` (additive + redundant-field removal). |
| M03.2 | Restructured `Ty` and `Value` (kind-based). Rule generalized to all protocol types. |
| M06 | Added `Ty::Ref`, `Value::Ref`. Filled `MemEvent::BorrowShared/BorrowMut/BorrowEnd` payloads. |
| M07 | Added `Ty::Box/Vec/String`, `Value::Box/Vec/String/Str`. **Restructured `Value::Ref`** (target_slot → target: Pointee). Filled `MemEvent::HeapAlloc/HeapRealloc/HeapFree` payloads. |
| M07.1 | Added `Ty::Slice(Box<Ty>)` and `Value::Slice { borrow_id, target, len, mutable, ... }`. Pure additive. |
| **M07.2** | Adds `StaticAddr` newtype, `Pointee::Static(StaticAddr)`, `Ty::Str` (sugar over `Ty::Slice(Ty::Int(U8))`), `MemEvent::StaticAlloc { addr, bytes, span }`, `ArrowTarget::Static(u32)`. **Removes** `Value::Str(String)` (M07's transient, now dead since literals become `Value::Slice`). |

The `Value::Str` removal is the second variant removal in the project (after M03.1 removed `FrameEnter.params`). Both are dead-code cleanups, not breaking changes — `Value::Str` was never UI-observable (only used in `String::from`'s internal arg-extraction).

## `Pointee` — additive variant

```rust
pub enum Pointee {
    Slot(SlotId),
    Heap(HeapAddr),
    // NEW in M07.2:
    Static(StaticAddr),
}
```

JSON shape gains one new tag: `{ "Static": <u32> }`.

## `Ty` — additive variant

```rust
pub enum Ty {
    Int(IntKind),
    Float(FloatKind),
    Bool,
    Unit,
    Ref { inner: Box<Ty>, mutable: bool },
    Box(Box<Ty>),
    Vec(Box<Ty>),
    String,
    Slice(Box<Ty>),
    // NEW in M07.2:
    Str,
}
```

JSON shape gains one new tag: `{ "Str": null }` (or `"Str"` depending on serde's tagging).

Semantic equivalence with `Slice(Box::new(Int(U8)))`: see VR-5 in data-model.md.

## `Value` — removes `Str`

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
    // REMOVED in M07.2: Str(String)
    Slice { borrow_id, target, start, len, mutable, byte_offset, byte_len },
}
```

JSON wire format: removes the `Str` variant tag. M07 traces containing `Value::Str` (none exist in shipped samples — `Value::Str` was only ever an internal transient for `String::from` plumbing) would fail to deserialize. Confirmed safe: no persisted trace files contain `Value::Str` (M05's trunk-hook pre-recorded traces were dropped; no test snapshots construct it).

## `MemEvent` — additive variant

```rust
pub enum MemEvent {
    // ... existing variants
    StaticAlloc {
        addr: StaticAddr,
        bytes: String,
        span: Span,
    },
}
```

JSON shape gains one new tag: `{ "StaticAlloc": { "addr": ..., "bytes": "...", "span": ... } }`.

### Emission semantics

- **`StaticAlloc { addr, bytes, span }`**: emitted by `Expr::StrLit` eval on the FIRST occurrence of a unique byte-content string. Subsequent literals with identical content reuse the existing addr — no new event.
- **No `StaticFree`**: static blocks persist for the trace's lifetime.
- **Ordering**: a `StaticAlloc` for a given literal fires BEFORE the `BorrowShared` event for that literal's slice. The two events are paired (alloc then borrow); dedup-reused literals only fire the borrow.

### `BorrowShared` for `&str` slices

Same shape as M07.1's `BorrowShared { borrow_id, target: Pointee::Heap(_), span }`, but with `target: Pointee::Static(addr)`. The borrow registers in the active-scope's borrow list and fires `BorrowEnd` at scope exit. The dangling-borrow scan in `realloc_heap` ignores `Pointee::Static(_)` targets (static memory is never freed/moved).

## `ArrowTarget` — additive variant

```rust
pub enum ArrowTarget {
    Slot(u32),
    Heap(u32),
    // NEW in M07.2:
    Static(u32),
}
```

JSON consumers in `web/index.js`:
- `arrow.target.Static` resolves via `[data-static-addr="..."]` (mirrors Heap and Slot resolvers).
- Slice-arrow hover-highlight on byte-cells works the same way as M07.1 for heap targets — query `[data-static-addr=X] .byte-cell`, toggle `.byte-slice-highlighted` on cells in `[byte_offset, byte_offset + byte_len)`.

## `StaticView` (UI snapshot)

```rust
pub struct StaticView {
    pub addr: u32,
    pub bytes: String,
    pub size: u32,
    pub display: String,
}
```

Added to `StateSnapshot.static_region: Vec<StaticView>` with `serde(default, skip_serializing_if = "Vec::is_empty")` — traces without literals omit the field for wire-format backwards-compat.

## Behavioral guarantees (post-M07.2)

- **B-M72-1**: Every string literal `"..."` evaluation interns its bytes in the static region and produces a `Value::Slice { target: Pointee::Static(_), .. }`.
- **B-M72-2**: Two identical literals produce exactly one `StaticAlloc` event (content-dedup); each occurrence produces its own `BorrowShared` event with its own `borrow_id`.
- **B-M72-3**: `String::from(literal)` evaluation emits one `StaticAlloc` (if first) + one `BorrowShared` (for the literal slice) + one `HeapAlloc` (for the fresh String buffer), in that order.
- **B-M72-4**: At end-of-scope for `let s: String`, `HeapFree` fires for `s`'s heap addr; the static block holding the literal stays.
- **B-M72-5**: `Slice::len()` works on `Ty::Str` returning `u64` (extends M07.1's method dispatch).
- **B-M72-6**: `Value::Slice.target = Pointee::Static(_)` borrows never go dangling (static memory is never freed).
- **B-M72-7**: Static blocks render in a distinct visual region with a "static memory (RO)" label.

## What this contract does NOT cover (deferred)

- **`&str` slicing** `&s[1..3]` where `s: &str` — out of scope (M07.1 deferred slice-of-slice).
- **`&str` indexing** `s[0]` — Rust rejects (UTF-8 byte indexing isn't safe); M07.2 follows.
- **`format!`, `println!`, `write!`** — out of scope.
- **String concat operators** `+`, `+=` — out of scope (would require owned-string semantics).
- **Generalized `&str` arg** to `push_str` / `String::from` — out of scope. Args must still be `Expr::StrLit` literals.
- **UTF-8 character-level pedagogy** — static blocks render raw bytes; no `char` boundary visualization.
- **Static integers / arrays** in `static` items — out of scope. Only string literals get static blocks in M07.2.
- **`&'a str` lifetimes** — out of scope (M07.2 only models `&'static str`).
