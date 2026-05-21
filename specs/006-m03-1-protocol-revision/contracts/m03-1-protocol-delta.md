# Contract — M03.1 Protocol Delta

M03.1 modifies M03's existing public contract. This document is the **delta**; the unchanged surface is documented in `specs/004-m03-event-eval/contracts/m03-api.md`. After M03.1 ships, M03's contract is amended in-place with these changes documented + cross-referenced here.

## Closed-enum rule relaxation

**Before (M03 contract)**:

> The `MemEvent` enum is **closed** from M03 onward — adding new variants is breaking. Adding fields to existing variants is breaking. Modifying payload field types is breaking.

**After (M03 contract, amended by M03.1)**:

> The `MemEvent` enum's variant set is stable from M03 onward. **Revision milestones** (e.g. `M03.1`, future `M0Xr` patterns) may:
>
> - Add **new variants** with explicit maintainer consent + coordinated update of all consumers (M04+).
> - **Remove redundant fields** from existing variants when the same information is reachable via other events in the stream.
>
> Removing or renaming existing variants remains a breaking change requiring full re-coordination. Modifying payload field semantics (without removal) is breaking.
>
> M03.1 is the first invocation of this rule: it adds `MemEvent::ReturnValue` and removes the redundant `FrameEnter.params` field.

## Variant additions (additive)

### `MemEvent::ReturnValue { frame_id, value, span }`

```rust
ReturnValue {
    /// The frame returning the value (matches a prior `FrameEnter.frame_id`).
    frame_id: FrameId,
    /// The value being returned. Mirrors the immediately-following
    /// `FrameLeave.return_value` for non-halted frames.
    value: Value,
    /// Source span — body tail expression if present, otherwise body block.
    span: Span,
},
```

Emitted by the evaluator after the function body finishes computing and before scope teardown (`SlotDrop`s, if any) and `FrameLeave`. Always paired with a subsequent `FrameLeave` for the same `frame_id`, **except** when execution halts on a `Note { kind: RuntimeError }` before the frame can return — in that case, no `ReturnValue` is emitted for the halted frame.

### Trace JSON schema example

```json
{ "ReturnValue": { "frame_id": 1, "value": { "Int": 5 }, "span": { "start": 30, "end": 35, "file": 1 } } }
```

## Variant changes (field removal)

### `MemEvent::FrameEnter`: `params` field removed

**Before**:

```rust
FrameEnter {
    frame_id: FrameId,
    fn_name: String,
    params: Vec<(SlotId, String, Value)>,
    span: Span,
},
```

**After**:

```rust
FrameEnter {
    frame_id: FrameId,
    fn_name: String,
    span: Span,
},
```

**Rationale for removal**: the same per-param information (slot id, name, initial value) is fully conveyed by the per-param `SlotAlloc` + `SlotWrite` events that the evaluator emits immediately after each `FrameEnter`. M04's renderer never read the `params` field. The redundancy bloated trace JSONs without giving consumers any unique information.

**Migration**: any consumer that *did* read `FrameEnter.params` must switch to consuming the per-param `SlotAlloc` + `SlotWrite` events instead. (As of M03.1 close, the only consumer is M04, which already does this.)

## Slot-drop emission gating

The evaluator's `drop_current_scope` function now gates `SlotDrop` emission on `Ty::is_copy()`:

```rust
fn drop_current_scope(&mut self) {
    let scope = /* ...pop scope... */;
    for local in scope.locals.into_iter().rev() {
        let ty = self.lookup_var_ty(local.binding_id).expect("var ty");
        if !ty.is_copy() {                    // NEW gate
            self.events.push(MemEvent::SlotDrop {
                slot_id: local.slot_id,
                span: local.decl_span,
            });
        }
    }
}
```

**Behavioral change**: in L1 (`Ty ∈ {I32, Bool, Unit}` — all Copy) no `SlotDrop` events fire at scope exit. Slots live in their frame card until the whole frame closes via `FrameLeave`.

**Forward compatibility**: M07 will add non-Copy `Ty` variants. Their `is_copy()` implementations return `false`. `SlotDrop` events resume firing for those types, carrying real semantic weight (destructors run, heap is freed via subsequent `HeapFree` events).

## Stable types — additive `StateSnapshot.pending_return`

From `specs/005-m04-ui-shell/contracts/m04-api.md`, the `StateSnapshot` JSON schema is amended:

```jsonc
{
  "frames": [...],
  "editor_highlight": null,
  "status": null,
  "pending_return": null,           // NEW — null or { "frame_id": <u32>, "value": "<string>" }
  "position": 7,
  "total": 12
}
```

JS consumers ignoring this field is safe — it's additive. Old consumers see the same `frames`, `status`, etc. behavior. New consumers (M04's renderer in this milestone) decorate the matching frame card with the return-value annotation when `pending_return !== null`.

## Behavioral guarantees (post-M03.1)

- **B-1 (revised)**: `evaluate(...).is_ok()` returns a `Vec<MemEvent>` ending with a `FrameLeave` for the outermost frame (typically `main`) on success, or a `Note { kind: RuntimeError }` if execution halted. For successful runs, the **last two events** are `ReturnValue` followed by `FrameLeave` for the outermost frame.
- **B-2 through B-5**: unchanged from M03.
- **B-6 (revised)**: `SlotMove` is never emitted from a pure L1 evaluation. **`SlotDrop` is also never emitted** in pure L1 traces (after M03.1) because all L1 types are Copy.
- **B-7**: unchanged from M03.
- **B-8 (new)**: every non-halted function call emits exactly one `ReturnValue` event for its frame, sequenced between the body's last evaluation step and the matching `FrameLeave`.
- **B-9 (new)**: the relaxed closed-enum rule applies to all future revision milestones (e.g. a hypothetical M07.1 may further extend the protocol with coordinated maintainer consent).

## What this contract does NOT cover (deferred)

- **Visual styling** of the return-value annotation: spec-level concern, plan defers exact CSS to implementation.
- **Frame-card animation** when the return value appears / disappears: out of scope; static text annotation is sufficient.
- **Multi-return-value functions** (tuples, etc.): L1 has no tuples; deferred to whenever they land.
- **Side-effects during `Drop`** (M07+): the `SlotDrop` event will carry the value being dropped; downstream events (e.g. `HeapFree`) will follow. M07 designs this; M03.1 just opens the gate.
