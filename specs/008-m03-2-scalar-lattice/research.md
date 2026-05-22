# Research — M03.2 Implementation Decisions

Decision / Rationale / Alternatives for the scalar lattice expansion.

## `Value` representation

### R-001 — Unified `Value::Int { kind, bits }` + `Value::Float { kind, value }`

- **Decision**: restructure `Value` as:

  ```rust
  pub enum Value {
      Int { kind: IntKind, bits: i128 },
      Float { kind: FloatKind, value: f64 },
      Bool(bool),
      Unit,
  }
  ```

  Integer values store as widened `i128` (sufficient for all 12 integer types). Float values store as widened `f64` (with `f32` narrowing on display).
- **Rationale**:
  - **One arithmetic dispatch per op** instead of 12+. `add(a, b)` matches on a `(IntKind, IntKind)` pair (homogenous) and does one `i128::checked_add` + range gate. Per-type variants would require 12 distinct branches per op.
  - **Compact JSON wire**: serializes as `{"Int": {"kind": "U8", "bits": 5}}` — one variant tag plus a discriminator field — instead of 14 distinct tags `{"U8": 5}`, `{"I64": ...}`, …. JS consumers (M04/M05 renderer) read one path.
  - **`Eq` was already absent** on the existing `Value` derive (`#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]`), so adding floats doesn't lose a derive that was being relied on.
  - Growing the lattice further (e.g. hypothetical M03.3 adding `f16`) is just one more `FloatKind` variant — no enum-arm explosion.
- **Alternatives considered**:
  - **Per-type variants** (`Value::I8(i8)`, …, `Value::F64(f64)`): 16 variants total. Pros: trivial to read in debugger; no widening. Cons: 12 match arms per arithmetic op (×5 ops = 60 branches), JSON wire is bulkier, and snapshot diffs are huge. **Rejected**.
  - **`Value::Int(i128) + Value::Float(f64) + Ty side-table`**: type tag stored separately on the `LocalSlot` and looked up at render time. Loses the value-self-describes property; harder to reason about during eval. **Rejected**.

### R-002 — `f32` widening to `f64` for storage

- **Decision**: `Value::Float { kind: F32, value: f64 }` stores `f32` values widened to `f64`. Narrowed back to `f32` on display + on arithmetic before re-widening.
- **Rationale**:
  - Single field type simplifies the unified `Value::Float` variant. Avoids a `(f32, f64)` union or a `u64`-bit-pattern hack.
  - `f32 → f64 → f32` round-trip is exact for all values representable in `f32` (per IEEE 754).
  - For NaN: the specific NaN payload may not round-trip, but the "NaN-ness" does — pedagogically sufficient.
- **Alternatives considered**:
  - **`bits: u64`** holding the IEEE 754 bit pattern: faithful to bit-level semantics but unergonomic in the evaluator (`f64::from_bits(...)`/`to_bits()` on every op). **Rejected**.
  - **Separate `Value::F32(f32) | Value::F64(f64)`**: pushes the per-type-variant explosion into the float side. **Rejected** for symmetry with the unified Int form.

## Helper enums

### R-003 — `IntKind` enum + methods, `FloatKind` enum + methods

- **Decision**: introduce in `src/typeck.rs`:

  ```rust
  pub enum IntKind { I8, I16, I32, I64, I128, U8, U16, U32, U64, U128, ISize, USize }
  pub enum FloatKind { F32, F64 }
  ```

  Methods on `IntKind`:
  - `pub fn min_value(self) -> i128` — lowest representable value.
  - `pub fn max_value(self) -> i128` — highest representable value.
  - `pub fn contains(self, v: i128) -> bool` — `min_value() <= v <= max_value()`.
  - `pub fn is_signed(self) -> bool` — `true` for I*; `false` for U*.
  - `pub fn name(self) -> &'static str` — `"u8"`, `"i64"`, …. Matches Rust's type-name syntax.

  Method on `FloatKind`:
  - `pub fn name(self) -> &'static str` — `"f32"` or `"f64"`.
