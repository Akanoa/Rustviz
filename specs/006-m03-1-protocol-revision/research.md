# Research — M03.1 Implementation Decisions

Decision / Rationale / Alternatives for the protocol revision.

## Type-classification helper

### R-001 — `impl Ty { pub fn is_copy(self) -> bool }` in `src/typeck.rs`

- **Decision**: add a method on the existing `Ty` enum. For L1 (`Ty ∈ {I32, Bool, Unit}`) all three are Copy → returns `true`. The method is a `match` on `self` for forward-extensibility.
- **Rationale**:
  - Idiomatic Rust — type classification lives with the type definition.
  - When M07 adds heap-allocated `Ty` variants (e.g. `Ty::Box`, `Ty::Vec`, `Ty::String`), they'll be added with `is_copy → false` arms in the same `match`. The compiler will flag the existing `is_copy` if a new variant is added without an arm (since `match` is exhaustive by default).
  - Method form keeps the type lattice and its classification co-located, easier to audit.
- **Alternatives considered**:
  - **Free function in `src/eval.rs`**: couples classification to the evaluator. Rejected — typeck owns the type lattice; classification belongs there.
  - **Hard-code in `src/eval.rs::drop_current_scope`**: `if matches!(ty, Ty::I32 | Ty::Bool | Ty::Unit) { /* skip */ }`. Rejected — duplicates the type list, easy to miss when M07 lands new variants.

## Event protocol changes

### R-002 — `MemEvent::ReturnValue { frame_id, value, span }` is additive

- **Decision**: add a new variant to the existing closed-enum `MemEvent`. Variant payload: `frame_id: FrameId`, `value: Value`, `span: Span`. Derive `Serialize` + `Deserialize` per the M03 contract (so it survives the JSON round-trip).
- **Rationale**:
  - Symmetric with `FrameLeave.return_value` — same `Value` type, same span semantics. Easy to validate in tests.
  - Closed-enum rule relaxation (R-008) explicitly permits additive variants in revision milestones.
  - JS / wasm consumers that exhaustively `match` on `MemEvent` will get a compile-time error pointing to the missing arm, surfacing the change cleanly.
- **Alternatives considered**:
  - **Reuse `Note { kind: ReturnValue, message: "..." }`**: leverages existing variant, no enum change. Rejected — `Note` is for human-readable pedagogical text; structured return-value data doesn't fit.
  - **Bundle return value into `FrameLeave`** (already there): exists, but the user can't *see* the value before the frame disappears. Rejected — that's exactly the gap M03.1 fixes.

### R-003 — Event ordering: `ReturnValue` BEFORE `SlotDrop`s, BEFORE `FrameLeave`

- **Decision**: emit `ReturnValue` immediately after body completion, then run scope teardown (which emits zero events in L1 because every type is Copy; emits drops for non-Copy in M07+), then emit `FrameLeave`.
- **Rationale**:
  - Pedagogically: "function computed a value → the value lives somewhere visible → locals are cleaned up → frame closes". This matches a reader's expected mental model.
  - For L1 (no SlotDrops) the cursor sees: body → `ReturnValue` → `FrameLeave`. Three steps, clean.
  - For M07+ with non-Copy locals: body → `ReturnValue` (value visible, locals still alive) → `SlotDrop`s (each emits a destructor side-effect like `HeapFree`) → `FrameLeave`. Each step is a meaningful animation tick.
- **Alternatives considered**:
  - **Drops → `ReturnValue` → `FrameLeave`**: drops first, then "the value materializes from nowhere just before the frame closes". Confusing for the M07+ case — locals are gone but the return value reappears. Rejected.

### R-004 — `ReturnValue.span` = body tail expression span (fallback: body block span)

- **Decision**: when the function body has a tail expression, `ReturnValue.span = decl.body.tail.unwrap().span()`. When the body has no tail (e.g. `fn main() { let x = 5; }` — tail is `None`), `ReturnValue.span = decl.body.span` (the whole body block).
- **Rationale**:
  - For typical functions with a tail (`fn add(a, b) -> i32 { a + b }`), the span highlights `a + b` in the editor at the moment the return value is announced. Pedagogically perfect.
  - For functions with no tail (implicit unit return), there's no expression to point at — falling back to the body's span (covering `{...}`) is a sensible visual that says "the body has finished, returning unit".
- **Alternatives considered**:
  - Always use body span — works but loses the tail-expression-specific highlight. Rejected for the value lost.
  - Function declaration's span — too broad. Rejected.

### R-005 — Drop the `FrameEnter.params` field

- **Decision**: remove the `params: Vec<(SlotId, String, Value)>` field from `MemEvent::FrameEnter`. The information is fully reproduced by the per-param `SlotAlloc` + `SlotWrite` events that follow.
- **Rationale**:
  - The field was always redundant. M04's renderer never used it (consumes the per-param events instead). Removing it tightens the contract.
  - JSON traces become smaller (less duplication per call).
  - The "closed enum, additive only" rule technically gets bent: this is a *removal*, not an addition. The relaxed rule (R-008) explicitly permits removing redundant fields in revision milestones with maintainer consent.
