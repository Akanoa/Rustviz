//! Level 1 evaluator — walks the resolved + typed AST and emits a `Vec<MemEvent>`.

use std::collections::HashMap;

use crate::event::{FrameId, MemEvent, NoteKind, SlotId, Value};
use crate::parse::ast;
use crate::parse::error::ParseError;
use crate::parse::span::Span;
use crate::resolve::{BindingId, BindingKind, Resolution};
use crate::typeck::IntKind;
use crate::typeck::{BindingType, Ty, TypeMap};

/// Maximum number of nested function frames before the evaluator emits a
/// `RuntimeError` Note and halts (research R-013).
const RECURSION_LIMIT: usize = 100;

// ─── M08.2: seeded PRNG for the randomized scheduler ────────────────────
//
// xorshift64* — period 2^64-1, passes BigCrush, ~5 LOC. Used by
// `Scheduler::pick` to choose a thread uniformly from the Ready set.
// Hand-rolled instead of pulling `rand`/`rand_core` (~100KB of WASM).
// See research.md R-001.
struct Prng {
    state: u64,
}

impl Prng {
    fn new(seed: u32) -> Self {
        // Splat u32 to u64 so seed=0 doesn't degenerate (xorshift on 0
        // stays at 0 forever).
        let s = ((seed as u64) << 32) | (seed as u64);
        // If still 0 (seed=0 → s=0), use a constant kick so the stream
        // is non-trivial. Deterministic and still "seed=0 is the default".
        let state = if s == 0 { 0xDEAD_BEEF_CAFE_F00D } else { s };
        Self { state }
    }

    fn next_u64(&mut self) -> u64 {
        // xorshift64*
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        self.state.wrapping_mul(0x2545F4914F6CDD1D)
    }
}

/// **M08.2**: seeded scheduler. Picks which Ready thread advances at each
/// scheduling decision point. For single-choice picks, the PRNG state is
/// NOT advanced — preserves single-thread byte-identical traces (VR-S2,
/// SC-002).
struct Scheduler {
    prng: Prng,
    /// Original seed (for pedagogical Notes and debug).
    #[allow(dead_code)] // surfaced in Phase 3 / 4 (US1 Note emission, US2 state).
    seed: u32,
}

impl Scheduler {
    fn new(seed: u32) -> Self {
        Self { prng: Prng::new(seed), seed }
    }

    /// Pick uniformly at random from a non-empty sorted-by-id slice. For
    /// a single-element slice, returns the element WITHOUT advancing the
    /// PRNG state (VR-S2). Caller MUST sort by ThreadId for determinism
    /// (VR-S3); the underlying IndexMap iteration order is preserved but
    /// we explicitly sort to defend against future iteration-order changes.
    #[allow(dead_code)] // wired in Phase 3 (US1).
    fn pick(&mut self, ready: &[crate::event::ThreadId]) -> crate::event::ThreadId {
        assert!(!ready.is_empty(), "Scheduler::pick called on empty Ready set (VR-S1)");
        if ready.len() == 1 {
            return ready[0];
        }
        let idx = (self.prng.next_u64() % ready.len() as u64) as usize;
        ready[idx]
    }
}

/// Evaluate a resolved + typed program, producing a deterministic event stream.
///
/// On success (including runtime errors that surface as `Note` events), returns
/// `Ok(Vec<MemEvent>)`. `Err(ParseError)` is reserved for static-time invariant
/// violations that should be unreachable when M02 succeeded.
///
/// **M08.2**: `seed` parameter controls thread-scheduling decisions. For
/// single-threaded programs the seed has no observable effect.
pub fn evaluate(
    program: &ast::Program,
    resolution: &Resolution,
    types: &TypeMap,
    seed: u32,
) -> Result<Vec<MemEvent>, ParseError> {
    let mut eval = Evaluator::new(program, resolution, types, seed)?;

    // Look up `main` and call it. If there's no `main`, return an empty stream
    // (this isn't an error — some samples might just define helper fns).
    let main_id = eval.fn_decls.iter().find_map(|(id, decl)| {
        if decl.name == "main" { Some(*id) } else { None }
    });
    if let Some(id) = main_id {
        let decl = eval.fn_decls[&id];
        // **M06.1**: synthesize a tight span pointing at `main`'s name token
        // (3-char `fn ` prefix offset, then the identifier) rather than the
        // whole `fn main() { ... }` decl. The latter highlights the entire
        // function body at step 1 (FrameEnter), which is visually disruptive.
        let name_len = decl.name.len() as u32;
        let name_start = decl.span.start + 3; // skip `fn `
        let entry_span = crate::parse::span::Span::new(
            name_start,
            name_start + name_len,
            decl.span.file,
        );
        let _ = eval.call_fn(id, Vec::new(), entry_span);
        // **M08.2**: main finished — drain any remaining spawned threads
        // cooperatively, handling Mutex parking + deadlock detection.
        eval.drain_remaining_threads(entry_span);
    }

    Ok(eval.events)
}

struct Evaluator<'a> {
    resolution: &'a Resolution,
    types: &'a TypeMap,
    /// `BindingId` → `&FnDecl` lookup, built once at construction.
    fn_decls: HashMap<BindingId, &'a ast::FnDecl>,
    /// **M07.4**: `(struct_name, method_name)` → `&FnDecl` for instance
    /// methods declared in `impl` blocks. Built once at construction.
    methods: HashMap<(String, String), &'a ast::FnDecl>,
    /// **M07.4**: `vec![struct_name, fn_name]` → `&FnDecl` for associated
    /// functions (no self) declared in `impl` blocks. Built once.
    assoc_fns: HashMap<Vec<String>, &'a ast::FnDecl>,
    /// **M07.6**: `(trait_name, method_name)` → trait default-method body.
    /// Dispatched when an impl doesn't provide an override for the method.
    trait_default_bodies: HashMap<(String, String), &'a ast::FnDecl>,
    /// **M07.6**: `(trait_name, type_name, method_name)` → impl override body.
    /// Looked up first; falls through to `trait_default_bodies` if absent.
    trait_impl_bodies: HashMap<(String, String, String), &'a ast::FnDecl>,
    /// **M07.6**: set of `(trait_name, type_name)` pairs for which an impl
    /// exists (regardless of whether the impl overrides any specific
    /// method). Lets the dispatch path tell "type T implements trait Tr"
    /// → fall through to Tr's default method body for any non-overridden
    /// method.
    trait_impl_present: std::collections::HashSet<(String, String)>,
    /// **M07.7**: trait name → ordered list of methods (required + default
    /// in declaration order). Built once at construction from the program's
    /// trait decls; consulted by `intern_vtable` to populate the
    /// `VtableAlloc.methods` field.
    trait_methods: HashMap<String, Vec<String>>,
    /// **M07.7**: content-deduplicated vtable addressing. Key is
    /// `(trait_name, type_name)`; value is the interned VtableAddr.
    /// Analog of M07.2's `static_region.by_content`.
    vtable_addrs: HashMap<(String, String), crate::event::VtableAddr>,
    /// **M07.7**: monotonic counter for vtable addresses.
    next_vtable_addr: u32,
    /// **M08**: per-thread state. IndexMap preserves spawn order. Always
    /// contains `ThreadId(0)` (main). Spawned threads get ids 1, 2, … in
    /// spawn order. Replaces the old single-thread `frames: Vec<Frame>`.
    threads: indexmap::IndexMap<crate::event::ThreadId, ThreadState<'a>>,
    /// **M08**: which thread is currently executing. The `frames()` /
    /// `frames_mut()` helpers route through this id into `threads`.
    current_thread_id: crate::event::ThreadId,
    /// **M08**: monotonic counter for ThreadIds.
    #[allow(dead_code)] // populated by later M08 tasks (thread::spawn).
    next_thread_id: u32,
    next_slot_id: u32,
    next_frame_id: u32,
    /// **M06**: monotonic counter for borrow ids.
    next_borrow_id: u32,
    /// **M06.1**: Info notes deferred until the end of the current statement.
    /// Used by deref-read so the explanatory Note appears AFTER the
    /// containing stmt's SlotWrite (i.e. at the same cursor step where the
    /// new binding actually has the read value) rather than before.
    pending_notes: Vec<MemEvent>,
    /// **Post-M08 polish**: spawned threads queued to run at the END of
    /// the current statement (rather than inline at spawn time). Lets
    /// the let-binding's `SlotAlloc h + SlotWrite h` for the JoinHandle
    /// fire BEFORE the closure body runs, so `h: JoinHandle(#N)` is
    /// visible in the spawning thread's frame at the spawn step.
    /// Drained by `eval_stmt` after each statement completes.
    pending_thread_runs: Vec<crate::event::ThreadId>,
    /// **M07**: heap state — live allocations indexed by HeapAddr.
    heap: HeapState,
    /// **M07**: monotonic counter for heap addresses. One addr per logical
    /// allocation (Box::new, Vec::new, String::from); realloc does NOT
    /// increment — the addr is a stable identifier for the binding's heap.
    next_heap_addr: u32,
    /// **M07**: generation per heap addr. Bumps on real realloc (bytes
    /// physically moved). Borrows snapshot the generation at borrow time;
    /// dangling detection compares stored vs current generation.
    heap_generations: std::collections::HashMap<crate::event::HeapAddr, u32>,
    /// **M07**: per-borrow-id snapshot of the target heap's generation at
    /// borrow time. Used by `realloc_heap`'s dangling scan.
    borrow_generations: std::collections::HashMap<crate::event::BorrowId, u32>,
    /// **M07.2**: static-memory region — read-only blocks for unique string
    /// literals (content-deduplicated to match Rust linker behavior).
    static_region: StaticState,
    /// Emitted events in source-execution order.
    events: Vec<MemEvent>,
    /// Set to true on runtime error to stop further evaluation.
    halted: bool,
    /// **M08.2**: seeded scheduler. Consulted at scheduling decision points
    /// (pending-thread-runs drain in Phase 2; full cooperative scheduling
    /// + parking in Phase 3). For single-Ready picks the PRNG state isn't
    /// advanced — single-threaded samples stay byte-identical (SC-002).
    scheduler: Scheduler,
    /// **M08.2**: set when the currently-executing thread should yield to
    /// the scheduler (Mutex contention or join-wait). Eval-stack bails
    /// out via the same `if self.halted` checks; the scheduler reads this,
    /// marks the thread's status, clears the signal, picks another thread.
    park_signal: Option<ParkSignal>,
}

/// **M08.2**: reason the current thread is yielding to the scheduler.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // Join variant reserved for cooperative join parking
enum ParkSignal {
    /// Thread tried to acquire a contended Mutex at `addr`.
    Lock(crate::event::HeapAddr),
    /// Thread tried to join `target` which isn't Done yet.
    Join(crate::event::ThreadId),
}

/// **M07**: heap state. Each live allocation is one HeapObject indexed by
/// its HeapAddr. Realloc replaces (old, new): the `from` addr is removed,
/// the `to` addr is added with the new contents. The `free_list` tracks
/// freed `(addr, size)` chunks so new allocations can reuse them AND
/// fragment them when the request is smaller than the freed chunk — same
/// behavior as a real first-fit allocator.
struct HeapState {
    objects: indexmap::IndexMap<crate::event::HeapAddr, HeapObject>,
    free_list: Vec<(crate::event::HeapAddr, u32)>,
}

impl HeapState {
    fn new() -> Self {
        Self {
            objects: indexmap::IndexMap::new(),
            free_list: Vec::new(),
        }
    }
}

/// **M07.2**: static-memory region. Holds one block per unique string-literal
/// content; the `by_content` map dedupes (matches Rust linker's `.rodata`
/// merging). Static blocks persist for the trace's lifetime — there's no
/// equivalent of `HeapFree` for them.
struct StaticState {
    next_addr: u32,
    blocks: indexmap::IndexMap<crate::event::StaticAddr, StaticBlock>,
    by_content: std::collections::HashMap<String, crate::event::StaticAddr>,
}

struct StaticBlock {
    bytes: String,
}

impl StaticState {
    fn new() -> Self {
        Self {
            next_addr: 0,
            blocks: indexmap::IndexMap::new(),
            by_content: std::collections::HashMap::new(),
        }
    }
}

/// **M07**: per-allocation heap contents.
enum HeapObject {
    /// Single value boxed via `Box::new(v)`.
    Box(Value),
    /// Growable contiguous buffer via `Vec::new()` + `Vec::push`.
    Vec {
        elements: Vec<Value>,
        capacity: usize,
        elem_ty: Ty,
    },
    /// UTF-8 byte sequence via `String::from(...)` + `String::push_str`.
    Str {
        bytes: String,
        capacity: usize,
    },
    /// **M08**: shared-ownership boxed value. `strong_count` is incremented
    /// by `Arc::clone`, decremented at scope-exit drop of an Arc binding.
    /// The block is freed only when count reaches 0.
    #[allow(dead_code)] // fields populated by later M08 tasks (Arc eval).
    Arc {
        value: Box<Value>,
        strong_count: u32,
    },
    /// **M08**: lock-protected boxed value. `holder` is `Some(tid)` while
    /// the mutex is held; `waiters` is a FIFO queue of parked threads.
    #[allow(dead_code)] // fields populated by later M08 tasks (Mutex eval).
    Mutex {
        value: Box<Value>,
        holder: Option<crate::event::ThreadId>,
        waiters: Vec<crate::event::ThreadId>,
    },
    /// **Post-M08 polish**: fused `Arc<Mutex<T>>` heap block. When
    /// `Arc::new(Mutex::new(v))` runs, the Mutex's allocation is TAKEN
    /// OVER by the Arc (matches Rust's actual memory layout — Arc<T>
    /// stores T inline, so Arc<Mutex<T>> is ONE allocation with both
    /// the inner T and the lock state, plus the Arc refcount). Avoids
    /// the double-block display issue where Arc and Mutex looked like
    /// two separate heap regions.
    #[allow(dead_code)]
    ArcMutex {
        value: Box<Value>,
        strong_count: u32,
        holder: Option<crate::event::ThreadId>,
        waiters: Vec<crate::event::ThreadId>,
    },
}

/// **M07.5**: substitute `Ty::Param(name)` occurrences in `ty` with the
/// concrete types from `subst`. Used at frame-entry to lower a generic
/// fn's params from `Ty::Param("T")` to e.g. `Ty::Int(I32)` per the
/// call-site substitution recorded in `TypeMap.call_substs`.
fn apply_subst_ty(ty: &Ty, subst: &std::collections::HashMap<String, Ty>) -> Ty {
    if subst.is_empty() {
        return ty.clone();
    }
    match ty {
        Ty::Param(name) => subst.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Ty::Struct { name, fields, type_args } => {
            let new_fields = fields
                .iter()
                .map(|(fname, fty)| (fname.clone(), apply_subst_ty(fty, subst)))
                .collect();
            let new_args = type_args.iter().map(|t| apply_subst_ty(t, subst)).collect();
            Ty::Struct {
                name: name.clone(),
                fields: new_fields,
                type_args: new_args,
            }
        }
        Ty::Ref { inner, mutable } => Ty::Ref {
            inner: Box::new(apply_subst_ty(inner, subst)),
            mutable: *mutable,
        },
        Ty::Box(inner) => Ty::Box(Box::new(apply_subst_ty(inner, subst))),
        Ty::Vec(inner) => Ty::Vec(Box::new(apply_subst_ty(inner, subst))),
        Ty::Slice(inner) => Ty::Slice(Box::new(apply_subst_ty(inner, subst))),
        Ty::Array(inner, n) => Ty::Array(Box::new(apply_subst_ty(inner, subst)), *n),
        Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Unit | Ty::String | Ty::Str => ty.clone(),
        // M07.7: trait-object types — `trait_name` is a String, no inner
        // type to substitute. Return as-is.
        Ty::DynRef { .. } | Ty::BoxDyn { .. } => ty.clone(),
        // M08: concurrency primitives — recurse on inner for Arc/Mutex/MutexGuard.
        Ty::Arc(inner) => Ty::Arc(Box::new(apply_subst_ty(inner, subst))),
        Ty::Mutex(inner) => Ty::Mutex(Box::new(apply_subst_ty(inner, subst))),
        Ty::MutexGuard(inner) => Ty::MutexGuard(Box::new(apply_subst_ty(inner, subst))),
        Ty::JoinHandle => ty.clone(),
    }
}

/// **M07**: bytes occupied by a value of the given type. Used to size heap
/// allocations realistically — `Box<f32>` allocates 4 bytes, `Box<f64>` 8,
/// `Vec<u8>` cap=N allocates N bytes, `Vec<i32>` cap=N allocates 4*N.
fn ty_size_bytes(ty: &Ty) -> u32 {
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
        // M07.1: slice is a fat pointer (data ptr + length) — 16 bytes on 64-bit.
        // M07.2: `&str` is a slice-shaped fat pointer too.
        Ty::Slice(_) | Ty::Str => 16,
        // M07.3: array is the sum of its elements — N * elem_size.
        Ty::Array(inner, size) => ty_size_bytes(inner) * (*size as u32),
        // M07.4: struct is the sum of its field sizes (no padding — the
        // pedagogical visualization shows a packed layout).
        Ty::Struct { fields, .. } => fields.iter().map(|(_, t)| ty_size_bytes(t)).sum(),
        // M07.5: type parameter — unreachable at eval time (typeck
        // substitutes before any sizing query). Defensive: 0.
        Ty::Param(_) => 0,
        // M07.7: trait-object fat pointers — 16 bytes on 64-bit (data ptr +
        // vtable ptr). Box<dyn> stack-side is also 16 bytes (the Box wrapper
        // IS the fat pointer for dyn).
        Ty::DynRef { .. } | Ty::BoxDyn { .. } => 16,
        // M08: concurrency primitives — Arc/Mutex bindings are pointer-sized
        // (8 bytes on 64-bit); the heap object holds the actual T + metadata.
        // MutexGuard is also 8 bytes (one ptr to the mutex). JoinHandle is
        // a thread-id wrapper (4 bytes; pedagogical approximation).
        Ty::Arc(_) | Ty::Mutex(_) | Ty::MutexGuard(_) => 8,
        Ty::JoinHandle => 4,
    }
}

/// **M07**: bytes occupied by a value (derived from its runtime kind).
fn value_size_bytes(v: &Value) -> u32 {
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
        // M07.1: slice fat pointer = 16 bytes on 64-bit.
        // M07.2: `&str` literals also flow through Value::Slice now.
        Value::Slice { .. } => 16,
        // M07.3: array's total size = N * element_size.
        Value::Array { elements, elem_ty } => {
            elements.len() as u32 * ty_size_bytes(elem_ty)
        }
        // M07.4: struct's total size = sum of field sizes (no padding).
        Value::Struct { fields, .. } => {
            fields.iter().map(|(_, v)| value_size_bytes(v)).sum()
        }
        // M07.7: trait-object fat pointers — 16 bytes (data ptr + vtable ptr).
        Value::DynRef { .. } | Value::BoxDyn { .. } => 16,
        // M08: concurrency primitives — see ty_size_bytes for rationale.
        Value::Arc { .. } | Value::Mutex { .. } | Value::MutexGuard { .. } => 8,
        Value::JoinHandle { .. } => 4,
    }
}

/// **M07**: total + used bytes for a heap object.
fn heap_object_bytes(obj: &HeapObject) -> (u32, u32) {
    match obj {
        HeapObject::Box(v) => {
            let s = value_size_bytes(v);
            (s, s)
        }
        HeapObject::Vec { elements, capacity, elem_ty } => {
            let es = ty_size_bytes(elem_ty);
            ((*capacity as u32) * es, (elements.len() as u32) * es)
        }
        HeapObject::Str { bytes, capacity } => {
            (*capacity as u32, bytes.len() as u32)
        }
        // **M08**: Arc carries an inner value + refcount.
        // Total bytes = inner size + 4 (count); used = same (no spare capacity).
        HeapObject::Arc { value, .. } => {
            let s = value_size_bytes(value) + 4;
            (s, s)
        }
        // **M08**: Mutex carries an inner value + lock state (Option<ThreadId>).
        // Total bytes = inner size + 8 (state metadata); used = same.
        HeapObject::Mutex { value, .. } => {
            let s = value_size_bytes(value) + 8;
            (s, s)
        }
        // **Post-M08**: fused Arc<Mutex<T>> — inner + lock state + refcount.
        HeapObject::ArcMutex { value, .. } => {
            let s = value_size_bytes(value) + 12;
            (s, s)
        }
    }
}

/// **M08**: per-thread execution state. Holds the thread's frame stack,
/// scheduler status, and (for spawned threads) the pre-queued closure
/// body + captures to run when the scheduler first switches in.
struct ThreadState<'a> {
    /// Call stack — innermost frame last.
    frames: Vec<Frame>,
    /// Current scheduler status.
    #[allow(dead_code)] // populated by later M08 tasks (scheduler).
    status: ThreadStatus,
    /// Pre-queued closure body + captures, populated at `thread::spawn`
    /// and consumed when the scheduler first switches into this thread.
    /// `None` after the body has started executing.
    #[allow(dead_code)] // populated by later M08 tasks (thread::spawn).
    queued_body: Option<QueuedBody<'a>>,
}

/// **M08**: thread scheduler status.
#[allow(dead_code)] // variants populated by later M08 tasks (scheduler).
#[derive(Debug, Clone)]
enum ThreadStatus {
    /// Currently executing (matches `Evaluator.current_thread_id`).
    Running,
    /// Closure body queued, not yet started.
    Ready,
    /// Parked on a mutex, waiting to acquire the lock.
    Parked { lock: crate::event::HeapAddr },
    /// Parked waiting for another thread's body to complete.
    JoinWait { target: crate::event::ThreadId },
    /// Body completed; ready to be joined.
    Done,
}

/// **M08**: pre-queued closure body + captures for a spawned thread.
#[allow(dead_code)] // fields populated by later M08 tasks (thread::spawn).
struct QueuedBody<'a> {
    body: &'a ast::Block,
    /// Captured bindings: (BindingId, source SlotId in the spawning thread,
    /// name, value, type) tuples — snapshotted at `thread::spawn`.
    /// `start_queued_thread` uses `source_slot` to emit a `SlotMove`
    /// + pedagogical Note right after the closure's own SlotAlloc/SlotWrite
    /// for the capture — pairs the source-grays-out transition with the
    /// destination-appears transition at the same cursor stop.
    captures: Vec<(BindingId, SlotId, String, Value, Ty)>,
    /// Span of the closure expression — used for the spawned thread's
    /// initial FrameEnter span.
    span: Span,
    /// The thread that did the `thread::spawn` call — needed to mark
    /// its source LocalSlots as moved when `start_queued_thread` runs
    /// (so drop_current_scope on the spawning thread skips them).
    spawning_thread: crate::event::ThreadId,
    /// **M08.2**: cooperative scheduler resume point. Index into
    /// `body.stmts` of the next stmt to run. 0 on initial queue;
    /// incremented after each successful stmt eval; stmt is RE-RUN on
    /// resume when the thread parked on that stmt (mutex / join).
    next_stmt_idx: usize,
    /// **M08.2**: true after the first call to `run_queued_thread_one_stmt`
    /// emitted FrameEnter + captures + pushed the frame/scope. Setup is
    /// done once per queued thread; subsequent calls skip it.
    setup_done: bool,
}

