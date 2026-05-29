//! UI shell — replay cursor + view types + wasm-bindgen `Player` for the browser.
//!
//! `Cursor` is a pure-Rust state machine over a `Vec<MemEvent>`. Its
//! `state_snapshot()` method computes the view (`StateSnapshot`) by replaying
//! events `[0..position)` over an internal world model. Pure / deterministic /
//! testable without a browser.
//!
//! The wasm-bindgen `Player` (gated `#[cfg(target_arch = "wasm32")]`) wraps a
//! `Cursor` and exposes the methods JS calls (see `contracts/m04-api.md`).

use serde::{Deserialize, Serialize};

use crate::event::{MemEvent, NoteKind, Value};
use crate::parse::span::Span;
use crate::typeck::Ty;

/// Replay cursor — a position into a `Vec<MemEvent>` with state-at-N computation.
#[derive(Debug, Clone)]
pub struct Cursor {
    /// The event trace.
    pub trace: Vec<MemEvent>,
    /// Cursor position (`0 ≤ position ≤ trace.len()`).
    pub position: usize,
}

/// Snapshot of the UI state at a cursor position. Serialized as JSON across
/// the WASM boundary; see `contracts/m04-api.md`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateSnapshot {
    /// Active function frames, outermost first.
    pub frames: Vec<FrameCardView>,
    /// Span the editor should highlight at this step.
    pub editor_highlight: Option<Span>,
    /// **M03.1**: span of the **call site** that opened the currently-
    /// executing frame (i.e. the `add(2, 3)` text in the source). Matches the
    /// `FrameEnter.span` of the innermost active frame, when there is at
    /// least one caller below it on the stack. The editor paints this span
    /// with a red border so the learner can tell which specific call is in
    /// flight — important when the same function is called multiple times
    /// from different lines.
    ///
    /// `None` when no callee is currently in flight: position 0, after the
    /// outermost frame returns, or while execution is in the entry function
    /// (typically `main`) itself with no nested call on the stack.
    pub current_call_span: Option<Span>,
    /// Status message (runtime error or info note).
    pub status: Option<StatusView>,
    /// **M03.1**: present when the most recent event is a `MemEvent::ReturnValue`.
    /// The JS renderer decorates the matching frame card with a transient
    /// return-value annotation. `None` on any other event.
    pub pending_return: Option<PendingReturnView>,
    /// **M06 (renamed in M07)**: active arrows at this cursor position. The
    /// JS renderer reads this to draw blue (shared), red (mut), and black
    /// (owning) arrows in the SVG overlay.
    pub arrows: Vec<ArrowView>,
    /// **M07**: live heap allocations at this cursor position. JS renders
    /// these as boxes in the heap panel.
    pub heap: Vec<HeapView>,
    /// **M07.2**: static-memory blocks (read-only data segment). One per
    /// unique string-literal content, content-deduplicated. Never shrinks
    /// — static blocks persist for the trace's lifetime. JS renders these
    /// as blocks in a separate "static memory (RO)" region between the
    /// stacks and heap panels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub static_region: Vec<StaticView>,
    /// **M07.7**: live vtables in the VTABLES panel. One per unique
    /// `(trait, type)` pair, content-deduplicated. Never shrinks — vtables
    /// persist for the trace's lifetime (analog of static memory).
    /// JS renders these as boxes in a dedicated "VTABLES" panel between
    /// the heap and static-memory panels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vtables: Vec<VtableView>,
    /// **M08**: per-thread stack columns. For single-threaded programs
    /// (no `thread::spawn` events), this has a single entry (thread 0 =
    /// main) carrying the same data as the legacy `frames` field —
    /// visually identical to pre-M08 single-column rendering.
    /// Empty `Vec` means single-threaded layout via the legacy `frames`
    /// field (back-compat).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub threads: Vec<ThreadColumnView>,
    /// **M07.2**: present when the most recent event is `BytesCopy` (fired
    /// by `String::from` / `push_str`). The UI renders a transient orange
    /// dashed arrow from the source region to the destination heap block
    /// at this cursor step only — making the copy that would otherwise
    /// be invisible (bytes "magically" appearing in the heap) explicit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_copy: Option<CopyView>,
    /// **M07.7**: present when the most recent event is a `FrameEnter` for
    /// a trait-object dispatch (frame name matches `<Type as Trait>::method`
    /// AND a slot in a caller frame holds a `Value::DynRef` / `Value::BoxDyn`
    /// with a vtable for the matching `(Trait, Type)` pair). Drives a
    /// transient two-step dispatch arrow at the call step:
    /// data → receiver location, vtable → vtable box → new frame card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_dispatch: Option<DispatchView>,
    /// Cursor position (mirrors `Cursor::position`).
    pub position: usize,
    /// Total events in the trace.
    pub total: usize,
    /// **Post-M08 polish**: user-visible step counter. Counts non-coalesced
    /// positions only, so the counter advances 1-by-1 even when raw cursor
    /// positions jump over coalesced pairs (SlotAlloc→SlotWrite,
    /// ArcClone→HeapRealloc, etc.). Always ≤ `position`.
    #[serde(default)]
    pub logical_position: usize,
    /// Total logical (user-visible) steps in the trace, after coalescing.
    /// Always ≤ `total`. Use as the denominator for the step counter.
    #[serde(default)]
    pub logical_total: usize,
    /// **M08.2**: seed used to generate this trace. JS UI displays this in
    /// the seed input field so the user knows which seed produced what
    /// they're looking at. `0` for traces from M01–M07.7 single-threaded
    /// samples (or any sample run with the default seed).
    #[serde(default)]
    pub seed: u32,
}

/// **M07.2**: transient "bytes copied" indication. Set on `StateSnapshot`
/// when the most recent event is `MemEvent::BytesCopy`; cleared on the
/// next step. Drives a one-shot orange arrow render in the JS layer
/// PLUS highlights the source byte-cells and char spans covered by the
/// copy — making "these specific bytes flowed into this block" tangible.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CopyView {
    /// Source region (Slot / Heap / Static).
    pub from: ArrowTarget,
    /// Byte offset within the source block where the copied range starts.
    /// Pairs with `n_bytes` to identify the highlighted byte-cell range.
    pub from_byte_offset: u32,
    /// Destination heap block.
    pub to: u32,
    /// Bytes copied.
    pub n_bytes: u32,
}

/// **M07.7**: one vtable in the VTABLES panel. Holds the trait + concrete
/// type pair plus a per-method dispatch-target label. Persists for the
/// trace's lifetime — vtables are never freed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VtableView {
    /// VtableAddr.0 — identifier driving the data-vtable hover wiring.
    pub addr: u32,
    /// Trait the vtable implements (e.g. `"Show"`).
    pub trait_name: String,
    /// Concrete type the vtable is for (e.g. `"Point"`).
    pub type_name: String,
    /// One entry per method: `(name, dispatch_target_label)`.
    /// Target label: `<TypeName as TraitName>::method` for overrides;
    /// `<TraitName>::method (default)` for unoverridden defaults.
    pub methods: Vec<(String, String)>,
}

/// **M07.7**: transient two-step dispatch indicator. Set on `StateSnapshot`
/// when the most recent event is `MemEvent::FrameEnter` for a trait-object
/// method dispatch (frame name in UFCS form AND a caller slot holds the
/// matching DynRef/BoxDyn). The renderer draws two arrows from the slot's
/// fat-pointer cells at this cursor step only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DispatchView {
    /// Source slot — the slot holding the DynRef/BoxDyn that initiated dispatch.
    pub source_slot: u32,
    /// Vtable addr — the box in the VTABLES panel involved in the dispatch.
    pub vtable_addr: u32,
    /// Newly-entered frame id — the target frame card.
    pub target_frame: u32,
    /// Resolved method name (the trait method that fired).
    pub method: String,
}

/// **M07.7**: fat-pointer rendering data for a trait-object slot. Drives the
/// two labeled cells (`data: → label` + `vtable: → label`) in the slot's
/// value area. Mutually exclusive with `value`, `inline_cells`, `struct_view`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DynView {
    /// Label for the data ptr — typically the binding name of the targeted
    /// slot (e.g. `"p"`) or `"heap[N]"` for `Box<dyn Trait>`.
    pub data_label: String,
    /// Label for the vtable ptr — `<TypeName as TraitName>` form.
    pub vtable_label: String,
    /// Vtable addr for arrow targeting at hover / dispatch time.
    pub vtable_addr: u32,
}

/// **M08**: one thread column in the stacks panel. For single-threaded
/// programs (no `thread::spawn` events), `StateSnapshot.threads` has a
/// single entry with id 0 (main) — visually identical to the pre-M08
/// single-column rendering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadColumnView {
    /// Thread id (0 = main).
    pub thread_id: u32,
    /// Human-readable label (`"main"` for thread 0; `"thread #N"` for spawned).
    pub label: String,
    /// Per-thread frame stack (innermost last, same as the pre-M08 single-
    /// thread `FrameCardView` ordering).
    pub frames: Vec<FrameCardView>,
    /// `true` for the currently-executing thread; drives a visual emphasis.
    pub is_current: bool,
    /// Thread lifecycle status: Running / Joined / Ready / Parked (M08.1 only).
    pub status: ThreadStatusView,
}

/// **M08**: thread lifecycle status for the column rendering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ThreadStatusView {
    /// Running or just unparked.
    Running,
    /// Body completed.
    Joined,
    /// Queued, never executed yet (between `thread::spawn` and first
    /// `ThreadSwitch` into this thread).
    Ready,
}

/// **M07.2**: one static-memory block. Holds raw bytes for a unique string
/// literal. Persists for the trace's lifetime — there is no equivalent of
/// `HeapFree` for static blocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StaticView {
    /// Identifier (matches the `StaticAddr.0` in events).
    pub addr: u32,
    /// Raw bytes (already-processed string after lexer escape resolution).
    pub bytes: String,
    /// Size in bytes (= `bytes.len()`).
    pub size: u32,
    /// Pre-rendered display label (e.g. `"\"hi\""` with surrounding quotes
    /// for visual clarity in the UI).
    pub display: String,
}

/// **M06 (restructured in M07)**: one active arrow as seen by the renderer.
/// Unifies borrow arrows (blue/red) and owning arrows (black).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrowView {
    /// Slot holding the source value (arrow origin).
    pub source_slot: u32,
    /// What the arrow points at — a stack slot OR a heap allocation.
    pub target: ArrowTarget,
    /// Visual style.
    pub kind: ArrowKind,
    /// **M07.1**: optional length annotation for slice arrows. `None` for
    /// non-slice borrows and owning arrows; `Some(n)` when the arrow
    /// originates from a `Value::Slice { len: n, .. }`. The renderer adds
    /// a `[len: N]` label to slice arrows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub len: Option<u64>,
    /// **M07.1**: byte offset of the slice's view within the target heap
    /// block. Drives the hover-highlight: on `mouseenter` the renderer
    /// highlights byte-cells `[byte_offset, byte_offset + byte_len)` in
    /// the target heap-box to show which bytes the slice covers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_offset: Option<u64>,
    /// **M07.1**: byte length of the slice's view (paired with `byte_offset`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_len: Option<u64>,
    /// **M07.1**: element index where the slice starts (the range's `start`
    /// bound). Drives the element-span highlight on hover: spans
    /// `[elem_start, elem_start + len)` in the target heap-box's display
    /// get the highlight class — so `&v[1..3]` lights up the 2nd and 3rd
    /// element labels (`2_i32, 3_i32`) alongside their bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elem_start: Option<u64>,
    /// **M07.4**: field name for sub-field borrow arrows (`&p.x` →
    /// `Some("x")`). Drives a `.x` annotation rendered at the arrow's
    /// midpoint (analogous to slice arrows' `[len: N]`) AND a per-field
    /// hover-highlight that lights up only the borrowed field's row in
    /// the target struct view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_label: Option<String>,
    /// **M07.4**: hide the arrow by default; reveal only on hover of the
    /// source slot. Set for method `self` receivers — the borrow is part
    /// of the calling convention, not explicit user code, so the always-
    /// on arrow added visual noise without pedagogical payoff. Hover
    /// reveals the arrow + targets the corresponding caller slot.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hover_only: bool,
}

/// **M07**: arrow target — slot (for borrows-of-locals), heap (for borrows-of-heap
/// and ownership), or static memory (for `&'static str` literals — M07.2).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ArrowTarget {
    /// Stack slot (M06's case, kept).
    Slot(u32),
    /// Heap allocation (M07's case).
    Heap(u32),
    /// **M07.2**: static-memory block (StaticAddr.0). Read-only; never freed.
    Static(u32),
}