- **Alternatives considered**:
  - Keep the field as `#[serde(skip)]` and deprecate — leaves dead weight in the in-memory representation. Rejected; M03.1 is the time for cleanup.

## Closed-enum rule relaxation

### R-006 — Permit additive variants + redundant-field removal in revision milestones

- **Decision**: amend `specs/004-m03-event-eval/contracts/m03-api.md` to relax the closed-enum rule. New text: "The `MemEvent` enum is closed from M03 onward in the sense that variant set is stable. Revision milestones (e.g. `M03.1`) may add new variants and remove redundant fields with explicit maintainer consent and a coordinated update of all consumers. Removing or renaming existing variants remains a breaking change requiring full re-coordination."
- **Rationale**: documents the actual policy in force after M03.1 lands. Future revisions have precedent. Captures the rationale for why this is OK (M04 is the only consumer and it ships in lock-step).
- **Alternatives considered**:
  - Leave the contract unchanged — pretends M03.1's changes are non-breaking. Misleading and brittle. Rejected.
  - Mark M03's contract as "open-ended" — too permissive; the closed-enum rule has real value for forcing coordinated changes. Rejected.

## M04 `StateSnapshot` extension

### R-007 — Add `pending_return: Option<PendingReturnView>` to `StateSnapshot`

- **Decision**: extend `src/ui.rs::StateSnapshot` with a new optional field. Defined as:

  ```rust
  pub struct PendingReturnView {
      pub frame_id: u32,
      pub value: String,
  }
  ```

  Populated by `Cursor::state_snapshot` when the most recently applied event is `MemEvent::ReturnValue`. `None` otherwise.
- **Rationale**:
  - Mirrors the existing `status: Option<StatusView>` field — both are "transient annotations attached to the most-recent event". Symmetric, low surprise.
  - JS renderer reads `pending_return` and decorates the matching frame card (a CSS class like `.frame-returning` plus a `→ 5` annotation in the frame header). No layout change required.
  - Additive change to `StateSnapshot` JSON; M04 consumers see one extra field. Per M04's stability rules, this is allowed (the M03.1 milestone explicitly authorizes it).
- **Alternatives considered**:
  - Add a `return_value` field directly on `FrameCardView`. Couples the transient state to the per-frame view. Rejected — the transient is more naturally a top-level snapshot field (it goes away on the next event).
  - Reuse `status` (set kind to `"return"`). Conflates two different concerns. Rejected.

## Event-count math (for SC-004)

### R-008 — Per-sample event-count diff documented up-front

- **Decision**: predict the expected event-count change per M03 sample so the audit can verify deterministically:

  | Sample              | Pre  | − SlotDrops (Copy) | + ReturnValue | Post |
  |---------------------|------|---------------------|----------------|------|
  | m03_arithmetic      | 5    | 1 (`x`)             | 1 (`main`)     | 5    |
  | m03_fn_call         | 13   | 3 (`a`, `b`, `r`)   | 2 (`add`, `main`) | 12 |
  | m03_if_then         | 5    | 1 (`v`)             | 1 (`main`)     | 5    |
  | m03_if_else         | 5    | 1 (`v`)             | 1 (`main`)     | 5    |
  | m03_shadow          | 8    | 2 (`x` ×2)          | 1 (`main`)     | 7    |
  | m03_nested_block    | 8    | 2 (`y`, `z`)        | 1 (`main`)     | 7    |
  | m03_short_circuit   | 17   | 4 (`a`,`b`,`c`,`d`) | 1 (`main`)     | 14   |
  | m03_div_by_zero     | 2    | 0 (halts before)    | 0 (halts before) | 2  |

- **Rationale**: SC-004 commits to deterministic event-count change. Documenting the math up front lets the audit verify the implementation matches expectations sample-by-sample. The `div_by_zero` sample is unchanged because the trace halts on `Note { RuntimeError }` before any drops or returns can fire.
- **Alternatives considered**: count after-the-fact — risk discovering an off-by-one and not knowing whether the implementation or the prediction was wrong. Rejected; predict first.

## Snapshot regeneration

### R-009 — `INSTA_UPDATE=always` for batch re-snapshotting

- **Decision**: re-baseline all M03 snapshots with one invocation of `INSTA_UPDATE=always cargo test --test m03`. Manually inspect the resulting `.snap` files to confirm the diff matches the R-008 table.
- **Rationale**: same workflow as M01–M03 first-pass acceptance. Snapshots are part of the contract; reviewing them by reading the `.snap` files is the standard audit.
- **Alternatives considered**: `cargo insta review` interactively — overkill for the batch update we know is correct. Use only if a snapshot's diff surprises us.

## Constitution

### R-010 — Same vacuous PASS

- **Decision**: `.specify/memory/constitution.md` still unfilled. No gates apply.

## Open question — not blocking

- **Visual styling of the `pending_return` annotation**: defaults to a `→ 5` text fragment in the frame card header with a CSS class for color/weight. Final look settles during M04 QA. The maintainer's visual call.