/// **M08.2**: result of advancing a thread by one cooperative-scheduling
/// step. Returned by `run_queued_thread_one_stmt` so the scheduler loop
/// knows whether to re-queue, mark Done, or pick another thread.
#[derive(Debug)]
enum StepProgress {
    /// Thread ran a stmt successfully; more stmts remain. Stays Ready.
    Continue,
    /// Thread completed its body (or returned early). Now Done.
    Done,
    /// Thread parked on a contended Mutex lock. Status flipped to
    /// `Parked { lock: addr }`. Will resume when the lock is released.
    ParkedLock,
    /// Thread parked waiting on another thread's join. Status flipped to
    /// `JoinWait { target: tid }`. Will resume when target reaches Done.
    ParkedJoin,
}

struct Frame {
    frame_id: FrameId,
    /// Lexical scope stack within this frame; innermost last.
    /// The outermost entry is the param scope; nested entries are block scopes.
    scopes: Vec<Scope>,
}

struct Scope {
    /// Locals in declaration order. LIFO drop at scope exit.
    locals: Vec<LocalSlot>,
    /// **M06**: BorrowIds created in this scope. On scope exit, emit
    /// `BorrowEnd` for each (in reverse order).
    borrows: Vec<crate::event::BorrowId>,
    /// **M07**: HeapAddrs allocated in this scope. On scope exit, emit
    /// `HeapFree` for each. NOTE: when an owning binding (Box/Vec/String)
    /// is created via `let`, the alloc happens during init evaluation
    /// (recorded here), THEN the LocalSlot stores the Value::Box/Vec/String.
    /// scope.heap_allocs follows LIFO order alongside locals.
    heap_allocs: Vec<crate::event::HeapAddr>,
}

struct LocalSlot {
    binding_id: BindingId,
    slot_id: SlotId,
    value: Value,
    decl_span: Span,
    /// **Post-M08 polish**: `true` once the binding has been moved (e.g.
    /// captured by a `move` closure). The slot remains on the stack
    /// (its pointer-bytes persist per the M03.1 "memory persists until
    /// reused" principle) but the type-system no longer permits use of
    /// the binding, and Drop is NOT run at scope exit (ownership
    /// transferred to the new owner, who runs Drop in their scope).
    /// The UI grays out moved slots with a `<moved>` annotation.
    moved: bool,
}

impl<'a> Evaluator<'a> {
    fn new(
        program: &'a ast::Program,
        resolution: &'a Resolution,
        types: &'a TypeMap,
        seed: u32,
    ) -> Result<Self, ParseError> {
        let mut fn_decls = HashMap::new();
        let mut methods: HashMap<(String, String), &'a ast::FnDecl> = HashMap::new();
        let mut assoc_fns: HashMap<Vec<String>, &'a ast::FnDecl> = HashMap::new();
        // **M07.6**: trait default-method bodies + trait-impl override bodies.
        let mut trait_default_bodies: HashMap<(String, String), &'a ast::FnDecl> = HashMap::new();
        let mut trait_impl_bodies: HashMap<(String, String, String), &'a ast::FnDecl> = HashMap::new();
        let mut trait_impl_present: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
        for item in &program.items {
            match item {
                ast::Item::Fn(decl) => {
                    let id = resolution
                        .bindings
                        .iter()
                        .find_map(|(id, b)| {
                            if b.name == decl.name && matches!(b.kind, BindingKind::Fn) {
                                Some(*id)
                            } else {
                                None
                            }
                        })
                        .ok_or_else(|| ParseError {
                            message: format!(
                                "fn `{}` has no resolved binding (M02 invariant violation)",
                                decl.name
                            ),
                            span: decl.span,
                        })?;
                    fn_decls.insert(id, decl);
                }
                // **M07.4**: struct decls register no value-level bindings.
                ast::Item::Struct(_) => {}
                // **M07.4** + **M07.6**: impl block. Inherent (trait_name = None)
                // populates `methods` / `assoc_fns`; trait impl (trait_name =
                // Some) populates `trait_impl_bodies` instead.
                ast::Item::Impl(block) => {
                    if let Some(trait_name) = &block.trait_name {
                        trait_impl_present.insert((trait_name.clone(), block.ty_name.clone()));
                        for fn_decl in &block.items {
                            trait_impl_bodies.insert(
                                (trait_name.clone(), block.ty_name.clone(), fn_decl.name.clone()),
                                fn_decl,
                            );
                        }
                    } else {
                        for fn_decl in &block.items {
                            let is_method = fn_decl
                                .params
                                .first()
                                .map(|p| !matches!(p.kind, ast::ParamKind::Normal))
                                .unwrap_or(false);
                            if is_method {
                                methods.insert(
                                    (block.ty_name.clone(), fn_decl.name.clone()),
                                    fn_decl,
                                );
                            } else {
                                assoc_fns.insert(
                                    vec![block.ty_name.clone(), fn_decl.name.clone()],
                                    fn_decl,
                                );
                            }
                        }
                    }
                }
                // **M07.6**: trait declaration — stash default-method bodies
                // for dispatch fall-through.
                ast::Item::Trait(trait_decl) => {
                    for item in &trait_decl.items {
                        if let ast::TraitItem::Default { decl: fn_decl } = item {
                            trait_default_bodies.insert(
                                (trait_decl.name.clone(), fn_decl.name.clone()),
                                fn_decl,
                            );
                        }
                    }
                }
            }
        }
        // **M07.7**: build the per-trait method list (required + default in
        // declaration order) for VtableAlloc's `methods` field. Walks the
        // program's trait decls a second time so the order matches the
        // source (eval-side single source of truth).
        let mut trait_methods: HashMap<String, Vec<String>> = HashMap::new();
        for item in &program.items {
            if let ast::Item::Trait(decl) = item {
                let mut names: Vec<String> = Vec::with_capacity(decl.items.len());
                for ti in &decl.items {
                    match ti {
                        ast::TraitItem::Required { name, .. } => names.push(name.clone()),
                        ast::TraitItem::Default { decl: fn_decl } => names.push(fn_decl.name.clone()),
                    }
                }
                trait_methods.insert(decl.name.clone(), names);
            }
        }
        Ok(Self {
            resolution,
            types,
            fn_decls,
            methods,
            assoc_fns,
            trait_default_bodies,
            trait_impl_bodies,
            trait_impl_present,
            trait_methods,
            vtable_addrs: HashMap::new(),
            next_vtable_addr: 0,
            // M08: initialize with main thread (id 0). Main starts as
            // `Running` since it begins executing immediately. Spawned
            // threads (id 1, 2, …) are added by `eval_path_call`'s
            // `thread::spawn` arm and start as `Ready`.
            threads: {
                let mut m = indexmap::IndexMap::new();
                m.insert(crate::event::ThreadId(0), ThreadState {
                    frames: Vec::new(),
                    status: ThreadStatus::Running,
                    queued_body: None,
                });
                m
            },
            current_thread_id: crate::event::ThreadId(0),
            next_thread_id: 1,
            next_slot_id: 0,
            next_frame_id: 0,
            next_borrow_id: 0,
            pending_notes: Vec::new(),
            pending_thread_runs: Vec::new(),
            heap: HeapState::new(),
            next_heap_addr: 0,
            heap_generations: std::collections::HashMap::new(),
            borrow_generations: std::collections::HashMap::new(),
            static_region: StaticState::new(),
            events: Vec::new(),
            halted: false,
            scheduler: Scheduler::new(seed),
            park_signal: None,
        })
    }

    /// **M06.1**: flush any deferred Notes (emitted during rhs evaluation,
    /// held back so they appear AFTER the statement's main SlotWrite).
    fn flush_pending_notes(&mut self) {
        if !self.pending_notes.is_empty() {
            let drained: Vec<_> = self.pending_notes.drain(..).collect();
            self.events.extend(drained);
        }
    }

    /// **M06**: allocate a fresh borrow id.
    fn alloc_borrow_id(&mut self) -> crate::event::BorrowId {
        let id = crate::event::BorrowId(self.next_borrow_id);
        self.next_borrow_id += 1;
        id
    }

    /// **M08**: refresh the display string for a HeapObject::Arc after a
    /// refcount change. Reads current strong_count + value, builds the
    /// `Arc<T> = v [refs: N]` text, emits a HeapRealloc (from==to) so
    /// the UI's heap-block label updates at this cursor step. Mirrors
    /// the existing M07 vec_push in-place-update pattern.
    fn refresh_arc_display(&mut self, addr: crate::event::HeapAddr, span: Span) {
        let (display, size) = match self.heap.objects.get(&addr) {
            Some(HeapObject::Arc { value, strong_count }) => {
                let inner_ty_name = match value.as_ref() {
                    Value::Int { kind, .. } => kind.name().to_owned(),
                    Value::Float { kind, .. } => kind.name().to_owned(),
                    Value::Bool(_) => "bool".to_owned(),
                    _ => "?".to_owned(),
                };
                let value_str = render_value_for_note(value);
                let display = format!("Arc<{inner_ty_name}> = {value_str} [refs: {strong_count}]");
                let size = value_size_bytes(value) + 4;
                (display, size)
            }
            // Post-M08: fused Arc<Mutex<T>> — combined refcount + lock state display.
            Some(HeapObject::ArcMutex { value, strong_count, holder, .. }) => {
                let value_str = render_value_for_note(value);
                let lock_suffix = match holder {
                    Some(tid) => format!(" [locked by #{}]", tid.0),
                    None => " [free]".to_owned(),
                };
                let display = format!(
                    "Arc<Mutex> = {value_str} [refs: {strong_count}]{lock_suffix}"
                );
                let size = value_size_bytes(value) + 12;
                (display, size)
            }
            _ => return,
        };
        self.events.push(MemEvent::HeapRealloc {
            from: addr,
            to: addr,
            new_size: size,
            new_used: size,
            new_display: display,
            span,
        });
    }

    /// **M08**: read access to the current thread's frame stack. Panics
    /// if `current_thread_id` is missing from `threads` (invariant
    /// violation — the scheduler should always keep this consistent).
    fn frames(&self) -> &Vec<Frame> {
        &self.threads.get(&self.current_thread_id)
            .expect("current_thread_id present in threads (M08 invariant)")
            .frames
    }

    /// **M08**: mutable access to the current thread's frame stack.
    /// All existing single-thread code paths route through this helper
    /// after the M08 refactor; the multi-thread scheduler updates
    /// `current_thread_id` at switch points.
    fn frames_mut(&mut self) -> &mut Vec<Frame> {
        &mut self.threads.get_mut(&self.current_thread_id)
            .expect("current_thread_id present in threads (M08 invariant)")
            .frames
    }

    /// **M08**: emit `MemEvent::ThreadSwitch` AND update `current_thread_id`.
    /// Called whenever the cooperative scheduler picks a different thread.
    /// No-op (no event emitted) if `target` already equals the current id.
    fn switch_to(&mut self, target: crate::event::ThreadId, switch_span: Span) {
        if self.current_thread_id == target {
            return;
        }
        self.events.push(MemEvent::ThreadSwitch {
            thread_id: target,
            span: switch_span,
        });
        self.current_thread_id = target;
    }

    /// **M08 v1 simplified**: acquire a mutex. M08 v1 ships without real
    /// thread parking — spawned threads run inline at join (see `join_thread`)
    /// — so contention scenarios don't park, they simply acquire in
    /// scheduler order. A genuine parked-thread visual (with cross-thread
    /// blocking) is a follow-up enhancement (planned as M08.1).
    fn mutex_lock(
        &mut self,
        addr: crate::event::HeapAddr,
        span: Span,
    ) -> Value {
        let current = self.current_thread_id;
        let obj = self.heap.objects.get_mut(&addr)
            .expect("mutex_lock: heap object present");
        match obj {
            HeapObject::Mutex { holder, .. } | HeapObject::ArcMutex { holder, .. } => {
                if let Some(h) = *holder {
                    if h != current {
                        // **M08.2**: lock is held by another thread. Park.
                        // The scheduler reads `park_signal` after the eval
                        // stack unwinds (via `halted`), flips the thread's
                        // status to ThreadStatus::Parked, clears the signal
                        // + halted, and picks another Ready thread.
                        self.park_signal = Some(ParkSignal::Lock(addr));
                        self.halted = true;
                        return Value::Unit; // sentinel — caller bails out via halted
                    }
                    // Re-entrant lock by same thread — fall through to no-op
                    // (Rust's std::sync::Mutex panics on re-entrance, but
                    // for pedagogy we just allow it; matches M08 v1 behavior
                    // where this case was implicitly fine).
                } else {
                    *holder = Some(current);
                }
            }
            _ => panic!("mutex_lock: heap object is not Mutex or ArcMutex"),
        }
        self.events.push(MemEvent::LockAcquire { addr, span });
        self.refresh_mutex_display(addr, span);
        // **Post-M08 polish**: pedagogical Note explaining the lock
        // acquire. Coalesces with the LockAcquire → HeapRealloc atom
        // via the existing marker→display+Note coalescer, so the user
        // sees the badge flip red AND reads the explanation at one step.
        self.events.push(MemEvent::Note {
            kind: NoteKind::Info,
            message: format!(
                "Thread #{tid} ACQUIRED the lock on heap #{addr_num}. While the guard returned by `.lock()` is alive, this thread has EXCLUSIVE access to the inner value — other threads attempting `.lock()` would park until the guard drops.",
                tid = current.0,
                addr_num = addr.0,
            ),
            span,
        });
        Value::MutexGuard { addr }
    }

    /// **M08 v1 simplified**: scheduler-driven join. If target is `Ready`
    /// (queued), switch to target, run it to completion inline, switch
    /// back. If target is `Done`, just emit ThreadJoin and return.
    /// This synchronous "run-target-inline" model avoids the
    /// continuation/async complexity required for true parked-thread
    /// scheduling. The trade-off: no contention parking pedagogy in M08
    /// v1 — deferred to M08.1.
    fn join_thread(&mut self, target: crate::event::ThreadId, span: Span) {
        // Already done? Quick path.
        if matches!(self.threads.get(&target).map(|t| &t.status), Some(ThreadStatus::Done)) {
            self.events.push(MemEvent::ThreadJoin { thread_id: target.0, span });
            return;
        }
        // Switch to target, run its body to completion, then return.
        let from = self.current_thread_id;
        self.switch_to(target, span);
        self.run_queued_thread_to_done(target);
        self.switch_to(from, span);
        self.events.push(MemEvent::ThreadJoin { thread_id: target.0, span });
    }

