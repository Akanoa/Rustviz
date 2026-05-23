//! Event model — the central [`MemEvent`] enum and supporting payload types.
//!
//! `MemEvent` enumerates **all** event categories from CLAUDE.md › Event model:
//! Threads, Frames, Stack slots, Heap, Borrows, Synchronization, Pedagogy.
//! M03's L1 evaluator emits the Frames + Stack-slots + Note subset; the other
//! variants are defined here so M06–M08 can fill in their payloads additively.

use crate::parse::span::Span;
use crate::typeck::Ty;

/// Unique, stable identifier for a runtime stack slot.
///
/// Allocated sequentially during evaluation. Distinct from [`crate::resolve::BindingId`]:
/// `BindingId` is static (one per declaration site), `SlotId` is dynamic (one per
/// runtime instance — recursive calls produce fresh `SlotId`s for the same `BindingId`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct SlotId(pub u32);

/// Unique, stable identifier for a stack frame instance (one per function call).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct FrameId(pub u32);

/// Forward-compatibility placeholder for heap addresses. Used only by M07+ heap events.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct HeapAddr(pub u32);

/// Forward-compatibility placeholder for borrow identifiers. Used only by M06+ borrow events.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct BorrowId(pub u32);

/// Where a pointer points — into the stack, onto the heap, or into static memory.
///
/// Per CLAUDE.md › Event model: "Pointee is an enum `Slot(SlotId) | Heap(HeapAddr)`".
/// **M07.2** extends with `Static(StaticAddr)` for `&'static str` and similar
/// borrows into the binary's read-only data segment.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Pointee {
    /// Points at a stack slot.
    Slot(SlotId),
    /// Points at a heap allocation (M07+).
    Heap(HeapAddr),
    /// **M07.2**: points into the static-memory region (read-only data
    /// segment). Used by string-literal slices (`&'static str`). Static
    /// blocks never go dangling — they persist for the trace's lifetime.
    Static(StaticAddr),
}

/// **M07.2**: identifier for a block in the static-memory region. Distinct
/// from `HeapAddr` because static blocks have different lifetime semantics
/// (never freed). Monotonic; never reused.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct StaticAddr(pub u32);