/// **M07**: arrow visual kind.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ArrowKind {
    /// `&T` borrow — blue.
    Shared,
    /// `&mut T` borrow — red.
    Mut,
    /// Ownership (`Box`/`Vec`/`String`) — black.
    Owning,
    /// **M08**: `Arc<T>` shared-ownership — dashed purple. Multiple Arc
    /// bindings can target the same heap addr; each gets its own arrow.
    Arc,
}

/// **M07**: one heap allocation (live OR freed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeapView {
    /// HeapAddr (M03's identifier).
    pub addr: u32,
    /// Type name (e.g. `"Box<i32>"`, `"Vec<i32>"`, `"String"`).
    pub ty_name: String,
    /// Renderer-ready content display.
    pub display: String,
    /// Total capacity in bytes (Box<f32>=4, Box<f64>=8, Vec<i32> cap=N → N*4).
    pub size: u32,
    /// **M07**: used bytes (Box always = size, Vec = len*elem_size, String = len).
    /// JS renders `size` byte-cells with the first `used` filled, the rest empty.
    pub used: u32,
    /// **M07**: `true` if the block has been freed. Renderer shows a grayed
    /// "freed, ready to be reused" visual instead of removing the DOM element.
    pub freed: bool,
    /// **M08**: present for `HeapObject::Arc` blocks — the current strong
    /// refcount. Renders as a `[refs: N]` suffix on the heap-block addr
    /// line. Updated on `ArcClone` (++) / `ArcDrop` (--). None for non-Arc
    /// blocks; serde skip-if-none keeps existing snapshots byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refcount: Option<u32>,
    /// **Post-M08 polish**: lock state for Mutex / Arc<Mutex<T>> blocks.
    /// `Some(Free)` → render green "🔓 free" badge; `Some(Locked { holder })`
    /// → render red "🔒 by #N" badge. None for non-Mutex blocks.
    /// Parsed from the heap-block display string's `[free]` / `[locked by
    /// #N]` suffix in apply_event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutex_state: Option<MutexLockState>,
}

/// **Post-M08 polish**: Mutex lock-state for the UI badge.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum MutexLockState {
    /// Lock is currently held — by `holder` (a ThreadId.0).
    Locked {
        /// `ThreadId.0` of the thread currently holding the lock.
        holder: u32,
    },
    /// Lock is currently free.
    Free,
}

/// One function-call frame's view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameCardView {
    /// Frame id (matches `MemEvent::FrameEnter.frame_id`).
    pub frame_id: u32,
    /// Function name.
    pub fn_name: String,
    /// Active slots in declaration order.
    pub slots: Vec<SlotRowView>,
    /// **M03.1**: `true` while the frame is between its `FrameEnter` and
    /// `FrameLeave` events; `false` after `FrameLeave` fires. Inactive frames
    /// linger in the visualization (grayed) to convey that the stack bytes
    /// don't physically disappear at function return — they persist on the
    /// stack until something else reuses the storage.
    pub active: bool,
    /// **M03.1**: rendered return value (e.g. `"5"`, `"()"`) once a
    /// `MemEvent::ReturnValue` has fired for this frame. Persists across the
    /// subsequent `FrameLeave` so the grayed-out frame card still shows the
    /// `→ <value>` annotation — the return value lives in the frame's memory
    /// until the bytes are reused, mirroring the machine-level reality.
    /// `None` for frames that haven't returned yet, or that halted on a
    /// runtime error before reaching `ReturnValue`.
    pub return_value: Option<String>,
    /// **M03.1**: `true` for the innermost active frame — the one whose body
    /// is currently executing. Other active frames are paused waiting for
    /// their callee to return. Distinguishing the "current" frame from the
    /// "caller" makes the call-stack relationship visible at a glance.
    /// At most one frame per snapshot has `current = true`; grayed frames
    /// always have `current = false`.
    pub current: bool,
}

/// One stack slot's view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlotRowView {
    /// Slot id (matches `MemEvent::SlotAlloc.slot_id`).
    pub slot_id: u32,
    /// Binding name.
    pub name: String,
    /// Type label (`"i32"`, `"bool"`, `"()"`).
    pub ty: String,
    /// Rendered value, or `None` between `SlotAlloc` and the first `SlotWrite`.
    pub value: Option<String>,
    /// **M07.3**: present when the slot holds a `Value::Array`. The JS
    /// renders inline byte-cells in the slot's value area (instead of
    /// the text `value` field). Mirrors the per-byte-cell + per-element
    /// rendering used for heap blocks, but visually distinct (gray-tinted)
    /// to convey "stack memory" not "heap memory".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_cells: Option<InlineCellsView>,
    /// **M07.4**: present when the slot holds a `Value::Struct`. The JS
    /// renders per-field labeled rows with byte-cells (research R-016
    /// Proposal A — vertical labeled rows). Mutually exclusive with
    /// both `value` (text fallback) and `inline_cells` (array rendering).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub struct_view: Option<StructView>,
    /// **M07.7**: present when the slot holds a `Value::DynRef` or
    /// `Value::BoxDyn`. The JS renders a two-cell fat-pointer view in the
    /// slot's value area (data label + vtable label). Mutually exclusive
    /// with `value` / `inline_cells` / `struct_view`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dyn_view: Option<DynView>,
    /// **Post-M08 polish**: `true` once the binding has been moved (e.g.
    /// captured by a `move ||` closure). The slot stays visible to
    /// convey "the stack bytes physically persist" (M03.1 principle),
    /// but JS renders it grayed-out with a `<moved>` annotation to
    /// signal the binding is no longer usable.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub moved: bool,
}

/// **M07.4**: per-struct render data for the stack-slot visualization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructView {
    /// Struct type name (e.g. `"Point"`).
    pub name: String,
    /// Fields in declaration order. Each entry drives one row.
    pub fields: Vec<StructFieldView>,
}

/// **M07.4**: per-field render data inside a `StructView`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructFieldView {
    /// Field name (e.g. `"x"`).
    pub name: String,
    /// Field type label (e.g. `"i32"`).
    pub ty_label: String,
    /// Byte size of the field (drives the byte-cell count).
    pub size: u32,
    /// Rendered field value (e.g. `"1_i32"`).
    pub display: String,
}

/// **M07.3**: inline byte-cell rendering for a stack-allocated array.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InlineCellsView {
    /// Total byte size (`N * elem_size`).
    pub size: u32,
    /// Used bytes — for arrays always equals `size` (fully populated at
    /// construction). Kept as a field for parallelism with HeapView.
    pub used: u32,
    /// Per-element display strings (e.g. `["1_i32", "2_i32", "3_i32"]`).
    /// Drives both the byte-cell count AND the per-element hover-highlight
    /// when a slice arrow points into this slot.
    pub elements: Vec<String>,
}

/// Status message — present when the most recent event was a `Note`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusView {
    /// Category (`"error"` for `RuntimeError`, `"info"` for `Info`).
    pub kind: String,
    /// Note message.
    pub message: String,
}

/// **M03.1**: Transient annotation surfaced when the most recent event is a
/// [`MemEvent::ReturnValue`]. The renderer paints a `→ <value>` indicator
/// on the matching frame card for one cursor step before `FrameLeave` closes
/// the frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingReturnView {
    /// The frame about to return (matches a `FrameCardView.frame_id` in the
    /// same snapshot).
    pub frame_id: u32,
    /// Rendered return value (e.g. `"5"`, `"true"`, `"()"`).
    pub value: String,
}

impl Cursor {
    /// Create a new cursor at position 0 over the given trace.
    pub fn new(trace: Vec<MemEvent>) -> Self {
        Self { trace, position: 0 }
    }

    /// Advance the cursor by 1 event. No-op at end of trace.
    pub fn step_forward(&mut self) {
        if self.position < self.trace.len() {
            self.position += 1;
        }
    }

    /// Decrement the cursor by 1. No-op at position 0.
    pub fn step_back(&mut self) {
        self.position = self.position.saturating_sub(1);
    }

    /// **Post-M08 polish**: returns true iff `pos` lands INSIDE a logical
    /// "atomic event group" — multiple consecutive events that together
    /// represent ONE user-visible step. The Player wrapper skips such
    /// positions so step-forward / step-back move between groups rather
    /// than between raw events.
    ///
    /// Recognized groups (each row = one atomic step):
    /// - `SlotAlloc { slot_id }` → `SlotWrite { slot_id }` — let-binding init.
    /// - `ArcClone | ArcDrop | LockAcquire | LockRelease { addr }`
    ///   → in-place `HeapRealloc { addr }` → `Note { Info }` [→ `HeapFree`]
    ///   — heap-state transition (marker + display refresh + pedagogical
    ///   note + optional free-on-last-drop), all visually emerging together.
    pub fn is_slot_alloc_write_pair_boundary(&self, pos: usize) -> bool {
        if pos == 0 || pos >= self.trace.len() {
            return false;
        }
        let prev = &self.trace[pos - 1];
        let next = &self.trace[pos];

        // (a) SlotAlloc → SlotWrite (same slot).
        if let (
            MemEvent::SlotAlloc { slot_id: a, .. },
            MemEvent::SlotWrite { slot_id: w, .. },
        ) = (prev, next)
        {
            if a == w {
                return true;
            }
        }
        // (a') SlotWrite → SlotMove (Post-M08): when a capture's
        // destination SlotWrite is immediately followed by a SlotMove
        // marking the source as moved, fold them into one logical step
        // — pairs source-grays with destination-appears visually.
        if matches!(prev, MemEvent::SlotWrite { .. })
            && matches!(next, MemEvent::SlotMove { .. })
        {
            return true;
        }
        // (a'') SlotMove → Note (Post-M08): the move's pedagogical Note
        // immediately follows. Fold it into the same step so the
        // explanation lands at the move tick.
        if matches!(prev, MemEvent::SlotMove { .. })
            && matches!(next, MemEvent::Note { kind: NoteKind::Info, .. })
        {
            return true;
        }

        // (b) Heap marker → in-place HeapRealloc on the same addr.
        let marker_addr = match prev {
            MemEvent::ArcClone { addr, .. }
            | MemEvent::ArcDrop { addr, .. }
            | MemEvent::LockAcquire { addr, .. }
            | MemEvent::LockRelease { addr, .. } => Some(*addr),
            _ => None,
        };
        if let Some(m_addr) = marker_addr {
            if let MemEvent::HeapRealloc { from, to, .. } = next {
                if from == to && *from == m_addr {
                    return true;
                }
            }
        }

        // (c) In-place HeapRealloc → Note { Info } when the preceding-
        // preceding event was a heap marker — extends the atom in (b)
        // to absorb the pedagogical note that ALWAYS follows a heap-
        // state transition. User sees refcount/holder display update
        // AND the explanatory note at the same cursor step.
        if let MemEvent::HeapRealloc { from, to, .. } = prev {
            if from == to && matches!(next, MemEvent::Note { kind: NoteKind::Info, .. }) {
                if pos >= 2 {
                    let pre_prev = &self.trace[pos - 2];
                    if matches!(
                        pre_prev,
                        MemEvent::ArcClone { .. }
                            | MemEvent::ArcDrop { .. }
                            | MemEvent::LockAcquire { .. }
                            | MemEvent::LockRelease { .. }
                    ) {
                        return true;
                    }
                }
            }
        }

        // (d) Note { Info } → HeapFree when the preceding-preceding event
        // was an in-place HeapRealloc — final-drop case where the full
        // sequence is ArcDrop → HeapRealloc(refs:0) → Note(explanation)
        // → HeapFree. All four events at one cursor step.
        if let MemEvent::Note { kind: NoteKind::Info, .. } = prev {
            if matches!(next, MemEvent::HeapFree { .. }) && pos >= 2 {
                let pre_prev = &self.trace[pos - 2];
                if matches!(pre_prev, MemEvent::HeapRealloc { from, to, .. } if from == to) {
                    return true;
                }
            }
        }

        false
    }

    /// Reset the cursor to position 0.
    pub fn rewind(&mut self) {
        self.position = 0;
    }

