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
    /// Cursor position (mirrors `Cursor::position`).
    pub position: usize,
    /// Total events in the trace.
    pub total: usize,
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
}

/// **M07**: arrow target — slot (for borrows-of-locals) or heap (for borrows-of-heap and ownership).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ArrowTarget {
    /// Stack slot (M06's case, kept).
    Slot(u32),
    /// Heap allocation (M07's case).
    Heap(u32),
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
        let status = last.and_then(note_to_status);
        let pending_return = last.and_then(return_to_pending);
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
                    },
                    kind: if b.mutable { ArrowKind::Mut } else { ArrowKind::Shared },
                })
            })
            .collect::<Vec<ArrowView>>();
        // M07: owning arrows (black) from world.owning.
        let mut arrows = arrows_from_borrows;
        for o in &world.owning {
            arrows.push(ArrowView {
                source_slot: o.source_slot,
                target: ArrowTarget::Heap(o.target_heap),
                kind: ArrowKind::Owning,
            });
        }
        let heap = world.heap.iter().map(|h| HeapView {
            addr: h.addr,
            ty_name: h.ty_name.clone(),
            display: h.display.clone(),
            size: h.size,
            used: h.used,
            freed: h.freed,
        }).collect::<Vec<HeapView>>();
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
            position: self.position,
            total: self.trace.len(),
        }
    }
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
}

struct ActiveBorrowState {
    borrow_id: u32,
    /// `None` until a `SlotWrite` of `Value::Ref` binds the reference to a slot.
    source_slot: Option<u32>,
    /// M07: a borrow can target a slot OR a heap allocation.
    target: BorrowTarget,
    mutable: bool,
}

#[derive(Copy, Clone)]
enum BorrowTarget {
    Slot(u32),
    Heap(u32),
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
}

struct OwningState {
    source_slot: u32,
    target_heap: u32,
}