/// A runtime value held in a stack slot.
///
/// **M03.2**: integer and float variants unified to `{ kind, bits|value }` form
/// so all 12 integer widths + 2 float widths dispatch through one match per op.
/// `Bool` and `Unit` stay as standalone variants.
///
/// `PartialEq` only (no `Eq`) because floats don't impl `Eq` (NaN != NaN).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Value {
    /// Integer value. `bits` stores the value widened to `i128`; the actual
    /// representable range is determined by `kind` (see `IntKind::contains`).
    Int {
        /// Width / signedness discriminator.
        kind: crate::typeck::IntKind,
        /// Value, widened to `i128` for unified storage.
        bits: i128,
    },
    /// Float value. Always stored as `f64`; narrowed to `f32` on display and
    /// after arithmetic when `kind == F32` (so f32-specific overflow → Inf
    /// surfaces correctly).
    Float {
        /// Width discriminator (F32 or F64).
        kind: crate::typeck::FloatKind,
        /// Value, always stored as f64.
        value: f64,
    },
    /// Boolean.
    Bool(bool),
    /// Unit `()`.
    Unit,
    /// **M06 (restructured in M07)**: a borrow value held in a stack slot.
    /// Created by an `Expr::Borrow` evaluation; identified by `borrow_id`
    /// matching a `BorrowShared` or `BorrowMut` event. `target` was
    /// `target_slot: SlotId` in M06; M07 widens to `Pointee` so heap
    /// borrows (`&v[0]` into a Vec's allocation) are representable.
    Ref {
        /// Identifier of the active borrow.
        borrow_id: BorrowId,
        /// What's being borrowed — a stack slot OR a heap allocation.
        target: Pointee,
        /// `true` for `&mut`, `false` for `&`.
        mutable: bool,
    },
    /// **M07**: owns a Box-allocated value. The actual value lives in the
    /// evaluator's heap state at `addr`.
    Box {
        /// Heap address of the Box's allocation.
        addr: HeapAddr,
    },
    /// **M07**: owns a Vec allocation.
    Vec {
        /// Heap address of the Vec's underlying buffer.
        addr: HeapAddr,
    },
    /// **M07**: owns a String allocation.
    String {
        /// Heap address of the String's underlying buffer.
        addr: HeapAddr,
    },
    /// **M07.1**: slice value — a fat pointer (target + length) into a heap
    /// allocation. Sibling of `Value::Ref` (not an extension); slices carry
    /// extra `len` metadata and live in the same active-borrow registry, so
    /// the dangling-detection scan catches them on later realloc.
    Slice {
        /// Identifier of the active borrow.
        borrow_id: BorrowId,
        /// What's being sliced. In M07.1 always `Pointee::Heap(addr)` of the
        /// underlying Vec's allocation; `Pointee::Slot(_)` is unreachable
        /// (no array-on-stack in M07.1).
        target: Pointee,
        /// **M07.1**: element index within the target Vec where this slice
        /// starts (the range's `start` bound; 0 for the `..` / `..end` forms).
        /// Drives the element-span highlight on slice-arrow hover.
        start: u64,
        /// Number of elements visible through this slice (end - start).
        len: u64,
        /// `true` for `&mut [T]`, `false` for `&[T]`. Always `false` in M07.1
        /// (typeck rejects mutable-slice construction).
        mutable: bool,
        /// **M07.1**: byte offset within the target block where the slice's
        /// data pointer starts. Computed at construction from
        /// `start * elem_size`. Used by the UI to highlight the covered
        /// byte-cells on slice-arrow hover, making "this slice views these
        /// specific bytes" tangible.
        byte_offset: u64,
        /// **M07.1**: length in bytes of the slice's view
        /// (`len * elem_size`). Together with `byte_offset` fully specifies
        /// the byte range highlighted on hover.
        byte_len: u64,
    },
}

impl Value {
    /// User-facing type name of this value (`"u8"`, `"f64"`, `"bool"`, `"()"`, `"&"`, `"&mut"`).
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Int { kind, .. } => kind.name(),
            Self::Float { kind, .. } => kind.name(),
            Self::Bool(_) => "bool",
            Self::Unit => "()",
            Self::Ref { mutable: false, .. } => "&",
            Self::Ref { mutable: true, .. } => "&mut",
            // **M07**: heap-owning types. Inner type info isn't carried at
            // the Value layer (it's in the heap state map); these short
            // names suffice for status messages.
            Self::Box { .. } => "Box",
            Self::Vec { .. } => "Vec",
            Self::String { .. } => "String",
            // M07.1: slice. Short tag — full `&[T]` rendering comes from the Ty layer.
            // M07.2: includes `&str` literals (Value::Slice with Pointee::Static target).
            Self::Slice { .. } => "&[]",
        }
    }
}

/// Classification of a [`MemEvent::Note`] event.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum NoteKind {
    /// Runtime error — integer overflow, division by zero, recursion depth exceeded.
    /// When emitted, the event stream ends after this note.
    RuntimeError,
    /// Informational note (pedagogical message, hint, etc.).
    Info,
}