    /// Compute the snapshot of the UI state at the current cursor position by
    /// replaying events `[0..self.position)` over an empty world.
    pub fn state_snapshot(&self, _source: &str) -> StateSnapshot {
        let mut world = World::default();
        for event in &self.trace[..self.position] {
            apply_event(&mut world, event);
        }
        let last = self.position.checked_sub(1).map(|i| &self.trace[i]);
        let editor_highlight = last.map(event_span);
        // **Post-M08 polish**: scan back through the coalesced atomic
        // group (positions where `is_slot_alloc_write_pair_boundary`
        // returns true) to find the most recent Note/ReturnValue/etc.
        // event. Without this, when the cursor lands at the END of a
        // coalesced group (e.g. after the HeapFree of an Arc last-drop
        // chain ArcDrop → HeapRealloc → Note → HeapFree), the `last`
        // event is HeapFree which has no Note — the explanatory Note
        // that fired earlier in the group would be invisible.
        let status = self.scan_back_for(self.position, note_to_status);
        let pending_return = self.scan_back_for(self.position, return_to_pending);
        // M07.2: transient copy arrow indicator. Set only on the BytesCopy
        // cursor step; cleared on next step.
        let pending_copy = self.scan_back_for(self.position, copy_to_pending);
        // M07.7: transient dispatch arrow indicator. Set only on a
        // FrameEnter cursor step where the entered frame's name has the
        // UFCS `<Type as Trait>::method` form AND a caller slot holds a
        // matching DynRef/BoxDyn. Drives the two-step (data + vtable)
        // arrow render.
        let pending_dispatch = last.and_then(|ev| dispatch_to_pending(ev, &world));
        // M03.1: the currently-executing frame is the topmost active frame in
        // the stack. All other active frames are paused callers waiting for
        // their callee to return; grayed frames have already returned.
        let current_frame = world.frames.iter().rev().find(|f| f.active);
        let current_frame_id = current_frame.map(|f| f.frame_id);
        // M03.1: only highlight a call site when there's an actual callee in
        // flight — i.e. ≥ 2 active frames (a caller waiting for its callee).
        // The bottommost active frame (typically `main`) is the program entry,
        // not invoked from any visible call site; its `enter_span` is just a
        // fallback to the function's declaration. Don't paint a misleading
        // red border on the whole entry function while it's the only active
        // frame on the stack.
        let active_count = world.frames.iter().filter(|f| f.active).count();
        let current_call_span = if active_count >= 2 {
            current_frame.map(|f| f.enter_span)
        } else {
            None
        };
        // M06 → M07: derive arrow list from active borrows (skip ones whose
        // source_slot isn't bound yet) PLUS owning relationships.
        let arrows_from_borrows: Vec<ArrowView> = world
            .borrows
            .iter()
            .filter_map(|b| {
                b.source_slot.map(|src| ArrowView {
                    source_slot: src,
                    target: match b.target {
                        BorrowTarget::Slot(id) => ArrowTarget::Slot(id),
                        BorrowTarget::Heap(addr) => ArrowTarget::Heap(addr),
                        // M07.2: &'static str borrows target the static region.
                        BorrowTarget::Static(addr) => ArrowTarget::Static(addr),
                    },
                    kind: if b.mutable { ArrowKind::Mut } else { ArrowKind::Shared },
                    // M07.1: slice borrows carry length + byte-range + element
                    // start. Populated from the source slot's Value::Slice.
                    // Single-element borrows (Value::Ref) leave these None.
                    len: b.slice_len,
                    byte_offset: b.slice_byte_offset,
                    byte_len: b.slice_byte_len,
                    elem_start: b.slice_elem_start,
                    // M07.4: field-borrow annotation (`.x`) — present for
                    // `&p.x` arrows; None for whole-binding borrows.
                    field_label: b.field_label.clone(),
                    // M07.4: method `self` borrows hide their arrow until
                    // the source slot is hovered (calling-convention info,
                    // not explicit user code).
                    hover_only: b.hover_only,
                })
            })
            .collect::<Vec<ArrowView>>();
        // M07: owning arrows (black) from world.owning.
        // **Post-M07.7 polish**: owning arrows are hover-only — consistent
        // with the fat-pointer data arrows (also hover-only) and with
        // calling-convention `&self` borrows from M07.4 (also hover-only).
        // The type column (`b : Box<i32>`) conveys "this owns heap memory";
        // the heap panel shows the target block independently. Hovering
        // the source slot row reveals the connection on demand.
        let mut arrows = arrows_from_borrows;
        for o in &world.owning {
            // M08: Arc owning relationships render as dashed-purple
            // arrows (ArrowKind::Arc); regular Box/Vec/String stay
            // black (ArrowKind::Owning).
            let arrow_kind = match o.kind {
                OwningKind::Box => ArrowKind::Owning,
                OwningKind::Arc => ArrowKind::Arc,
            };
            arrows.push(ArrowView {
                source_slot: o.source_slot,
                target: ArrowTarget::Heap(o.target_heap),
                kind: arrow_kind,
                len: None,
                byte_offset: None,
                byte_len: None,
                elem_start: None,
                field_label: None,
                hover_only: true,
            });
        }
        let heap = world.heap.iter().map(|h| HeapView {
            addr: h.addr,
            ty_name: h.ty_name.clone(),
            display: h.display.clone(),
            size: h.size,
            used: h.used,
            freed: h.freed,
            refcount: h.refcount,
            mutex_state: h.mutex_state,
        }).collect::<Vec<HeapView>>();
        // M07.2: clone the static region for the snapshot. Static blocks
        // persist; this is just a read-only view.
        let static_region = world.static_region.clone();
        // M07.7: clone the vtables region (analog of static memory; persists).
        let vtables = world.vtables.clone();
        // M08: build per-thread columns. For single-threaded programs
        // (no thread::spawn events fired), thread_meta only has thread 0
        // (or is empty — main is implicit) and all frames have thread_id
        // == 0. The legacy `frames` field also carries the same data
        // (main's frames) for full back-compat with M01-M07.7 JS rendering.
        let current_thread_id = world.current_thread_id;
        let mut thread_ids: Vec<u32> = vec![0];
        for &id in world.thread_meta.keys() {
            if !thread_ids.contains(&id) {
                thread_ids.push(id);
            }
        }
        // Single-threaded programs (no spawned threads + main is the
        // implicit only thread): leave `threads` empty so JS falls back
        // to the legacy single-column rendering via `frames`. This keeps
        // M01-M07.7 visualizations byte-identical.
        let threads: Vec<ThreadColumnView> = if thread_ids.len() == 1
            && world.thread_meta.is_empty()
        {
            Vec::new()
        } else {
            thread_ids
                .iter()
                .map(|&tid| {
                    let label = world.thread_meta.get(&tid)
                        .map(|m| m.label.clone())
                        .unwrap_or_else(|| if tid == 0 { "main".to_owned() } else { format!("thread #{tid}") });
                    let status = world.thread_meta.get(&tid)
                        .map(|m| m.status.clone())
                        .unwrap_or(ThreadStatusView::Running);
                    let frames: Vec<FrameCardView> = world.frames.iter()
                        .filter(|f| f.thread_id == tid)
                        .cloned()
                        .map(|f| frame_to_view(f, current_frame_id))
                        .collect();
                    ThreadColumnView {
                        thread_id: tid,
                        label,
                        frames,
                        is_current: tid == current_thread_id,
                        status,
                    }
                })
                .collect()
        };
        StateSnapshot {
            frames: world
                .frames
                .into_iter()
                .map(|f| frame_to_view(f, current_frame_id))
                .collect(),
            editor_highlight,
            current_call_span,
            status,
            pending_return,
            arrows,
            heap,
            static_region,
            vtables,
            threads,
            pending_copy,
            pending_dispatch,
            position: self.position,
            total: self.trace.len(),
            logical_position: self.logical_position(self.position),
            logical_total: self.logical_position(self.trace.len()),
            // **M08.2**: overwritten by the Player wrapper (which owns the
            // seed). Cursor itself doesn't know about scheduling.
            seed: 0,
        }
    }

    /// **Post-M08 polish**: walk back from `position - 1` through the
    /// current atomic group (consecutive boundary positions), applying
    /// `extract` to each event. Returns the first non-None result.
    /// Used by the snapshot's `last`-event lookups (Note status, etc.)
    /// so events that were coalesced INTO the current step are still
    /// visible. Without this, the explanatory Note in a coalesced
    /// Arc/Lock atom would be invisible because the cursor lands AFTER
    /// the final event in the atom (e.g. HeapFree, not the Note).
    fn scan_back_for<T>(&self, position: usize, extract: impl Fn(&MemEvent) -> Option<T>) -> Option<T> {
        if position == 0 {
            return None;
        }
        let mut p = position;
        loop {
            p -= 1;
            if let Some(result) = extract(&self.trace[p]) {
                return Some(result);
            }
            // Stop when we reach the START of the atomic group — i.e.
            // when position p is NOT a boundary (it's a real stopping
            // point). If p == 0, we've scanned the whole trace; stop.
            if p == 0 || !self.is_slot_alloc_write_pair_boundary(p) {
                return None;
            }
        }
    }

    /// **Post-M08 polish**: count how many cursor stops the user would
    /// see if they stepped from position 0 to `raw_pos` — i.e. raw
    /// positions MINUS the count of intermediate "boundary" positions
    /// that the Player wrapper skips over. Used to drive a user-facing
    /// step counter that advances 1-by-1.
    fn logical_position(&self, raw_pos: usize) -> usize {
        let cap = raw_pos.min(self.trace.len());
        let mut count = 0usize;
        for p in 1..=cap {
            if !self.is_slot_alloc_write_pair_boundary(p) {
                count += 1;
            }
        }
        count
    }
}

/// **M07.7**: build the transient dispatch indicator. Triggered only when
/// the most recent event is a `FrameEnter` whose `fn_name` is in the
/// UFCS form `<TypeName as TraitName>::method`. Walks the caller frames
/// for a slot whose `dyn_view` has a matching `<Type as Trait>` label —
/// that's the slot that initiated dispatch.
fn dispatch_to_pending(event: &MemEvent, world: &World) -> Option<DispatchView> {
    let (frame_id, fn_name) = match event {
        MemEvent::FrameEnter { frame_id, fn_name, .. } => (frame_id.0, fn_name.as_str()),
        _ => return None,
    };
    // Parse the UFCS form: `<Type as Trait>::method`.
    let (vtable_label, method) = parse_ufcs(fn_name)?;
    // Search caller frames (skip the innermost active = the just-entered
    // dispatch frame itself) for a slot whose dyn_view matches the label.
    let candidates: Vec<&FrameInProgress> = world
        .frames
        .iter()
        .filter(|f| f.active && f.frame_id != frame_id)
        .collect();
    // Scan from innermost outward so the most recent caller wins on
    // overlapping vtables.
    for frame in candidates.iter().rev() {
        for slot in &frame.slots {
            if let Some(dv) = &slot.dyn_view {
                if dv.vtable_label == vtable_label {
                    return Some(DispatchView {
                        source_slot: slot.slot_id,
                        vtable_addr: dv.vtable_addr,
                        target_frame: frame_id,
                        method: method.to_owned(),
                    });
                }
            }
        }
    }
    None
}

/// **M07.7**: parse a UFCS-style frame name `<Type as Trait>::method` into
/// `(label "<Type as Trait>", method)`. Returns `None` for non-UFCS names.
fn parse_ufcs(s: &str) -> Option<(String, &str)> {
    if !s.starts_with('<') {
        return None;
    }
    let close = s.find('>')?;
    let after = &s[close + 1..];
    let method = after.strip_prefix("::")?;
    let label = s[..=close].to_owned();
    Some((label, method))
}

// ─── Internal world model ──────────────────────────────────────────────────

#[derive(Default)]
struct World {
    /// Active frames, outermost first.
    frames: Vec<FrameInProgress>,
    /// **M06**: active borrows. Push on BorrowShared/Mut, remove on BorrowEnd.
    /// **M07**: target widened to support heap allocations.
    borrows: Vec<ActiveBorrowState>,
    /// **M07**: live heap allocations.
    heap: Vec<HeapAllocState>,
    /// **M07**: owning relationships (slot → heap). Push on SlotWrite of a
    /// Value::Box/Vec/String. Removed when the heap addr is freed OR when
    /// the slot is overwritten with a different value.
    owning: Vec<OwningState>,
    /// **M07.2**: static-memory blocks (one per unique string-literal content,
    /// deduplicated). Push on StaticAlloc; never remove (static memory
    /// persists for the trace's lifetime).
    static_region: Vec<StaticView>,
    /// **M07.7**: live vtables (one per unique `(trait, type)` pair). Push
    /// on VtableAlloc; never remove (vtables persist for the trace's
    /// lifetime).
    vtables: Vec<VtableView>,
    /// **M08**: which thread is currently executing. Updated by
    /// `MemEvent::ThreadSwitch`. Default 0 (main). New FrameInProgress
    /// entries are tagged with this id so the snapshot groups frames
    /// per-thread into ThreadColumnView entries.
    current_thread_id: u32,
    /// **M08**: thread metadata (label, status). Keyed by thread_id.
    /// Built from `ThreadSpawn` / `ThreadJoin` events. Thread 0 (main)
    /// is implicit (default label "main", status Running) — only
    /// spawned threads + completed threads need explicit entries.
    thread_meta: indexmap::IndexMap<u32, ThreadMeta>,
}

/// **M08**: per-thread metadata for the UI (label, lifecycle status).
struct ThreadMeta {
    label: String,
    status: ThreadStatusView,
}

