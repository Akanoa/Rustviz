# Contract — M03.2 Protocol Delta

M03.2 modifies M03's existing public contract on two surfaces: `Ty` (now 14 + 2 = 16 type-name leaves under 4 variant constructors) and `Value` (unified `Int { kind, bits }` + `Float { kind, value }` shape). Behavior changes: integer overflow halts via the existing `Note { kind: RuntimeError }`; float `Inf`/`NaN` surfaces via the existing `Note { kind: Info }`. This document is the **delta**; the unchanged surface stays in `specs/004-m03-event-eval/contracts/m03-api.md`. After M03.2 ships, M03's contract is amended in-place with these changes cross-referenced here.

## Closed-enum rule — extended to `Ty` and `Value`

**Before (M03 contract, post-M03.1 amendment)**: the `MemEvent` enum's variant set is stable from M03 onward, with additive variants and redundant-field removal permitted in revision milestones.

**After (M03 contract, amended by M03.2)**: the same rule now applies to **`Ty` and `Value` too**. Specifically:

- `Ty`'s **variant constructors** (`Int`, `Float`, `Bool`, `Unit`) are stable. Adding new constructors requires maintainer consent + coordinated update of all consumers.
- `IntKind`'s variants (`I8`, …, `USize`) and `FloatKind`'s variants (`F32`, `F64`) are stable but may grow additively in revision milestones (e.g. a hypothetical future M03.3 adding `F16`).
- `Value`'s constructors (`Int`, `Float`, `Bool`, `Unit`) are stable. The internal field shape (`Int { kind, bits }`, `Float { kind, value }`) is documented and stable as of M03.2.

This generalizes the M03.1 "additive revision" rule from just `MemEvent` to the broader event-protocol type universe.

## `Ty` — shape change (breaking)

**Before (M03 + M03.1)**:

```rust
pub enum Ty {
    I32,
    Bool,
    Unit,
}
```

**After (M03.2)**:

```rust
pub enum Ty {
    Int(IntKind),
    Float(FloatKind),
    Bool,
    Unit,
}
```

**Migration**: every site referencing `Ty::I32` becomes `Ty::Int(IntKind::I32)`. This is a breaking change to the M03 contract's `Ty` shape; M03.2 invokes the closed-enum relaxation (above) to authorize it.

The `is_copy()` and `name()` methods are preserved (semantics identical for the existing `I32`/`Bool`/`Unit`).

## `Value` — shape change (breaking)

**Before (M03 + M03.1)**:

```rust
pub enum Value {
    Int(i64),
    Bool(bool),
    Unit,
}
```

**After (M03.2)**:

```rust
pub enum Value {
    Int { kind: IntKind, bits: i128 },
    Float { kind: FloatKind, value: f64 },
    Bool(bool),
    Unit,
}
```

**Migration**:

- `Value::Int(5)` → `Value::Int { kind: IntKind::I32, bits: 5 }`.
- JSON wire shape: `{"Int": 5}` → `{"Int": {"kind": "I32", "bits": 5}}`. JS consumers (M04/M05 render path) update their `Value → display string` conversion.
- Debug format: `Int(5)` → `Int { kind: I32, bits: 5 }`. M03 snapshot tests re-baseline.

`Value` continues to derive `Clone, Debug, PartialEq, Serialize, Deserialize`. **No `Eq`** (floats don't impl `Eq`; verified by audit that `Value` never had `Eq`). `MemEvent` is similarly unaffected.

## `IntKind` / `FloatKind` — new helper types

Defined in `src/typeck.rs`. Re-exported from `src/lib.rs`.

```rust
pub enum IntKind {
    I8, I16, I32, I64, I128,
    U8, U16, U32, U64, U128,
    ISize, USize,
}

pub enum FloatKind {
    F32, F64,
}
```

Both have:
- `Copy + Clone + Debug + PartialEq + Eq + Hash + Serialize + Deserialize` derives.
- A `name() -> &'static str` method.

`IntKind` additionally has:
- `min_value() -> i128`, `max_value() -> i128`, `contains(i128) -> bool`, `is_signed() -> bool`.

**Stability**: closed enum, additive growth permitted in revision milestones.

## `MemEvent` variants — unchanged

M03.2 emits no new `MemEvent` variants. The existing `Note { kind: NoteKind::RuntimeError | NoteKind::Info, message, span }` is sufficient for both integer overflow (RuntimeError, halts) and float NaN/Inf (Info, does not halt). `SlotWrite`, `SlotAlloc`, etc. continue with the new `Value` shape transparently.

## Behavioral additions

- **B-M3.2-1**: For any `IntKind`, arithmetic operations `+`/`-`/`*` that produce a result outside the type's range emit `Note { kind: RuntimeError, message: "<type> overflow: …" }` and halt the trace.
- **B-M3.2-2**: For `FloatKind::F64`, native `f64` arithmetic produces `±Inf` on overflow and `NaN` on `0.0/0.0`. The first such occurrence (where neither operand was already special) emits `Note { kind: Info, message: "produced <NaN|+Inf|-Inf>" }`. Trace continues.
- **B-M3.2-3**: For `FloatKind::F32`, arithmetic is computed in `f64` and narrowed; if the narrowed result is `±Inf` or `NaN` (and neither operand was already special), the Info note fires.
- **B-M3.2-4**: Cross-type arithmetic (e.g. `Ty::Int(U8) + Ty::Int(I32)` or `Ty::Int(I32) + Ty::Float(F64)`) is a typeck error with span on the right operand. Same applies to mixing `Bool` with numeric.
- **B-M3.2-5**: Literal-out-of-range (e.g. `let x: u8 = 300;`) is a typeck error with span on the literal.
- **B-M3.2-6**: Unary `-` on an unsigned-integer-typed expression is a typeck error (matches Rust's missing `Neg` impl).
- **B-M3.2-7**: `usize` / `isize` have the range of `u64` / `i64` for visualizer determinism.

## What this contract does NOT cover (deferred)

- **Literal type suffixes** (`5u8`, `2.5_f32`): annotation-driven typing only.
- **`as` casts**: implicit annotation-driven conversion only.
- **`f16` / `f128`**: not in stable Rust; not in scope.
- **Mixed-width promotion** (e.g. `u8 + i32` auto-promoting to `i32`): not Rust semantics; rejected as typeck error.
- **Pedagogical UX for NaN ordering** (e.g. visually showing that `NaN < 1.0 == false`): not covered. Use Rust's `PartialOrd` semantics; no special UI.