/// Memory and control-flow events emitted by the evaluator.
///
/// **Closed enum** from M03 onward — adding new variants is a breaking change.
/// Later milestones (M06 borrows, M07 heap, M08 threads) fill in payloads on
/// existing variants rather than adding new ones. See `contracts/m03-api.md`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MemEvent {
    // ─── Threads (M08) ──────────────────────────────────────────────────────
    /// A new thread was spawned.
    ThreadSpawn {
        /// Identifier of the new thread.
        thread_id: u32,
        /// Source location of the `thread::spawn` call.
        span: Span,
    },
    /// A thread was joined (its termination was awaited).
    ThreadJoin {
        /// Identifier of the joined thread.
        thread_id: u32,
        /// Source location of the `.join()` call.
        span: Span,
    },
    /// A thread parked itself, typically waiting on a lock.
    ThreadPark {
        /// Identifier of the parked thread.
        thread_id: u32,
        /// The lock (heap address) the thread is parked on.
        lock: HeapAddr,
        /// Source location of the parking point.
        span: Span,
    },

    // ─── Frames (M03 + M03.1) ───────────────────────────────────────────────
    /// A function call began. A new stack frame opens.
    ///
    /// **M03.1**: the `params` field (originally `Vec<(SlotId, String, Value)>`)
    /// was removed in the M03.1 revision. The same information is now solely
    /// conveyed by the per-param `SlotAlloc` + `SlotWrite` events that fire
    /// immediately after this `FrameEnter`. See `specs/006-m03-1-protocol-revision/`.
    FrameEnter {
        /// Identifier of the new frame.
        frame_id: FrameId,
        /// Function name being called.
        fn_name: String,
        /// Source location of the function declaration.
        span: Span,
    },
    /// A function call returned. The frame closes.
    FrameLeave {
        /// Identifier of the closing frame (matches the prior [`FrameEnter`]).
        frame_id: FrameId,
        /// Value returned by the function (or `Value::Unit` for implicit unit return).
        return_value: Value,
        /// Source location at end of the function body.
        span: Span,
    },
    /// **M03.1**: A function's body has finished evaluating and its return
    /// value is visible. Always emitted immediately before the matching
    /// `FrameLeave` for non-halted frames; never emitted for frames that
    /// halt on a `Note { kind: RuntimeError }`.
    ///
    /// Pedagogically: the value is "in transit" between callee body and
    /// caller frame — it lives in a return register / caller-provided slot
    /// at the ABI level. This event makes that step visible for one tick.
    ReturnValue {
        /// Identifier of the frame returning (matches a prior `FrameEnter.frame_id`).
        frame_id: FrameId,
        /// The value being returned. Mirrors the subsequent `FrameLeave.return_value`.
        value: Value,
        /// Source location — body tail expression's span, or body block's span if no tail.
        span: Span,
    },

    // ─── Stack slots (M03) ──────────────────────────────────────────────────
    /// A new stack slot was allocated (let-binding or function parameter).
    SlotAlloc {
        /// Identifier of the new slot.
        slot_id: SlotId,
        /// Source name of the binding.
        name: String,
        /// Declared (or inferred) value type.
        ty: Ty,
        /// Source location of the declaration.
        span: Span,
    },
    /// A value was written into a stack slot.
    SlotWrite {
        /// Slot being written.
        slot_id: SlotId,
        /// Value written.
        value: Value,
        /// Source location of the write (typically the initializer expression).
        span: Span,
    },
    /// A value was moved from one slot to another (non-Copy types).
    ///
    /// Never emitted by the M03 L1 evaluator (L1 has only Copy types — `i32`, `bool`).
    /// M07+ will emit this for `Box` / `Vec` / `String` moves.
    SlotMove {
        /// Source slot.
        from: SlotId,
        /// Destination slot.
        to: SlotId,
        /// The moved value.
        value: Value,
        /// Source location of the move expression.
        span: Span,
    },
    /// A stack slot was dropped (its scope ended).
    SlotDrop {
        /// Slot being dropped.
        slot_id: SlotId,
        /// Source location — defaults to the declaration site (research R-014).
        span: Span,
    },

    // ─── Heap (M07) ─────────────────────────────────────────────────────────
    /// A heap allocation occurred.
    HeapAlloc {
        /// Identifier of the new allocation.
        addr: HeapAddr,
        /// Total capacity in bytes.
        size: u32,
        /// **M07**: used bytes (≤ size). Box always = size; Vec = len*elem_size; String = len.
        #[serde(default)]
        used: u32,
        /// Human-readable type name (e.g. `"i32"`, `"Vec<i32>"`).
        ty_name: String,
        /// **M07**: `Some(parent_addr)` if this "allocation" is actually
        /// a leftover fragment after the allocator split the freed block
        /// at `parent_addr`. Kept for backwards-compat with earlier
        /// traces; M07.2+ uses `split_remainder` on the same event
        /// instead so the alloc + fragment appear at the same cursor
        /// step (avoids a transient misleading "the freed bytes
        /// disappeared" frame between the two events).
        #[serde(default)]
        fragment_of: Option<HeapAddr>,
        /// **M07.2**: when this allocation reuses a freed chunk that was
        /// larger than `size`, the leftover bytes are reported here so
        /// the UI inserts them as a sibling freed block at the SAME
        /// cursor step as the main allocation. Without this, the
        /// allocator-split was emitted as two consecutive events; the
        /// in-between cursor state showed the reuse without the
        /// remainder, which read as "the freed bytes vanished".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        split_remainder: Option<(HeapAddr, u32)>,
        /// Source location of the allocating expression.
        span: Span,
    },
    /// A heap allocation was reallocated (typically `Vec` growth).
    HeapRealloc {
        /// Previous heap address (invalidated).
        from: HeapAddr,
        /// New heap address.
        to: HeapAddr,
        /// New total capacity in bytes.
        new_size: u32,
        /// **M07**: used bytes after the realloc.
        #[serde(default)]
        new_used: u32,
        /// **M07 polish**: human-readable display of the new contents.
        new_display: String,
        /// Source location of the operation triggering the realloc.
        span: Span,
    },
    /// A heap allocation was freed.
    HeapFree {
        /// Allocation being freed.
        addr: HeapAddr,
        /// Source location where the owning value goes out of scope.
        span: Span,
    },
    /// **M07.2**: a static-memory block was allocated. Fires ONCE per unique
    /// string-literal content (content-deduplicated to match Rust linker
    /// behavior — duplicate literals in `.rodata` share one block). Static
    /// blocks never fire a corresponding free event; they persist for the
    /// trace's lifetime.
    StaticAlloc {
        /// Identifier of the static block.
        addr: StaticAddr,
        /// The block's byte content (already-processed string after escape
        /// resolution in the lexer).
        bytes: String,
        /// Source location of the literal that first interned this content.
        span: Span,
    },
    /// **M07.2**: N bytes were copied from a source memory region into a
    /// heap allocation. Emitted by `String::from(s)` (copies from `s`'s
    /// region into a fresh heap String buffer) and `push_str(s)` (copies
    /// from `s`'s region into the receiver's existing heap buffer). Makes
    /// the data-flow visible — without this, the copy looked magical
    /// (bytes appeared in the heap with no link to the source).
    BytesCopy {
        /// Source region (typically `Pointee::Static(_)` for `&str` args).
        from: Pointee,
        /// Byte offset within the source block where the copied range
        /// starts. Pairs with `n_bytes` to identify the exact sub-range —
        /// e.g. `String::from(&"hello"[1..4])` copies from offset 1.
        from_byte_offset: u32,
        /// Destination heap allocation.
        to: HeapAddr,
        /// Number of bytes copied.
        n_bytes: u32,
        /// Source location of the call site.
        span: Span,
    },

    // ─── Borrows (M06) ──────────────────────────────────────────────────────
    /// A shared (`&`) borrow began.
    BorrowShared {
        /// Identifier of the borrow.
        borrow_id: BorrowId,
        /// What the borrow points at.
        target: Pointee,
        /// Source location of the `&` expression.
        span: Span,
    },
    /// A mutable (`&mut`) borrow began.
    BorrowMut {
        /// Identifier of the borrow.
        borrow_id: BorrowId,
        /// What the borrow points at.
        target: Pointee,
        /// Source location of the `&mut` expression.
        span: Span,
    },
    /// A borrow's lifetime ended.
    BorrowEnd {
        /// Borrow that ended.
        borrow_id: BorrowId,
        /// Source location at the end of the borrow's scope.
        span: Span,
    },

    // ─── Synchronization (M08) ──────────────────────────────────────────────
    /// A mutex was locked.
    LockAcquire {
        /// The mutex (heap address).
        addr: HeapAddr,
        /// Source location of the `.lock()` call.
        span: Span,
    },
    /// A mutex was unlocked.
    LockRelease {
        /// The mutex (heap address).
        addr: HeapAddr,
        /// Source location at the end of the guard's scope.
        span: Span,
    },
    /// An `Arc` was cloned, bumping the reference count.
    ArcClone {
        /// The `Arc`'s heap address.
        addr: HeapAddr,
        /// Source location of the `.clone()` call.
        span: Span,
    },
    /// An `Arc` was dropped, decrementing the reference count.
    ArcDrop {
        /// The `Arc`'s heap address.
        addr: HeapAddr,
        /// Source location where the `Arc` goes out of scope.
        span: Span,
    },

    // ─── Pedagogy (M03 infrastructure; all milestones may emit) ─────────────
    /// A pedagogical note attached to an event-stream point.
    ///
    /// `NoteKind::RuntimeError` notes always terminate the stream.
    Note {
        /// Classification of the note.
        kind: NoteKind,
        /// Human-readable message.
        message: String,
        /// Source location the note attaches to.
        span: Span,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::span::{FileId, Span};

    fn dummy_span() -> Span {
        Span::new(0, 1, FileId(1))
    }

    // FR-006 / research R-008: L1 doesn't exercise SlotMove from real programs
    // (Copy-only types), so we verify the variant constructs cleanly here.
    #[test]
    fn constructs_slot_move() {
        let e = MemEvent::SlotMove {
            from: SlotId(0),
            to: SlotId(1),
            value: Value::Int { kind: crate::typeck::IntKind::I32, bits: 5 },
            span: dummy_span(),
        };
        let dbg = format!("{e:?}");
        assert!(!dbg.is_empty());
        assert!(dbg.contains("SlotMove"));
    }

    // T012 / US3: smoke tests for the other forward-compat variants that L1
    // doesn't exercise. Catches variant removal during future refactors.

    #[test]
    fn constructs_thread_spawn() {
        let e = MemEvent::ThreadSpawn { thread_id: 7, span: dummy_span() };
        assert!(format!("{e:?}").contains("ThreadSpawn"));
    }

    #[test]
    fn constructs_heap_alloc() {
        let e = MemEvent::HeapAlloc {
            addr: HeapAddr(0),
            size: 8,
            used: 8,
            ty_name: "i32".into(),
            fragment_of: None,
            split_remainder: None,
            span: dummy_span(),
        };
        assert!(format!("{e:?}").contains("HeapAlloc"));
    }

    #[test]
    fn constructs_borrow_shared() {
        let e = MemEvent::BorrowShared {
            borrow_id: BorrowId(0),
            target: Pointee::Slot(SlotId(0)),
            span: dummy_span(),
        };
        assert!(format!("{e:?}").contains("BorrowShared"));
    }

    #[test]
    fn constructs_lock_acquire() {
        let e = MemEvent::LockAcquire { addr: HeapAddr(0), span: dummy_span() };
        assert!(format!("{e:?}").contains("LockAcquire"));
    }

    #[test]
    fn constructs_note_info() {
        let e = MemEvent::Note {
            kind: NoteKind::Info,
            message: "hello".into(),
            span: dummy_span(),
        };
        assert!(format!("{e:?}").contains("Info"));
    }
}