struct ActiveBorrowState {
    borrow_id: u32,
    /// `None` until a `SlotWrite` of `Value::Ref` binds the reference to a slot.
    source_slot: Option<u32>,
    /// M07: a borrow can target a slot OR a heap allocation.
    target: BorrowTarget,
    mutable: bool,
    /// **M07.1**: `Some(len)` when this borrow is a slice (the source slot
    /// holds a `Value::Slice { len, .. }`); `None` for single-element borrows.
    /// Populated by the SlotWrite arm when it sees a Value::Slice.
    slice_len: Option<u64>,
    /// **M07.1**: byte offset of the slice's view within the target heap
    /// block; populated alongside `slice_len`. Used by the renderer to
    /// drive the hover-highlight on byte-cells.
    slice_byte_offset: Option<u64>,
    /// **M07.1**: byte length of the slice's view.
    slice_byte_len: Option<u64>,
    /// **M07.1**: element index where the slice starts; populated alongside
    /// the byte fields. Drives the element-span highlight on hover.
    slice_elem_start: Option<u64>,
    /// **M07.4**: field name when this borrow targets a sub-field of a
    /// struct (`&p.x` → `Some("x")`). Populated by the lazy-materialization
    /// path in apply_event's SlotWrite arm when it sees a Value::Ref with
    /// non-empty field_path.
    field_label: Option<String>,
    /// **M07.4**: hide the arrow until the source slot is hovered. Set on
    /// method self-receiver borrows so the always-on arrow doesn't clutter
    /// the visualization (the borrow is calling-convention, not user code).
    hover_only: bool,
}

#[derive(Copy, Clone)]
enum BorrowTarget {
    Slot(u32),
    Heap(u32),
    /// **M07.2**: borrow into the static-memory region (StaticAddr.0).
    Static(u32),
}

struct HeapAllocState {
    addr: u32,
    ty_name: String,
    display: String,
    size: u32,
    used: u32,
    /// **M07**: true after the allocation has been freed but kept visible
    /// in the heap panel (grayed) to convey "memory still physically there,
    /// just available for the allocator to reuse." Same pedagogy as M03.1's
    /// "stack slots persist in grayed frames until reused."
    freed: bool,
    /// **M08**: Arc refcount when this heap object is `HeapObject::Arc`;
    /// `None` for non-Arc blocks. Set by ArcClone/ArcDrop apply_event arms.
    refcount: Option<u32>,
    /// **Post-M08 polish**: Mutex lock state when this block is a Mutex
    /// or fused Arc<Mutex<T>>. Parsed from display-string suffixes
    /// (`[free]` / `[locked by #N]`) at apply_event time.
    mutex_state: Option<MutexLockState>,
}

/// **M08**: ownership flavor for an `OwningState` entry. `Box` → black
/// solid arrow (M07 behavior); `Arc` → dashed purple (post-M08 polish).
#[derive(Copy, Clone, PartialEq)]
enum OwningKind {
    Box,
    Arc,
}

struct OwningState {
    source_slot: u32,
    target_heap: u32,
    kind: OwningKind,
}

#[derive(Clone)]
struct FrameInProgress {
    frame_id: u32,
    /// **M08**: which thread this frame belongs to. Defaults to 0 (main)
    /// for single-threaded programs; spawned-thread frames carry their
    /// owning thread's id. `state_snapshot` groups frames by this id
    /// into `ThreadColumnView` entries.
    thread_id: u32,
    fn_name: String,
    slots: Vec<LiveSlot>,
    /// M03.1: false after `FrameLeave`. The frame stays in `World.frames` so
    /// the visualization can show it grayed out.
    active: bool,
    /// M03.1: rendered return value once `MemEvent::ReturnValue` has fired
    /// for this frame. Persists across `FrameLeave`.
    return_value: Option<String>,
    /// M03.1: span of the `FrameEnter` event that opened this frame — i.e.
    /// the call-site span (`add(2, 3)` text). Used to paint a red border on
    /// the active call site in the editor so the learner can see which
    /// specific call site is in flight.
    enter_span: Span,
}

#[derive(Clone)]
struct LiveSlot {
    slot_id: u32,
    name: String,
    ty: String,
    value: Option<String>,
    /// **M07.3**: populated when the slot holds a `Value::Array` — drives
    /// the inline byte-cell rendering in the slot's value area.
    inline_cells: Option<InlineCellsView>,
    /// **M07.4**: populated when the slot holds a `Value::Struct` — drives
    /// the per-field labeled-row rendering. Mutually exclusive with `value`
    /// and `inline_cells`.
    struct_view: Option<StructView>,
    /// **M07.7**: populated when the slot holds a `Value::DynRef` /
    /// `Value::BoxDyn` — drives the fat-pointer two-cell rendering.
    dyn_view: Option<DynView>,
    /// **Post-M08 polish**: `true` once a `SlotMove` event for this slot
    /// has fired (e.g. binding captured by a `move ||` closure). Stays
    /// `true` for the rest of the trace — moved bindings can't be
    /// un-moved.
    moved: bool,
}