- **Rationale**:
  - **Range methods centralize the per-type limits**, used both at typeck (literal range checks) and eval (overflow detection). No magic numbers scattered through the code.
  - **`is_signed()` gates unary `-`**: typeck rejects `let x: u8 = -1;` because `-` requires a signed type.
  - **`name()` is used by `Ty::name()`** for rendering — single source of truth.
  - `Ty` itself embeds `IntKind` / `FloatKind`: `Ty::Int(IntKind)`, `Ty::Float(FloatKind)`, plus the existing `Ty::Bool` and `Ty::Unit`. This keeps `Ty` to 4 variants visually (was 3) while the per-kind enums hold the 14-way fan-out.
- **Alternatives considered**:
  - **Flatten Ty to 14 + 2 variants directly** (`Ty::I8`, `Ty::I16`, …, `Ty::F32`, `Ty::F64`, `Ty::Bool`, `Ty::Unit`): 16 variants of Ty. Slightly noisier match arms everywhere. Plus `Ty::is_copy()` becomes a 16-arm match. Equivalent expressiveness but bulkier. **Rejected** for the nested-enum form.

### R-004 — `usize` / `isize` ≡ `u64` / `i64`

- **Decision**: `IntKind::USize.min_value()` / `max_value()` return the same values as `IntKind::U64`. Same for `ISize` ↔ `I64`.
- **Rationale**:
  - Per FR-011, pin to 64-bit equivalents for deterministic browser-side behavior. Real Rust varies by target (32-bit vs 64-bit); the visualizer doesn't need fidelity here, just determinism.
  - Documented in the M03 contract amendment.
- **Alternatives considered**:
  - **Match the WASM target's pointer width** (32-bit): `usize = u32`. Possible but couples the visualizer's pedagogy to the target architecture. **Rejected**.

## Lexer / parser

### R-005 — Float literal token: `digits . digits`

