# Data Model — M03.2 Entities

M03.2 modifies three top-level enums (`Ty`, `Value`, AST `Expr`) and introduces two new helper enums (`IntKind`, `FloatKind`).

## New: `IntKind`

```rust
// In src/typeck.rs

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum IntKind {
    I8, I16, I32, I64, I128,
    U8, U16, U32, U64, U128,
    ISize, USize,
}

impl IntKind {
    pub fn min_value(self) -> i128 { /* ... */ }
    pub fn max_value(self) -> i128 { /* ... */ }
    pub fn contains(self, v: i128) -> bool {
        v >= self.min_value() && v <= self.max_value()
    }
    pub fn is_signed(self) -> bool { /* I* + ISize → true */ }
    pub fn name(self) -> &'static str { /* "u8", "i32", "usize", ... */ }
}
```

### Validation rules

- **VR-1**: `IntKind::contains` is the single source of truth for range. typeck calls it for literal range checks; eval calls it for overflow detection.
- **VR-2**: `USize` and `ISize` share min/max with `U64` and `I64` respectively (FR-011).
- **VR-3**: `is_signed()` is exhaustive: `I*` and `ISize` are signed; `U*` and `USize` are not.
- **VR-4**: `name()` returns the Rust type-name verbatim. `Ty::name()` calls into this for integer kinds.

## New: `FloatKind`

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FloatKind {
    F32, F64,
}

impl FloatKind {
    pub fn name(self) -> &'static str { /* "f32" or "f64" */ }
}
```

### Validation rules

- **VR-5**: `FloatKind` is closed (2 variants). Future revision adds `F16`/`F128` if Rust stabilizes them.

## Modified: `Ty` — nested kind enums

```rust
// In src/typeck.rs

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Ty {
    Int(IntKind),
    Float(FloatKind),
    Bool,
    Unit,
}

impl Ty {
    pub fn name(self) -> String { /* delegate to IntKind/FloatKind or static for Bool/Unit */ }
    pub fn is_copy(self) -> bool { true /* all 14 variants are Copy in L1 */ }
}
```

### Validation rules

- **VR-6**: Every existing `Ty::I32` site refactors to `Ty::Int(IntKind::I32)`. Mechanical change but touches many files.
- **VR-7**: `Ty::is_copy()` returns `true` for all variants (still — all integers + floats + bool + unit are Copy in L1). M07+ will add non-Copy variants under a separate constructor.
- **VR-8**: `Ty::name()` now returns `String` (allocates) instead of `&'static str` — the int/float kinds delegate to their kind's name(). If allocation is unwelcome here, plan-phase could use a 16-arm match returning `&'static str` per (Ty, kind) pair. Trivial.

## Modified: `Value` — unified scalar form

```rust
// In src/event.rs

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Value {
    Int { kind: IntKind, bits: i128 },
    Float { kind: FloatKind, value: f64 },
    Bool(bool),
    Unit,
}
```

### Validation rules

- **VR-9**: `Value::Int.bits` is stored as `i128` regardless of the actual `IntKind`. Convert via `kind.contains(bits)` for range checks; truncate via `bits as u8` etc. when narrow-width display is needed.
- **VR-10**: `Value::Float.value` is stored as `f64`. For `FloatKind::F32`, narrow via `value as f32` on display and after each arithmetic op (to detect f32-specific overflow → Inf).
- **VR-11**: `Value` derives `PartialEq` (not `Eq`) — confirmed safe because the existing M03 `Value` already lacks `Eq`. `MemEvent` also lacks `Eq`.
- **VR-12**: `PartialEq` on `Value::Float` follows IEEE 754 — `NaN != NaN` returns `false`. Test assertions using `assert_eq!` on NaN-containing values must use a custom helper.
- **VR-13**: Debug format changes — `Value::Int { kind: I32, bits: 5 }` not `Int(5)`. Forces M03 snapshot re-baseline (R-014).

## Modified: AST `Expr` — new `FloatLit` variant

```rust
// In src/parse/ast.rs

pub enum Expr {
    IntLit(i64, Span),
    FloatLit(f64, Span),  // NEW
    BoolLit(bool, Span),
    // ... existing variants
}
```

### Validation rules

- **VR-14**: `FloatLit` stores `f64`; narrows to `f32` at typeck if the annotation says `f32`.
- **VR-15**: The lexer produces `Token::Float(f64)` when it sees `digits.digits`. The parser consumes `Token::Float(v)` into `Expr::FloatLit(v, span)`.
- **VR-16**: A bare `digits` literal (no `.`) parses as `Expr::IntLit` regardless of context. typeck assigns the type from annotation or defaults to `i32`.

## Modified: Lexer / Token

```rust
// In src/parse/token.rs

pub enum TokenKind {
    Int(i64),
    Float(f64),  // NEW
    // ... existing
}
```

### Validation rules

- **VR-17**: A `digits.digits` sequence becomes a single `Token::Float(f64)`. The intermediate `.` is consumed as part of the float literal, not as a standalone `Token::Dot`.
- **VR-18**: A `digits.alpha` sequence (no L1 use case yet but anticipating M07+) lexes as `Int(digits)` + `Dot` + `Ident(alpha)`. Two-char lookahead at the `.` is enough.

## Re-baselined artifacts

| Path                                           | Change                                              |
|------------------------------------------------|-----------------------------------------------------|
| `tests/snapshots/emits_*.snap`                 | re-baselined (Value Debug format changes)           |
| `tests/snapshots/m01_*.snap`, `m02_*.snap`     | unchanged (M01/M02 don't reference Value)           |

## New: M03.2 reference samples

| File                          | Notes                                                          |
|-------------------------------|----------------------------------------------------------------|
| `tests/samples/m03_2_basic_u8.rs`     | `let a: u8 = 5; let b: u8 = 3; let c: u8 = a + b;` |
| `web/samples/m03_2_basic_u8.rs`       | Same.                                                |
| `tests/samples/m03_2_u8_overflow.rs`  | `let a: u8 = 250; let b: u8 = a + 10;` → halt.       |
| `web/samples/m03_2_u8_overflow.rs`    | Same.                                                |
| `tests/samples/m03_2_float_nan.rs`    | `let a: f64 = 0.0; let b: f64 = a / a;` → Info note. |
| `web/samples/m03_2_float_nan.rs`      | Same.                                                |