fn apply_event(world: &mut World, event: &MemEvent) {
    match event {
        MemEvent::FrameEnter { frame_id, fn_name, span, .. } => {
            // M03.1: a new frame opens by reusing the stack region above the
            // current top-active frame. Any grayed (inactive) frames sitting
            // above the active top represent bytes about to be overwritten by
            // this push — drop them. Real machine semantics: when main calls
            // add() twice, the second call writes over the first call's
            // freed-but-not-zeroed stack slot.
            // M08: only collapse inactive frames belonging to the SAME
            // thread (otherwise we'd wipe a quiescent thread's frame stack
            // when a different thread spawns or enters a new frame).
            while world
                .frames
                .last()
                .is_some_and(|f| !f.active && f.thread_id == world.current_thread_id)
            {
                world.frames.pop();
            }
            world.frames.push(FrameInProgress {
                frame_id: frame_id.0,
                thread_id: world.current_thread_id,
                fn_name: fn_name.clone(),
                slots: Vec::new(),
                active: true,
                return_value: None,
                enter_span: *span,
            });
        }
        MemEvent::FrameLeave { .. } => {
            // M03.1: mark the innermost active frame as inactive instead of
            // popping it. The frame card stays in the visualization (grayed)
            // so the learner can see that the stack bytes persist after the
            // function returns — there is no physical "frame disappears"
            // event at the machine level, just storage that's now free to be
            // reused by the next call.
            // Post-M08: route to the innermost active frame OF THE
            // CURRENT THREAD (not the globally innermost). Otherwise a
            // joined thread's FrameLeave could close main's frame if
            // main is still mid-flow but other threads have intervening
            // frames.
            let current_tid = world.current_thread_id;
            if let Some(frame) = world.frames.iter_mut().rev()
                .find(|f| f.active && f.thread_id == current_tid)
            {
                frame.active = false;
            }
        }
        MemEvent::SlotAlloc { slot_id, name, ty, .. } => {
            // M03.1: route the alloc to the innermost ACTIVE frame; inactive
            // (grayed) frames shouldn't receive new slots.
            // Post-M08: filter by current_thread_id so a spawned thread's
            // captures land in its OWN frame, not in main's (which would
            // make the captures invisible in the thread's column).
            let current_tid = world.current_thread_id;
            if let Some(frame) = world.frames.iter_mut().rev()
                .find(|f| f.active && f.thread_id == current_tid)
            {
                frame.slots.push(LiveSlot {
                    slot_id: slot_id.0,
                    name: name.clone(),
                    ty: render_ty(ty),
                    value: None,
                    inline_cells: None,
                    struct_view: None,
                    dyn_view: None,
                    moved: false,
                });
            }
        }
        MemEvent::SlotWrite { slot_id, value, .. } => {
            // M06: if this write lands a Value::Ref, bind the borrow's source_slot.
            // M07.1: if this write lands a Value::Slice, bind source_slot AND
            // record the slice's length on the borrow (drives the [len: N] label).
            if let Value::Ref { borrow_id, .. } = value {
                if let Some(borrow) = world
                    .borrows
                    .iter_mut()
                    .find(|b| b.borrow_id == borrow_id.0)
                {
                    borrow.source_slot = Some(slot_id.0);
                }
            }
            if let Value::Slice { borrow_id, start, len, byte_offset, byte_len, target, mutable } = value {
                if let Some(borrow) = world
                    .borrows
                    .iter_mut()
                    .find(|b| b.borrow_id == borrow_id.0)
                {
                    borrow.source_slot = Some(slot_id.0);
                    borrow.slice_len = Some(*len);
                    borrow.slice_byte_offset = Some(*byte_offset);
                    borrow.slice_byte_len = Some(*byte_len);
                    borrow.slice_elem_start = Some(*start);
                } else {
                    // **M07.2**: no prior BorrowShared event. Static-target
                    // slices skip BorrowShared/BorrowEnd entirely (the
                    // borrow lifecycle would only produce silent no-op
                    // cursor steps since the borrow's arrow never existed
                    // before this SlotWrite). Materialize the borrow entry
                    // lazily here with source_slot already bound, so the
                    // arrow renders.
                    let t = match target {
                        crate::event::Pointee::Slot(id) => BorrowTarget::Slot(id.0),
                        crate::event::Pointee::Heap(addr) => BorrowTarget::Heap(addr.0),
                        crate::event::Pointee::Static(addr) => BorrowTarget::Static(addr.0),
                    };
                    world.borrows.push(ActiveBorrowState {
                        borrow_id: borrow_id.0,
                        source_slot: Some(slot_id.0),
                        target: t,
                        mutable: *mutable,
                        slice_len: Some(*len),
                        slice_byte_offset: Some(*byte_offset),
                        slice_byte_len: Some(*byte_len),
                        slice_elem_start: Some(*start),
                        // Slice borrows don't carry field labels — slicing
                        // a struct field isn't an M07.4 path.
                        field_label: None,
                        // Post-M07.7 polish: all borrow / slice / owning
                        // arrows are hover-only by default. See the matching
                        // BorrowShared/BorrowMut arms.
                        hover_only: true,
                    });
                }
            }
            // **M07.4**: lazy-materialize a Value::Ref when no prior borrow
            // entry exists for its borrow_id. Two distinct shapes both hit
            // this path:
            //   1. Field borrows (`&p.x` → non-empty `field_path`) — eval
            //      skips BorrowShared per the M07.3 lazy pattern; here we
            //      create the entry with a `.x` field_label that drives
            //      the arrow's annotation + per-field hover highlight.
            //   2. Method-call self-receivers (`&self` / `&mut self`) —
            //      eval skips BorrowShared because a separate BorrowShared
            //      cursor step between FrameEnter and the self SlotWrite
            //      would just be a silent tick. The arrow should still
            //      render; no field label.
            // M06 paths (`&p`, `&mut p`) emit BorrowShared first → the
            // borrow entry already exists, so this lazy path is a no-op.
            if let Value::Ref { borrow_id, target, mutable, field_path } = value {
                if !world.borrows.iter().any(|b| b.borrow_id == borrow_id.0) {
                    let t = match target {
                        crate::event::Pointee::Slot(id) => BorrowTarget::Slot(id.0),
                        crate::event::Pointee::Heap(addr) => BorrowTarget::Heap(addr.0),
                        crate::event::Pointee::Static(addr) => BorrowTarget::Static(addr.0),
                    };
                    let field_label = if field_path.is_empty() {
                        None
                    } else {
                        Some(format!(".{}", field_path.join(".")))
                    };
                    // Post-M07.7 polish: lazy-materialized borrow arrows
                    // (field borrows, self-receiver borrows, etc.) are
                    // hover-only by default. Consistent with M06's
                    // BorrowShared/BorrowMut handlers and owning arrows.
                    let _ = lookup_slot_name;
                    world.borrows.push(ActiveBorrowState {
                        borrow_id: borrow_id.0,
                        source_slot: Some(slot_id.0),
                        target: t,
                        mutable: *mutable,
                        slice_len: None,
                        slice_byte_offset: None,
                        slice_byte_len: None,
                        slice_elem_start: None,
                        field_label,
                        hover_only: true,
                    });
                }
            }
            // M06.1: render Value::Ref using the *binding name* of the target
            // slot (`&x`, `&mut x`) instead of `&slot0` / `&mut slot0`. The
            // SlotId is implementation jargon; learners think in binding names.
            // Fall back to `slot{N}` only if the target slot isn't found.
            // **M07.4**: when field_path is non-empty (`&p.x`), append the
            // path to the binding name (`&p.x`, `&mut p.x`).
            let rendered = match value {
                // M06.1 → M07: target widened from SlotId to Pointee.
                Value::Ref {
                    target: crate::event::Pointee::Slot(target_slot),
                    mutable,
                    field_path,
                    ..
                } => {
                    let name = lookup_slot_name(&world.frames, target_slot.0)
                        .unwrap_or_else(|| format!("slot{}", target_slot.0));
                    let suffix = if field_path.is_empty() {
                        String::new()
                    } else {
                        format!(".{}", field_path.join("."))
                    };
                    if *mutable {
                        format!("&mut {name}{suffix}")
                    } else {
                        format!("&{name}{suffix}")
                    }
                }
                Value::Ref { target: crate::event::Pointee::Heap(addr), mutable, .. } => {
                    if *mutable {
                        format!("&mut heap[{}]", addr.0)
                    } else {
                        format!("&heap[{}]", addr.0)
                    }
                }
                _ => render_value(value),
            };
            // M07: if this write lands a Value::Box/Vec/String, register the
            // owning relationship AND suppress the redundant value-cell text
            // (the black owning arrow + the type column already convey the
            // pointer; `Vec→heap[2]` adds noise without adding info).
            // M07.1: same suppression for Value::Slice — the blue arrow with
            // its [len: N] annotation conveys everything; text would clutter.
            let rendered = if let Value::Box { addr } | Value::Vec { addr } | Value::String { addr } = value {
                world.owning.retain(|o| o.source_slot != slot_id.0);
                world.owning.push(OwningState {
                    source_slot: slot_id.0,
                    target_heap: addr.0,
                    kind: OwningKind::Box,
                });
                String::new() // empty value cell — arrow says it all
            } else if let Value::Arc { addr } = value {
                // **M08**: Arc bindings get a dashed-purple arrow (hover-only
                // per Rule 1) to the shared heap allocation. Multiple Arc
                // bindings can target the same addr; each registers its own
                // owning relationship so each slot gets its own arrow. The
                // refcount on the heap block conveys the share count.
                world.owning.retain(|o| o.source_slot != slot_id.0);
                world.owning.push(OwningState {
                    source_slot: slot_id.0,
                    target_heap: addr.0,
                    kind: OwningKind::Arc,
                });
                String::new() // empty value cell — arrow says it all
            } else if matches!(value, Value::Slice { .. }) {
                String::new() // empty value cell — slice arrow + [len: N] annotation say it all
            } else if matches!(value, Value::Array { .. }) {
                // M07.3: text value suppressed; the inline byte-cells (set
                // below) are the visualization.
                String::new()
            } else if matches!(value, Value::Struct { .. }) {
                // M07.4: text value suppressed; the per-field StructView
                // (built below) is the visualization.
                String::new()
            } else if matches!(value, Value::DynRef { .. } | Value::BoxDyn { .. }) {
                // M07.7: text value suppressed; the DynView (built below)
                // is the fat-pointer two-cell visualization.
                String::new()
            } else {
                rendered
            };
            // **M07.3**: if Value::Array, build the InlineCellsView from
            // element values + size. The slot's `value` is set to empty
            // (handled above); the inline_cells field carries the per-byte
            // / per-element rendering data.
            let inline_cells = if let Value::Array { elements, elem_ty } = value {
                let elem_size = ty_size_bytes_ui(elem_ty);
                let size = elements.len() as u32 * elem_size;
                let elem_strs: Vec<String> = elements.iter().map(render_value).collect();
                Some(InlineCellsView {
                    size,
                    used: size,
                    elements: elem_strs,
                })
            } else {
                None
            };
            // **M07.4**: if Value::Struct, build the StructView from the
            // field values. Drives the per-field labeled-row rendering in
            // the JS (research R-016 Proposal A).
            // **M07.7**: build the DynView for trait-object slots AND
            // lazy-materialize the underlying data borrow (so the data-arrow
            // renders pointing from this slot's `data` cell to the target).
            // Mirror M07.4's lazy-materialization pattern for borrows that
            // never fired a BorrowShared event (the `as` cast and implicit
            // coercion paths skip the BorrowShared step — they reuse the
            // inner Ref's borrow_id or allocate a fresh one in eval).
            let dyn_view = match value {
                Value::DynRef { borrow_id, target, vtable, mutable, trait_name } => {
                    let data_label = match target {
                        crate::event::Pointee::Slot(id) => {
                            lookup_slot_name(&world.frames, id.0)
                                .unwrap_or_else(|| format!("slot{}", id.0))
                        }
                        crate::event::Pointee::Heap(addr) => format!("heap[{}]", addr.0),
                        crate::event::Pointee::Static(addr) => format!("static[{}]", addr.0),
                    };
                    // Find the vtable's (trait, type) labels in the world.
                    let vtable_label = world
                        .vtables
                        .iter()
                        .find(|v| v.addr == vtable.0)
                        .map(|v| format!("<{} as {}>", v.type_name, v.trait_name))
                        .unwrap_or_else(|| format!("<? as {trait_name}>"));
                    // Lazy-materialize a borrow entry for the data ptr arrow
                    // if one doesn't already exist for this borrow_id.
                    // **M07.7**: data-ptr arrows for trait-object slots are
                    // hover-only — the `data: → p` text inside the fat-pointer
                    // cell already conveys the pointer; the always-on arrow
                    // was just visual noise alongside the more-important
                    // dispatch arrow. Matches [[feedback_arrow_viz_rules]]:
                    // implicit / calling-convention borrows reveal on hover.
                    if !world.borrows.iter().any(|b| b.borrow_id == borrow_id.0) {
                        let t = match target {
                            crate::event::Pointee::Slot(id) => BorrowTarget::Slot(id.0),
                            crate::event::Pointee::Heap(addr) => BorrowTarget::Heap(addr.0),
                            crate::event::Pointee::Static(addr) => BorrowTarget::Static(addr.0),
                        };
                        world.borrows.push(ActiveBorrowState {
                            borrow_id: borrow_id.0,
                            source_slot: Some(slot_id.0),
                            target: t,
                            mutable: *mutable,
                            slice_len: None,
                            slice_byte_offset: None,
                            slice_byte_len: None,
                            slice_elem_start: None,
                            field_label: None,
                            hover_only: true,
                        });
                    } else if let Some(b) = world.borrows.iter_mut().find(|b| b.borrow_id == borrow_id.0) {
                        b.source_slot = Some(slot_id.0);
                        // Upgrade an existing (e.g. created by an explicit
                        // `&p` BorrowShared) entry to hover-only when it's
                        // now bound to a fat-pointer slot.
                        b.hover_only = true;
                    }
                    Some(DynView {
                        data_label,
                        vtable_label,
                        vtable_addr: vtable.0,
                    })
                }
                Value::BoxDyn { addr, vtable, trait_name } => {
                    let data_label = format!("heap[{}]", addr.0);
                    let vtable_label = world
                        .vtables
                        .iter()
                        .find(|v| v.addr == vtable.0)
                        .map(|v| format!("<{} as {}>", v.type_name, v.trait_name))
                        .unwrap_or_else(|| format!("<? as {trait_name}>"));
                    // Register an owning relationship for the data ptr arrow.
                    world.owning.retain(|o| o.source_slot != slot_id.0);
                    world.owning.push(OwningState {
                        source_slot: slot_id.0,
                        target_heap: addr.0,
                        kind: OwningKind::Box,
                    });
                    Some(DynView {
                        data_label,
                        vtable_label,
                        vtable_addr: vtable.0,
                    })
                }
                _ => None,
            };
            let struct_view = if let Value::Struct { name, fields } = value {
                let field_views: Vec<StructFieldView> = fields
                    .iter()
                    .map(|(fname, fval)| {
                        let ty_label = match fval {
                            Value::Int { kind, .. } => kind.name().to_owned(),
                            Value::Float { kind, .. } => kind.name().to_owned(),
                            Value::Bool(_) => "bool".to_owned(),
                            Value::Unit => "()".to_owned(),
                            // Non-primitive field types are out of M07.4
                            // scope but the fallback still renders cleanly.
                            other => other.type_name().to_owned(),
                        };
                        StructFieldView {
                            name: fname.clone(),
                            ty_label,
                            size: value_size_bytes_ui(fval),
                            display: render_value(fval),
                        }
                    })
                    .collect();
                Some(StructView {
                    name: name.clone(),
                    fields: field_views,
                })
            } else {
                None
            };
            for frame in &mut world.frames {
                if let Some(slot) = frame.slots.iter_mut().find(|s| s.slot_id == slot_id.0) {
                    slot.value = Some(rendered);
                    slot.inline_cells = inline_cells;
                    slot.struct_view = struct_view;
                    slot.dyn_view = dyn_view;
                    return;
                }
            }
        }
        MemEvent::SlotDrop { slot_id, .. } => {
            for frame in &mut world.frames {
                if let Some(idx) = frame.slots.iter().position(|s| s.slot_id == slot_id.0) {
                    frame.slots.remove(idx);
                    return;
                }
            }
        }
        MemEvent::ReturnValue { frame_id, value, .. } => {
            // M03.1: record the return value on the matching frame so the
            // annotation persists into the subsequent FrameLeave (and the
            // resulting grayed frame keeps the `→ <value>` indicator visible).
            if let Some(frame) = world
                .frames
                .iter_mut()
                .find(|f| f.frame_id == frame_id.0)
            {
                frame.return_value = Some(render_value(value));
            }
        }
        // **M06 → M07**: borrow events update World.borrows. Target widened
        // to support both Slot and Heap pointees.
        MemEvent::BorrowShared { borrow_id, target, .. } => {
            let t = match target {
                crate::event::Pointee::Slot(id) => BorrowTarget::Slot(id.0),
                crate::event::Pointee::Heap(addr) => BorrowTarget::Heap(addr.0),
                crate::event::Pointee::Static(addr) => BorrowTarget::Static(addr.0),
            };
            world.borrows.push(ActiveBorrowState {
                borrow_id: borrow_id.0,
                source_slot: None,
                target: t,
                mutable: false,
                slice_len: None,
                slice_byte_offset: None,
                slice_byte_len: None,
                slice_elem_start: None,
                field_label: None,
                // Post-M07.7 polish: all arrows hover-only by default.
                // The borrow's existence is visible from the source slot's
                // type column + the target's identity in the heap/slot
                // panel; hovering the source reveals the connection on
                // demand. Matches owning + dispatch + slice + dyn data
                // arrows — every arrow flavor is reveal-on-hover now.
                hover_only: true,
            });
        }
        MemEvent::BorrowMut { borrow_id, target, .. } => {
            let t = match target {
                crate::event::Pointee::Slot(id) => BorrowTarget::Slot(id.0),
                crate::event::Pointee::Heap(addr) => BorrowTarget::Heap(addr.0),
                crate::event::Pointee::Static(addr) => BorrowTarget::Static(addr.0),
            };
            world.borrows.push(ActiveBorrowState {
                borrow_id: borrow_id.0,
                source_slot: None,
                target: t,
                mutable: true,
                slice_len: None,
                slice_byte_offset: None,
                slice_byte_len: None,
                slice_elem_start: None,
                field_label: None,
                // Post-M07.7: see BorrowShared.
                hover_only: true,
            });
        }
        MemEvent::BorrowEnd { borrow_id, .. } => {
            world.borrows.retain(|b| b.borrow_id != borrow_id.0);
        }
        // **M07**: heap events.
        MemEvent::HeapAlloc { addr, size, used, ty_name, fragment_of, split_remainder, .. } => {
            // **M08**: detect Arc blocks via the eval-side display
            // string (`Arc<T> = v [refs: 1]`). Newly-allocated Arcs start
            // at refcount 1; subsequent ArcClone/ArcDrop events mutate.
            let refcount = if ty_name.starts_with("Arc<") {
                Some(1)
            } else {
                None
            };
            // Post-M08: detect Mutex blocks for the lock-state badge.
            // Initial state is always Free (Mutex::new creates an
            // unlocked mutex).
            let mutex_state = if ty_name.starts_with("Mutex<") {
                Some(MutexLockState::Free)
            } else {
                None
            };
            let new_state = HeapAllocState {
                addr: addr.0,
                ty_name: ty_name.clone(),
                display: ty_name.clone(),
                size: *size,
                used: *used,
                freed: fragment_of.is_some(),
                refcount,
                mutex_state,
            };
            let inserted_at = if let Some(parent) = fragment_of {
                // (Legacy path) Fragment: insert immediately after the parent
                // live block. M07.2 collapses this into split_remainder on
                // the same event, but old traces still go through here.
                let parent_idx = world.heap.iter()
                    .position(|h| h.addr == parent.0 && !h.freed);
                match parent_idx {
                    Some(i) => { world.heap.insert(i + 1, new_state); i + 1 }
                    None => { world.heap.push(new_state); world.heap.len() - 1 }
                }
            } else {
                // Real allocation: if reusing a freed addr, REPLACE the
                // freed entry IN PLACE (keeps the visual position stable
                // instead of moving the block to the end of the panel).
                let idx = world.heap.iter()
                    .position(|h| h.addr == addr.0 && h.freed);
                match idx {
                    Some(i) => { world.heap[i] = new_state; i }
                    None => { world.heap.push(new_state); world.heap.len() - 1 }
                }
            };
            // **M07.2**: if the allocator split a larger freed chunk to
            // satisfy this request, materialize the leftover as a freed
            // fragment block immediately after the live block — at the
            // SAME cursor step so the user never sees a transient frame
            // where the freed bytes appear to have vanished.
            if let Some((frag_addr, frag_size)) = split_remainder {
                let frag = HeapAllocState {
                    addr: frag_addr.0,
                    ty_name: "(fragment from split)".to_owned(),
                    display: "(fragment from split)".to_owned(),
                    size: *frag_size,
                    used: 0,
                    freed: true,
                    refcount: None,
                    mutex_state: None,
                };
                world.heap.insert(inserted_at + 1, frag);
            }
        }
        MemEvent::HeapRealloc { from, to, new_size, new_used, new_display, .. } => {
            if from == to {
                if let Some(h) = world.heap.iter_mut().find(|h| h.addr == from.0 && !h.freed) {
                    h.size = *new_size;
                    h.used = *new_used;
                    h.display = new_display.clone();
                    // **M08**: extract refcount from `[refs: N]` suffix in
                    // the Arc display string updated by `refresh_arc_display`.
                    // For non-Arc reallocs the suffix is absent → leave
                    // refcount unchanged.
                    if let Some(rc) = parse_refcount_suffix(new_display) {
                        h.refcount = Some(rc);
                    }
                    // Post-M08: extract mutex lock state from display
                    // suffix (`[free]` / `[locked by #N]`). For ArcMutex
                    // fusion the display carries BOTH refcount and lock
                    // state; we parse both.
                    if let Some(state) = parse_mutex_state_suffix(new_display) {
                        h.mutex_state = Some(state);
                    }
                }
            } else {
                if let Some(h) = world.heap.iter_mut().find(|h| h.addr == from.0 && !h.freed) {
                    h.freed = true;
                }
                world.heap.push(HeapAllocState {
                    addr: to.0,
                    ty_name: new_display.clone(),
                    display: new_display.clone(),
                    size: *new_size,
                    used: *new_used,
                    freed: false,
                    refcount: None,
                    mutex_state: None,
                });
                // Update owning relationships from `from` to `to`.
                for o in world.owning.iter_mut() {
                    if o.target_heap == from.0 {
                        o.target_heap = to.0;
                    }
                }
            }
            // M07 simplification: borrows pointing at the old addr stay
            // pointing at the old addr (which is gone) — visually shows as
            // a dangling arrow. The dangling-borrow Note (emitted by eval)
            // delivers the pedagogy.
        }
        MemEvent::HeapFree { addr, .. } => {
            // M07: mark freed (keep visible, grayed) instead of removing.
            if let Some(h) = world.heap.iter_mut().find(|h| h.addr == addr.0 && !h.freed) {
                h.freed = true;
            }
            world.owning.retain(|o| o.target_heap != addr.0);
        }
        // **M07.2**: static-memory block allocation (fires once per unique
        // literal content). Push to the static region; never remove.
        MemEvent::StaticAlloc { addr, bytes, .. } => {
            let size = bytes.len() as u32;
            world.static_region.push(StaticView {
                addr: addr.0,
                bytes: bytes.clone(),
                size,
                display: format!("\"{bytes}\""),
            });
        }
        // **M07.7**: vtable allocation (fires once per unique
        // `(trait, type)` pair). Push to the vtables region; never remove.
        // The per-method target labels render as `<TypeName as TraitName>::method`.
        MemEvent::VtableAlloc { addr, trait_name, type_name, methods, .. } => {
            let method_entries: Vec<(String, String)> = methods
                .iter()
                .map(|m| {
                    (
                        m.clone(),
                        format!("<{type_name} as {trait_name}>::{m}"),
                    )
                })
                .collect();
            world.vtables.push(VtableView {
                addr: addr.0,
                trait_name: trait_name.clone(),
                type_name: type_name.clone(),
                methods: method_entries,
            });
        }
        // The remaining variants don't modify world state. `Note` surfaces via
        // `note_to_status`; `BytesCopy` surfaces via `pending_copy` on the
        // most-recent-event side path.
        // **M08**: thread lifecycle + scheduler events.
        MemEvent::ThreadSpawn { thread_id, .. } => {
            world.thread_meta.insert(*thread_id, ThreadMeta {
                label: format!("thread #{thread_id}"),
                status: ThreadStatusView::Ready,
            });
        }
        MemEvent::ThreadSwitch { thread_id, .. } => {
            world.current_thread_id = thread_id.0;
            // Newly-current thread transitions from Ready to Running on
            // first switch-in.
            if let Some(meta) = world.thread_meta.get_mut(&thread_id.0) {
                if matches!(meta.status, ThreadStatusView::Ready) {
                    meta.status = ThreadStatusView::Running;
                }
            }
        }
        MemEvent::ThreadJoin { thread_id, .. } => {
            if let Some(meta) = world.thread_meta.get_mut(thread_id) {
                meta.status = ThreadStatusView::Joined;
            }
        }
        // **M08**: Arc refcount transitions. The actual refcount value comes
        // from the eval-side `refresh_arc_display` HeapRealloc (which carries
        // the new `[refs: N]` suffix). Here we just ensure the slot owns no
        // stale state and let the HeapRealloc arm pick up the count update.
        // ArcDrop also marks the source slot's owning relationship for removal
        // — actual HeapFree (when count reaches 0) is a separate event.
        MemEvent::ArcClone { .. } | MemEvent::ArcDrop { .. } => {
            // No direct state mutation: the count value is conveyed via
            // HeapRealloc.display (parsed in the HeapRealloc arm above);
            // owning-arrow lifecycle stays driven by SlotWrite of Value::Arc
            // and slot-overwrite/scope-exit on Arc bindings.
        }
        // **Post-M08 polish**: SlotMove marks the source slot as moved.
        // The slot stays on the frame card (its pointer-bytes persist
        // per the M03.1 "memory persists until reused" principle) but
        // gets a grayed-out `<moved>` rendering — matches Rust's actual
        // move semantics where the stack bytes don't physically move,
        // only the type-system's ownership tracking changes.
        MemEvent::SlotMove { from, .. } => {
            for frame in &mut world.frames {
                if let Some(slot) = frame.slots.iter_mut().find(|s| s.slot_id == from.0) {
                    slot.moved = true;
                    break;
                }
            }
        }
        MemEvent::LockAcquire { .. }
        | MemEvent::LockRelease { .. }
        | MemEvent::ThreadPark { .. }
        | MemEvent::Note { .. }
        | MemEvent::BytesCopy { .. } => {
            // No world-state change. Phase 5 (US3) wires
            // LockAcquire/LockRelease/ThreadPark into the mutex-display
            // + parked-thread machinery (deferred to M08.1 per the
            // simplified scheduler — no contention in M08 v1).
        }
        MemEvent::Deadlock { .. } => {
            // **M08.2**: terminal event. No world-state change here; the
            // UI surfaces deadlock via the status bar (Phase 6).
        }
    }
}