- **Decision**: lexer recognizes a float literal when seeing a digit sequence followed by `.` followed by another digit sequence. Emits a new `Token::Float(f64)`.
- **Rationale**:
  - Minimal lexer change. The two-char lookahead (`.<digit>`) is unambiguous in L1 because L1 has no method calls on integers — `5.something` doesn't appear anywhere in L1 source.
  - When M07+ adds heap types with methods, a float-literal rule of "digit + `.` + digit" still wins over "digit + `.` + identifier" via prefix priority.
  - The standalone `.` token is unaffected (it's not used in L1 syntax anyway).
- **Alternatives considered**:
  - **Require explicit float suffix** (`2.5_f64`): annotation-driven typing per FR-009 means we don't have suffix parsing in M03.2 at all. **Rejected**.
  - **Use `f` prefix or `.0` for floats**: non-Rust syntax. **Rejected**.

### R-006 — `FloatLit(f64)` AST node alongside `IntLit(i64)`

- **Decision**: `ast::Expr` gains a `FloatLit(f64)` variant. The existing `IntLit(i64)` stays.
- **Rationale**:
  - AST stays clean: ints and floats are different kinds of expression, with different typeck rules. Sharing a `NumLit` parent would force unwrapping at every type-check site.
  - `f64` storage in `FloatLit` is sufficient: f32 literals get narrowed at typeck after the annotation is known.
- **Alternatives considered**:
  - **Unified `NumLit { value: NumLitValue }` with `enum NumLitValue { Int(i64), Float(f64) }`**: forces every Expr consumer to match an extra level. **Rejected**.

## Arithmetic dispatch

### R-007 — Integer arithmetic: `i128::checked_op` + `IntKind::contains` range gate

- **Decision**: for each integer binary op (`+`, `-`, `*`, `/`, `%`):

  ```rust
  fn add_int(a_kind: IntKind, a_bits: i128, b_bits: i128) -> Result<i128, OverflowError> {
      let raw = a_bits.checked_add(b_bits).ok_or(OverflowError(a_kind))?;
      if a_kind.contains(raw) { Ok(raw) } else { Err(OverflowError(a_kind)) }
  }
  ```

  Two-step: `i128::checked_add` prevents the i128 itself from overflowing (only matters for I128/U128). The `IntKind::contains` gate enforces the target type's range. Same pattern for `-` (`checked_sub`), `*` (`checked_mul`). Division and modulo by zero map to the existing div-by-zero `Note { kind: RuntimeError }`.
- **Rationale**:
  - **Defense in depth**: catches both the wide-storage overflow AND the narrow-type overflow with one helper.
  - **Consistent with M03's existing div-by-zero pattern**: overflow becomes a `Note { kind: RuntimeError }` halting the trace.
  - **No `panic!` paths** — every overflow is a typed result, never an uncontrolled abort.
- **Alternatives considered**:
  - **Use `IntKind` to dispatch to native-width `u8::checked_add` etc.**: 12 sites per op. **Rejected** for the wide-storage single-dispatch.
  - **Wrapping arithmetic + post-op range check**: wrapping behavior is wrong pedagogically (the M03 div-by-zero precedent halts; we should match). **Rejected**.

### R-008 — Float arithmetic: native `f64` ops + post-op NaN/Inf detection

- **Decision**: float arithmetic uses native `f64` `+`/`-`/`*`/`/` operators. After computing the result, check `is_nan()` and `is_infinite()`. If the result is special AND neither operand was already special, emit a `Note { kind: Info, message: "produced <kind>" }` once. For `f32` results, narrow the `f64` back to `f32` before checking (catches the case where the f64 computation overflowed f32's range producing f32's Inf).
- **Rationale**:
  - **Float arithmetic never panics in Rust** — these ops are safe by construction.
  - **The "de-novo creation" rule** (per FR-005) avoids note spam when special values propagate. A learner sees the "produced NaN" note once; subsequent operations using that NaN don't re-trigger.
  - The "neither operand was special" check is cheap (`!a.is_finite() == false && !b.is_finite() == false`). Total cost per op is 3 boolean checks.
- **Alternatives considered**:
  - **Emit on every appearance**: noisy. **Rejected**.
  - **Track per-binding "has seen special"**: stateful, brittle. **Rejected** for the simpler op-level check.

### R-009 — Cross-type arithmetic: typeck error

- **Decision**: in `typeck.rs`'s binary-op checker, if the two operands have different `Ty` (e.g. `Ty::Int(U8)` vs. `Ty::Int(I32)`, or `Ty::Int(I32)` vs. `Ty::Float(F64)`), emit a typeck error spanning the right operand. Message: `"expected `<a_type>`, found `<b_type>`"`.
- **Rationale**: matches Rust's actual typeck behavior. Span on the right operand makes the error visually pair with the binary op.
- **Alternatives considered**:
  - **Implicit numeric promotion** (e.g. `u8 + i32` → `i32 + i32`): not Rust semantics. Pedagogically wrong. **Rejected**.

## Literal range / negation

### R-010 — Integer literal range check at typeck (not at eval)

- **Decision**: when typeck sees `let x: T = N` (or any other annotated int literal), check `IntKind::from(T).contains(N as i128)`. If false, typeck error with span on the literal.
- **Rationale**:
  - Range-violating literals (e.g. `let x: u8 = 300;`) are a STATIC error, not a runtime overflow. Reporting at typeck matches Rust's behavior and gives the learner an error span on the literal itself.
- **Alternatives considered**:
  - **Wait until eval**: gives a runtime-error note, but is misleading — the value `300` exists, it just doesn't fit. **Rejected**.

### R-011 — Unary `-` on unsigned types: typeck error

- **Decision**: typeck for `-expr` requires `expr` to have `Ty::Int(k)` with `k.is_signed()`, or `Ty::Float(_)`. Otherwise error.
- **Rationale**: matches Rust's `Neg` trait — unsigned types don't impl `Neg`. Catches `let x: u8 = -1;` (the `-1` is `-(1)` which fails because `1` is `u8` per annotation).
- **Alternatives considered**:
  - **Allow the negation and wrap to type's max**: pedagogically wrong. **Rejected**.

## Untyped literal defaults

### R-012 — Untyped int literal stays `i32`; untyped float literal defaults to `f64`

- **Decision**:
  - `let x = 5;` → `x: i32` (current M03 behavior, preserved).
  - `let y = 2.5;` → `y: f64` (new in M03.2; matches Rust's default float type).
- **Rationale**:
  - Preserves M03 backward compatibility for integer-only programs.
  - `f64` is Rust's default float type; learners writing `let pi = 3.14;` expect `f64`.
- **Alternatives considered**:
  - **Untyped int → `i64`**: changes M03 behavior. **Rejected**.
  - **Untyped float → `f32`**: surprises learners. **Rejected**.

## NaN-aware Debug / PartialEq

### R-013 — `Value` keeps `PartialEq` only; no `Eq`

- **Decision**: `Value` derives `Clone, Debug, PartialEq, Serialize, Deserialize`. No `Eq` (floats don't impl `Eq`). **Already the existing state** — the M03 `Value` didn't derive `Eq` either, so this is a no-op confirmation.
- **Rationale**:
  - Verified by audit: `Value`'s current derives are `Clone, Debug, PartialEq, Serialize, Deserialize`. Adding `Float { kind, value: f64 }` is safe.
  - `MemEvent`'s derives are also `Clone, Debug, PartialEq, Serialize, Deserialize` — no cascade.
  - Unit tests using `assert_eq!` on `Value::Float` containing NaN will fail (NaN != NaN). Add a `Value::eq_with_nan` helper for tests that need it, or use `format!("{:?}", val)` comparison.
- **Alternatives considered**:
  - **Custom `PartialEq` impl** where NaN == NaN: violates the trait's reflexivity contract for f64. **Rejected**.

## M03 snapshot churn

### R-014 — Re-baseline all M03 snapshots

- **Decision**: `Value`'s Debug format changes from `Int(5)` to `Int { kind: I32, bits: 5 }`. All M03 sample snapshots reference `Value` in event payloads — they all re-baseline.
- **Rationale**:
  - Re-baseline is consistent with the M03.1 precedent (which also re-baselined M03 snapshots due to protocol changes).
  - Predictable diff: search-and-replace `Int(N)` → `Int { kind: I32, bits: N }` everywhere. Visually inspect each `.snap.new` and accept.
  - M01 and M02 snapshots don't reference `Value` — they stay byte-identical (SC-005).
- **Alternatives considered**:
  - **Custom Debug impl for backward compat**: lies about the struct. **Rejected**.

## Reference samples

### R-015 — Three M03.2 reference programs

| File                       | Purpose                                                        |
|----------------------------|----------------------------------------------------------------|
| `m03_2_basic_u8.rs`        | `fn main() { let a: u8 = 5; let b: u8 = 3; let c: u8 = a + b; }` — same-type non-i32 arithmetic. |
| `m03_2_u8_overflow.rs`     | `fn main() { let a: u8 = 250; let b: u8 = a + 10; }` — integer overflow → RuntimeError halt. |
| `m03_2_float_nan.rs`       | `fn main() { let a: f64 = 0.0; let b: f64 = a / a; }` — float NaN with Info note. |

Plus the dropdown entries in `web/index.html`:

```html
<option value="m03_2_basic_u8">u8 arithmetic (M03.2)</option>
<option value="m03_2_u8_overflow">u8 overflow (M03.2)</option>
<option value="m03_2_float_nan">f64 NaN (M03.2)</option>
```

## Constitution

### R-016 — Same vacuous PASS

- **Decision**: `.specify/memory/constitution.md` still unfilled. No gates apply.

## Open question — not blocking

- **Type-tag rendering style**: `5_u8` vs. `5 : u8` vs. `5<u8>` vs. `(u8) 5`. The spec defaults to `_T` suffix matching Rust's literal-suffix style. Maintainer tunes during QA.
- **Info-note message wording**: `"produced NaN"`, `"produced +Inf"`, `"produced -Inf"`. Could include the producing expression for more context (`"0.0/0.0 produced NaN"`). Plan-phase defaults to terse; QA may suggest verbose.