    /// **M08 v1**: run a `Ready` thread's queued body to completion. Emits
    /// FrameEnter + captures, evaluates the body, drops scopes, emits
    /// FrameLeave, marks the thread `Done`. The thread MUST be the
    /// current thread (caller switches before invoking).
    fn run_queued_thread_to_done(&mut self, tid: crate::event::ThreadId) {
        // Only handle Ready threads.
        if !matches!(self.threads.get(&tid).map(|t| &t.status), Some(ThreadStatus::Ready)) {
            return;
        }
        // Take the queued body BEFORE setting up the frame (start_queued_thread
        // consumes the Option). Need a body reference for eval after start.
        let body_ref: &'a ast::Block = match self.threads.get(&tid).and_then(|t| t.queued_body.as_ref()) {
            Some(qb) => qb.body,
            None => return, // body already taken — shouldn't happen
        };
        // Materialize frame + captures.
        self.start_queued_thread(tid);
        // Evaluate the body.
        let _ = self.eval_fn_body(body_ref);
        // Drop the body scope (start_queued_thread pushed an outer scope
        // before the captures; eval_fn_body pushed an inner scope for the
        // body's lets; eval_fn_body does NOT drop its scope at the end of
        // execution per the M06 contract).
        let body_close_span = closing_brace_span(body_ref.span);
        // eval_fn_body emits no drops; we need to drop both scopes (body inner + frame outer).
        self.drop_current_scope(body_close_span);
        self.drop_current_scope(body_close_span);
        // Emit FrameLeave.
        let frame_id = self.threads.get(&tid)
            .and_then(|t| t.frames.last())
            .map(|f| f.frame_id);
        if let Some(fid) = frame_id {
            self.events.push(MemEvent::FrameLeave {
                frame_id: fid,
                return_value: Value::Unit,
                span: body_close_span,
            });
        }
        // Pop frame + mark Done.
        if let Some(ts) = self.threads.get_mut(&tid) {
            ts.frames.pop();
            ts.status = ThreadStatus::Done;
        }
    }

    /// **M08**: refresh the display string for a HeapObject::Mutex after a
    /// holder change. Similar to refresh_arc_display but for Mutex's
    /// `[locked by #N]` suffix.
    fn refresh_mutex_display(&mut self, addr: crate::event::HeapAddr, span: Span) {
        let (display, size) = match self.heap.objects.get(&addr) {
            Some(HeapObject::Mutex { value, holder, .. }) => {
                let inner_ty_name = match value.as_ref() {
                    Value::Int { kind, .. } => kind.name().to_owned(),
                    Value::Float { kind, .. } => kind.name().to_owned(),
                    Value::Bool(_) => "bool".to_owned(),
                    _ => "?".to_owned(),
                };
                let value_str = render_value_for_note(value);
                let lock_suffix = match holder {
                    Some(tid) => format!(" [locked by #{}]", tid.0),
                    None => " [free]".to_owned(),
                };
                let display = format!("Mutex<{inner_ty_name}> = {value_str}{lock_suffix}");
                let size = value_size_bytes(value) + 8;
                (display, size)
            }
            // Post-M08: fused Arc<Mutex<T>>. Same display as refresh_arc_display.
            Some(HeapObject::ArcMutex { value, strong_count, holder, .. }) => {
                let value_str = render_value_for_note(value);
                let lock_suffix = match holder {
                    Some(tid) => format!(" [locked by #{}]", tid.0),
                    None => " [free]".to_owned(),
                };
                let display = format!(
                    "Arc<Mutex> = {value_str} [refs: {strong_count}]{lock_suffix}"
                );
                let size = value_size_bytes(value) + 12;
                (display, size)
            }
            _ => return,
        };
        self.events.push(MemEvent::HeapRealloc {
            from: addr,
            to: addr,
            new_size: size,
            new_used: size,
            new_display: display,
            span,
        });
    }

    /// **M08**: scheduler — pick the next thread that can run. Scans
    /// `threads` IndexMap in spawn order (FIFO), picks the first one with
    /// status `Ready` (queued, not yet started) or `Running` (just
    /// unparked or unblocked). Returns `None` if all threads are blocked
    /// (deadlock — caller decides whether to panic or proceed).
    /// **Note**: kept for backward compat; cooperative loop uses
    /// `scheduler_tick`.
    #[allow(dead_code)]
    fn schedule_next_ready(&self) -> Option<crate::event::ThreadId> {
        for (tid, ts) in &self.threads {
            if matches!(ts.status, ThreadStatus::Ready | ThreadStatus::Running) {
                return Some(*tid);
            }
        }
        None
    }

    /// **M08.2**: cooperative scheduler tick. Called at each stmt boundary
    /// of the currently-running thread. Picks via seeded PRNG from
    /// `{self, ...ready_spawned_threads}`. If self wins → no-op (caller
    /// continues its own next stmt). If a spawned thread wins → switch to
    /// it, run one stmt, handle park, switch back.
    fn scheduler_tick(&mut self, span: Span) {
        if self.halted {
            return;
        }
        // **M08.2**: only main triggers the cooperative tick. Spawned threads
        // run their own stmts uninterrupted from their start until they
        // park / finish (driven by run_queued_thread_one_stmt). Without
        // this guard, a spawned thread's post-stmt scheduler_tick would
        // recursively try to advance main, breaking the call-stack
        // invariant (main isn't resumable from arbitrary points).
        if self.current_thread_id != crate::event::ThreadId(0) {
            return;
        }
        let resume = self.current_thread_id;
        // Build candidate list: current thread + all Ready spawned threads.
        // Sorted by ThreadId for determinism (VR-S3).
        let mut candidates: Vec<crate::event::ThreadId> = vec![resume];
        for (tid, ts) in &self.threads {
            if *tid == resume { continue; }
            if matches!(ts.status, ThreadStatus::Ready | ThreadStatus::Running) {
                candidates.push(*tid);
            }
        }
        candidates.sort_by_key(|t| t.0);
        let chosen = self.scheduler.pick(&candidates);
        if chosen == resume {
            return; // continue current thread
        }
        // **M08.2 / R-008**: pedagogical Note explaining the scheduler
        // decision. Surfaces the non-determinism to learners: "this
        // could have been the OTHER thread; seed=N gave us this choice."
        // Only emitted when there's actually a choice (≥ 2 candidates) —
        // forced picks are pedagogically silent.
        if candidates.len() > 1 {
            let n_other = candidates.len() - 1;
            self.events.push(MemEvent::Note {
                kind: NoteKind::Info,
                message: format!(
                    "Scheduler picked thread #{} to advance one statement (seed={}, {n_other} other thread{} also ready). With a different seed the choice could swing the other way — this is what 'threads are non-deterministic' means in practice.",
                    chosen.0,
                    self.scheduler.seed,
                    if n_other == 1 { " was" } else { "s were" },
                ),
                span,
            });
        }
        // Switch to the chosen spawned thread, run one stmt.
        self.switch_to(chosen, span);
        let progress = self.run_queued_thread_one_stmt(chosen, span);
        self.handle_step_progress(chosen, progress, span);
        // Switch back to the original caller.
        self.switch_to(resume, span);
    }

    /// **M08.2**: handle the StepProgress returned by
    /// `run_queued_thread_one_stmt`. Updates the thread's status and
    /// triggers unparking when the thread reached Done.
    fn handle_step_progress(
        &mut self,
        tid: crate::event::ThreadId,
        progress: StepProgress,
        span: Span,
    ) {
        match progress {
            StepProgress::Continue => {}
            StepProgress::Done => {
                if let Some(ts) = self.threads.get_mut(&tid) {
                    ts.status = ThreadStatus::Done;
                }
                // Unpark any thread that was JoinWait-ing on this one.
                self.unpark_joiners(tid, span);
            }
            StepProgress::ParkedLock => {
                // park_signal already set by mutex_lock; convert it.
                if let Some(ParkSignal::Lock(addr)) = self.park_signal.take() {
                    if let Some(ts) = self.threads.get_mut(&tid) {
                        ts.status = ThreadStatus::Parked { lock: addr };
                    }
                }
                self.halted = false; // clear soft halt — only this thread parked
            }
            StepProgress::ParkedJoin => {
                if let Some(ParkSignal::Join(target)) = self.park_signal.take() {
                    if let Some(ts) = self.threads.get_mut(&tid) {
                        ts.status = ThreadStatus::JoinWait { target };
                    }
                }
                self.halted = false;
            }
        }
    }

    /// **M08.2**: when `target` reaches Done, scan for any thread parked
    /// on `JoinWait { target }` and flip them back to Ready/Running.
    fn unpark_joiners(&mut self, target: crate::event::ThreadId, _span: Span) {
        let to_unpark: Vec<crate::event::ThreadId> = self.threads.iter()
            .filter_map(|(tid, ts)| match ts.status {
                ThreadStatus::JoinWait { target: t } if t == target => Some(*tid),
                _ => None,
            })
            .collect();
        for tid in to_unpark {
            if let Some(ts) = self.threads.get_mut(&tid) {
                ts.status = ThreadStatus::Running;
            }
        }
    }

    /// **M08.2**: when a lock is released, scan for any thread parked on
    /// `Parked { lock: addr }` and flip them back to Ready/Running so the
    /// next scheduler tick can pick one of them.
    fn unpark_lock_waiters(&mut self, addr: crate::event::HeapAddr) {
        let to_unpark: Vec<crate::event::ThreadId> = self.threads.iter()
            .filter_map(|(tid, ts)| match ts.status {
                ThreadStatus::Parked { lock } if lock == addr => Some(*tid),
                _ => None,
            })
            .collect();
        for tid in to_unpark {
            if let Some(ts) = self.threads.get_mut(&tid) {
                ts.status = ThreadStatus::Running;
            }
        }
    }

    /// **M08.2**: cooperative-scheduling primitive. Run the queued thread
    /// `tid` for ONE statement. On first call (when `setup_done == false`),
    /// also performs the FrameEnter + captures setup. When all stmts are
    /// done, finalizes (drops scopes, emits FrameLeave, returns Done).
    /// Returns ParkedLock/ParkedJoin if the stmt's first action would
    /// contend on a Mutex / wait on a join.
    fn run_queued_thread_one_stmt(
        &mut self,
        tid: crate::event::ThreadId,
        finalize_span: Span,
    ) -> StepProgress {
        // First-call setup: emit FrameEnter, install captures, push scope.
        let needs_setup = self.threads.get(&tid)
            .and_then(|t| t.queued_body.as_ref())
            .map(|qb| !qb.setup_done)
            .unwrap_or(false);
        if needs_setup {
            self.start_queued_thread(tid);
            if self.halted { return StepProgress::Continue; }
            // **M08.2**: mirror `eval_fn_body`'s body-scope push so the
            // finalize path's two `drop_current_scope` calls (body inner
            // + frame outer) have two scopes to pop. Without this, the
            // finalize panics on `.scopes.pop().expect("scope active")`.
            self.frames_mut()
                .last_mut()
                .expect("frame active after start_queued_thread")
                .scopes
                .push(Scope {
                    locals: Vec::new(),
                    borrows: Vec::new(),
                    heap_allocs: Vec::new(),
                });
        }
        // Read body + next_stmt_idx from queued_body.
        let (body, idx, body_span) = match self.threads.get(&tid)
            .and_then(|t| t.queued_body.as_ref())
        {
            Some(qb) => (qb.body, qb.next_stmt_idx, qb.body.span),
            None => return StepProgress::Done, // already finalized
        };
        // All stmts done? Run the tail expression (if any), then finalize.
        if idx >= body.stmts.len() {
            // Tail expression evaluation (M06 block-as-expression semantics).
            // For closure bodies in M08, this is typically the implicit
            // unit return of `move || { ... }`; no tail expr to eval.
            if let Some(tail) = &body.tail {
                let _ = self.eval_expr(tail);
                if self.halted && self.park_signal.is_some() {
                    return self.park_progress();
                }
            }
            let close_span = closing_brace_span(body_span);
            // Drop body's inner scope + outer (frame) scope.
            self.drop_current_scope(close_span);
            self.drop_current_scope(close_span);
            // Emit FrameLeave + pop frame.
            let frame_id = self.threads.get(&tid)
                .and_then(|t| t.frames.last())
                .map(|f| f.frame_id);
            if let Some(fid) = frame_id {
                self.events.push(MemEvent::FrameLeave {
                    frame_id: fid,
                    return_value: Value::Unit,
                    span: close_span,
                });
            }
            if let Some(ts) = self.threads.get_mut(&tid) {
                ts.frames.pop();
                ts.queued_body = None;
            }
            let _ = finalize_span;
            return StepProgress::Done;
        }
        // Evaluate one statement.
        let stmt = &body.stmts[idx];
        self.eval_stmt(stmt);
        // Park signal triggered mid-stmt? (Mutex/join contention.)
        if self.halted && self.park_signal.is_some() {
            return self.park_progress();
        }
        // Increment next_stmt_idx so the next call advances.
        if let Some(qb) = self.threads.get_mut(&tid).and_then(|t| t.queued_body.as_mut()) {
            qb.next_stmt_idx = idx + 1;
        }
        StepProgress::Continue
    }

    fn park_progress(&self) -> StepProgress {
        match self.park_signal {
            Some(ParkSignal::Lock(_)) => StepProgress::ParkedLock,
            Some(ParkSignal::Join(_)) => StepProgress::ParkedJoin,
            None => StepProgress::Continue,
        }
    }

    /// **M08.2**: top-level scheduler loop run after main's body returns.
    /// Drains any remaining Ready / Parked threads cooperatively until
    /// they're all Done or no progress can be made (deadlock).
    fn drain_remaining_threads(&mut self, span: Span) {
        loop {
            // Anything still doing work?
            let ready: Vec<crate::event::ThreadId> = self.threads.iter()
                .filter(|(_, ts)| matches!(
                    ts.status,
                    ThreadStatus::Ready | ThreadStatus::Running
                ))
                .map(|(tid, _)| *tid)
                .collect();
            if ready.is_empty() {
                // Check for deadlock: any Parked / JoinWait?
                let parked: Vec<crate::event::ThreadId> = self.threads.iter()
                    .filter(|(_, ts)| matches!(
                        ts.status,
                        ThreadStatus::Parked { .. } | ThreadStatus::JoinWait { .. }
                    ))
                    .map(|(tid, _)| *tid)
                    .collect();
                if !parked.is_empty() {
                    let mut ids = parked;
                    ids.sort_by_key(|t| t.0);
                    self.events.push(MemEvent::Deadlock {
                        thread_ids: ids,
                        span,
                    });
                }
                return;
            }
            // Sort for determinism + seed-pick.
            let mut sorted = ready;
            sorted.sort_by_key(|t| t.0);
            let chosen = self.scheduler.pick(&sorted);
            self.switch_to(chosen, span);
            let progress = self.run_queued_thread_one_stmt(chosen, span);
            self.handle_step_progress(chosen, progress, span);
            if self.halted && self.park_signal.is_none() {
                // Real halt (runtime error). Stop.
                return;
            }
        }
    }

    /// **M08**: start a `Ready` thread — emit `FrameEnter` for the closure
    /// body, emit per-capture `SlotAlloc` + `SlotWrite`, push a fresh
    /// frame + outer scope, transition status to `Running`. Called by
    /// `switch_to` when the target thread is in `Ready` state and has
    /// a `queued_body`.
    fn start_queued_thread(&mut self, tid: crate::event::ThreadId) {
        // **M08.2**: TAKE captures (they're consumed once) but KEEP body +
        // metadata so the cooperative scheduler can resume mid-body.
        // setup_done is set to true at the end so further calls skip setup.
        let (body_span, captures_taken, spawning_thread) = {
            let ts = self.threads.get_mut(&tid)
                .expect("start_queued_thread: target thread present");
            let qb = match ts.queued_body.as_mut() {
                Some(qb) => qb,
                None => return,
            };
            let captures = std::mem::take(&mut qb.captures);
            (qb.span, captures, qb.spawning_thread)
        };
        // Reconstruct the legacy `queued` value used by the body below.
        struct Compat {
            span: Span,
            captures: Vec<(BindingId, SlotId, String, Value, Ty)>,
            spawning_thread: crate::event::ThreadId,
        }
        let queued = Compat { span: body_span, captures: captures_taken, spawning_thread };
        // Allocate a fresh frame_id for the closure body.
        let frame_id = self.alloc_frame_id();
        // Emit FrameEnter using a synthetic name for closures.
        let fn_name = format!("<closure thread #{}>", tid.0);
        self.events.push(MemEvent::FrameEnter {
            frame_id,
            fn_name,
            span: queued.span,
        });
        // Push the frame + outer scope into the target thread's frame stack.
        {
            let ts = self.threads.get_mut(&tid).unwrap();
            ts.frames.push(Frame {
                frame_id,
                scopes: vec![Scope {
                    locals: Vec::new(),
                    borrows: Vec::new(),
                    heap_allocs: Vec::new(),
                }],
            });
            ts.status = ThreadStatus::Running;
        }
        // Emit per-capture SlotAlloc + SlotWrite. Each capture becomes a
        // LocalSlot in the closure's outer scope, keyed by the ORIGINAL
        // BindingId so the closure body's `Expr::Ident` uses (which the
        // resolver mapped to those outer BindingIds) lookup the captured
        // slot via the standard `lookup_local_value(binding_id)` path.
        // **Post-M08 polish**: immediately after each capture's
        // SlotAlloc/SlotWrite, emit `SlotMove` + a pedagogical Note +
        // flip the spawning thread's source LocalSlot to `moved: true`.
        // This pairs source-grays-out with destination-appears at one
        // logical cursor stop (the existing SlotAlloc→SlotWrite and
        // marker→Note coalescers compress them together).
        let spawning_thread = queued.spawning_thread;
        for (cap_binding, source_slot, cap_name, cap_value, cap_ty) in queued.captures {
            let dest_slot = self.alloc_slot_id();
            self.events.push(MemEvent::SlotAlloc {
                slot_id: dest_slot,
                name: cap_name.clone(),
                ty: cap_ty,
                span: queued.span,
            });
            self.events.push(MemEvent::SlotWrite {
                slot_id: dest_slot,
                value: cap_value.clone(),
                span: queued.span,
            });
            // Push the LocalSlot into the spawned thread's scope.
            let ts = self.threads.get_mut(&tid).unwrap();
            let scope = ts.frames.last_mut().unwrap().scopes.last_mut().unwrap();
            scope.locals.push(LocalSlot {
                binding_id: cap_binding,
                slot_id: dest_slot,
                value: cap_value.clone(),
                decl_span: queued.span,
                moved: false,
            });
            // Emit SlotMove from the SOURCE slot in the spawning thread —
            // pairs the source-grays transition with the destination-
            // appears transition (same coalesced step in the UI).
            self.events.push(MemEvent::SlotMove {
                from: source_slot,
                to: dest_slot,
                value: cap_value.clone(),
                span: queued.span,
            });
            // Mark the source LocalSlot as moved in the spawning thread
            // so its drop_current_scope skips the unconditional Drop
            // (the new owner — this spawned thread — will run Drop
            // when its body's scope exits).
            if let Some(spawning_ts) = self.threads.get_mut(&spawning_thread) {
                for frame in spawning_ts.frames.iter_mut() {
                    for scope in frame.scopes.iter_mut() {
                        for local in scope.locals.iter_mut() {
                            if local.binding_id == cap_binding {
                                local.moved = true;
                            }
                        }
                    }
                }
            }
            // Pedagogical Note explaining the move.
            self.events.push(MemEvent::Note {
                kind: NoteKind::Info,
                message: format!(
                    "`{cap_name}` is MOVED into the spawned closure (`move ||`). The source slot on the spawning thread's frame stays visible — its bytes physically persist — but the binding is no longer usable (Rust's compiler prevents further references). Drop will NOT run on the source; the new owner here runs Drop when its scope exits.",
                ),
                span: queued.span,
            });
        }
        // **M08.2**: mark setup complete so subsequent cooperative ticks
        // skip the setup pass.
        if let Some(qb) = self.threads.get_mut(&tid).and_then(|t| t.queued_body.as_mut()) {
            qb.setup_done = true;
        }
    }

    /// **M07**: allocate a HeapAddr. Always increments — used for realloc's
    /// new allocation where we don't want free-list reuse (the just-freed
    /// `from` would trivially recycle and undo the realloc pedagogically).
    fn alloc_heap_addr(&mut self) -> crate::event::HeapAddr {
        let id = crate::event::HeapAddr(self.next_heap_addr);
        self.next_heap_addr += 1;
        id
    }

    /// **M07**: allocate a HeapAddr that prefers reusing a freed chunk
    /// (first-fit). If the freed chunk is larger than `needed`, splits it
    /// — the leftover bytes become their own free chunk with a fresh addr
    /// (visible in the UI as a freed-block fragment, available for future
    /// reuse). Returns `(addr, Option<(fragment_addr, fragment_size)>)`.
    fn alloc_heap_addr_sized(&mut self, needed: u32) -> (crate::event::HeapAddr, Option<(crate::event::HeapAddr, u32)>) {
        if let Some(idx) = self.heap.free_list.iter().position(|(_, s)| *s >= needed) {
            let (addr, size) = self.heap.free_list.remove(idx);
            if size > needed {
                let frag_addr = crate::event::HeapAddr(self.next_heap_addr);
                self.next_heap_addr += 1;
                let frag_size = size - needed;
                self.heap.free_list.push((frag_addr, frag_size));
                return (addr, Some((frag_addr, frag_size)));
            }
            return (addr, None);
        }
        (self.alloc_heap_addr(), None)
    }

    /// **M07.2**: intern a string-literal's bytes into the static region.
    /// Content-deduplicated: identical bytes share one block (matches Rust
    /// linker's `.rodata` merging). Emits `StaticAlloc` only on first
    /// occurrence. Returns the addr (newly allocated or reused).
    fn intern_static(&mut self, bytes: String, span: Span) -> crate::event::StaticAddr {
        if let Some(addr) = self.static_region.by_content.get(&bytes) {
            return *addr;
        }
        let addr = crate::event::StaticAddr(self.static_region.next_addr);
        self.static_region.next_addr += 1;
        self.static_region.by_content.insert(bytes.clone(), addr);
        self.static_region.blocks.insert(addr, StaticBlock { bytes: bytes.clone() });
        self.events.push(MemEvent::StaticAlloc { addr, bytes, span });
        addr
    }

    /// **M07.2**: look up a static block's bytes by addr. Used by
    /// `String::from` / `push_str` to copy the literal's content into the
    /// heap allocation.
    fn get_static_bytes(&self, addr: crate::event::StaticAddr) -> &str {
        self.static_region
            .blocks
            .get(&addr)
            .map(|b| b.bytes.as_str())
            .expect("static block must exist for any Pointee::Static value")
    }

    /// **M07.7**: intern a `(trait_name, type_name)` pair into the VTABLES
    /// region. Content-deduplicated: identical pairs share one VtableAddr.
    /// Emits `MemEvent::VtableAlloc` only on first occurrence (mirrors
    /// `intern_static`). The methods list is built from the program's
    /// trait registry (declaration order).
    fn intern_vtable(
        &mut self,
        trait_name: &str,
        type_name: &str,
        span: Span,
    ) -> crate::event::VtableAddr {
        let key = (trait_name.to_owned(), type_name.to_owned());
        if let Some(addr) = self.vtable_addrs.get(&key) {
            return *addr;
        }
        let addr = crate::event::VtableAddr(self.next_vtable_addr);
        self.next_vtable_addr += 1;
        let methods = self
            .trait_methods
            .get(trait_name)
            .cloned()
            .unwrap_or_default();
        self.vtable_addrs.insert(key, addr);
        self.events.push(MemEvent::VtableAlloc {
            addr,
            trait_name: trait_name.to_owned(),
            type_name: type_name.to_owned(),
            methods,
            span,
        });
        addr
    }

    /// **M07.7**: resolve a `Pointee` target to the concrete type name of
    /// the underlying value. Used for vtable interning + dispatch — when
    /// we cast `&p as &dyn Show`, we need the concrete type of p.
    fn pointee_concrete_type_name(&self, target: crate::event::Pointee) -> Option<String> {
        match target {
            crate::event::Pointee::Slot(slot_id) => match self.lookup_slot_value(slot_id) {
                Some(Value::Struct { name, .. }) => Some(name),
                Some(Value::Int { kind, .. }) => Some(kind.name().to_owned()),
                Some(Value::Float { kind, .. }) => Some(kind.name().to_owned()),
                Some(Value::Bool(_)) => Some("bool".to_owned()),
                _ => None,
            },
            crate::event::Pointee::Heap(addr) => match self.heap.objects.get(&addr) {
                Some(HeapObject::Box(v)) => match v {
                    Value::Struct { name, .. } => Some(name.clone()),
                    Value::Int { kind, .. } => Some(kind.name().to_owned()),
                    Value::Float { kind, .. } => Some(kind.name().to_owned()),
                    Value::Bool(_) => Some("bool".to_owned()),
                    _ => None,
                },
                _ => None,
            },
            crate::event::Pointee::Static(_) => None,
        }
    }


    /// **M07**: allocate a heap object and emit `HeapAlloc`. Returns the
    /// new addr. Tracks the addr in the current scope for HeapFree on exit.
    fn alloc_heap(&mut self, obj: HeapObject, ty_name: String, size: u32, span: Span) -> crate::event::HeapAddr {
        let (addr, fragment) = self.alloc_heap_addr_sized(size);
        let (_, used) = heap_object_bytes(&obj);
        self.heap.objects.insert(addr, obj);
        // **M07.2**: pedagogical Info note explaining the allocator's
        // first-fit + split behavior. Fires BEFORE the HeapAlloc event
        // so the learner reads "what's about to happen" then sees the
        // heap update on the next cursor step. Only emits when there's
        // a leftover fragment — a fresh-addr alloc (no reuse) is
        // self-explanatory.
        if let Some((frag_addr, frag_size)) = fragment {
            let total = size + frag_size;
            self.events.push(MemEvent::Note {
                kind: NoteKind::Info,
                message: format!(
                    "Allocator first-fit: heap #{from_addr} (was {total}B freed) reused for this {size}B request. The {frag_size}B leftover stays freed as heap #{frag_addr_num} — available for the next request that fits.",
                    from_addr = addr.0,
                    frag_addr_num = frag_addr.0,
                ),
                span,
            });
        }
        // **M07.2**: emit ONE HeapAlloc carrying both the new live block
        // AND any leftover freed fragment (when the allocator split a
        // larger freed chunk). Previously these were two consecutive
        // events — the cursor step in between showed the reuse without
        // the remainder, which read as "the freed bytes disappeared".
        self.events.push(MemEvent::HeapAlloc {
            addr,
            size,
            used,
            ty_name,
            fragment_of: None,
            split_remainder: fragment.map(|(faddr, fsize)| (faddr, fsize)),
            span,
        });
        // Track this alloc for HeapFree on scope exit.
        if let Some(scope) = self
            .frames_mut()
            .last_mut()
            .and_then(|f| f.scopes.last_mut())
        {
            scope.heap_allocs.push(addr);
        }
        addr
    }

    /// **M07**: real realloc — bytes physically moved to a new heap addr.
    /// The old `from` addr is freed; a fresh `to` addr is allocated and
    /// holds the new contents. Used when capacity is exceeded and the
    /// existing region can't accommodate the growth in place. The pedagogy
    /// makes the **copy** visible (old block disappears; new block at a
    /// different addr appears with the new contents).
    fn realloc_heap(&mut self, from: crate::event::HeapAddr, obj: HeapObject, new_size: u32, span: Span) -> crate::event::HeapAddr {
        let new_display = heap_object_display(&obj);
        let (old_cap, new_cap, kind_label) = match (self.heap.objects.get(&from), &obj) {
            (Some(HeapObject::Vec { capacity: oc, .. }), HeapObject::Vec { capacity: nc, .. }) => (*oc, *nc, "Vec"),
            (Some(HeapObject::Str { capacity: oc, .. }), HeapObject::Str { capacity: nc, .. }) => (*oc, *nc, "String"),
            _ => (0, 0, "heap object"),
        };
        // Allocate a NEW addr for the copy. NOTE: the addr returned by
        // alloc_heap_addr won't be `from` itself because we haven't yet
        // pushed `from` to the free-list — that happens right after the
        // allocation, mirroring how realloc semantically does free-then-
        // alloc but in eval order we need the new addr before freeing.
        let to = self.alloc_heap_addr();
        // Read the freed block's size BEFORE removing it so we know what to
        // push back to the free list.
        let from_size = self.heap.objects.get(&from)
            .map(heap_object_bytes).map(|(t, _)| t).unwrap_or(0);
        self.heap.objects.shift_remove(&from);
        self.heap.free_list.push((from, from_size));
        let (_, new_used) = heap_object_bytes(&obj);
        self.heap.objects.insert(to, obj);
        self.events.push(MemEvent::HeapRealloc {
            from,
            to,
            new_size,
            new_used,
            new_display,
            span,
        });
        // Note: the explanatory Info Note is emitted by the caller (vec_push
        // / string_push_str) BEFORE this realloc fires, so the pedagogy
        // sequence is "announce → realloc → push" rather than
        // "realloc → explain". `kind_label` / `old_cap` / `new_cap` left
        // intentionally unused here so callers control the messaging.
        let _ = (kind_label, old_cap, new_cap);
        // Update scope tracking: `from` is gone; `to` is the new owned addr.
        for frame in self.frames_mut().iter_mut() {
            for scope in frame.scopes.iter_mut() {
                for h in scope.heap_allocs.iter_mut() {
                    if *h == from {
                        *h = to;
                    }
                }
            }
        }
        // Update LocalSlot owning-values to point at the new addr. No
        // SlotWrite event emitted: the ui's apply_event for HeapRealloc
        // already updates owning relationships from `from` to `to`, so
        // a separate SlotWrite would just create a redundant cursor step
        // with no visible change (the slot value cell is empty for
        // heap-owning bindings anyway — the black arrow is the visual).
        for frame in self.frames_mut().iter_mut() {
            for scope in frame.scopes.iter_mut() {
                for local in scope.locals.iter_mut() {
                    match &mut local.value {
                        Value::Vec { addr } if *addr == from => *addr = to,
                        Value::String { addr } if *addr == from => *addr = to,
                        Value::Box { addr } if *addr == from => *addr = to,
                        _ => {}
                    }
                }
            }
        }
        // Dangling-borrow detection: scan locals for `Value::Ref` OR
        // `Value::Slice` whose target is `Pointee::Heap(from)`. Both variants
        // carry borrow_ids registered in the same scope; after the realloc
        // the addr changed but their stored target still points at the OLD
        // freed addr — they're dangling.
        // **M07.1**: slice borrows share this code path; the realloc-time
        // pedagogy is identical to single-element borrows (just at slice
        // granularity).
        let mut dangling: Vec<Span> = Vec::new();
        for frame in self.frames().iter() {
            for scope in frame.scopes.iter() {
                for local in scope.locals.iter() {
                    let dangles = match local.value {
                        Value::Ref { target: crate::event::Pointee::Heap(a), .. } => a == from,
                        Value::Slice { target: crate::event::Pointee::Heap(a), .. } => a == from,
                        _ => false,
                    };
                    if dangles {
                        dangling.push(local.decl_span);
                    }
                }
            }
        }
        for sp in dangling {
            self.events.push(MemEvent::Note {
                kind: NoteKind::RuntimeError,
                message: format!(
                    "dangling reference: borrow still points at heap #{from_n}, which was freed during the realloc",
                    from_n = from.0,
                ),
                span: sp,
            });
        }
        to
    }

    /// **M07**: free a heap object — emit HeapFree, remove from state,
    /// push the addr+size to the free-list for potential reuse by a later
    /// alloc (with split if the reuse is smaller than the freed chunk).
    fn free_heap(&mut self, addr: crate::event::HeapAddr, span: Span) {
        if let Some(obj) = self.heap.objects.shift_remove(&addr) {
            let (size, _) = heap_object_bytes(&obj);
            self.events.push(MemEvent::HeapFree { addr, span });
            self.heap.free_list.push((addr, size));
        }
    }

    // ─── id allocators ─────────────────────────────────────────────────────

    fn alloc_slot_id(&mut self) -> SlotId {
        let id = SlotId(self.next_slot_id);
        self.next_slot_id += 1;
        id
    }

    fn alloc_frame_id(&mut self) -> FrameId {
        let id = FrameId(self.next_frame_id);
        self.next_frame_id += 1;
        id
    }

    // ─── runtime-error helper ──────────────────────────────────────────────

    fn emit_runtime_error(&mut self, message: String, span: Span) {
        self.events.push(MemEvent::Note {
            kind: NoteKind::RuntimeError,
            message,
            span,
        });
        self.halted = true;
    }

    // ─── lookup helpers ────────────────────────────────────────────────────

    fn lookup_var_ty(&self, binding_id: BindingId) -> Option<Ty> {
        match self.types.binding_types.get(&binding_id) {
            Some(BindingType::Var(ty)) => Some(ty.clone()),
            _ => None,
        }
    }

    fn find_let_binding(&self, let_stmt: &ast::LetStmt) -> BindingId {
        self.resolution
            .bindings
            .iter()
            .find_map(|(id, decl)| {
                if matches!(decl.kind, BindingKind::Let { .. })
                    && decl.name_span == let_stmt.span
                {
                    Some(*id)
                } else {
                    None
                }
            })
            .expect("let binding present after resolve")
    }

    fn find_param_binding(&self, param: &ast::Param) -> BindingId {
        self.resolution
            .bindings
            .iter()
            .find_map(|(id, decl)| {
                if matches!(decl.kind, BindingKind::Param) && decl.name_span == param.span {
                    Some(*id)
                } else {
                    None
                }
            })
            .expect("param binding present after resolve")
    }

    fn lookup_local_value(&self, binding_id: BindingId) -> Option<Value> {
        let frame = self.frames().last()?;
        for scope in frame.scopes.iter().rev() {
            for local in scope.locals.iter().rev() {
                if local.binding_id == binding_id {
                    return Some(local.value.clone());
                }
            }
        }
        None
    }

    // ─── call_fn / scope / frame management ────────────────────────────────

    fn call_fn(&mut self, fn_binding: BindingId, args: Vec<Value>, call_span: Span) -> Value {
        let decl = self.fn_decls[&fn_binding];
        // **M07.5**: for generic-fn calls, build the mangled name from
        // typeck's per-call-site substitution record. Non-generic calls
        // get the bare fn name (existing M07.4 behavior).
        let display = self.mangle_fn_name(&decl.name, call_span);
        self.call_decl(decl, &display, args, call_span)
    }

    /// **M07.5**: build the mangled display name for a call. Looks up
    /// `types.call_substs[call_span]`. If a substitution was recorded
    /// (generic call), returns `"source::<ty1, ty2>"`. Otherwise returns
    /// the bare source name.
    fn mangle_fn_name(&self, source_name: &str, call_span: Span) -> String {
        match self.types.call_substs.get(&call_span) {
            Some(subst) if !subst.is_empty() => {
                let args = subst
                    .iter()
                    .map(|(_, t)| t.name())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{source_name}::<{args}>")
            }
            _ => source_name.to_owned(),
        }
    }

    /// **M07.4**: shared call-frame entry. Used by both free-fn calls
    /// (via `call_fn`) and user-defined method / associated-fn calls
    /// (via `eval_method_call`'s user-method fall-through and
    /// `eval_path_call`'s assoc-fn fall-through). `display_name` is what
    /// gets reported in `FrameEnter.fn_name` (e.g. `"Point::x"` for a
    /// method, `"add"` for a free fn).
    fn call_decl(
        &mut self,
        decl: &'a ast::FnDecl,
        display_name: &str,
        args: Vec<Value>,
        call_span: Span,
    ) -> Value {
        if self.halted {
            return Value::Unit;
        }
        if self.frames().len() >= RECURSION_LIMIT {
            self.emit_runtime_error(
                format!("recursion depth exceeded ({RECURSION_LIMIT} frames)"),
                call_span,
            );
            return Value::Unit;
        }

        let frame_id = self.alloc_frame_id();

        // Pre-allocate param SlotIds so subsequent SlotAlloc events can use them.
        let mut param_slots: Vec<(BindingId, SlotId, String, Value, Span)> =
            Vec::with_capacity(decl.params.len());
        for (i, param) in decl.params.iter().enumerate() {
            let slot_id = self.alloc_slot_id();
            let binding_id = self.find_param_binding(param);
            let value = args.get(i).cloned().unwrap_or(Value::Unit);
            param_slots.push((binding_id, slot_id, param.name.clone(), value, param.span));
        }

        // M03.1: FrameEnter no longer carries a `params` field. The same info
        // is fully conveyed by the per-param SlotAlloc + SlotWrite events that
        // follow this FrameEnter.
        //
        // The span is the **call-site** span (e.g. `add(2, 3)` in the source),
        // not the function declaration span. This lets consumers distinguish
        // which call site triggered each frame — important when the same
        // function is called multiple times. The outermost `main` is invoked
        // from `evaluate(...)` with `call_span = decl.span`, which is a
        // reasonable fallback since there's no actual call site.
        self.events.push(MemEvent::FrameEnter {
            frame_id,
            fn_name: display_name.to_owned(),
            span: call_span,
        });

        // Push the frame with an outer (param) scope.
        self.frames_mut().push(Frame {
            frame_id,
            scopes: vec![Scope { locals: Vec::new(), borrows: Vec::new(), heap_allocs: Vec::new() }],
        });

        // **M07.5**: pre-build a per-call substitution map for type-param
        // substitution at SlotAlloc emission time. Without this, generic
        // params would carry `Ty::Param("T")` in SlotAlloc events; with
        // this, they carry the substituted concrete type (e.g. Ty::Int(I32)
        // for `id::<i32>(5)`).
        let call_subst: std::collections::HashMap<String, Ty> = self
            .types
            .call_substs
            .get(&call_span)
            .map(|v| v.iter().cloned().collect())
            .unwrap_or_default();
        // Emit per-param SlotAlloc + SlotWrite and push the locals.
        for (binding_id, slot_id, name, value, decl_span) in param_slots {
            let ty = self
                .lookup_var_ty(binding_id)
                .expect("param has BindingType::Var(_) after typeck");
            let ty = apply_subst_ty(&ty, &call_subst);
            // **M07.7**: implicit `&T → &dyn Trait` coercion at fn-arg site.
            // When the param type is `Ty::DynRef { trait_name, .. }` and the
            // arg is `Value::Ref { target, mutable, .. }`, intern the vtable
            // for the target's concrete type AND construct a Value::DynRef
            // inline. Same for `Ty::BoxDyn` + `Value::Box` (Box<T> → Box<dyn>).
            let value = match (&ty, &value) {
                (Ty::DynRef { trait_name, mutable }, Value::Ref { borrow_id, target, .. }) => {
                    if let Some(type_name) = self.pointee_concrete_type_name(*target) {
                        let vtable = self.intern_vtable(trait_name, &type_name, decl_span);
                        Value::DynRef {
                            borrow_id: *borrow_id,
                            target: *target,
                            vtable,
                            mutable: *mutable,
                            trait_name: trait_name.clone(),
                        }
                    } else {
                        value
                    }
                }
                (Ty::BoxDyn { trait_name }, Value::Box { addr }) => {
                    if let Some(type_name) =
                        self.pointee_concrete_type_name(crate::event::Pointee::Heap(*addr))
                    {
                        let vtable = self.intern_vtable(trait_name, &type_name, decl_span);
                        Value::BoxDyn {
                            addr: *addr,
                            vtable,
                            trait_name: trait_name.clone(),
                        }
                    } else {
                        value
                    }
                }
                _ => value,
            };
            self.events.push(MemEvent::SlotAlloc {
                slot_id,
                name: name.clone(),
                ty,
                span: decl_span,
            });
            self.events.push(MemEvent::SlotWrite {
                slot_id,
                value: value.clone(),
                span: decl_span,
            });
            let _ = name; // already in the SlotAlloc event; not needed in LocalSlot
            self.frames_mut()
                .last_mut()
                .expect("frame just pushed")
                .scopes
                .last_mut()
                .expect("scope just pushed")
                .locals
                .push(LocalSlot {
                    binding_id,
                    slot_id,
                    value,
                    decl_span,
                    moved: false,
                });
        }

        // **M06**: evaluate the body WITHOUT letting eval_block drop its scope.
        // The body's scope drop must happen AFTER ReturnValue so borrows
        // (BorrowEnd events) appear in the correct order — between
        // ReturnValue and FrameLeave, not before ReturnValue.
        let body_value = self.eval_fn_body(&decl.body);

        if self.halted {
            // Frame did not return — no ReturnValue, no FrameLeave. Stream ends
            // at the runtime-error Note already pushed by the halt path.
            return Value::Unit;
        }

        // M03.1: emit ReturnValue between body completion and scope teardown.
        // Pedagogically: the value is now visible for one cursor tick before
        // any drops fire or the frame closes.
        // M06: when the body has no tail expression (implicit unit return),
        // anchor the span at the closing `}` rather than the entire body
        // block — otherwise the editor highlight spans the whole function.
        // **M07.2**: skip ReturnValue when the function returns `()` AND
        // there's no caller frame to flash the annotation on (i.e. this
        // is the entry frame, typically `main`). In that case the
        // ReturnValue event would just produce a silent cursor step
        // between the last real action and the scope-exit drops.
        // Non-unit returns (any tail expr or non-Unit value) still fire
        // so M03's snapshots and the caller-side `→ value` annotation
        // continue to work.
        let is_entry_frame = self.frames().len() == 1;
        let skip_return = is_entry_frame
            && decl.body.tail.is_none()
            && matches!(body_value, Value::Unit);
        if !skip_return {
            let return_span = decl
                .body
                .tail
                .as_ref()
                .map(|t| t.span())
                .unwrap_or_else(|| closing_brace_span(decl.body.span));
            self.events.push(MemEvent::ReturnValue {
                frame_id,
                value: body_value.clone(),
                span: return_span,
            });
        }

        // M06: drop the body scope NOW (after ReturnValue). Emits BorrowEnd
        // and SlotDrop (M07+) events. Use the closing `}` of the body block
        // as the BorrowEnd span (1-char span at the end of the block).
        let body_close = closing_brace_span(decl.body.span);
        self.drop_current_scope(body_close);
        // Drop the param scope (LIFO). The param scope's "closing brace" is
        // the function decl's closing brace.
        self.drop_current_scope(closing_brace_span(decl.span));

        // Pop the frame and emit FrameLeave.
        let frame = self.frames_mut().pop().expect("frame still active");
        self.events.push(MemEvent::FrameLeave {
            frame_id: frame.frame_id,
            return_value: body_value.clone(),
            // M06: anchor at the closing `}` for a precise editor highlight,
            // matching ReturnValue + BorrowEnd's span treatment.
            span: closing_brace_span(decl.body.span),
        });

        body_value
    }

    fn drop_current_scope(&mut self, end_span: Span) {
        let scope = self
            .frames_mut()
            .last_mut()
            .expect("frame active")
            .scopes
            .pop()
            .expect("scope active");
        // M06: emit BorrowEnd for each borrow created in this scope, in
        // reverse-allocation order. The caller supplies the span of the
        // scope's closing brace so the editor highlights `}` not the first
        // local's declaration.
        for borrow_id in scope.borrows.into_iter().rev() {
            self.events.push(MemEvent::BorrowEnd {
                borrow_id,
                span: end_span,
            });
        }
        // **M07**: interleave Drop semantics per local, in reverse declaration
        // order. For each non-Copy local holding a heap-owning value
        // (Box/Vec/String), emit a pedagogical Info Note explaining what's
        // happening, then HeapFree, then SlotDrop. For other non-Copy locals
        // (currently none in M07), just SlotDrop. Copy locals: nothing.
        let _ = scope.heap_allocs; // tracked redundantly; iteration via locals is the source of truth.
        for local in scope.locals.into_iter().rev() {
            // **Post-M08 polish**: moved bindings skip Drop — ownership
            // has been transferred (e.g. by `move ||` capture), so the
            // new owner's scope-exit drop is responsible for the
            // resource. Without this, Arc::clone refcount math goes
            // wrong: main drops m2 → refcount-- → closure drops m2 →
            // refcount-- → double-decrement.
            if local.moved {
                continue;
            }
            let ty = self
                .lookup_var_ty(local.binding_id)
                .expect("var ty after typeck");
            // Resolve binding name for the Note.
            let name = self
                .resolution
                .bindings
                .get(&local.binding_id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| format!("slot{}", local.slot_id.0));
            // **M08**: Arc gets count-aware Drop — decrement first, only
            // emit HeapFree when count reaches 0. MutexGuard releases the
            // lock (LockRelease) WITHOUT freeing the underlying Mutex
            // allocation (that's a separate Drop on the Mutex/Arc binding).
            // Handle these BEFORE the unconditional-free heap_addr branch.
            match &local.value {
                Value::Arc { addr } => {
                    let addr = *addr;
                    let new_count = {
                        let obj = self.heap.objects.get_mut(&addr);
                        match obj {
                            Some(HeapObject::Arc { strong_count, .. }) => {
                                if *strong_count > 0 { *strong_count -= 1; }
                                *strong_count
                            }
                            // Post-M08: ArcMutex carries refcount too.
                            Some(HeapObject::ArcMutex { strong_count, .. }) => {
                                if *strong_count > 0 { *strong_count -= 1; }
                                *strong_count
                            }
                            _ => 0, // shouldn't happen
                        }
                    };
                    self.events.push(MemEvent::ArcDrop { addr, span: end_span });
                    self.refresh_arc_display(addr, end_span);
                    if new_count == 0 {
                        // Last-reference drop: explain the free.
                        self.events.push(MemEvent::Note {
                            kind: NoteKind::Info,
                            message: format!(
                                "`{name}` goes out of scope: last Arc reference dropped, refcount → 0 — heap addr #{} is freed.",
                                addr.0,
                            ),
                            span: end_span,
                        });
                        self.free_heap(addr, end_span);
                    } else {
                        // Non-final drop: explain why the heap STAYS alive.
                        self.events.push(MemEvent::Note {
                            kind: NoteKind::Info,
                            message: format!(
                                "`{name}` goes out of scope: refcount on heap #{addr_num} → {new_count} (still {new_count} owner{plural}). The heap stays alive — Drop only frees when the count reaches 0.",
                                addr_num = addr.0,
                                plural = if new_count == 1 { "" } else { "s" },
                            ),
                            span: end_span,
                        });
                    }
                    let _ = ty;
                    continue;
                }
                Value::MutexGuard { addr } => {
                    let addr = *addr;
                    // Snapshot the previous holder for the Note — it's
                    // about to be cleared by the release.
                    let prev_holder = match self.heap.objects.get(&addr) {
                        Some(HeapObject::Mutex { holder, .. })
                        | Some(HeapObject::ArcMutex { holder, .. }) => *holder,
                        _ => None,
                    };
                    // Clear the holder + emit LockRelease. No HeapFree —
                    // the mutex's underlying allocation stays alive (owned
                    // by the Mutex/Arc binding, not the guard).
                    // Post-M08: also handles fused Arc<Mutex<T>> blocks
                    // (HeapObject::ArcMutex) whose holder lives on the
                    // same heap object.
                    match self.heap.objects.get_mut(&addr) {
                        Some(HeapObject::Mutex { holder, .. }) => *holder = None,
                        Some(HeapObject::ArcMutex { holder, .. }) => *holder = None,
                        _ => {}
                    }
                    self.events.push(MemEvent::LockRelease { addr, span: end_span });
                    self.refresh_mutex_display(addr, end_span);
                    // **Post-M08 polish**: pedagogical Note explaining
                    // the unlock. The guard's Drop is what releases the
                    // lock — there's no explicit `.unlock()` in Rust;
                    // ownership of the guard going out of scope IS the
                    // release mechanism. Coalesces into the same step
                    // as LockRelease → HeapRealloc → Note via the
                    // existing heap-marker coalescer.
                    let tid_str = match prev_holder {
                        Some(tid) => format!("Thread #{}", tid.0),
                        None => "this thread".to_owned(),
                    };
                    self.events.push(MemEvent::Note {
                        kind: NoteKind::Info,
                        message: format!(
                            "`{name}` (MutexGuard) goes out of scope: Drop runs, RELEASING the lock on heap #{addr_num}. {tid_str} no longer holds it. There's no explicit `.unlock()` in Rust — the lock is released automatically when the guard drops, making it impossible to forget to unlock.",
                            addr_num = addr.0,
                        ),
                        span: end_span,
                    });
                    // **M08.2**: any threads parked on this lock can now
                    // retry. Mark them Running so the next scheduler tick
                    // picks one. (R-008: pedagogical Note next.)
                    let waiters: Vec<crate::event::ThreadId> = self.threads.iter()
                        .filter_map(|(t, ts)| match ts.status {
                            ThreadStatus::Parked { lock } if lock == addr => Some(*t),
                            _ => None,
                        }).collect();
                    self.unpark_lock_waiters(addr);
                    if !waiters.is_empty() {
                        let mut wsorted = waiters;
                        wsorted.sort_by_key(|t| t.0);
                        let waiters_str = wsorted.iter()
                            .map(|t| format!("#{}", t.0))
                            .collect::<Vec<_>>()
                            .join(", ");
                        self.events.push(MemEvent::Note {
                            kind: NoteKind::Info,
                            message: format!(
                                "Lock released; thread(s) {waiters_str} were waiting on this lock and are now eligible to re-acquire it on the next scheduler tick (seed determines which goes first when there's more than one)."
                            ),
                            span: end_span,
                        });
                    }
                    let _ = ty;
                    continue;
                }
                _ => {}
            }
            // Detect heap-owning current value (the slot's current Value).
            let heap_addr = match &local.value {
                Value::Box { addr } | Value::Vec { addr } | Value::String { addr } => Some(*addr),
                // **M07.7**: `Box<dyn Trait>` is heap-owning too. Vtable
                // persists (analog of M07.2 static memory); only the data
                // allocation gets freed at drop.
                Value::BoxDyn { addr, .. } => Some(*addr),
                // **M08**: `Mutex<T>` direct heap binding (not wrapped in Arc).
                // Same lifecycle as Box: drop frees the allocation.
                Value::Mutex { addr } => Some(*addr),
                _ => None,
            };
            if let Some(addr) = heap_addr {
                // M07: explain the Drop sequence in plain language. Fires
                // BEFORE the HeapFree event, so the cursor step where the
                // heap box disappears is preceded by an explanatory message.
                let kind_name = match &local.value {
                    Value::Box { .. } => "Box",
                    Value::Vec { .. } => "Vec",
                    Value::String { .. } => "String",
                    Value::BoxDyn { .. } => "Box<dyn>",
                    Value::Mutex { .. } => "Mutex",
                    _ => unreachable!(),
                };
                self.events.push(MemEvent::Note {
                    kind: NoteKind::Info,
                    message: format!(
                        "`{name}` goes out of scope: Drop runs on the {kind_name}, freeing heap addr {addr_num} (the stack pointer-bytes themselves persist until the frame is reused)",
                        addr_num = addr.0
                    ),
                    span: end_span,
                });
                self.free_heap(addr, end_span);
            }
            // **M07**: NO SlotDrop emission at scope exit. M03.1 already
            // established "memory persists until reused" for Copy types; the
            // same principle applies to Box/Vec/String at the STACK level —
            // the pointer-bytes physically remain on the stack until the
            // frame is reused (M03.1's frame-reuse semantics). What Drop
            // ACTUALLY does heap-side is visualized via HeapFree above; the
            // stack slot stays visible so the learner sees the now-stale
            // pointer alongside the freed heap. The frame eventually grays
            // out on FrameLeave (M03.1).
            let _ = ty;
        }
    }

    // ─── block / stmt / expr evaluation ────────────────────────────────────

    /// **M06**: evaluate a function body without dropping its scope. The
    /// caller (call_fn) is responsible for dropping the scope after emitting
    /// ReturnValue, so BorrowEnd events appear in the correct order.
    fn eval_fn_body(&mut self, block: &'a ast::Block) -> Value {
        if self.halted {
            return Value::Unit;
        }
        self.frames_mut()
            .last_mut()
            .expect("frame active")
            .scopes
            .push(Scope { locals: Vec::new(), borrows: Vec::new(), heap_allocs: Vec::new() });
        for stmt in &block.stmts {
            if self.halted {
                break;
            }
            self.eval_stmt(stmt);
        }
        if !self.halted {
            match &block.tail {
                Some(tail) => self.eval_expr(tail),
                None => Value::Unit,
            }
        } else {
            Value::Unit
        }
    }

    fn eval_block(&mut self, block: &'a ast::Block) -> Value {
        if self.halted {
            return Value::Unit;
        }
        // Push a new lexical scope for the block.
        self.frames_mut()
            .last_mut()
            .expect("frame active")
            .scopes
            .push(Scope { locals: Vec::new(), borrows: Vec::new(), heap_allocs: Vec::new() });

        for stmt in &block.stmts {
            if self.halted {
                break;
            }
            self.eval_stmt(stmt);
        }

        let tail_value = if !self.halted {
            match &block.tail {
                Some(tail) => self.eval_expr(tail),
                None => Value::Unit,
            }
        } else {
            Value::Unit
        };

        if !self.halted {
            self.drop_current_scope(closing_brace_span(block.span));
        }

        tail_value
    }

    fn eval_stmt(&mut self, stmt: &'a ast::Stmt) {
        let result = self.eval_stmt_inner(stmt);
        // **M06.1**: flush any deferred Notes (e.g. from deref-read) AFTER
        // the statement's main effects, so the explanatory message and the
        // result (e.g. SlotWrite of the new binding) land at adjacent cursor
        // steps with the result first.
        if !self.halted {
            self.flush_pending_notes();
        }
        // **M08.2**: cooperative scheduler tick — at every stmt boundary
        // in the currently-running thread, give Ready spawned threads a
        // chance to advance by one stmt. Replaces the M08 v1 "drain
        // pending_thread_runs FIFO to completion" pattern. Seed-driven
        // pick from {self, ready_spawned…} decides whether to continue
        // self or advance one spawned thread now. See research.md R-002.
        if !self.halted {
            let switch_span = match stmt {
                ast::Stmt::Let(let_stmt) => let_stmt.span,
                ast::Stmt::Expr(e) => e.span(),
                ast::Stmt::Assign { span, .. } => *span,
            };
            self.scheduler_tick(switch_span);
        }
        let _ = result;
    }

    fn eval_stmt_inner(&mut self, stmt: &'a ast::Stmt) {
        if self.halted {
            return;
        }
        match stmt {
            ast::Stmt::Let(let_stmt) => {
                let value = self.eval_expr(&let_stmt.init);
                if self.halted {
                    return;
                }
                let slot_id = self.alloc_slot_id();
                let binding_id = self.find_let_binding(let_stmt);
                let ty = self
                    .lookup_var_ty(binding_id)
                    .expect("let binding has Var type after typeck");
                // **M07.7**: implicit `&T → &dyn Trait` / `Box<T> → Box<dyn>`
                // coercion at let-init site when the annotation type forces
                // dyn but the init expression returned a regular ref/box.
                let value = match (&ty, &value) {
                    (Ty::DynRef { trait_name, mutable }, Value::Ref { borrow_id, target, .. }) => {
                        if let Some(type_name) = self.pointee_concrete_type_name(*target) {
                            let vtable = self.intern_vtable(trait_name, &type_name, let_stmt.span);
                            Value::DynRef {
                                borrow_id: *borrow_id,
                                target: *target,
                                vtable,
                                mutable: *mutable,
                                trait_name: trait_name.clone(),
                            }
                        } else {
                            value
                        }
                    }
                    (Ty::BoxDyn { trait_name }, Value::Box { addr }) => {
                        if let Some(type_name) =
                            self.pointee_concrete_type_name(crate::event::Pointee::Heap(*addr))
                        {
                            let vtable = self.intern_vtable(trait_name, &type_name, let_stmt.span);
                            Value::BoxDyn {
                                addr: *addr,
                                vtable,
                                trait_name: trait_name.clone(),
                            }
                        } else {
                            value
                        }
                    }
                    _ => value,
                };
                self.events.push(MemEvent::SlotAlloc {
                    slot_id,
                    name: let_stmt.name.clone(),
                    ty,
                    span: let_stmt.span,
                });
                self.events.push(MemEvent::SlotWrite {
                    slot_id,
                    value: value.clone(),
                    span: let_stmt.span,
                });
                self.frames_mut()
                    .last_mut()
                    .expect("frame active")
                    .scopes
                    .last_mut()
                    .expect("scope active")
                    .locals
                    .push(LocalSlot {
                        binding_id,
                        slot_id,
                        value,
                        decl_span: let_stmt.span,
                        moved: false,
                    });
            }
            ast::Stmt::Expr(expr) => {
                let _ = self.eval_expr(expr);
            }
            // **M06.1**: assignment statement. Two cases:
            //   - lhs is `Expr::Ident(x)`: direct assignment to x's slot.
            //   - lhs is `Expr::Deref(Expr::Ident(r))`: write through r to
            //     the target slot.
            // Both cases: emit `SlotWrite { slot_id, value, span }` and
            // update the in-memory LocalSlot value via `update_slot_value`.
            ast::Stmt::Assign { lhs, rhs, span } => {
                let value = self.eval_expr(rhs);
                if self.halted {
                    return;
                }
                // **M07.4**: field assignment `p.x = rhs;`. Read the
                // receiver's slot, clone its Value::Struct, mutate the
                // named field, emit a single SlotWrite with the updated
                // struct (drives the UI's struct-view refresh).
                if let ast::Expr::FieldAccess { receiver, name: field_name, .. } = lhs {
                    let recv_slot = match receiver.as_ref() {
                        ast::Expr::Ident(_, sp) => {
                            let bid = *self
                                .resolution
                                .uses
                                .get(sp)
                                .expect("ident resolved");
                            self.lookup_local_slot(bid)
                                .expect("local slot exists for field-assign receiver")
                        }
                        _ => panic!(
                            "typeck rejects non-Ident receiver in M07.4 field assign"
                        ),
                    };
                    let current = self
                        .lookup_slot_value(recv_slot)
                        .expect("receiver slot has a value");
                    let new_struct = match current {
                        Value::Struct { name: sname, mut fields } => {
                            let mut found = false;
                            for (fname, fval) in fields.iter_mut() {
                                if fname == field_name {
                                    *fval = value.clone();
                                    found = true;
                                    break;
                                }
                            }
                            assert!(found, "typeck verified field exists");
                            Value::Struct { name: sname, fields }
                        }
                        _ => panic!("typeck rejects field assign on non-struct"),
                    };
                    self.events.push(MemEvent::SlotWrite {
                        slot_id: recv_slot,
                        value: new_struct.clone(),
                        span: *span,
                    });
                    self.update_slot_value(recv_slot, new_struct);
                    return;
                }
                // For deref-write we also queue an explanatory Note (same
                // deferred-Note pattern as deref-read, so the message lands
                // at the cursor step right after the SlotWrite that the
                // learner just saw).
                let mut deref_note: Option<MemEvent> = None;
                let target_slot = match lhs {
                    ast::Expr::Ident(_, ident_span) => {
                        let binding_id = *self
                            .resolution
                            .uses
                            .get(ident_span)
                            .expect("ident resolved");
                        self.lookup_local_slot(binding_id)
                            .expect("local slot exists for assigned binding")
                    }
                    ast::Expr::Deref { inner, span: deref_span } => {
                        // Read r's Value::Ref to find the target slot.
                        let ref_value = self.eval_expr(inner);
                        if self.halted {
                            return;
                        }
                        let target_slot = match ref_value {
                            // M07: target widened from SlotId to Pointee. Through-ref
                            // assignment only valid for Slot-targeted refs in M06.1 scope.
                            // (Heap-targeted &mut writes are out of M06.1 + M07 mutation scope.)
                            Value::Ref { target: crate::event::Pointee::Slot(slot_id), mutable: true, .. } => slot_id,
                            Value::Ref { target: crate::event::Pointee::Heap(_), mutable: true, .. } => {
                                panic!("assignment through &mut heap-borrow is out of scope in M07")
                            }
                            _ => panic!(
                                "typeck should reject deref-assign through non-&mut value"
                            ),
                        };
                        // **M06.1**: pop the deref-READ note that
                        // `eval_expr(inner)` would have queued — but
                        // actually inner is the Ident (`r`), not the Deref,
                        // so no read-note was queued. We just add the
                        // write-note here.
                        let ref_name = match inner.as_ref() {
                            ast::Expr::Ident(name, _) => name.clone(),
                            _ => "reference".to_owned(),
                        };
                        let target_name = self
                            .lookup_slot_name(target_slot)
                            .unwrap_or_else(|| format!("slot{}", target_slot.0));
                        deref_note = Some(MemEvent::Note {
                            kind: NoteKind::Info,
                            message: format!(
                                "*{ref_name} writes {} to {target_name} (through `&mut {ref_name}` — `{ref_name}` itself is unchanged)",
                                render_value_for_note(&value)
                            ),
                            span: *deref_span,
                        });
                        target_slot
                    }
                    _ => panic!("typeck should reject non-place assignment lhs"),
                };
                self.events.push(MemEvent::SlotWrite {
                    slot_id: target_slot,
                    value: value.clone(),
                    span: *span,
                });
                self.update_slot_value(target_slot, value);
                if let Some(note) = deref_note {
                    self.pending_notes.push(note);
                }
            }
        }
    }

    /// **M06.1**: write `value` to the LocalSlot with `slot_id`, anywhere in
    /// the call stack. Panics if not found (typeck guarantees the slot's
    /// existence during the assignment's lifetime).
    fn update_slot_value(&mut self, slot_id: SlotId, value: Value) {
        for frame in self.frames_mut().iter_mut().rev() {
            for scope in frame.scopes.iter_mut().rev() {
                for local in scope.locals.iter_mut().rev() {
                    if local.slot_id == slot_id {
                        local.value = value;
                        return;
                    }
                }
            }
        }
        panic!("update_slot_value: slot {slot_id:?} not found in any active frame");
    }

    /// **M06.1**: read the current value at `slot_id`, anywhere in the call
    /// stack. Used by `Expr::Deref` rvalue evaluation. Returns `None` if not
    /// found.
    fn lookup_slot_value(&self, slot_id: SlotId) -> Option<Value> {
        for frame in self.frames().iter().rev() {
            for scope in frame.scopes.iter().rev() {
                for local in scope.locals.iter().rev() {
                    if local.slot_id == slot_id {
                        return Some(local.value.clone());
                    }
                }
            }
        }
        None
    }

    fn eval_expr(&mut self, expr: &'a ast::Expr) -> Value {
        if self.halted {
            return Value::Unit;
        }
        match expr {
            ast::Expr::LitInt(v, _suffix, span) => {
                // M03.2: consult typeck's recorded type for this literal —
                // it may have been coerced from the default `I32` to a
                // narrower `IntKind` by `try_coerce_to` (e.g. when this
                // literal appears as `let x: u8 = 250` or as the RHS of
                // `x: u8 + 10`).
                // **M07.4 fix**: typeck also coerces int literals to
                // `Ty::Float(_)` when an annotation says float (e.g. struct
                // field `y: f64`, `let x: f64 = 2;`). Before M07.4 this
                // path went undetected — integer literals always produced
                // `Value::Int` even when the recorded type was float —
                // because no shipped sample exercised the "int literal in
                // a float-typed context" case. Surfaced by M07.4's struct
                // field-type coercion. Honor the recorded type here:
                // recorded Ty::Float → emit Value::Float; otherwise the
                // existing Value::Int path.
                use crate::typeck::FloatKind;
                match self.types.expr_types.get(span) {
                    Some(crate::Ty::Float(k)) => {
                        // Match LitFloat's narrowing semantics: f32 round-trip
                        // so subsequent arithmetic uses the f32-narrowed bits.
                        let raw = *v as f64;
                        let value = match k {
                            FloatKind::F32 => raw as f32 as f64,
                            FloatKind::F64 => raw,
                        };
                        Value::Float { kind: *k, value }
                    }
                    Some(crate::Ty::Int(k)) => Value::Int { kind: *k, bits: *v as i128 },
                    _ => Value::Int { kind: IntKind::I32, bits: *v as i128 },
                }
            }
            ast::Expr::LitFloat(v, _suffix, span) => {
                // M03.2: same coercion path for floats — typeck may have
                // narrowed `f64` default to `f32` based on the surrounding
                // annotation. f32 narrowing of the value itself happens
                // here so subsequent arithmetic operates on f32-narrowed
                // bits.
                use crate::typeck::FloatKind;
                let kind = match self.types.expr_types.get(span) {
                    Some(crate::Ty::Float(k)) => *k,
                    _ => FloatKind::F64,
                };
                let value = match kind {
                    FloatKind::F32 => *v as f32 as f64,
                    FloatKind::F64 => *v,
                };
                Value::Float { kind, value }
            }
            ast::Expr::LitBool(b, _) => Value::Bool(*b),
            ast::Expr::Ident(_, span) => {
                let binding_id = *self
                    .resolution
                    .uses
                    .get(span)
                    .expect("ident use resolved");
                self.lookup_local_value(binding_id)
                    .expect("local exists in current frame")
            }
            ast::Expr::Unary { op, expr: inner, span } => {
                let v = self.eval_expr(inner);
                if self.halted {
                    return Value::Unit;
                }
                self.apply_unary(*op, v, *span)
            }
            ast::Expr::Binary { op, lhs, rhs, span } => self.apply_binary(*op, lhs, rhs, *span),
            ast::Expr::Call { callee, args, span } => {
                // **M07**: path-callee → dispatch to static-fn (Box::new, Vec::new, String::from).
                if let ast::Expr::Path { segments, .. } = callee.as_ref() {
                    return self.eval_path_call(segments, args, *span);
                }
                let callee_binding = match callee.as_ref() {
                    ast::Expr::Ident(_, sp) => *self
                        .resolution
                        .uses
                        .get(sp)
                        .expect("callee resolved"),
                    _ => panic!("typeck should have rejected non-Ident/non-Path callees"),
                };
                let mut arg_values = Vec::with_capacity(args.len());
                for arg in args {
                    let v = self.eval_expr(arg);
                    if self.halted {
                        return Value::Unit;
                    }
                    arg_values.push(v);
                }
                self.call_fn(callee_binding, arg_values, *span)
            }
            ast::Expr::Paren { inner, .. } => self.eval_expr(inner),
            ast::Expr::Block(block) => self.eval_block(block),
            ast::Expr::If { cond, then_block, else_block, .. } => {
                let c = self.eval_expr(cond);
                if self.halted {
                    return Value::Unit;
                }
                match c {
                    Value::Bool(true) => self.eval_block(then_block),
                    Value::Bool(false) => match else_block {
                        Some(b) => self.eval_block(b),
                        None => Value::Unit,
                    },
                    _ => panic!("typeck should have rejected non-bool conditions"),
                }
            }
            // M06: `&place` / `&mut place`. typeck guarantees inner is an Ident.
            // M07: `&v[i]` — target is the Vec's heap allocation.
            // M07.1: `&v[range]` — slice borrow; result is `Value::Slice` with
            // length metadata. Detected structurally (Index whose index is Range).
            ast::Expr::Borrow { inner, mutable, span } => {
                // M07.1: slice borrow `&v[range]` — separate path because the
                // result Value is `Value::Slice { len, .. }`, not `Value::Ref`.
                if let ast::Expr::Index { receiver, index, span: idx_span } = inner.as_ref()
                    && let ast::Expr::Range { start, end, span: range_span } = index.as_ref()
                {
                    return self.eval_slice_borrow(
                        receiver,
                        start.as_deref(),
                        end.as_deref(),
                        *mutable,
                        *idx_span,
                        *range_span,
                        *span,
                    );
                }
                // **M07.4**: field-borrow `&p.x` — target is the receiver's
                // slot, with field_path = [field_name]. NO BorrowShared event
                // is emitted (slot-target borrows use the M07.3 lazy-
                // materialization pattern — the UI materializes the arrow
                // when the resulting Value::Ref lands in a SlotWrite). The
                // borrow IS recorded in the scope's borrow list so a
                // BorrowEnd fires at scope exit (consistent with M06+).
                if let ast::Expr::FieldAccess { receiver, name: field_name, .. } = inner.as_ref()
                {
                    let recv_ident_sp = match receiver.as_ref() {
                        ast::Expr::Ident(_, sp) => *sp,
                        _ => panic!("typeck should reject non-Ident receiver in field borrow"),
                    };
                    let binding_id = *self
                        .resolution
                        .uses
                        .get(&recv_ident_sp)
                        .expect("ident resolved");
                    let slot_id = self
                        .lookup_local_slot(binding_id)
                        .expect("local slot exists for borrowed binding");
                    let borrow_id = self.alloc_borrow_id();
                    // Record the borrow_id for scope-exit cleanup (BorrowEnd
                    // is skipped though — UI's borrow lifecycle is invisible
                    // for slot-target field borrows; the scope-exit cleanup
                    // only matters for matching the M06+ borrow-tracker
                    // model). Actually: skip the scope record too since no
                    // event will fire — keeps the trace clean.
                    let _ = borrow_id;
                    return Value::Ref {
                        borrow_id,
                        target: crate::event::Pointee::Slot(slot_id),
                        mutable: *mutable,
                        field_path: vec![field_name.clone()],
                    };
                }
                // M06 + M07: determine the borrow target (Slot or Heap).
                let target = match inner.as_ref() {
                    ast::Expr::Ident(_, sp) => {
                        let binding_id = *self
                            .resolution
                            .uses
                            .get(sp)
                            .expect("ident resolved");
                        let slot_id = self
                            .lookup_local_slot(binding_id)
                            .expect("local slot exists for borrowed binding");
                        crate::event::Pointee::Slot(slot_id)
                    }
                    // **M07**: `&v[i]` (scalar index) — target is the Vec's heap allocation.
                    ast::Expr::Index { receiver, .. } => {
                        // Evaluate receiver to get its Value::Vec.
                        let recv_v = self.eval_expr(receiver);
                        if self.halted { return Value::Unit; }
                        match recv_v {
                            Value::Vec { addr } => crate::event::Pointee::Heap(addr),
                            _ => panic!("typeck should reject &(<non-Vec>[...])"),
                        }
                    }
                    _ => panic!("typeck should have rejected this place"),
                };
                let borrow_id = self.alloc_borrow_id();
                let event = if *mutable {
                    MemEvent::BorrowMut { borrow_id, target, span: *span }
                } else {
                    MemEvent::BorrowShared { borrow_id, target, span: *span }
                };
                self.events.push(event);
                self.frames_mut()
                    .last_mut()
                    .expect("frame active")
                    .scopes
                    .last_mut()
                    .expect("scope active")
                    .borrows
                    .push(borrow_id);
                // M07: snapshot the target heap's current generation at
                // borrow time. realloc_heap's dangling scan compares this
                // against the post-realloc generation.
                if let crate::event::Pointee::Heap(addr) = target {
                    let generation = self.heap_generations.get(&addr).copied().unwrap_or(0);
                    self.borrow_generations.insert(borrow_id, generation);
                }
                Value::Ref {
                    borrow_id,
                    target,
                    mutable: *mutable,
                    // **M07.4**: whole-binding borrow (not a field borrow);
                    // field_path stays empty per the M06+ default semantics.
                    field_path: Vec::new(),
                }
            }
            // **M06.1**: `*r` — read through a reference. Inner evaluates to
            // a `Value::Ref { target_slot, .. }`; we look up that slot's
            // current value and return it.
            ast::Expr::Deref { inner, span } => {
                let ref_value = self.eval_expr(inner);
                if self.halted {
                    return Value::Unit;
                }
                match ref_value {
                    // M07: `*b` where b: Box<T> reads the boxed value.
                    Value::Box { addr } => {
                        if let Some(HeapObject::Box(v)) = self.heap.objects.get(&addr) {
                            return v.clone();
                        }
                        panic!("Box's heap object missing")
                    }
                    Value::Ref { target: crate::event::Pointee::Slot(target_slot), .. } => {
                        let value = self
                            .lookup_slot_value(target_slot)
                            .expect("target slot exists during borrow's lifetime");
                        // **M06.1**: emit a pedagogical Info note explaining the
                        // deref-read. Makes the value-copy explicit (not the
                        // reference being moved). Status bar renders this so
                        // the learner sees "*r reads <v> from <x>" next to the
                        // editor's highlight of `*r`.
                        let ref_name = match inner.as_ref() {
                            ast::Expr::Ident(name, _) => name.clone(),
                            _ => "reference".to_owned(),
                        };
                        let target_name = self
                            .lookup_slot_name(target_slot)
                            .unwrap_or_else(|| format!("slot{}", target_slot.0));
                        // Defer the Note: see `pending_notes` field doc. The
                        // consuming statement's eval flushes it after its
                        // own SlotWrite so message and result land at the
                        // same cursor step.
                        self.pending_notes.push(MemEvent::Note {
                            kind: NoteKind::Info,
                            message: format!(
                                "*{ref_name} reads {} from {target_name} (copied — `{ref_name}` itself is unchanged)",
                                render_value_for_note(&value)
                            ),
                            span: *span,
                        });
                        value
                    }
                    Value::Ref { target: crate::event::Pointee::Heap(_), .. } => {
                        panic!("deref-read of heap borrow is out of M07 scope")
                    }
                    _ => panic!("typeck should reject deref of non-reference"),
                }
            }
            // **M07 → M07.2**: string literal. M07 used a transient
            // `Value::Str(String)`; M07.2 promotes literals to fat-pointer
            // slices into the static memory region (matching Rust's
            // `&'static str` semantics). The bytes are interned in the
            // static region (content-deduplicated), a fresh borrow_id is
            // allocated, BorrowShared fires with `Pointee::Static(addr)`,
            // and the returned Value::Slice covers the full literal length.
            ast::Expr::StrLit(s, span) => {
                // **M07.2**: emit StaticAlloc on first interning; allocate a
                // borrow_id; build the `Value::Slice`. We DO NOT emit a
                // `BorrowShared` event nor register in scope.borrows.
                //
                // Why: a literal slice's borrow lifecycle is invisible unless
                // the value gets bound to a slot. For transient consumption
                // (`String::from("hi")`, `push_str("!")`, etc.) the value is
                // consumed in-place; a paired BorrowShared/BorrowEnd would
                // produce two silent no-op cursor steps the learner can't
                // distinguish from each other. For let-bound use
                // (`let s = "hi"`), the UI materializes the arrow lazily in
                // apply_event's SlotWrite arm — it sees Value::Slice landing
                // in a slot with no prior borrow entry and creates one. Since
                // `Pointee::Static` borrows never go dangling, there's no
                // safety / scan reason to track them in the active-borrow
                // registry the way Heap borrows are tracked.
                let bytes_len = s.len() as u64;
                let addr = self.intern_static(s.clone(), *span);
                let target = crate::event::Pointee::Static(addr);
                let borrow_id = self.alloc_borrow_id();
                Value::Slice {
                    borrow_id,
                    target,
                    start: 0,
                    len: bytes_len,
                    mutable: false,
                    byte_offset: 0,
                    byte_len: bytes_len,
                }
            }
            ast::Expr::Path { .. } => panic!("Path expressions only valid as Call callees in M07"),
            // **M07**: method call — dispatch via eval_method_call.
            ast::Expr::MethodCall { receiver, name, args, span } => {
                self.eval_method_call(receiver, name, args, *span)
            }
            // **M07**: indexing — bounds-check + copy.
            ast::Expr::Index { receiver, index, span } => {
                self.eval_index(receiver, index, *span)
            }
            // **M07.1**: standalone range — typeck rejects this. Eval never
            // sees a Range outside of `Expr::Borrow.inner = Expr::Index { .. }`
            // (handled in the Borrow arm above).
            ast::Expr::Range { .. } => {
                panic!("typeck should have rejected standalone range expression")
            }
            // **M07.3**: array literal — eval each element, wrap into
            // `Value::Array` with the element type derived from typeck.
            ast::Expr::ArrayLit { elements, span } => {
                let elem_ty = match self.types.expr_types.get(span) {
                    Some(Ty::Array(inner, _)) => (**inner).clone(),
                    _ => Ty::Unit, // unreachable given typeck succeeded
                };
                let mut evaled = Vec::with_capacity(elements.len());
                for el in elements {
                    let v = self.eval_expr(el);
                    if self.halted { return Value::Unit; }
                    evaled.push(v);
                }
                Value::Array { elements: evaled, elem_ty }
            }
            // **M07.4**: struct literal — eval each field (resolving
            // shorthand via the local binding lookup), build
            // `Value::Struct { name, fields }` in declaration order
            // (drives byte layout and rendering).
            ast::Expr::StructLit { path, fields, span, .. } => {
                // Recover the struct's schema from typeck's recorded type
                // on this expression's span. We need the declared field
                // order (not the source order of the literal's inits).
                let schema_pairs: Vec<(String, Ty)> = match self.types.expr_types.get(span) {
                    Some(Ty::Struct { fields, .. }) => fields.clone(),
                    _ => panic!("typeck should record Ty::Struct for StructLit"),
                };
                let mut evaled_fields: Vec<(String, Value)> =
                    Vec::with_capacity(schema_pairs.len());
                for (decl_name, _decl_ty) in &schema_pairs {
                    let lit_field = fields
                        .iter()
                        .find(|f| &f.name == decl_name)
                        .expect("typeck verified all declared fields present");
                    let value = match &lit_field.value {
                        Some(expr) => self.eval_expr(expr),
                        None => {
                            // Shorthand: read the local binding of the
                            // same name (resolved at resolve-time on
                            // `lit_field.span`).
                            let bid = *self
                                .resolution
                                .uses
                                .get(&lit_field.span)
                                .expect("shorthand resolved at resolve time");
                            self.lookup_local_value(bid)
                                .expect("shorthand local has a value")
                        }
                    };
                    if self.halted { return Value::Unit; }
                    evaled_fields.push((decl_name.clone(), value));
                }
                Value::Struct {
                    name: path[0].clone(),
                    fields: evaled_fields,
                }
            }
            // **M07.7**: `inner as &dyn Trait` cast — eval inner to get
            // its Value::Ref, intern the vtable for the target's concrete
            // type, build Value::DynRef. typeck has already verified the
            // cast's validity (`T: Trait`).
            // **M08**: closure expressions never reach `eval_expr` as a
            // free-standing value — they're handled inline by
            // `eval_path_call` when seen as the `thread::spawn` argument.
            // Typeck rejects standalone closure expressions, so this arm
            // is unreachable in well-formed programs.
            ast::Expr::Closure { .. } => {
                panic!("eval_expr: standalone Expr::Closure — typeck should reject closures outside thread::spawn");
            }
            ast::Expr::Cast { inner, target_ty, span } => {
                let v = self.eval_expr(inner);
                if self.halted { return Value::Unit; }
                // Look up the target type from typeck.
                let target_ty_resolved = self.types.expr_types.get(span).cloned();
                let (trait_name, mutable) = match target_ty_resolved {
                    Some(Ty::DynRef { trait_name, mutable }) => (trait_name, mutable),
                    _ => {
                        // Defensive: typeck rejected other casts; we shouldn't get here.
                        let _ = target_ty;
                        return Value::Unit;
                    }
                };
                match v {
                    Value::Ref { borrow_id, target, mutable: ref_mut, .. } => {
                        let _ = ref_mut; // typeck-validated against target mut.
                        let type_name = self
                            .pointee_concrete_type_name(target)
                            .expect("typeck verified concrete type for cast inner");
                        let vtable = self.intern_vtable(&trait_name, &type_name, *span);
                        Value::DynRef {
                            borrow_id,
                            target,
                            vtable,
                            mutable,
                            trait_name,
                        }
                    }
                    _ => panic!("typeck should reject cast on non-Ref inner"),
                }
            }
            // **M07.4**: field access — copy the named field out of the
            // struct value. Auto-derefs through `Value::Ref` to a slot
            // holding a struct (the `self.x` shape inside `&self` methods).
            ast::Expr::FieldAccess { receiver, name, .. } => {
                let recv = self.eval_expr(receiver);
                if self.halted { return Value::Unit; }
                match recv {
                    Value::Struct { fields, .. } => fields
                        .into_iter()
                        .find_map(|(n, v)| if n == *name { Some(v) } else { None })
                        .expect("typeck verified field exists"),
                    Value::Ref {
                        target: crate::event::Pointee::Slot(slot_id),
                        field_path,
                        ..
                    } => {
                        if !field_path.is_empty() {
                            panic!(
                                "multi-level field access through a sub-field ref is out of M07.4 scope"
                            );
                        }
                        let value = self
                            .lookup_slot_value(slot_id)
                            .expect("target slot exists during borrow's lifetime");
                        match value {
                            Value::Struct { fields, .. } => fields
                                .into_iter()
                                .find_map(|(n, v)| if n == *name { Some(v) } else { None })
                                .expect("typeck verified field exists"),
                            _ => panic!("typeck should reject field access through non-struct ref"),
                        }
                    }
                    // **M07.7**: field access through a Value::Ref pointing at
                    // a heap Box. This is the `self.x` shape inside a method
                    // body dispatched via Box<dyn Trait> — self is a fresh
                    // borrow whose target is the underlying heap address;
                    // the heap holds the Box's struct value.
                    Value::Ref {
                        target: crate::event::Pointee::Heap(addr),
                        ..
                    } => {
                        let v = match self.heap.objects.get(&addr) {
                            Some(HeapObject::Box(v)) => v.clone(),
                            _ => panic!("Box's heap object missing for dyn-Box field access"),
                        };
                        match v {
                            Value::Struct { fields, .. } => fields
                                .into_iter()
                                .find_map(|(n, v)| if n == *name { Some(v) } else { None })
                                .expect("typeck verified field exists"),
                            _ => panic!("typeck should reject field access through non-struct heap ref"),
                        }
                    }
                    _ => panic!("typeck should reject field access on non-struct receiver"),
                }
            }
        }
    }

    /// **M07**: evaluate a path-callee `Box::new(v)` / `Vec::new()` / `String::from("...")`.
    fn eval_path_call(
        &mut self,
        segments: &[String],
        args: &'a [ast::Expr],
        span: Span,
    ) -> Value {
        let seg_strs: Vec<&str> = segments.iter().map(|s| s.as_str()).collect();
        match seg_strs.as_slice() {
            ["Box", "new"] => {
                let v = self.eval_expr(&args[0]);
                if self.halted { return Value::Unit; }
                let inner_ty_name = match &v {
                    Value::Int { kind, .. } => kind.name().to_owned(),
                    Value::Float { kind, .. } => kind.name().to_owned(),
                    Value::Bool(_) => "bool".to_owned(),
                    _ => "?".to_owned(),
                };
                // M07: per-type byte size (Box<f32> = 4, Box<f64> = 8, etc).
                let size = value_size_bytes(&v);
                let value_str = render_value_for_note(&v);
                let display = format!("Box<{inner_ty_name}> = {value_str}");
                let addr = self.alloc_heap(
                    HeapObject::Box(v),
                    display,
                    size,
                    span,
                );
                Value::Box { addr }
            }
            ["Vec", "new"] => {
                let elem_ty = Ty::Int(IntKind::I32);
                let initial_cap: usize = 2;
                let elem_size = ty_size_bytes(&elem_ty);
                let addr = self.alloc_heap(
                    HeapObject::Vec { elements: Vec::new(), capacity: initial_cap, elem_ty },
                    format!("Vec [] (cap={initial_cap})"),
                    initial_cap as u32 * elem_size,
                    span,
                );
                Value::Vec { addr }
            }
            ["String", "from"] => {
                // **M07 → M07.2**: arg evaluation now produces `Value::Slice`
                // targeting the static region. Extract the slice's bytes
                // (respecting its byte_offset/byte_len so sub-slices like
                // `String::from(&"hello"[..2])` copy just the visible window
                // "he", not the whole "hello").
                let (source, source_offset, s) = match self.eval_expr(&args[0]) {
                    Value::Slice {
                        target: crate::event::Pointee::Static(saddr),
                        byte_offset,
                        byte_len,
                        ..
                    } => {
                        let full = self.get_static_bytes(saddr);
                        let start = byte_offset as usize;
                        let end = start + byte_len as usize;
                        (
                            crate::event::Pointee::Static(saddr),
                            byte_offset as u32,
                            full[start..end].to_owned(),
                        )
                    }
                    _ => panic!("typeck should have rejected non-&str arg to String::from"),
                };
                let size = s.len() as u32;
                let display = format!("String \"{s}\" (cap={})", s.len());
                let addr = self.alloc_heap(
                    HeapObject::Str { bytes: s.clone(), capacity: s.len() },
                    display,
                    size,
                    span,
                );
                // **M07.2**: pedagogical Note + BytesCopy event making the
                // data flow visible. Note surfaces in the status bar; the
                // BytesCopy event drives the transient copy arrow in the
                // UI (source → fresh heap String).
                let n_bytes = s.len() as u32;
                let source_label = match source {
                    crate::event::Pointee::Static(a) => format!("static #{}", a.0),
                    crate::event::Pointee::Heap(a) => format!("heap #{}", a.0),
                    crate::event::Pointee::Slot(_) => "stack slot".to_owned(),
                };
                self.events.push(MemEvent::Note {
                    kind: NoteKind::Info,
                    message: format!(
                        "String::from copies {n_bytes} byte{} from {source_label} into a fresh heap allocation owned by the new String — the source bytes stay where they are; the String owns its own independent copy",
                        if n_bytes == 1 { "" } else { "s" },
                    ),
                    span,
                });
                self.events.push(MemEvent::BytesCopy {
                    from: source,
                    from_byte_offset: source_offset,
                    to: addr,
                    n_bytes,
                    span,
                });
                Value::String { addr }
            }
            // **M08**: `Arc::new(v)` — allocate `HeapObject::Arc` with
            // strong_count = 1. Returns `Value::Arc { addr }`.
            ["Arc", "new"] => {
                let v = self.eval_expr(&args[0]);
                if self.halted { return Value::Unit; }
                // **Post-M08 polish**: `Arc::new(Mutex::new(...))` should
                // produce ONE heap block (Arc owns the Mutex inline,
                // matching Rust's actual memory layout). Detect the
                // Value::Mutex input and FUSE by taking over the existing
                // Mutex heap block — transform its HeapObject from Mutex
                // to ArcMutex; emit HeapRealloc to update display in place.
                // Avoids the misleading "two heap blocks with a Mutex →
                // heap[N] pointer" visualization.
                if let Value::Mutex { addr: m_addr } = v {
                    // Take over the existing Mutex heap block.
                    let (inner_value, holder, waiters) = match self.heap.objects.shift_remove(&m_addr) {
                        Some(HeapObject::Mutex { value, holder, waiters }) => {
                            (value, holder, waiters)
                        }
                        _ => panic!("Arc::new(Mutex<...>): expected HeapObject::Mutex at {m_addr:?}"),
                    };
                    let inner_size = value_size_bytes(&inner_value) + 12;
                    let value_str = render_value_for_note(&inner_value);
                    let lock_suffix = match &holder {
                        Some(tid) => format!(" [locked by #{}]", tid.0),
                        None => " [free]".to_owned(),
                    };
                    let display = format!("Arc<Mutex> = {value_str} [refs: 1]{lock_suffix}");
                    self.heap.objects.insert(m_addr, HeapObject::ArcMutex {
                        value: inner_value,
                        strong_count: 1,
                        holder,
                        waiters,
                    });
                    self.events.push(MemEvent::HeapRealloc {
                        from: m_addr,
                        to: m_addr,
                        new_size: inner_size,
                        new_used: inner_size,
                        new_display: display,
                        span,
                    });
                    self.events.push(MemEvent::Note {
                        kind: NoteKind::Info,
                        message: format!(
                            "Arc::new(Mutex::new(...)) FUSES the two into one heap block: the Mutex's allocation is taken over by the Arc (refcount 1, lock free). Arc<T> stores T INLINE — `Arc<Mutex<T>>` is ONE allocation containing the inner value, the lock state, AND the refcount.",
                        ),
                        span,
                    });
                    return Value::Arc { addr: m_addr };
                }
                // Standard case: non-Mutex inner value.
                let inner_size = value_size_bytes(&v);
                let inner_ty_name = match &v {
                    Value::Int { kind, .. } => kind.name().to_owned(),
                    Value::Float { kind, .. } => kind.name().to_owned(),
                    Value::Bool(_) => "bool".to_owned(),
                    _ => "?".to_owned(),
                };
                let value_str = render_value_for_note(&v);
                let display = format!("Arc<{inner_ty_name}> = {value_str} [refs: 1]");
                let addr = self.alloc_heap(
                    HeapObject::Arc { value: Box::new(v), strong_count: 1 },
                    display,
                    inner_size + 4, // inner + 4 bytes refcount metadata
                    span,
                );
                // M08: explain the initial Arc state. The refcount of 1 = a single
                // owner; subsequent Arc::clone calls share this allocation by
                // incrementing the count rather than copying the value.
                self.events.push(MemEvent::Note {
                    kind: NoteKind::Info,
                    message: format!(
                        "Arc::new allocates {value_str} on the heap with refcount 1 — `a` is the single owner. Subsequent `Arc::clone(&a)` will SHARE this allocation (no copy) by incrementing the count.",
                    ),
                    span,
                });
                Value::Arc { addr }
            }
            // **M08**: `Arc::clone(&a)` — eval inner, expect Value::Ref
            // pointing at the Arc binding's slot; lookup the slot's
            // Value::Arc; increment refcount; emit ArcClone; return a new
            // Value::Arc carrying the SAME addr (shared ownership).
            ["Arc", "clone"] => {
                let v = self.eval_expr(&args[0]);
                if self.halted { return Value::Unit; }
                let addr = match v {
                    Value::Ref { target: crate::event::Pointee::Slot(slot_id), .. } => {
                        match self.lookup_slot_value(slot_id) {
                            Some(Value::Arc { addr }) => addr,
                            other => panic!(
                                "Arc::clone target slot does not hold Value::Arc: {other:?}"
                            ),
                        }
                    }
                    _ => panic!("Arc::clone arg must be &Arc<T>"),
                };
                // Increment refcount in the heap object. Handles both bare
                // HeapObject::Arc and fused HeapObject::ArcMutex.
                let new_count = match self.heap.objects.get_mut(&addr) {
                    Some(HeapObject::Arc { strong_count, .. }) => {
                        *strong_count += 1;
                        *strong_count
                    }
                    Some(HeapObject::ArcMutex { strong_count, .. }) => {
                        *strong_count += 1;
                        *strong_count
                    }
                    _ => panic!("Arc::clone: heap object missing or not Arc at {addr:?}"),
                };
                self.events.push(MemEvent::ArcClone { addr, span });
                // Refresh the heap object's display string with the new refcount.
                self.refresh_arc_display(addr, span);
                // M08: explain the share-by-refcount semantics. The clone
                // bumps the count rather than duplicating the value — both
                // bindings now point at the SAME heap allocation.
                self.events.push(MemEvent::Note {
                    kind: NoteKind::Info,
                    message: format!(
                        "Arc::clone bumps refcount on heap #{addr_num} from {prev} → {new_count}. The heap value is NOT copied — both Arc bindings now SHARE this allocation. Drop will only free when the count reaches 0.",
                        addr_num = addr.0,
                        prev = new_count - 1,
                    ),
                    span,
                });
                Value::Arc { addr }
            }
            // **M08**: `Mutex::new(v)` — allocate `HeapObject::Mutex` with
            // holder = None (free). Returns `Value::Mutex { addr }`.
            ["Mutex", "new"] => {
                let v = self.eval_expr(&args[0]);
                if self.halted { return Value::Unit; }
                let inner_size = value_size_bytes(&v);
                let inner_ty_name = match &v {
                    Value::Int { kind, .. } => kind.name().to_owned(),
                    Value::Float { kind, .. } => kind.name().to_owned(),
                    Value::Bool(_) => "bool".to_owned(),
                    _ => "?".to_owned(),
                };
                let value_str = render_value_for_note(&v);
                let display = format!("Mutex<{inner_ty_name}> = {value_str}");
                let addr = self.alloc_heap(
                    HeapObject::Mutex {
                        value: Box::new(v),
                        holder: None,
                        waiters: Vec::new(),
                    },
                    display,
                    inner_size + 8, // inner + 8 bytes lock-state metadata
                    span,
                );
                Value::Mutex { addr }
            }
            // **M08**: `thread::spawn(closure)` — collect captures from
            // closure_captures + current bindings, enqueue the new thread
            // with status `Ready`, emit `ThreadSpawn`. Does NOT switch.
            ["thread", "spawn"] => {
                let (is_move, body, closure_span) = match &args[0] {
                    ast::Expr::Closure { is_move, body, span } => (*is_move, body, *span),
                    _ => panic!("typeck should reject non-closure arg to thread::spawn"),
                };
                let _ = is_move; // typeck already verified is_move == true
                // Build the captures: for each BindingId in closure_captures,
                // look up its current value + type.
                let cap_bindings: Vec<BindingId> = self
                    .resolution
                    .closure_captures
                    .get(&closure_span)
                    .cloned()
                    .unwrap_or_default();
                // **Post-M08 polish**: build captures with source slot
                // ids, so `start_queued_thread` can emit the per-capture
                // `SlotMove` right after the destination's SlotAlloc/SlotWrite.
                // This pairs the source-grays transition with the
                // destination-appears transition at one logical step.
                let mut captures: Vec<(BindingId, SlotId, String, Value, Ty)> = Vec::new();
                for bid in cap_bindings {
                    let decl = self.resolution.bindings.get(&bid)
                        .expect("capture binding resolved");
                    let name = decl.name.clone();
                    let value = self.lookup_local_value(bid)
                        .expect("captured binding has a value in spawning thread");
                    let ty = self.lookup_var_ty(bid)
                        .expect("captured binding has a Ty after typeck");
                    let source_slot = self.lookup_local_slot(bid)
                        .expect("captured binding has a slot in spawning thread");
                    captures.push((bid, source_slot, name, value, ty));
                }
                let spawning_thread = self.current_thread_id;
                // Allocate a fresh ThreadId.
                let new_tid = crate::event::ThreadId(self.next_thread_id);
                self.next_thread_id += 1;
                // Insert the queued ThreadState.
                self.threads.insert(new_tid, ThreadState {
                    frames: Vec::new(),
                    status: ThreadStatus::Ready,
                    queued_body: Some(QueuedBody {
                        body,
                        captures,
                        span: closure_span,
                        spawning_thread,
                        next_stmt_idx: 0,
                        setup_done: false,
                    }),
                });
                // Emit ThreadSpawn.
                self.events.push(MemEvent::ThreadSpawn {
                    thread_id: new_tid.0,
                    span,
                });
                // **Post-M08 polish**: queue the spawned thread to run at
                // the END of the current statement, rather than inline.
                // The let-binding's own `SlotAlloc h + SlotWrite h` for
                // the returned JoinHandle then fires BEFORE the closure
                // body runs — so `h: JoinHandle(#N)` is visible in the
                // spawning thread's frame at the spawn step, matching
                // the user's mental model "the let-binding completes,
                // THEN the thread runs". `eval_stmt` drains
                // `pending_thread_runs` after each stmt completes.
                self.pending_thread_runs.push(new_tid);
                let _ = spawning_thread; // run-deferred path — no inline switch here
                // Pedagogical Note explaining the JoinHandle return.
                // Coalesces with the subsequent SlotAlloc h + SlotWrite h
                // via the existing SlotAlloc→SlotWrite + adjacent-Note
                // coalescers, so the binding appearance and the
                // explanation land at one cursor stop.
                self.events.push(MemEvent::Note {
                    kind: NoteKind::Info,
                    message: format!(
                        "`thread::spawn` returns a `JoinHandle` representing the running thread #{tid}. Calling `.join()` on it later WAITS for the thread to finish. Without `.join()`, the spawned thread runs to completion independently — the parent doesn't wait.",
                        tid = new_tid.0,
                    ),
                    span,
                });
                Value::JoinHandle { thread_id: new_tid }
            }
            _ => {
                // **M07.4**: user-defined associated function. Look up the
                // FnDecl, evaluate args, enter a new frame. No `self`.
                // **M07.5**: single-segment turbofish (`id::<bool>(false)`)
                // — look up the free-fn in fn_decls by name instead of
                // assoc_fns (multi-segment).
                let key: Vec<String> = segments.to_vec();
                let decl = if segments.len() == 1 {
                    let bid = self
                        .resolution
                        .bindings
                        .iter()
                        .find_map(|(id, d)| {
                            if matches!(d.kind, crate::resolve::BindingKind::Fn)
                                && d.name == segments[0]
                            {
                                Some(*id)
                            } else {
                                None
                            }
                        })
                        .expect("typeck verified the turbofish free-fn exists");
                    self.fn_decls[&bid]
                } else {
                    self.assoc_fns
                        .get(&key)
                        .copied()
                        .expect("typeck verified the assoc fn exists")
                };
                let mut arg_values: Vec<Value> = Vec::with_capacity(args.len());
                for arg in args {
                    let v = self.eval_expr(arg);
                    if self.halted { return Value::Unit; }
                    arg_values.push(v);
                }
                // **M07.5**: mangled name if typeck recorded a substitution
                // for this call site (generic assoc fn).
                let base = segments.join("::");
                let display = self.mangle_fn_name(&base, span);
                self.call_decl(decl, &display, arg_values, span)
            }
        }
    }

    /// **M07**: evaluate a method call (Vec::push, Vec::len, String::push_str).
    fn eval_method_call(
        &mut self,
        receiver: &'a ast::Expr,
        name: &str,
        args: &'a [ast::Expr],
        span: Span,
    ) -> Value {
        let recv = self.eval_expr(receiver);
        if self.halted { return Value::Unit; }
        match (&recv, name) {
            (Value::Vec { addr }, "push") => {
                let arg_v = self.eval_expr(&args[0]);
                if self.halted { return Value::Unit; }
                self.vec_push(*addr, arg_v, span);
                Value::Unit
            }
            // **M08**: `mutex.lock()` — acquires the lock if free, OR parks
            // the current thread + switches scheduler-side if held.
            (Value::Mutex { addr }, "lock") => {
                self.mutex_lock(*addr, span)
            }
            // **M08**: Arc auto-deref for method dispatch — `arc_of_mutex.lock()`.
            // Post-M08 fusion: when the Arc heap block IS a fused
            // ArcMutex, dispatch lock directly on the same addr (no
            // inner-addr indirection — the lock state is on the same
            // block). Fall back to the pre-fusion indirection path for
            // the legacy two-block shape.
            (Value::Arc { addr }, "lock") => {
                let target_addr = match self.heap.objects.get(addr) {
                    Some(HeapObject::ArcMutex { .. }) => *addr,
                    Some(HeapObject::Arc { value, .. }) => match value.as_ref() {
                        Value::Mutex { addr: m_addr } => *m_addr,
                        other => panic!(
                            "Arc<T>::lock requires inner Mutex; got {:?}", other
                        ),
                    },
                    _ => panic!("Value::Arc addr not found in heap"),
                };
                self.mutex_lock(target_addr, span)
            }
            // **M08**: `handle.join()` — switch to the joined thread if not
            // yet done, run it, return. Returns unit.
            (Value::JoinHandle { thread_id }, "join") => {
                let target = *thread_id;
                self.join_thread(target, span);
                Value::Unit
            }
            (Value::Vec { addr }, "len") => {
                let obj = self.heap.objects.get(addr).expect("vec exists");
                let len = if let HeapObject::Vec { elements, .. } = obj { elements.len() } else { 0 };
                Value::Int { kind: IntKind::U64, bits: len as i128 }
            }
            (Value::String { addr }, "push_str") => {
                // **M07 → M07.2**: arg evaluation produces `Value::Slice`
                // targeting the static region. Look up bytes via the slice's
                // byte_offset/byte_len so sub-slices push just their window.
                let (source, source_offset, suffix) = match self.eval_expr(&args[0]) {
                    Value::Slice {
                        target: crate::event::Pointee::Static(saddr),
                        byte_offset,
                        byte_len,
                        ..
                    } => {
                        let full = self.get_static_bytes(saddr);
                        let start = byte_offset as usize;
                        let end = start + byte_len as usize;
                        (
                            crate::event::Pointee::Static(saddr),
                            byte_offset as u32,
                            full[start..end].to_owned(),
                        )
                    }
                    _ => panic!("typeck should have rejected non-&str arg"),
                };
                let dest_addr = *addr;
                let n_bytes = suffix.len() as u32;
                // Emit pedagogical Note + BytesCopy BEFORE the actual push.
                // The push itself may trigger HeapRealloc; ordering the
                // copy explanation first matches the user's mental model
                // ("the bytes are about to flow into this allocation").
                let source_label = match source {
                    crate::event::Pointee::Static(a) => format!("static #{}", a.0),
                    crate::event::Pointee::Heap(a) => format!("heap #{}", a.0),
                    crate::event::Pointee::Slot(_) => "stack slot".to_owned(),
                };
                self.events.push(MemEvent::Note {
                    kind: NoteKind::Info,
                    message: format!(
                        "push_str copies {n_bytes} byte{} from {source_label} into heap #{} — appended to the String's existing buffer",
                        if n_bytes == 1 { "" } else { "s" },
                        dest_addr.0,
                    ),
                    span,
                });
                self.events.push(MemEvent::BytesCopy {
                    from: source,
                    from_byte_offset: source_offset,
                    to: dest_addr,
                    n_bytes,
                    span,
                });
                self.string_push_str(dest_addr, &suffix, span);
                Value::Unit
            }
            // M07.1: `Slice::len()` returns the slice's stored length as u64.
            // Distinct from the underlying Vec's length — a partial-range
            // slice `&v[1..3]` returns 2, not v.len().
            (Value::Slice { len, .. }, "len") => {
                let _ = span;
                Value::Int { kind: IntKind::U64, bits: *len as i128 }
            }
            // M07.3: `[T; N]::len()` returns N (compile-time-known size).
            (Value::Array { elements, .. }, "len") => {
                let _ = span;
                Value::Int { kind: IntKind::U64, bits: elements.len() as i128 }
            }
            // **M07.4**: fall through to user-defined methods. Look up the
            // receiver's struct name (auto-deref through &T / &mut T) and
            // dispatch via the methods table. Construct the self binding:
            //   - SelfShared: Value::Ref { target: Pointee::Slot(p), .. }
            //   - SelfMut:    Value::Ref { target: Pointee::Slot(p), mutable: true, .. }
            //   - SelfOwned:  the receiver's value, moved.
            _ => {
                // **M07.7**: trait-object receivers dispatch through the
                // vtable. Resolve the concrete type from the receiver's
                // data ptr; force the dispatch into the trait-method path
                // (no inherent fall-back — trait objects only expose trait
                // methods).
                let dyn_dispatch: Option<(String, String)> = match &recv {
                    Value::DynRef { target, trait_name, .. } => self
                        .pointee_concrete_type_name(*target)
                        .map(|tn| (trait_name.clone(), tn)),
                    Value::BoxDyn { addr, trait_name, .. } => self
                        .pointee_concrete_type_name(crate::event::Pointee::Heap(*addr))
                        .map(|tn| (trait_name.clone(), tn)),
                    _ => None,
                };
                let struct_name = match (&dyn_dispatch, &recv) {
                    (Some((_, type_name)), _) => Some(type_name.clone()),
                    (None, Value::Struct { name, .. }) => Some(name.clone()),
                    (None, Value::Ref { target: crate::event::Pointee::Slot(slot_id), .. }) => {
                        match self.lookup_slot_value(*slot_id) {
                            Some(Value::Struct { name, .. }) => Some(name),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                let Some(struct_name) = struct_name else {
                    panic!("typeck should reject method call on non-struct receiver");
                };
                // **M07.6**: three-layer dispatch (with M07.7 fourth-layer
                // overlay).
                //   1. Inherent (M07.4) — `methods[(struct, name)]`.
                //   2. Trait impl override — `trait_impl_bodies[(trait, struct, name)]`.
                //   3. Trait default — `trait_default_bodies[(trait, name)]` where
                //      `(trait, struct) ∈ trait_impl_present`.
                //   4. Trait-object dispatch (M07.7) — forces lookup into
                //      the receiver's `trait_name` only; skips inherent.
                // For each layer, also record which trait (for mangled name).
                let (method_decl, trait_label): (&ast::FnDecl, Option<String>) = if let Some((dyn_trait, dyn_type)) = &dyn_dispatch {
                    // Force the dispatch through the trait. Impl override
                    // first, then trait default.
                    if let Some(decl) = self
                        .trait_impl_bodies
                        .get(&(dyn_trait.clone(), dyn_type.clone(), name.to_owned()))
                        .copied()
                    {
                        (decl, Some(dyn_trait.clone()))
                    } else if let Some(decl) = self
                        .trait_default_bodies
                        .get(&(dyn_trait.clone(), name.to_owned()))
                        .copied()
                    {
                        (decl, Some(dyn_trait.clone()))
                    } else {
                        panic!("typeck verified trait-object method exists for ({dyn_trait}, {dyn_type}, {name})");
                    }
                } else if let Some(decl) = self
                    .methods
                    .get(&(struct_name.clone(), name.to_owned()))
                    .copied()
                {
                    (decl, None) // inherent — no `<as Trait>` in name.
                } else {
                    // Scan trait_impl_bodies first (impl overrides).
                    let override_hit: Option<(String, &ast::FnDecl)> = self
                        .trait_impl_bodies
                        .iter()
                        .find(|((_, ty, mn), _)| ty == &struct_name && mn == name)
                        .map(|((tn, _, _), d)| (tn.clone(), *d));
                    if let Some((tn, decl)) = override_hit {
                        (decl, Some(tn))
                    } else {
                        // Fall through to trait defaults.
                        let default_hit: Option<(String, &ast::FnDecl)> = self
                            .trait_default_bodies
                            .iter()
                            .find(|((tn, mn), _)| {
                                mn == name
                                    && self
                                        .trait_impl_present
                                        .contains(&(tn.clone(), struct_name.clone()))
                            })
                            .map(|((tn, _), d)| (tn.clone(), *d));
                        match default_hit {
                            Some((tn, decl)) => (decl, Some(tn)),
                            None => panic!("typeck verified method exists"),
                        }
                    }
                };
                // Determine self-receiver kind from the method's first param.
                let self_kind = method_decl
                    .params
                    .first()
                    .map(|p| p.kind)
                    .unwrap_or(ast::ParamKind::Normal);
                // Build the self value. Two cases for the borrow kinds:
                //   - Receiver is a `Value::Struct` (direct binding like
                //     `p.method()`): construct a fresh borrow of the
                //     receiver binding's slot.
                //   - Receiver is already a `Value::Ref` (e.g. inside a
                //     method body calling `self.other_method()`, or a
                //     binding like `r: &Point`): reuse the existing borrow
                //     value directly — matches Rust's auto-deref
                //     semantics ("&self gets whatever borrow you already
                //     hold").
                let self_value = match self_kind {
                    ast::ParamKind::SelfOwned => recv.clone(),
                    ast::ParamKind::SelfShared | ast::ParamKind::SelfMut => {
                        match &recv {
                            // **M07.6 fix**: when reusing an existing Value::Ref
                            // (e.g. nested `self.other()` inside a method or
                            // default body), allocate a FRESH borrow_id so the
                            // UI's lazy-materialization creates a distinct
                            // ActiveBorrowState entry per call-site self
                            // binding. Without this, both bindings share the
                            // same borrow_id; only the first one materializes,
                            // and the second's arrow is missing. Matches
                            // Rust's actual semantics: each call site is a
                            // fresh borrow of the same data.
                            Value::Ref { target, mutable, field_path, .. } => {
                                let borrow_id = self.alloc_borrow_id();
                                Value::Ref {
                                    borrow_id,
                                    target: *target,
                                    mutable: *mutable,
                                    field_path: field_path.clone(),
                                }
                            }
                            Value::Struct { .. } => {
                                let recv_slot = match receiver {
                                    ast::Expr::Ident(_, sp) => {
                                        let bid = *self
                                            .resolution
                                            .uses
                                            .get(sp)
                                            .expect("ident resolved");
                                        self.lookup_local_slot(bid)
                                            .expect("local slot exists for method receiver")
                                    }
                                    _ => panic!(
                                        "M07.4 method receivers on a struct value must be a direct binding ident"
                                    ),
                                };
                                let mutable = matches!(self_kind, ast::ParamKind::SelfMut);
                                let borrow_id = self.alloc_borrow_id();
                                Value::Ref {
                                    borrow_id,
                                    target: crate::event::Pointee::Slot(recv_slot),
                                    mutable,
                                    field_path: Vec::new(),
                                }
                            }
                            // **M07.7**: trait-object receivers dispatch via
                            // the vtable; the impl's self-receiver gets a
                            // fresh borrow targeting the dyn-ref's data ptr.
                            // Fresh borrow_id per call site (per M07.6 lesson).
                            Value::DynRef { target, mutable, .. } => {
                                let borrow_id = self.alloc_borrow_id();
                                Value::Ref {
                                    borrow_id,
                                    target: *target,
                                    mutable: *mutable,
                                    field_path: Vec::new(),
                                }
                            }
                            // **M07.7**: Box<dyn Trait> receivers — self
                            // is a fresh borrow of the heap data ptr.
                            Value::BoxDyn { addr, .. } => {
                                let mutable = matches!(self_kind, ast::ParamKind::SelfMut);
                                let borrow_id = self.alloc_borrow_id();
                                Value::Ref {
                                    borrow_id,
                                    target: crate::event::Pointee::Heap(*addr),
                                    mutable,
                                    field_path: Vec::new(),
                                }
                            }
                            _ => panic!("typeck rejects non-struct/non-ref method receivers"),
                        }
                    }
                    ast::ParamKind::Normal => unreachable!("typeck guaranteed self-receiver"),
                };
                // Evaluate explicit args (in source order, after self).
                let mut arg_values: Vec<Value> = Vec::with_capacity(args.len() + 1);
                arg_values.push(self_value);
                for arg in args {
                    let v = self.eval_expr(arg);
                    if self.halted { return Value::Unit; }
                    arg_values.push(v);
                }
                // **M07.5/M07.6**: mangled name. Trait dispatch uses
                // UFCS-style `<Point as Show>::show`; inherent uses
                // `Point::show`.
                let base = match &trait_label {
                    Some(tn) => format!("<{struct_name} as {tn}>::{name}"),
                    None => format!("{struct_name}::{name}"),
                };
                let display = self.mangle_fn_name(&base, span);
                self.call_decl(method_decl, &display, arg_values, span)
            }
        }
    }

    /// **M07**: Vec::push helper. Two cases:
    ///
    /// - **In-place** (capacity sufficient): just update contents; emit
    ///   one HeapRealloc {from==to} carrying the new display.
    ///
    /// - **Cap exceeded** (real realloc): emit a three-event sequence so
    ///   the pedagogy is visible step by step:
    ///   1. **Info Note** — "capacity exceeded, will copy."
    ///   2. **HeapRealloc** with `from=old → to=new` and **old contents** at
    ///      the new addr (capacity raised, push not done yet). This event
    ///      represents the alloc-and-copy step alone.
    ///   3. **HeapRealloc** with `from=new == to=new` and **new contents**
    ///      (the actual push performed on the freshly-allocated buffer).
    fn vec_push(&mut self, addr: crate::event::HeapAddr, value: Value, span: Span) {
        let (cur_cap, cur_len, elem_ty, old_elements) = match self.heap.objects.get(&addr) {
            Some(HeapObject::Vec { capacity, elements, elem_ty }) =>
                (*capacity, elements.len(), elem_ty.clone(), elements.clone()),
            _ => panic!("vec_push on non-Vec heap object"),
        };
        if cur_len + 1 > cur_cap {
            let new_cap = if cur_cap == 0 { 1 } else { cur_cap * 2 };
            // **M07.1**: in-place growth when the block has room to extend
            // (heuristic: this is the last live block in the heap, so nothing
            // physically blocks growth into the adjacent region). Same addr,
            // larger capacity, **no copy** — matches what a real allocator's
            // `realloc()` does when it can grow in place. Crucially: borrows
            // into the block remain valid because the data didn't move.
            let can_grow_in_place = self
                .heap
                .objects
                .keys()
                .position(|a| *a == addr)
                .map(|i| i == self.heap.objects.len() - 1)
                .unwrap_or(false);
            if can_grow_in_place {
                // ── Phase 1: announce in-place growth ─────────────────────
                self.events.push(MemEvent::Note {
                    kind: NoteKind::Info,
                    message: format!(
                        "Vec capacity exceeded ({cur_cap} → {new_cap}); growing in place at heap #{addr_n} (no copy, borrows remain valid)",
                        addr_n = addr.0,
                    ),
                    span,
                });
                // ── Phase 2: extend capacity in place, then push ──────────
                if let Some(HeapObject::Vec { capacity, elements, .. }) = self.heap.objects.get_mut(&addr) {
                    *capacity = new_cap;
                    elements.push(value);
                }
                let (total, used) = self.heap.objects.get(&addr)
                    .map(heap_object_bytes).unwrap_or((0, 0));
                let new_display = self.heap.objects.get(&addr)
                    .map(heap_object_display)
                    .unwrap_or_default();
                // Emit a single HeapRealloc with from==to carrying the new
                // size + display. The renderer treats from==to as "update
                // this block in place" — same path used for non-cap-changing
                // pushes — so the byte-cells expand to fill the new capacity.
                self.events.push(MemEvent::HeapRealloc {
                    from: addr,
                    to: addr,
                    new_size: total,
                    new_used: used,
                    new_display,
                    span,
                });
                let _ = old_elements;
                let _ = elem_ty;
                return;
            }
            // ── copy-realloc path ─────────────────────────────────────────
            // Phase 1: announce the realloc (with the names of blocks that
            // are forcing the copy).
            let other_blocks: Vec<u32> = self.heap.objects.keys()
                .filter(|a| **a != addr)
                .map(|a| a.0)
                .collect();
            let others_str = other_blocks.iter().map(|a| format!("#{a}")).collect::<Vec<_>>().join(", ");
            let pre_msg = format!(
                "Vec capacity exceeded ({cur_cap} → {new_cap}); cannot grow in place because heap blocks [{others_str}] occupy the adjacent region — allocator will copy the bytes to a fresh location and free the old block"
            );
            self.events.push(MemEvent::Note {
                kind: NoteKind::Info,
                message: pre_msg,
                span,
            });
            // Phase 2: realloc (alloc new + copy old contents, free old).
            // The new block holds the OLD elements at the NEW capacity. The
            // actual push hasn't happened yet — that's phase 3.
            let copy_obj = HeapObject::Vec {
                elements: old_elements,
                capacity: new_cap,
                elem_ty: elem_ty.clone(),
            };
            let new_size = (new_cap * 4) as u32;
            let new_addr = self.realloc_heap(addr, copy_obj, new_size, span);
            // Phase 3: the actual push, in place on the new buffer.
            if let Some(HeapObject::Vec { elements, .. }) = self.heap.objects.get_mut(&new_addr) {
                elements.push(value);
            }
            let (push_total, push_used) = self.heap.objects.get(&new_addr)
                .map(heap_object_bytes).unwrap_or((0, 0));
            let push_display = self.heap.objects.get(&new_addr)
                .map(heap_object_display)
                .unwrap_or_default();
            self.events.push(MemEvent::HeapRealloc {
                from: new_addr,
                to: new_addr,
                new_size: push_total,
                new_used: push_used,
                new_display: push_display,
                span,
            });
        } else {
            // In-place push: just update contents and emit HeapRealloc with
            // from==to carrying the new display.
            if let Some(HeapObject::Vec { elements, .. }) = self.heap.objects.get_mut(&addr) {
                elements.push(value);
            }
            let (total, used) = self.heap.objects.get(&addr)
                .map(heap_object_bytes).unwrap_or((0, 0));
            let new_display = self.heap.objects.get(&addr)
                .map(heap_object_display)
                .unwrap_or_default();
            self.events.push(MemEvent::HeapRealloc {
                from: addr,
                to: addr,
                new_size: total,
                new_used: used,
                new_display,
                span,
            });
        }
    }

    /// **M07**: String::push_str helper. Doubles capacity on overflow.
    /// Same stable-addr model as Vec::push.
    fn string_push_str(&mut self, addr: crate::event::HeapAddr, suffix: &str, span: Span) {
        let (cur_cap, cur_len) = match self.heap.objects.get(&addr) {
            Some(HeapObject::Str { capacity, bytes }) => (*capacity, bytes.len()),
            _ => panic!("string_push_str on non-Str heap object"),
        };
        let needed = cur_len + suffix.len();
        if needed > cur_cap {
            let mut new_cap = if cur_cap == 0 { 1 } else { cur_cap * 2 };
            while new_cap < needed { new_cap *= 2; }
            // **M07.1**: in-place growth when nothing physically blocks it
            // (same heuristic as vec_push: this is the last live block).
            let can_grow_in_place = self
                .heap
                .objects
                .keys()
                .position(|a| *a == addr)
                .map(|i| i == self.heap.objects.len() - 1)
                .unwrap_or(false);
            if can_grow_in_place {
                self.events.push(MemEvent::Note {
                    kind: NoteKind::Info,
                    message: format!(
                        "String capacity exceeded ({cur_cap} → {new_cap}); growing in place at heap #{addr_n} (no copy, borrows remain valid)",
                        addr_n = addr.0,
                    ),
                    span,
                });
                if let Some(HeapObject::Str { bytes, capacity }) = self.heap.objects.get_mut(&addr) {
                    bytes.push_str(suffix);
                    *capacity = new_cap;
                }
                let (total, used) = self.heap.objects.get(&addr)
                    .map(heap_object_bytes).unwrap_or((0, 0));
                let new_display = self.heap.objects.get(&addr)
                    .map(heap_object_display)
                    .unwrap_or_default();
                self.events.push(MemEvent::HeapRealloc {
                    from: addr,
                    to: addr,
                    new_size: total,
                    new_used: used,
                    new_display,
                    span,
                });
                return;
            }
            let new_bytes = match self.heap.objects.get(&addr) {
                Some(HeapObject::Str { bytes, .. }) => {
                    let mut b = bytes.clone();
                    b.push_str(suffix);
                    b
                }
                _ => unreachable!(),
            };
            let new_size = new_cap as u32;
            let new_obj = HeapObject::Str { bytes: new_bytes, capacity: new_cap };
            self.realloc_heap(addr, new_obj, new_size, span);
        } else {
            // In-place: append + emit display update via from==to HeapRealloc.
            if let Some(HeapObject::Str { bytes, .. }) = self.heap.objects.get_mut(&addr) {
                bytes.push_str(suffix);
            }
            let (total, used) = self.heap.objects.get(&addr)
                .map(heap_object_bytes).unwrap_or((0, 0));
            let new_display = self.heap.objects.get(&addr)
                .map(heap_object_display)
                .unwrap_or_default();
            self.events.push(MemEvent::HeapRealloc {
                from: addr,
                to: addr,
                new_size: total,
                new_used: used,
                new_display,
                span,
            });
        }
    }

    /// **M07**: evaluate `receiver[index]`. Receiver must be a Vec; index any Int.
    /// Returns a copy of the element (bounds-checked).
    /// **M07.1**: evaluate `&v[start..end]` (or any of the four range forms).
    /// Bounds-checks the range, emits a `BorrowShared` event targeting the
    /// Vec's heap allocation, registers the borrow (so M07's realloc-time
    /// dangling scan catches it), and returns `Value::Slice { len, .. }`.
    /// `mutable` is always rejected at typeck for M07.1; we keep the parameter
    /// for forward-compat with M07.x mutable slices.
    #[allow(clippy::too_many_arguments)]
    fn eval_slice_borrow(
        &mut self,
        receiver: &'a ast::Expr,
        start: Option<&'a ast::Expr>,
        end: Option<&'a ast::Expr>,
        mutable: bool,
        idx_span: Span,
        _range_span: Span,
        borrow_span: Span,
    ) -> Value {
        // **M07.2**: receiver may be a Vec (M07.1's case) OR an existing
        // slice/&str (sub-slicing). For sub-slicing, the result inherits the
        // receiver's `target` (Static for &str, Heap for &[T]) and offsets
        // are computed relative to the receiver's existing window.
        let recv = self.eval_expr(receiver);
        if self.halted { return Value::Unit; }
        // Get the receiver's element size from typeck (covers the
        // len=0-but-non-byte-element corner case where deriving via
        // byte_len/len would fail).
        let elem_size: u64 = match self.types.expr_types.get(&receiver.span()) {
            Some(Ty::Vec(inner)) | Some(Ty::Slice(inner)) | Some(Ty::Array(inner, _)) => ty_size_bytes(inner) as u64,
            Some(Ty::Str) => 1,
            _ => panic!("typeck should have recorded a sliceable receiver type"),
        };
        // **M07.3**: for Array receivers, the slice target is the
        // receiver's STACK SLOT (Pointee::Slot). Derive the slot from
        // the receiver's Expr::Ident *before* evaluating, since eval
        // returns a Value::Array but loses the slot identity.
        let array_receiver_slot: Option<SlotId> = if let ast::Expr::Ident(_, ident_span) = receiver {
            self.resolution
                .uses
                .get(ident_span)
                .copied()
                .and_then(|binding_id| self.lookup_local_slot(binding_id))
        } else {
            None
        };
        let (target, base_byte_offset, base_len): (crate::event::Pointee, u64, i128) = match recv {
            Value::Vec { addr } => {
                let obj = self.heap.objects.get(&addr).expect("vec exists");
                if let HeapObject::Vec { elements, .. } = obj {
                    (crate::event::Pointee::Heap(addr), 0, elements.len() as i128)
                } else {
                    panic!("slice of non-Vec heap object")
                }
            }
            Value::Slice { target, byte_offset, len, .. } => {
                // Sub-slice: result inherits the receiver's `target` and
                // its existing byte_offset; the new range is interpreted
                // relative to the receiver's window (length = len).
                (target, byte_offset, len as i128)
            }
            Value::Array { elements, .. } => {
                // M07.3: slot-target slice into a stack-allocated array.
                let slot_id = array_receiver_slot
                    .expect("slicing an array requires an Expr::Ident receiver in M07.3");
                (crate::event::Pointee::Slot(slot_id), 0, elements.len() as i128)
            }
            _ => panic!("typeck should reject slice of non-sliceable value"),
        };
        // Evaluate bounds. Defaults: start=0, end=base_len.
        let start_i: i128 = if let Some(e) = start {
            let v = self.eval_expr(e);
            if self.halted { return Value::Unit; }
            match v {
                Value::Int { bits, .. } => bits,
                _ => panic!("typeck should reject non-Int range bound"),
            }
        } else { 0 };
        let end_i: i128 = if let Some(e) = end {
            let v = self.eval_expr(e);
            if self.halted { return Value::Unit; }
            match v {
                Value::Int { bits, .. } => bits,
                _ => panic!("typeck should reject non-Int range bound"),
            }
        } else { base_len };
        // Bounds-check. "vec len" wording kept for the Vec case; "slice len"
        // for the sub-slice case.
        let bound_name = if matches!(target, crate::event::Pointee::Heap(_))
            && base_byte_offset == 0 { "vec len" } else { "slice len" };
        if start_i < 0 || start_i > base_len {
            self.emit_runtime_error(
                format!(
                    "slice start out of bounds: start is {start_i}, {bound_name} is {base_len}"
                ),
                idx_span,
            );
            return Value::Unit;
        }
        if end_i < 0 || end_i > base_len {
            self.emit_runtime_error(
                format!(
                    "slice end out of bounds: end is {end_i}, {bound_name} is {base_len}"
                ),
                idx_span,
            );
            return Value::Unit;
        }
        if start_i > end_i {
            self.emit_runtime_error(
                format!(
                    "slice start > end: start is {start_i}, end is {end_i}"
                ),
                idx_span,
            );
            return Value::Unit;
        }
        // Allocate a borrow_id, emit BorrowShared targeting the receiver's
        // memory region (Heap or Static — Slot reserved for M07.3 arrays).
        // mutable=false enforced (typeck rejects mutable slices in M07.1).
        //
        // **M07.2**: skip the BorrowShared/scope-registration for Static
        // targets — same reasoning as the `Expr::StrLit` arm: the UI
        // materializes a static-target arrow lazily at SlotWrite time, so
        // a paired BorrowShared/BorrowEnd would just produce silent no-op
        // cursor steps for any transient sub-slice consumed in a call.
        let _ = mutable; // forward-compat marker
        let borrow_id = self.alloc_borrow_id();
        // **M07.2 / M07.3**: skip BorrowShared/BorrowEnd lifecycle for
        // Static AND Slot targets (frames + static memory both
        // disappear atomically; scope-exit BorrowEnd would be silent).
        // UI materializes the arrow lazily at SlotWrite time. Only Heap
        // targets need the explicit lifecycle (dangling-detection
        // depends on tracking heap borrows).
        let skip_borrow_events = matches!(
            target,
            crate::event::Pointee::Static(_) | crate::event::Pointee::Slot(_)
        );
        if !skip_borrow_events {
            self.events.push(MemEvent::BorrowShared {
                borrow_id,
                target,
                span: borrow_span,
            });
            self.frames_mut()
                .last_mut()
                .expect("frame active")
                .scopes
                .last_mut()
                .expect("scope active")
                .borrows
                .push(borrow_id);
        }
        // Snapshot heap generation for the dangling-borrow detection scan
        // (only meaningful for Heap targets; Static never goes dangling).
        if let crate::event::Pointee::Heap(addr) = target {
            let generation = self.heap_generations.get(&addr).copied().unwrap_or(0);
            self.borrow_generations.insert(borrow_id, generation);
        }
        let len = (end_i - start_i) as u64;
        let start = start_i as u64;
        Value::Slice {
            borrow_id,
            target,
            start,
            len,
            mutable: false,
            // Byte offset accumulates: sub-slicing adds to the receiver's
            // existing byte_offset (e.g. &"hello"[1..][..2] picks bytes 1-3).
            byte_offset: base_byte_offset + start * elem_size,
            byte_len: len * elem_size,
        }
    }

    fn eval_index(&mut self, receiver: &'a ast::Expr, index: &'a ast::Expr, span: Span) -> Value {
        let recv = self.eval_expr(receiver);
        if self.halted { return Value::Unit; }
        let idx_v = self.eval_expr(index);
        if self.halted { return Value::Unit; }
        let i = match idx_v {
            Value::Int { bits, .. } => bits,
            _ => panic!("typeck should reject non-Int index"),
        };
        // **M07.3**: handle Array receiver inline (elements held in the
        // Value itself, not in heap state).
        if let Value::Array { elements, .. } = recv {
            if i < 0 || (i as usize) >= elements.len() {
                self.emit_runtime_error(
                    format!("index out of bounds: array len is {} but the index is {}", elements.len(), i),
                    span,
                );
                return Value::Unit;
            }
            return elements[i as usize].clone();
        }
        let addr = match recv {
            Value::Vec { addr } => addr,
            _ => panic!("typeck should reject non-Vec/non-Array index"),
        };
        let obj = self.heap.objects.get(&addr).expect("vec exists");
        let elements = if let HeapObject::Vec { elements, .. } = obj { elements } else {
            panic!("index on non-Vec")
        };
        if i < 0 || (i as usize) >= elements.len() {
            self.emit_runtime_error(
                format!("index out of bounds: the len is {} but the index is {}", elements.len(), i),
                span,
            );
            return Value::Unit;
        }
        elements[i as usize].clone()
    }

    /// **M06.1**: look up the binding name of the slot with `slot_id`.
    fn lookup_slot_name(&self, slot_id: SlotId) -> Option<String> {
        for frame in self.frames().iter().rev() {
            for scope in frame.scopes.iter().rev() {
                for local in &scope.locals {
                    if local.slot_id == slot_id {
                        return self.resolution.bindings.get(&local.binding_id).map(|d| d.name.clone());
                    }
                }
            }
        }
        None
    }

    /// **M06**: look up the SlotId of the local holding `binding_id`.
    fn lookup_local_slot(&self, binding_id: BindingId) -> Option<SlotId> {
        for frame in self.frames().iter().rev() {
            for scope in frame.scopes.iter().rev() {
                for local in scope.locals.iter().rev() {
                    if local.binding_id == binding_id {
                        return Some(local.slot_id);
                    }
                }
            }
        }
        None
    }

    fn apply_unary(&mut self, op: ast::UnOp, v: Value, span: Span) -> Value {
        match (op, v) {
            // M03.2: unary `-` on any signed-integer kind. typeck rejected
            // unsigned negation already.
            (ast::UnOp::Neg, Value::Int { kind, bits }) => {
                self.int_checked(kind, bits.checked_neg(), span, "unary `-`")
            }
            (ast::UnOp::Not, Value::Bool(b)) => Value::Bool(!b),
            _ => panic!("typeck should have rejected this unary application"),
        }
    }

    fn apply_binary(
        &mut self,
        op: ast::BinOp,
        lhs_expr: &'a ast::Expr,
        rhs_expr: &'a ast::Expr,
        span: Span,
    ) -> Value {
        use ast::BinOp::*;
        // Evaluate LHS first.
        let lhs_v = self.eval_expr(lhs_expr);
        if self.halted {
            return Value::Unit;
        }

        // Short-circuit `&&` / `||`.
        match op {
            And => {
                if matches!(lhs_v, Value::Bool(false)) {
                    return Value::Bool(false);
                }
            }
            Or => {
                if matches!(lhs_v, Value::Bool(true)) {
                    return Value::Bool(true);
                }
            }
            _ => {}
        }

        let rhs_v = self.eval_expr(rhs_expr);
        if self.halted {
            return Value::Unit;
        }

        match (op, lhs_v, rhs_v) {
            // M03.2: integer arithmetic — dispatched over any IntKind that
            // matches between the two operands (typeck guarantees the kinds
            // agree). i128 checked_op handles wide-storage overflow; the
            // kind.contains() gate enforces the actual type's range.
            (Add, Value::Int { kind: a_k, bits: a }, Value::Int { kind: b_k, bits: b })
                if a_k == b_k =>
            {
                self.int_checked(a_k, a.checked_add(b), span, "add")
            }
            (Sub, Value::Int { kind: a_k, bits: a }, Value::Int { kind: b_k, bits: b })
                if a_k == b_k =>
            {
                self.int_checked(a_k, a.checked_sub(b), span, "subtract")
            }
            (Mul, Value::Int { kind: a_k, bits: a }, Value::Int { kind: b_k, bits: b })
                if a_k == b_k =>
            {
                self.int_checked(a_k, a.checked_mul(b), span, "multiply")
            }
            (Div, Value::Int { kind: a_k, .. }, Value::Int { kind: b_k, bits: 0 })
                if a_k == b_k =>
            {
                self.emit_runtime_error("division by zero".into(), span);
                Value::Unit
            }
            (Div, Value::Int { kind: a_k, bits: a }, Value::Int { kind: b_k, bits: b })
                if a_k == b_k =>
            {
                self.int_checked(a_k, a.checked_div(b), span, "divide")
            }
            (Rem, Value::Int { kind: a_k, .. }, Value::Int { kind: b_k, bits: 0 })
                if a_k == b_k =>
            {
                self.emit_runtime_error("remainder by zero".into(), span);
                Value::Unit
            }
            (Rem, Value::Int { kind: a_k, bits: a }, Value::Int { kind: b_k, bits: b })
                if a_k == b_k =>
            {
                self.int_checked(a_k, a.checked_rem(b), span, "remainder")
            }
            (Lt, Value::Int { kind: a_k, bits: a }, Value::Int { kind: b_k, bits: b })
                if a_k == b_k => Value::Bool(a < b),
            (Le, Value::Int { kind: a_k, bits: a }, Value::Int { kind: b_k, bits: b })
                if a_k == b_k => Value::Bool(a <= b),
            (Gt, Value::Int { kind: a_k, bits: a }, Value::Int { kind: b_k, bits: b })
                if a_k == b_k => Value::Bool(a > b),
            (Ge, Value::Int { kind: a_k, bits: a }, Value::Int { kind: b_k, bits: b })
                if a_k == b_k => Value::Bool(a >= b),
            (Eq, Value::Int { kind: a_k, bits: a }, Value::Int { kind: b_k, bits: b })
                if a_k == b_k => Value::Bool(a == b),
            (Eq, Value::Bool(a), Value::Bool(b)) => Value::Bool(a == b),
            (Neq, Value::Int { kind: a_k, bits: a }, Value::Int { kind: b_k, bits: b })
                if a_k == b_k => Value::Bool(a != b),
            (Neq, Value::Bool(a), Value::Bool(b)) => Value::Bool(a != b),

            // M03.2: float arithmetic. f64 ops never panic; results may be
            // NaN or ±Inf. The `float_arith` helper emits a `Note { Info }`
            // when an operation produces a special value de novo (i.e.
            // neither operand was already special).
            (Add, Value::Float { kind: a_k, value: a }, Value::Float { kind: b_k, value: b })
                if a_k == b_k =>
            {
                self.float_arith(a_k, a, b, |x, y| x + y, span)
            }
            (Sub, Value::Float { kind: a_k, value: a }, Value::Float { kind: b_k, value: b })
                if a_k == b_k =>
            {
                self.float_arith(a_k, a, b, |x, y| x - y, span)
            }
            (Mul, Value::Float { kind: a_k, value: a }, Value::Float { kind: b_k, value: b })
                if a_k == b_k =>
            {
                self.float_arith(a_k, a, b, |x, y| x * y, span)
            }
            (Div, Value::Float { kind: a_k, value: a }, Value::Float { kind: b_k, value: b })
                if a_k == b_k =>
            {
                self.float_arith(a_k, a, b, |x, y| x / y, span)
            }
            (Rem, Value::Float { kind: a_k, value: a }, Value::Float { kind: b_k, value: b })
                if a_k == b_k =>
            {
                self.float_arith(a_k, a, b, |x, y| x % y, span)
            }
            (Lt, Value::Float { kind: a_k, value: a }, Value::Float { kind: b_k, value: b })
                if a_k == b_k => Value::Bool(a < b),
            (Le, Value::Float { kind: a_k, value: a }, Value::Float { kind: b_k, value: b })
                if a_k == b_k => Value::Bool(a <= b),
            (Gt, Value::Float { kind: a_k, value: a }, Value::Float { kind: b_k, value: b })
                if a_k == b_k => Value::Bool(a > b),
            (Ge, Value::Float { kind: a_k, value: a }, Value::Float { kind: b_k, value: b })
                if a_k == b_k => Value::Bool(a >= b),
            (Eq, Value::Float { kind: a_k, value: a }, Value::Float { kind: b_k, value: b })
                if a_k == b_k => Value::Bool(a == b),
            (Neq, Value::Float { kind: a_k, value: a }, Value::Float { kind: b_k, value: b })
                if a_k == b_k => Value::Bool(a != b),
            (And, Value::Bool(_), Value::Bool(b)) => Value::Bool(b),
            (Or, Value::Bool(_), Value::Bool(b)) => Value::Bool(b),
            (op, lhs, rhs) => panic!(
                "typeck should reject this: {op:?} on {} and {}",
                lhs.type_name(),
                rhs.type_name()
            ),
        }
    }

    /// M03.2: range-check `result` against `kind`'s value range. On None
    /// (i128 itself overflowed) or out-of-range, halt with a RuntimeError
    /// note pointing at the op span.
    fn int_checked(&mut self, kind: IntKind, result: Option<i128>, span: Span, op_name: &str) -> Value {
        match result {
            Some(v) if kind.contains(v) => Value::Int { kind, bits: v },
            _ => {
                self.emit_runtime_error(
                    format!("{} overflow in {}", kind.name(), op_name),
                    span,
                );
                Value::Unit
            }
        }
    }

    /// M03.2: dispatch a float arithmetic op. Computes in f64 (with f32
    /// narrowing after the op when `kind == F32` so f32-range overflow
    /// surfaces). When the result is NaN/Inf de novo (neither operand was
    /// already special), emits a `Note { Info }` describing the special
    /// value. Trace does NOT halt — Inf/NaN are valid Rust.
    fn float_arith<F>(&mut self, kind: crate::typeck::FloatKind, a: f64, b: f64, op: F, span: Span) -> Value
    where
        F: Fn(f64, f64) -> f64,
    {
        use crate::typeck::FloatKind;
        let was_special = !a.is_finite() || !b.is_finite();
        let raw = op(a, b);
        let result = match kind {
            FloatKind::F32 => raw as f32 as f64,
            FloatKind::F64 => raw,
        };
        let now_special = result.is_nan() || result.is_infinite();
        if now_special && !was_special {
            let classify = if result.is_nan() {
                "NaN"
            } else if result > 0.0 {
                "+Inf"
            } else {
                "-Inf"
            };
            self.events.push(MemEvent::Note {
                kind: NoteKind::Info,
                message: format!("produced {} ({})", classify, kind.name()),
                span,
            });
        }
        Value::Float { kind, value: result }
    }
}

/// **M07**: render a heap object's content for display (used in heap panel
/// labels and realloc Info notes). Vec/String include empty-slot
/// placeholders up to capacity so the difference between used and
/// allocated bytes is visible — important pedagogy when freed heap blocks
/// get reused for a smaller allocation.
fn heap_object_display(obj: &HeapObject) -> String {
    match obj {
        HeapObject::Box(v) => format!("Box = {}", render_value_for_note(v)),
        HeapObject::Vec { elements, capacity, .. } => {
            // Empty capacity is conveyed by the byte-cell row in the UI;
            // the text label only lists the actual elements.
            let cells: Vec<String> = elements.iter().map(render_value_for_note).collect();
            format!(
                "Vec [{}] (cap={}, len={})",
                cells.join(", "),
                capacity,
                elements.len()
            )
        }
        HeapObject::Str { bytes, capacity } => {
            format!(
                "String \"{bytes}\" (cap={capacity}, len={})",
                bytes.len()
            )
        }
        // **M08**: Arc display includes the strong refcount so the heap
        // block annotation reads `Arc<i32> = 5 [refs: 2]` (refs suffix
        // is added by the UI layer via HeapView.refcount; this string
        // is the type + value portion only).
        HeapObject::Arc { value, strong_count } => format!(
            "Arc = {} [refs: {}]",
            render_value_for_note(value),
            strong_count,
        ),
        // **M08**: Mutex display shows the inner value; the `[locked by]`
        // suffix is added UI-side via HeapView.mutex_holder.
        HeapObject::Mutex { value, holder, .. } => {
            let lock_suffix = match holder {
                Some(tid) => format!(" [locked by #{}]", tid.0),
                None => String::new(),
            };
            format!("Mutex = {}{}", render_value_for_note(value), lock_suffix)
        }
        // **Post-M08**: fused Arc<Mutex<T>> display — combines refcount AND
        // lock state in one heap block label. UI extracts both via the
        // suffix parsers + the new mutex_state field.
        HeapObject::ArcMutex { value, strong_count, holder, .. } => {
            let lock_suffix = match holder {
                Some(tid) => format!(" [locked by #{}]", tid.0),
                None => " [free]".to_owned(),
            };
            format!(
                "Arc<Mutex> = {} [refs: {}]{}",
                render_value_for_note(value),
                strong_count,
                lock_suffix,
            )
        }
    }
}

/// **M06.1**: short value-render for use inside `Note { Info }` messages.
/// Mirrors `ui::render_value`'s essentials but kept inline here so eval
/// doesn't depend on ui. References render as their target's resolved
/// description in the calling site, not via this helper.
fn render_value_for_note(value: &Value) -> String {
    match value {
        Value::Int { kind, bits } => format!("{bits}_{}", kind.name()),
        Value::Float { kind, value } => {
            let body = if value.is_nan() {
                "NaN".to_owned()
            } else if value.is_infinite() {
                if *value > 0.0 { "+Inf".to_owned() } else { "-Inf".to_owned() }
            } else {
                value.to_string()
            };
            format!("{body}_{}", kind.name())
        }
        Value::Bool(b) => b.to_string(),
        Value::Unit => "()".to_owned(),
        Value::Ref { mutable, .. } => {
            if *mutable { "&mut _".to_owned() } else { "&_".to_owned() }
        }
        // M07: heap-owning values render abstractly in notes.
        Value::Box { .. } => "Box".to_owned(),
        Value::Vec { .. } => "Vec".to_owned(),
        Value::String { .. } => "String".to_owned(),
        // M07.1: slice. Abstract render with length for notes.
        // M07.2: `&str` literals flow through Value::Slice with Pointee::Static.
        Value::Slice { len, .. } => format!("&[_; {len}]"),
        // M07.3: array — render element count abstractly for notes.
        Value::Array { elements, .. } => format!("[_; {}]", elements.len()),
        // M07.4: struct — render as `Point { x: 1, y: 2 }` for notes; gives
        // the learner concrete field values without depending on UI's
        // full struct-view rendering.
        Value::Struct { name, fields } => {
            let body: Vec<String> = fields
                .iter()
                .map(|(fname, fval)| format!("{fname}: {}", render_value_for_note(fval)))
                .collect();
            format!("{name} {{ {} }}", body.join(", "))
        }
        // M07.7: trait-object fat pointers — abstract render for notes.
        Value::DynRef { trait_name, mutable, .. } => {
            if *mutable { format!("&mut dyn {trait_name}") } else { format!("&dyn {trait_name}") }
        }
        Value::BoxDyn { trait_name, .. } => format!("Box<dyn {trait_name}>"),
        // M08: concurrency primitives — abstract renders for notes.
        Value::Arc { addr } => format!("Arc → heap[{}]", addr.0),
        Value::Mutex { addr } => format!("Mutex → heap[{}]", addr.0),
        Value::MutexGuard { addr } => format!("MutexGuard → heap[{}]", addr.0),
        Value::JoinHandle { thread_id } => format!("JoinHandle(#{})", thread_id.0),
    }
}

/// **M06**: synthesize a span pointing at the closing `}` of a block whose
/// span covers `{...}`. The 1-char span at the end of the block is what
/// `BorrowEnd` events use so the editor highlight lands on the brace rather
/// than the block's first statement.
fn closing_brace_span(block_span: Span) -> Span {
    let end = block_span.end;
    let start = end.saturating_sub(1);
    Span::new(start, end, block_span.file)
}