/// **M08**: parse the `[refs: N]` suffix from an Arc heap-block display
/// string. Returns `Some(N)` when present; `None` for non-Arc displays.
fn parse_refcount_suffix(display: &str) -> Option<u32> {
    let start = display.rfind("[refs: ")?;
    let end = display[start..].find(']')?;
    let n_str = &display[start + 7..start + end];
    n_str.trim().parse().ok()
}

/// **Post-M08 polish**: parse the Mutex lock-state suffix from a heap-block
/// display string. Returns `Some(Locked { holder })` for `[locked by #N]`,
/// `Some(Free)` for `[free]`, `None` otherwise.
fn parse_mutex_state_suffix(display: &str) -> Option<MutexLockState> {
    if display.contains("[free]") {
        return Some(MutexLockState::Free);
    }
    let start = display.rfind("[locked by #")?;
    let end = display[start..].find(']')?;
    let n_str = &display[start + 12..start + end];
    let holder: u32 = n_str.trim().parse().ok()?;
    Some(MutexLockState::Locked { holder })
}

fn frame_to_view(frame: FrameInProgress, current_frame_id: Option<u32>) -> FrameCardView {
    let current = current_frame_id == Some(frame.frame_id);
    FrameCardView {
        frame_id: frame.frame_id,
        fn_name: frame.fn_name,
        slots: frame
            .slots
            .into_iter()
            .map(|s| SlotRowView {
                slot_id: s.slot_id,
                name: s.name,
                ty: s.ty,
                value: s.value,
                inline_cells: s.inline_cells,
                struct_view: s.struct_view,
                dyn_view: s.dyn_view,
                moved: s.moved,
            })
            .collect(),
        active: frame.active,
        return_value: frame.return_value,
        current,
    }
}

fn note_to_status(event: &MemEvent) -> Option<StatusView> {
    match event {
        MemEvent::Note { kind, message, .. } => Some(StatusView {
            kind: match kind {
                NoteKind::RuntimeError => "error".to_owned(),
                NoteKind::Info => "info".to_owned(),
            },
            message: message.clone(),
        }),
        // **M08.2**: deadlock surfaces in the status bar as an error.
        MemEvent::Deadlock { thread_ids, .. } => {
            let ids = thread_ids.iter()
                .map(|t| format!("#{}", t.0))
                .collect::<Vec<_>>()
                .join(", ");
            Some(StatusView {
                kind: "error".to_owned(),
                message: format!(
                    "Deadlock: threads {ids} are all waiting on each other's locks. No further progress is possible. The trace ends here — step back to inspect the prior state, or try a different seed to see if the program completes under another schedule."
                ),
            })
        }
        _ => None,
    }
}

/// **M03.1**: extract the transient `pending_return` view from a
/// `MemEvent::ReturnValue`; `None` for any other variant.
fn return_to_pending(event: &MemEvent) -> Option<PendingReturnView> {
    match event {
        MemEvent::ReturnValue { frame_id, value, .. } => Some(PendingReturnView {
            frame_id: frame_id.0,
            value: render_value(value),
        }),
        _ => None,
    }
}

/// **M07.2**: extract the transient `pending_copy` view from a
/// `MemEvent::BytesCopy`; `None` for any other variant. Drives the
/// one-shot orange arrow render in the JS layer.
fn copy_to_pending(event: &MemEvent) -> Option<CopyView> {
    match event {
        MemEvent::BytesCopy { from, from_byte_offset, to, n_bytes, .. } => Some(CopyView {
            from: match from {
                crate::event::Pointee::Slot(id) => ArrowTarget::Slot(id.0),
                crate::event::Pointee::Heap(addr) => ArrowTarget::Heap(addr.0),
                crate::event::Pointee::Static(addr) => ArrowTarget::Static(addr.0),
            },
            from_byte_offset: *from_byte_offset,
            to: to.0,
            n_bytes: *n_bytes,
        }),
        _ => None,
    }
}

/// **M06.1**: look up the binding name of the slot with `slot_id` anywhere
/// in the call stack's live frames. Used by SlotWrite's render path so
/// `Value::Ref { target_slot: 0 }` renders as `&x` (the binding name) rather
/// than `&slot0` (the internal id).
fn lookup_slot_name(frames: &[FrameInProgress], slot_id: u32) -> Option<String> {
    for frame in frames {
        for slot in &frame.slots {
            if slot.slot_id == slot_id {
                return Some(slot.name.clone());
            }
        }
    }
    None
}

/// **M07.4**: byte size of a `Value` for UI sizing — used to size the
/// per-field byte-cell strip in a struct's slot rendering.
fn value_size_bytes_ui(v: &Value) -> u32 {
    use crate::typeck::{IntKind, FloatKind};
    match v {
        Value::Int { kind, .. } => match kind {
            IntKind::I8 | IntKind::U8 => 1,
            IntKind::I16 | IntKind::U16 => 2,
            IntKind::I32 | IntKind::U32 => 4,
            IntKind::I64 | IntKind::U64 | IntKind::ISize | IntKind::USize => 8,
            IntKind::I128 | IntKind::U128 => 16,
        },
        Value::Float { kind, .. } => match kind {
            FloatKind::F32 => 4,
            FloatKind::F64 => 8,
        },
        Value::Bool(_) => 1,
        Value::Unit => 0,
        Value::Ref { .. } | Value::Box { .. } | Value::String { .. } | Value::Vec { .. } => 8,
        Value::Slice { .. } => 16,
        Value::Array { elements, elem_ty } => {
            elements.len() as u32 * ty_size_bytes_ui(elem_ty)
        }
        Value::Struct { fields, .. } => {
            fields.iter().map(|(_, v)| value_size_bytes_ui(v)).sum()
        }
        // M07.7: trait-object fat pointers — 16 bytes (data ptr + vtable ptr).
        Value::DynRef { .. } | Value::BoxDyn { .. } => 16,
        // M08: concurrency primitives — pointer-sized bindings.
        Value::Arc { .. } | Value::Mutex { .. } | Value::MutexGuard { .. } => 8,
        Value::JoinHandle { .. } => 4,
    }
}