struct FrameInProgress {
    frame_id: u32,
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

struct LiveSlot {
    slot_id: u32,
    name: String,
    ty: String,
    value: Option<String>,
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
            while world.frames.last().is_some_and(|f| !f.active) {
                world.frames.pop();
            }
            world.frames.push(FrameInProgress {
                frame_id: frame_id.0,
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
            if let Some(frame) = world.frames.iter_mut().rev().find(|f| f.active) {
                frame.active = false;
            }
        }
        MemEvent::SlotAlloc { slot_id, name, ty, .. } => {
            // M03.1: route the alloc to the innermost ACTIVE frame; inactive
            // (grayed) frames shouldn't receive new slots.
            if let Some(frame) = world.frames.iter_mut().rev().find(|f| f.active) {
                frame.slots.push(LiveSlot {
                    slot_id: slot_id.0,
                    name: name.clone(),
                    ty: render_ty(ty),
                    value: None,
                });
            }
        }
        MemEvent::SlotWrite { slot_id, value, .. } => {
            // M06: if this write lands a Value::Ref, bind the borrow's source_slot.
            if let Value::Ref { borrow_id, .. } = value {
                if let Some(borrow) = world
                    .borrows
                    .iter_mut()
                    .find(|b| b.borrow_id == borrow_id.0)
                {
                    borrow.source_slot = Some(slot_id.0);
                }
            }
            // M06.1: render Value::Ref using the *binding name* of the target
            // slot (`&x`, `&mut x`) instead of `&slot0` / `&mut slot0`. The
            // SlotId is implementation jargon; learners think in binding names.
            // Fall back to `slot{N}` only if the target slot isn't found.
            let rendered = match value {
                // M06.1 → M07: target widened from SlotId to Pointee.
                Value::Ref { target: crate::event::Pointee::Slot(target_slot), mutable, .. } => {
                    let name = lookup_slot_name(&world.frames, target_slot.0)
                        .unwrap_or_else(|| format!("slot{}", target_slot.0));
                    if *mutable {
                        format!("&mut {name}")
                    } else {
                        format!("&{name}")
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
            let rendered = if let Value::Box { addr } | Value::Vec { addr } | Value::String { addr } = value {
                world.owning.retain(|o| o.source_slot != slot_id.0);
                world.owning.push(OwningState {
                    source_slot: slot_id.0,
                    target_heap: addr.0,
                });
                String::new() // empty value cell — arrow says it all
            } else {
                rendered
            };
            for frame in &mut world.frames {
                if let Some(slot) = frame.slots.iter_mut().find(|s| s.slot_id == slot_id.0) {
                    slot.value = Some(rendered);
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
            };
            world.borrows.push(ActiveBorrowState {
                borrow_id: borrow_id.0,
                source_slot: None,
                target: t,
                mutable: false,
            });
        }
        MemEvent::BorrowMut { borrow_id, target, .. } => {
            let t = match target {
                crate::event::Pointee::Slot(id) => BorrowTarget::Slot(id.0),
                crate::event::Pointee::Heap(addr) => BorrowTarget::Heap(addr.0),
            };
            world.borrows.push(ActiveBorrowState {
                borrow_id: borrow_id.0,
                source_slot: None,
                target: t,
                mutable: true,
            });
        }
        MemEvent::BorrowEnd { borrow_id, .. } => {
            world.borrows.retain(|b| b.borrow_id != borrow_id.0);
        }
        // **M07**: heap events.
        MemEvent::HeapAlloc { addr, size, used, ty_name, fragment_of, .. } => {
            let new_state = HeapAllocState {
                addr: addr.0,
                ty_name: ty_name.clone(),
                display: ty_name.clone(),
                size: *size,
                used: *used,
                freed: fragment_of.is_some(),
            };
            if let Some(parent) = fragment_of {
                // Fragment: insert immediately after the parent live block
                // (the just-allocated one this fragment was split from).
                let parent_idx = world.heap.iter()
                    .position(|h| h.addr == parent.0 && !h.freed);
                match parent_idx {
                    Some(i) => world.heap.insert(i + 1, new_state),
                    None => world.heap.push(new_state),
                }
            } else {
                // Real allocation: if reusing a freed addr, REPLACE the
                // freed entry IN PLACE (keeps the visual position stable
                // instead of moving the block to the end of the panel).
                let idx = world.heap.iter()
                    .position(|h| h.addr == addr.0 && h.freed);
                match idx {
                    Some(i) => world.heap[i] = new_state,
                    None => world.heap.push(new_state),
                }
            }
        }
        MemEvent::HeapRealloc { from, to, new_size, new_used, new_display, .. } => {
            if from == to {
                if let Some(h) = world.heap.iter_mut().find(|h| h.addr == from.0 && !h.freed) {
                    h.size = *new_size;
                    h.used = *new_used;
                    h.display = new_display.clone();
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
        // The remaining variants don't modify world state. `Note` surfaces via
        // `note_to_status` on the most-recent-event side path.
        MemEvent::SlotMove { .. }
        | MemEvent::LockAcquire { .. }
        | MemEvent::LockRelease { .. }
        | MemEvent::ArcClone { .. }
        | MemEvent::ArcDrop { .. }
        | MemEvent::ThreadSpawn { .. }
        | MemEvent::ThreadJoin { .. }
        | MemEvent::ThreadPark { .. }
        | MemEvent::Note { .. } => {
            // No world-state change.
        }
    }
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
        Value::Str(s) => format!("\"{s}\""),
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
        | MemEvent::BorrowShared { span, .. }
        | MemEvent::BorrowMut { span, .. }
        | MemEvent::BorrowEnd { span, .. }
        | MemEvent::LockAcquire { span, .. }
        | MemEvent::LockRelease { span, .. }
        | MemEvent::ArcClone { span, .. }
        | MemEvent::ArcDrop { span, .. }
        | MemEvent::Note { span, .. }
        | MemEvent::ReturnValue { span, .. } => *span,
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
            };
            // Discard the returned JSON; constructor exists for the side effect
            // of compiling-and-loading. JS can call `state()` / `error_json()`
            // separately if it needs the initial result.
            let _ = player.set_source(source);
            player
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
        pub fn set_source(&mut self, source: &str) -> String {
            self.source = source.to_owned();
            match crate::pipeline::run_pipeline(source) {
                Ok(events) => {
                    self.cursor = Cursor::new(events);
                    self.last_error = None;
                    let snapshot = self.cursor.state_snapshot(&self.source);
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
            serde_json::to_string(&self.cursor.state_snapshot(&self.source))
                .expect("StateSnapshot is always Serialize")
        }

        /// The current editor source code.
        pub fn source(&self) -> String {
            self.source.clone()
        }

        /// Advance by one event. Returns the new state JSON.
        pub fn step_forward(&mut self) -> String {
            self.cursor.step_forward();
            self.state()
        }

        /// Step back by one event. Returns the new state JSON.
        pub fn step_back(&mut self) -> String {
            self.cursor.step_back();
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
