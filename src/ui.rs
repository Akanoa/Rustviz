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
    /// Status message (runtime error or info note).
    pub status: Option<StatusView>,
    /// Cursor position (mirrors `Cursor::position`).
    pub position: usize,
    /// Total events in the trace.
    pub total: usize,
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
        let editor_highlight = if self.position == 0 {
            None
        } else {
            Some(event_span(&self.trace[self.position - 1]))
        };
        let status = if self.position == 0 {
            None
        } else {
            note_to_status(&self.trace[self.position - 1])
        };
        StateSnapshot {
            frames: world.frames.into_iter().map(frame_to_view).collect(),
            editor_highlight,
            status,
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
}

struct FrameInProgress {
    frame_id: u32,
    fn_name: String,
    slots: Vec<LiveSlot>,
}

struct LiveSlot {
    slot_id: u32,
    name: String,
    ty: String,
    value: Option<String>,
}

fn apply_event(world: &mut World, event: &MemEvent) {
    match event {
        MemEvent::FrameEnter { frame_id, fn_name, .. } => {
            world.frames.push(FrameInProgress {
                frame_id: frame_id.0,
                fn_name: fn_name.clone(),
                slots: Vec::new(),
            });
        }
        MemEvent::FrameLeave { .. } => {
            world.frames.pop();
        }
        MemEvent::SlotAlloc { slot_id, name, ty, .. } => {
            if let Some(frame) = world.frames.last_mut() {
                frame.slots.push(LiveSlot {
                    slot_id: slot_id.0,
                    name: name.clone(),
                    ty: render_ty(*ty),
                    value: None,
                });
            }
        }
        MemEvent::SlotWrite { slot_id, value, .. } => {
            for frame in &mut world.frames {
                if let Some(slot) = frame.slots.iter_mut().find(|s| s.slot_id == slot_id.0) {
                    slot.value = Some(render_value(value));
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
        // M04 doesn't visualize the remaining variants. They're inert here so
        // future-extended traces (M06+ events) don't crash an L1-only player.
        MemEvent::SlotMove { .. }
        | MemEvent::HeapAlloc { .. }
        | MemEvent::HeapRealloc { .. }
        | MemEvent::HeapFree { .. }
        | MemEvent::BorrowShared { .. }
        | MemEvent::BorrowMut { .. }
        | MemEvent::BorrowEnd { .. }
        | MemEvent::LockAcquire { .. }
        | MemEvent::LockRelease { .. }
        | MemEvent::ArcClone { .. }
        | MemEvent::ArcDrop { .. }
        | MemEvent::ThreadSpawn { .. }
        | MemEvent::ThreadJoin { .. }
        | MemEvent::ThreadPark { .. }
        | MemEvent::Note { .. } => {
            // Note doesn't modify world state; its message surfaces via
            // `note_to_status` on the last-applied-event side path.
        }
    }
}

fn frame_to_view(frame: FrameInProgress) -> FrameCardView {
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

fn render_value(value: &Value) -> String {
    match value {
        Value::Int(i) => i.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Unit => "()".to_owned(),
    }
}

fn render_ty(ty: Ty) -> String {
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
        | MemEvent::Note { span, .. } => *span,
    }
}

// ─── wasm-bindgen Player (browser entry point) ─────────────────────────────

#[cfg(target_arch = "wasm32")]
#[allow(unreachable_pub)] // wasm-bindgen exports the inner items via the macro attrs.
mod wasm {
    use super::*;
    use wasm_bindgen::prelude::*;

    /// Trace file shape — `gen_traces` writes this, `Player::new` reads it.
    #[derive(Deserialize)]
    struct TraceFile {
        source: String,
        events: Vec<MemEvent>,
    }

    /// Browser-facing player. Wraps a `Cursor` + the sample's source code.
    #[wasm_bindgen]
    pub struct Player {
        cursor: Cursor,
        source: String,
    }

    #[wasm_bindgen]
    impl Player {
        /// Parse a trace JSON document and create a player at position 0.
        #[wasm_bindgen(constructor)]
        pub fn new(trace_json: &str) -> Result<Player, JsValue> {
            let file: TraceFile = serde_json::from_str(trace_json)
                .map_err(|e| JsValue::from_str(&format!("trace parse error: {e}")))?;
            Ok(Player {
                cursor: Cursor::new(file.events),
                source: file.source,
            })
        }

        /// Current state snapshot as JSON.
        pub fn state(&self) -> String {
            serde_json::to_string(&self.cursor.state_snapshot(&self.source))
                .expect("StateSnapshot is always Serialize")
        }

        /// The sample's Rust source code.
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
        MemEvent::FrameEnter {
            frame_id: FrameId(frame_id),
            fn_name: name.into(),
            params: Vec::new(),
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
    }

    #[test]
    fn slot_alloc_then_write_then_drop() {
        let trace = vec![
            frame_enter("main", 0),
            slot_alloc(0, "x", Ty::I32),
            slot_write(0, Value::Int(5)),
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
        // After SlotWrite: value Some("5").
        c.step_forward();
        let s = c.state_snapshot("");
        assert_eq!(s.frames[0].slots[0].value, Some("5".into()));
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
            slot_alloc(0, "x", Ty::I32),
            slot_write(0, Value::Int(5)),
            slot_alloc(1, "y", Ty::I32),
            slot_write(1, Value::Int(6)),
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

    #[test]
    fn frame_leave_pops_frame() {
        let mut c = Cursor::new(vec![frame_enter("main", 0), frame_leave(0, Value::Unit)]);
        c.step_forward();
        assert_eq!(c.state_snapshot("").frames.len(), 1);
        c.step_forward();
        assert_eq!(c.state_snapshot("").frames.len(), 0);
    }
}