/// **M07.3**: byte size of a `Ty` for UI sizing — duplicates the eval-side
/// `ty_size_bytes` to avoid pulling eval into ui's compile graph. Element
/// types are restricted to primitives in M07.3, so the surface is small.
fn ty_size_bytes_ui(ty: &Ty) -> u32 {
    use crate::typeck::{IntKind, FloatKind};
    match ty {
        Ty::Int(k) => match k {
            IntKind::I8 | IntKind::U8 => 1,
            IntKind::I16 | IntKind::U16 => 2,
            IntKind::I32 | IntKind::U32 => 4,
            IntKind::I64 | IntKind::U64 | IntKind::ISize | IntKind::USize => 8,
            IntKind::I128 | IntKind::U128 => 16,
        },
        Ty::Float(k) => match k {
            FloatKind::F32 => 4,
            FloatKind::F64 => 8,
        },
        Ty::Bool => 1,
        Ty::Unit => 0,
        Ty::Ref { .. } | Ty::Box(_) | Ty::String | Ty::Vec(_) => 8,
        Ty::Slice(_) | Ty::Str => 16,
        Ty::Array(inner, size) => ty_size_bytes_ui(inner) * (*size as u32),
        // M07.4: struct = sum of field sizes (no padding).
        Ty::Struct { fields, .. } => fields.iter().map(|(_, t)| ty_size_bytes_ui(t)).sum(),
        // M07.5: type parameter — unreachable at eval/UI time (typeck
        // substitutes before any sizing query). Defensive: 0.
        Ty::Param(_) => 0,
        // M07.7: trait-object types — 16 bytes (fat pointer).
        Ty::DynRef { .. } | Ty::BoxDyn { .. } => 16,
        // M08: concurrency primitives — pointer-sized.
        Ty::Arc(_) | Ty::Mutex(_) | Ty::MutexGuard(_) => 8,
        Ty::JoinHandle => 4,
    }
}

fn render_value(value: &Value) -> String {
    match value {
        // M03.2: type-tag suffix on numeric values (`5_i32`, `2.5_f64`, `NaN_f64`, ...).
        Value::Int { kind, bits } => format!("{bits}_{}", kind.name()),
        Value::Float { kind, value } => {
            let v = *value;
            let body = if v.is_nan() {
                "NaN".to_owned()
            } else if v.is_infinite() {
                if v > 0.0 { "+Inf".to_owned() } else { "-Inf".to_owned() }
            } else {
                // Narrow to f32 for display when the value is an F32.
                match kind {
                    crate::typeck::FloatKind::F32 => (v as f32).to_string(),
                    crate::typeck::FloatKind::F64 => v.to_string(),
                }
            };
            format!("{body}_{}", kind.name())
        }
        Value::Bool(b) => b.to_string(),
        Value::Unit => "()".to_owned(),
        // M06: reference value. Renders as `&slot{N}` here as a fallback only.
        // The SlotWrite path special-cases Value::Ref to use the BINDING name
        // (`&x`, `&mut x`) via `lookup_slot_name`. This fallback is reached
        // only if the value is rendered outside the SlotWrite path (e.g.
        // future ReturnValue of a ref — not constructible in M06.1).
        Value::Ref { target, mutable, .. } => {
            let target_str = match target {
                crate::event::Pointee::Slot(id) => format!("slot{}", id.0),
                crate::event::Pointee::Heap(addr) => format!("heap[{}]", addr.0),
                // M07.2: unreachable in practice (only Value::Slice targets
                // static memory; Value::Ref never does), but exhaustive.
                crate::event::Pointee::Static(addr) => format!("static[{}]", addr.0),
            };
            if *mutable {
                format!("&mut {target_str}")
            } else {
                format!("&{target_str}")
            }
        }
        // M07: heap-owning values in the fallback render path (typically not
        // used since SlotWrite path is specialized for these too).
        Value::Box { addr } => format!("Box→heap[{}]", addr.0),
        Value::Vec { addr } => format!("Vec→heap[{}]", addr.0),
        Value::String { addr } => format!("String→heap[{}]", addr.0),
        // M07.1: slice fallback render. Normal path (SlotWrite) uses empty
        // text because the arrow + length-annotation conveys everything.
        Value::Slice { target, len, .. } => match target {
            crate::event::Pointee::Heap(addr) => format!("&heap[{}; {len}]", addr.0),
            crate::event::Pointee::Slot(id) => format!("&slot{}; {len}]", id.0),
            // M07.2: `&str` literals render with their content if available;
            // fallback path doesn't have the static-region lookup, so just
            // show the addr + len.
            crate::event::Pointee::Static(addr) => format!("&static[{}; {len}]", addr.0),
        },
        // M07.3: array fallback. Normal path (SlotWrite) renders inline
        // byte-cells via SlotRowView.inline_cells, so this text fallback
        // is reached only outside that path (e.g. future ReturnValue of
        // an array — not constructible currently).
        Value::Array { elements, .. } => format!("[_; {}]", elements.len()),
        // M07.4: struct fallback — short `Name { x: 1, y: 2 }` form. The
        // normal SlotWrite path uses struct_view for the full per-field
        // visualization; this fallback fires e.g. when a method returns
        // a struct (ReturnValue annotation).
        Value::Struct { name, fields } => {
            let body: Vec<String> = fields
                .iter()
                .map(|(fname, fval)| format!("{fname}: {}", render_value(fval)))
                .collect();
            format!("{name} {{ {} }}", body.join(", "))
        }
        // M07.7: trait-object fallback render — fat pointer label.
        // Normal SlotWrite path uses dyn_view for the two-cell rendering.
        Value::DynRef { target, mutable, trait_name, vtable, .. } => {
            let target_str = match target {
                crate::event::Pointee::Slot(id) => format!("slot{}", id.0),
                crate::event::Pointee::Heap(addr) => format!("heap[{}]", addr.0),
                crate::event::Pointee::Static(addr) => format!("static[{}]", addr.0),
            };
            let prefix = if *mutable { "&mut dyn" } else { "&dyn" };
            format!("{prefix} {trait_name} {{ data: {target_str}, vtable: #{} }}", vtable.0)
        }
        Value::BoxDyn { addr, trait_name, vtable } => {
            format!("Box<dyn {trait_name}> {{ data: heap[{}], vtable: #{} }}", addr.0, vtable.0)
        }
        // M08: concurrency primitives — fallback renders for notes / debug.
        Value::Arc { addr } => format!("Arc→heap[{}]", addr.0),
        Value::Mutex { addr } => format!("Mutex→heap[{}]", addr.0),
        Value::MutexGuard { addr } => format!("MutexGuard→heap[{}]", addr.0),
        Value::JoinHandle { thread_id } => format!("JoinHandle(#{})", thread_id.0),
    }
}

fn render_ty(ty: &Ty) -> String {
    ty.name().to_owned()
}

/// Extract the `span` field from any `MemEvent` variant.
fn event_span(event: &MemEvent) -> Span {
    match event {
        MemEvent::ThreadSpawn { span, .. }
        | MemEvent::ThreadJoin { span, .. }
        | MemEvent::ThreadPark { span, .. }
        | MemEvent::FrameEnter { span, .. }
        | MemEvent::FrameLeave { span, .. }
        | MemEvent::SlotAlloc { span, .. }
        | MemEvent::SlotWrite { span, .. }
        | MemEvent::SlotMove { span, .. }
        | MemEvent::SlotDrop { span, .. }
        | MemEvent::HeapAlloc { span, .. }
        | MemEvent::HeapRealloc { span, .. }
        | MemEvent::HeapFree { span, .. }
        | MemEvent::StaticAlloc { span, .. }
        | MemEvent::BytesCopy { span, .. }
        | MemEvent::VtableAlloc { span, .. }
        | MemEvent::ThreadSwitch { span, .. }
        | MemEvent::BorrowShared { span, .. }
        | MemEvent::BorrowMut { span, .. }
        | MemEvent::BorrowEnd { span, .. }
        | MemEvent::LockAcquire { span, .. }
        | MemEvent::LockRelease { span, .. }
        | MemEvent::ArcClone { span, .. }
        | MemEvent::ArcDrop { span, .. }
        | MemEvent::Note { span, .. }
        | MemEvent::ReturnValue { span, .. }
        | MemEvent::Deadlock { span, .. } => *span,
    }
}

// ─── wasm-bindgen Player (browser entry point) ─────────────────────────────

#[cfg(target_arch = "wasm32")]
#[allow(unreachable_pub)] // wasm-bindgen exports the inner items via the macro attrs.
mod wasm {
    use super::*;
    use wasm_bindgen::prelude::*;

    /// **M05**: Browser-facing player. Owns a `Cursor` + the current editor
    /// source + the most recent `CompileError` if compilation failed.
    #[wasm_bindgen]
    pub struct Player {
        cursor: Cursor,
        source: String,
        last_error: Option<crate::pipeline::CompileError>,
        /// **M08.2**: current seed used by the most recent `set_source` call.
        /// Surfaced via state snapshot so the JS UI can display it.
        seed: u32,
    }

    /// Serialized form of a successful `set_source` call.
    #[derive(Serialize)]
    struct SetSourceOk<'a> {
        ok: bool, // always true
        state: &'a StateSnapshot,
    }

    /// Serialized form of a failed `set_source` call.
    #[derive(Serialize)]
    struct SetSourceErr<'a> {
        ok: bool, // always false
        error: &'a crate::pipeline::CompileError,
    }

    #[wasm_bindgen]
    impl Player {
        /// **M05**: takes Rust source (not a trace JSON document).
        /// Infallible — on parse/resolve/typeck/eval error, the Player is
        /// created with an empty cursor and a recorded `last_error`.
        ///
        /// Replaces the M04 signature `new(trace_json: &str) -> Result<Player, JsValue>`.
        /// See `specs/007-live-l1-editing/contracts/m05-api.md`.
        #[wasm_bindgen(constructor)]
        pub fn new(source: &str) -> Player {
            let mut player = Player {
                cursor: Cursor::new(Vec::new()),
                source: String::new(),
                last_error: None,
                seed: 0,
            };
            // Discard the returned JSON; constructor exists for the side effect
            // of compiling-and-loading. JS can call `state()` / `error_json()`
            // separately if it needs the initial result.
            let _ = player.set_source(source, 0);
            player
        }

        /// **M08.2**: re-run the pipeline with the current source and the new
        /// seed. Returns JSON of the same shape as `set_source`.
        pub fn set_seed(&mut self, seed: u32) -> String {
            let source = self.source.clone();
            self.set_source(&source, seed)
        }

        /// **M05**: compile + load fresh source. Returns JSON of shape:
        ///   `{ "ok": true,  "state": <StateSnapshot> }`        on success
        ///   `{ "ok": false, "error": <CompileError> }`         on failure
        ///
        /// On success: cursor is replaced with a fresh `Cursor::new(events)`
        /// at position 0; `self.source` is updated; `self.last_error = None`.
        ///
        /// On failure: cursor is replaced with an empty `Cursor::new(vec![])`;
        /// `self.source` is still updated (so `source()` reflects what the
        /// user typed); `self.last_error = Some(err)`.
        pub fn set_source(&mut self, source: &str, seed: u32) -> String {
            self.source = source.to_owned();
            self.seed = seed;
            match crate::pipeline::run_pipeline(source, seed) {
                Ok(events) => {
                    self.cursor = Cursor::new(events);
                    self.last_error = None;
                    let mut snapshot = self.cursor.state_snapshot(&self.source);
                    snapshot.seed = self.seed;
                    serde_json::to_string(&SetSourceOk {
                        ok: true,
                        state: &snapshot,
                    })
                    .expect("SetSourceOk is always Serialize")
                }
                Err(err) => {
                    self.cursor = Cursor::new(Vec::new());
                    let json = serde_json::to_string(&SetSourceErr {
                        ok: false,
                        error: &err,
                    })
                    .expect("SetSourceErr is always Serialize");
                    self.last_error = Some(err);
                    json
                }
            }
        }

        /// Current state snapshot as JSON.
        pub fn state(&self) -> String {
            let mut snapshot = self.cursor.state_snapshot(&self.source);
            snapshot.seed = self.seed;
            serde_json::to_string(&snapshot)
                .expect("StateSnapshot is always Serialize")
        }

        /// The current editor source code.
        pub fn source(&self) -> String {
            self.source.clone()
        }

        /// Advance by one event. Returns the new state JSON.
        /// **Post-M08 polish**: coalesce multi-event atomic groups
        /// (SlotAlloc+SlotWrite, ArcClone+HeapRealloc+Note, etc.) so the
        /// user sees ONE step per logical action. Loops while landing
        /// inside an atom; the raw Cursor.step_forward stays single-event
        /// for tests / programmatic access.
        pub fn step_forward(&mut self) -> String {
            self.cursor.step_forward();
            while self.cursor.is_slot_alloc_write_pair_boundary(self.cursor.position) {
                self.cursor.step_forward();
            }
            self.state()
        }

        /// Step back by one event. Returns the new state JSON.
        /// Symmetric coalesce with `step_forward`.
        pub fn step_back(&mut self) -> String {
            self.cursor.step_back();
            while self.cursor.is_slot_alloc_write_pair_boundary(self.cursor.position) {
                self.cursor.step_back();
            }
            self.state()
        }

        /// Rewind to position 0. Returns the new state JSON.
        pub fn rewind(&mut self) -> String {
            self.cursor.rewind();
            self.state()
        }

        /// Current cursor position.
        pub fn position(&self) -> usize {
            self.cursor.position
        }

        /// Total events in the trace.
        pub fn total(&self) -> usize {
            self.cursor.trace.len()
        }
    }

    /// Module initializer — improves browser-side panic messages.
    #[wasm_bindgen(start)]
    pub fn start_wasm() {
        console_error_panic_hook::set_once();
    }
}

// ─── Unit tests (cargo test --lib) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{FrameId, MemEvent, NoteKind, SlotId, Value};
    use crate::parse::span::{FileId, Span};
    use crate::typeck::Ty;

    fn span() -> Span {
        Span::new(0, 1, FileId(1))
    }

    fn frame_enter(name: &str, frame_id: u32) -> MemEvent {
        // M03.1: `params` field removed.
        MemEvent::FrameEnter {
            frame_id: FrameId(frame_id),
            fn_name: name.into(),
            span: span(),
        }
    }

    fn frame_leave(frame_id: u32, return_value: Value) -> MemEvent {
        MemEvent::FrameLeave {
            frame_id: FrameId(frame_id),
            return_value,
            span: span(),
        }
    }

    fn return_value(frame_id: u32, value: Value) -> MemEvent {
        MemEvent::ReturnValue {
            frame_id: FrameId(frame_id),
            value,
            span: span(),
        }
    }

    fn slot_alloc(slot_id: u32, name: &str, ty: Ty) -> MemEvent {
        MemEvent::SlotAlloc {
            slot_id: SlotId(slot_id),
            name: name.into(),
            ty,
            span: span(),
        }
    }

    fn slot_write(slot_id: u32, value: Value) -> MemEvent {
        MemEvent::SlotWrite {
            slot_id: SlotId(slot_id),
            value,
            span: span(),
        }
    }

    fn slot_drop(slot_id: u32) -> MemEvent {
        MemEvent::SlotDrop {
            slot_id: SlotId(slot_id),
            span: span(),
        }
    }

    fn rt_err(message: &str) -> MemEvent {
        MemEvent::Note {
            kind: NoteKind::RuntimeError,
            message: message.into(),
            span: span(),
        }
    }

    #[test]
    fn cursor_at_zero_is_empty() {
        let c = Cursor::new(Vec::new());
        let s = c.state_snapshot("");
        assert_eq!(s.frames.len(), 0);
        assert_eq!(s.editor_highlight, None);
        assert_eq!(s.status, None);
        assert_eq!(s.position, 0);
        assert_eq!(s.total, 0);
    }

    #[test]
    fn frame_enter_pushes_frame() {
        let mut c = Cursor::new(vec![frame_enter("main", 0)]);
        c.step_forward();
        let s = c.state_snapshot("");
        assert_eq!(s.frames.len(), 1);
        assert_eq!(s.frames[0].fn_name, "main");
        assert_eq!(s.frames[0].frame_id, 0);
        assert!(s.frames[0].slots.is_empty());
        assert!(s.frames[0].active, "freshly-entered frame is active");
        assert!(
            s.frames[0].current,
            "only-frame is the current (executing) one"
        );
    }

    /// M03.1: only the innermost active frame is `current`. Caller is paused
    /// while callee executes; grayed frames are never current. Also covers
    /// the `current_call_span` semantics: only set when ≥ 2 active frames.
    #[test]
    fn current_marks_innermost_active_frame() {
        let trace = vec![
            frame_enter("main", 0),
            frame_enter("add", 1),
            return_value(1, Value::Int { kind: crate::typeck::IntKind::I32, bits: 5 }),
            frame_leave(1, Value::Int { kind: crate::typeck::IntKind::I32, bits: 5 }),
        ];
        let mut c = Cursor::new(trace);
        // After FrameEnter(main): main is the only frame, current.
        // current_call_span is None: main is the program entry, no real caller.
        c.step_forward();
        let s = c.state_snapshot("");
        assert!(s.frames[0].current);
        assert_eq!(s.current_call_span, None);
        // After FrameEnter(add): main paused, add current. Two active frames
        // → current_call_span is Some (the add() call-site span).
        c.step_forward();
        let s = c.state_snapshot("");
        assert_eq!(s.frames.len(), 2);
        assert!(!s.frames[0].current, "caller (main) is paused");
        assert!(s.frames[1].current, "callee (add) is currently executing");
        assert!(s.current_call_span.is_some());
        // After ReturnValue(add): add still active+current+highlighted.
        c.step_forward();
        let s = c.state_snapshot("");
        assert!(s.frames[1].current);
        assert!(s.current_call_span.is_some());
        // After FrameLeave(add): add grayed, main becomes current again.
        // Only 1 active frame → current_call_span clears.
        c.step_forward();
        let s = c.state_snapshot("");
        assert!(s.frames[0].current, "caller resumes as current after callee returns");
        assert!(!s.frames[1].current, "grayed frame is never current");
        assert_eq!(
            s.current_call_span, None,
            "no call site highlight while only the entry frame is active"
        );
    }

    #[test]
    fn slot_alloc_then_write_then_drop() {
        let trace = vec![
            frame_enter("main", 0),
            slot_alloc(0, "x", Ty::Int(crate::typeck::IntKind::I32)),
            slot_write(0, Value::Int { kind: crate::typeck::IntKind::I32, bits: 5 }),
            slot_drop(0),
        ];
        let mut c = Cursor::new(trace);
        // After SlotAlloc: slot present, value None.
        for _ in 0..2 {
            c.step_forward();
        }
        let s = c.state_snapshot("");
        assert_eq!(s.frames[0].slots.len(), 1);
        assert_eq!(s.frames[0].slots[0].value, None);
        // After SlotWrite: value Some("5_i32") — M03.2 type-tag suffix.
        c.step_forward();
        let s = c.state_snapshot("");
        assert_eq!(s.frames[0].slots[0].value, Some("5_i32".into()));
        // After SlotDrop: slot gone.
        c.step_forward();
        let s = c.state_snapshot("");
        assert!(s.frames[0].slots.is_empty());
    }

    /// SC-003 determinism: rewinding to step 0 and stepping forward N times
    /// produces the same visual state as stepping back from a later position.
    #[test]
    fn step_back_undoes_step_forward() {
        let trace = vec![
            frame_enter("main", 0),
            slot_alloc(0, "x", Ty::Int(crate::typeck::IntKind::I32)),
            slot_write(0, Value::Int { kind: crate::typeck::IntKind::I32, bits: 5 }),
            slot_alloc(1, "y", Ty::Int(crate::typeck::IntKind::I32)),
            slot_write(1, Value::Int { kind: crate::typeck::IntKind::I32, bits: 6 }),
        ];
        let mut c = Cursor::new(trace);
        // Step to position 3.
        for _ in 0..3 {
            c.step_forward();
        }
        let s_forward = c.state_snapshot("");
        // Step back to position 2, then forward to 3 — must match s_forward.
        c.step_forward();
        c.step_back();
        let s_round_trip = c.state_snapshot("");
        assert_eq!(s_forward, s_round_trip);
        // Rewind and step forward 3 times — must also match.
        c.rewind();
        for _ in 0..3 {
            c.step_forward();
        }
        let s_rewound = c.state_snapshot("");
        assert_eq!(s_forward, s_rewound);
    }

    #[test]
    fn rewind_zeros_position() {
        let mut c = Cursor::new(vec![frame_enter("main", 0), frame_leave(0, Value::Unit)]);
        c.step_forward();
        c.step_forward();
        c.rewind();
        assert_eq!(c.position, 0);
        let s = c.state_snapshot("");
        assert_eq!(s.frames.len(), 0);
    }

    #[test]
    fn step_past_end_is_noop() {
        let mut c = Cursor::new(vec![frame_enter("main", 0)]);
        c.step_forward();
        assert_eq!(c.position, 1);
        c.step_forward();
        assert_eq!(c.position, 1, "step past end must be no-op");
    }

    #[test]
    fn step_back_from_zero_is_noop() {
        let mut c = Cursor::new(vec![frame_enter("main", 0)]);
        c.step_back();
        assert_eq!(c.position, 0);
    }

    #[test]
    fn runtime_error_note_surfaces_in_status() {
        let mut c = Cursor::new(vec![frame_enter("main", 0), rt_err("division by zero")]);
        c.step_forward();
        c.step_forward();
        let s = c.state_snapshot("");
        let status = s.status.expect("status should be populated by RuntimeError Note");
        assert_eq!(status.kind, "error");
        assert_eq!(status.message, "division by zero");
    }

    /// M03.1: `FrameLeave` now marks the frame inactive instead of popping
    /// it. The frame card persists (grayed) — the stack bytes don't physically
    /// disappear at function return.
    #[test]
    fn frame_leave_grays_frame() {
        let mut c = Cursor::new(vec![frame_enter("main", 0), frame_leave(0, Value::Unit)]);
        c.step_forward();
        let s = c.state_snapshot("");
        assert_eq!(s.frames.len(), 1);
        assert!(s.frames[0].active);
        c.step_forward();
        let s = c.state_snapshot("");
        // Frame still present, just inactive (renderer paints it grayed).
        assert_eq!(s.frames.len(), 1);
        assert!(!s.frames[0].active);
    }

    /// M03.1: a new `FrameEnter` overwrites grayed frames sitting above the
    /// current top-active frame (their stack bytes are being reused).
    #[test]
    fn frame_enter_overwrites_grayed_frames() {
        let trace = vec![
            frame_enter("main", 0),
            frame_enter("add", 1),
            return_value(1, Value::Int { kind: crate::typeck::IntKind::I32, bits: 5 }),
            frame_leave(1, Value::Int { kind: crate::typeck::IntKind::I32, bits: 5 }),
            // Second call to add: should overwrite the grayed first add frame.
            frame_enter("add", 2),
        ];
        let mut c = Cursor::new(trace);
        // Step through everything.
        for _ in 0..5 {
            c.step_forward();
        }
        let s = c.state_snapshot("");
        // Expected: 2 frames — main (active) and the SECOND add (active).
        // The first grayed add was overwritten by the second FrameEnter.
        assert_eq!(s.frames.len(), 2);
        assert_eq!(s.frames[0].fn_name, "main");
        assert!(s.frames[0].active);
        assert_eq!(s.frames[1].frame_id, 2, "first add was overwritten by second add");
        assert!(s.frames[1].active);
    }

    /// M03.1 / US2: `MemEvent::ReturnValue` records the value on the frame
    /// AND on the transient `pending_return`. The frame-level value persists
    /// across the subsequent `FrameLeave` so the grayed frame still shows
    /// `→ <value>`.
    #[test]
    fn return_value_persists_on_grayed_frame() {
        let trace = vec![
            frame_enter("main", 0),
            return_value(0, Value::Int { kind: crate::typeck::IntKind::I32, bits: 5 }),
            frame_leave(0, Value::Int { kind: crate::typeck::IntKind::I32, bits: 5 }),
        ];
        let mut c = Cursor::new(trace);
        // Step to ReturnValue: frame still active, return_value set.
        c.step_forward();
        c.step_forward();
        let s = c.state_snapshot("");
        assert_eq!(s.frames.len(), 1);
        assert!(s.frames[0].active);
        assert_eq!(s.frames[0].return_value.as_deref(), Some("5_i32"));
        // pending_return still Some on the ReturnValue tick (transient highlight).
        let pending = s.pending_return.expect("pending_return on ReturnValue tick");
        assert_eq!(pending.frame_id, 0);
        assert_eq!(pending.value, "5_i32");
        // Step past to FrameLeave: pending_return clears, but frame.return_value persists.
        c.step_forward();
        let s = c.state_snapshot("");
        assert_eq!(s.pending_return, None);
        assert_eq!(s.frames.len(), 1);
        assert!(!s.frames[0].active);
        assert_eq!(
            s.frames[0].return_value.as_deref(),
            Some("5_i32"),
            "return value persists on the grayed frame"
        );
    }
}
