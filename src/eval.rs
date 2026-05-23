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

/// Evaluate a resolved + typed program, producing a deterministic event stream.
///
/// On success (including runtime errors that surface as `Note` events), returns
/// `Ok(Vec<MemEvent>)`. `Err(ParseError)` is reserved for static-time invariant
/// violations that should be unreachable when M02 succeeded.
pub fn evaluate(
    program: &ast::Program,
    resolution: &Resolution,
    types: &TypeMap,
) -> Result<Vec<MemEvent>, ParseError> {
    let mut eval = Evaluator::new(program, resolution, types)?;

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
    }

    Ok(eval.events)
}

struct Evaluator<'a> {
    resolution: &'a Resolution,
    types: &'a TypeMap,
    /// `BindingId` → `&FnDecl` lookup, built once at construction.
    fn_decls: HashMap<BindingId, &'a ast::FnDecl>,
    /// Call stack — innermost frame last.
    frames: Vec<Frame>,
    next_slot_id: u32,
    next_frame_id: u32,
    /// **M06**: monotonic counter for borrow ids.
    next_borrow_id: u32,
    /// **M06.1**: Info notes deferred until the end of the current statement.
    /// Used by deref-read so the explanatory Note appears AFTER the
    /// containing stmt's SlotWrite (i.e. at the same cursor step where the
    /// new binding actually has the read value) rather than before.
    pending_notes: Vec<MemEvent>,
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
    /// Emitted events in source-execution order.
    events: Vec<MemEvent>,
    /// Set to true on runtime error to stop further evaluation.
    halted: bool,
}

/// **M07**: heap state. Each live allocation is one HeapObject indexed by
/// its HeapAddr. Realloc replaces (old, new): the `from` addr is removed,
/// the `to` addr is added with the new contents.
struct HeapState {
    objects: indexmap::IndexMap<crate::event::HeapAddr, HeapObject>,
}

impl HeapState {
    fn new() -> Self {
        Self { objects: indexmap::IndexMap::new() }
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
}

impl<'a> Evaluator<'a> {
    fn new(
        program: &'a ast::Program,
        resolution: &'a Resolution,
        types: &'a TypeMap,
    ) -> Result<Self, ParseError> {
        let mut fn_decls = HashMap::new();
        for item in &program.items {
            let ast::Item::Fn(decl) = item;
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
        Ok(Self {
            resolution,
            types,
            fn_decls,
            frames: Vec::new(),
            next_slot_id: 0,
            next_frame_id: 0,
            next_borrow_id: 0,
            pending_notes: Vec::new(),
            heap: HeapState::new(),
            next_heap_addr: 0,
            heap_generations: std::collections::HashMap::new(),
            borrow_generations: std::collections::HashMap::new(),
            events: Vec::new(),
            halted: false,
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

    /// **M07**: allocate a fresh HeapAddr.
    fn alloc_heap_addr(&mut self) -> crate::event::HeapAddr {
        let id = crate::event::HeapAddr(self.next_heap_addr);
        self.next_heap_addr += 1;
        id
    }

    /// **M07**: allocate a heap object and emit `HeapAlloc`. Returns the
    /// new addr. Tracks the addr in the current scope for HeapFree on exit.
    fn alloc_heap(&mut self, obj: HeapObject, ty_name: String, size: u32, span: Span) -> crate::event::HeapAddr {
        let addr = self.alloc_heap_addr();
        self.heap.objects.insert(addr, obj);
        self.events.push(MemEvent::HeapAlloc {
            addr,
            size,
            ty_name,
            span,
        });
        // Track this alloc for HeapFree on scope exit.
        if let Some(scope) = self
            .frames
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
        // Allocate a NEW addr for the copy.
        let to = self.alloc_heap_addr();
        self.heap.objects.shift_remove(&from);
        self.heap.objects.insert(to, obj);
        self.events.push(MemEvent::HeapRealloc {
            from,
            to,
            new_size,
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
        for frame in self.frames.iter_mut() {
            for scope in frame.scopes.iter_mut() {
                for h in scope.heap_allocs.iter_mut() {
                    if *h == from {
                        *h = to;
                    }
                }
            }
        }
        // Update LocalSlot owning-values to point at the new addr; emit
        // SlotWrite for each so the UI's owning arrow follows.
        let mut writes_needed = Vec::new();
        for frame in self.frames.iter_mut() {
            for scope in frame.scopes.iter_mut() {
                for local in scope.locals.iter_mut() {
                    match &mut local.value {
                        Value::Vec { addr } if *addr == from => {
                            *addr = to;
                            writes_needed.push((local.slot_id, local.value.clone()));
                        }
                        Value::String { addr } if *addr == from => {
                            *addr = to;
                            writes_needed.push((local.slot_id, local.value.clone()));
                        }
                        Value::Box { addr } if *addr == from => {
                            *addr = to;
                            writes_needed.push((local.slot_id, local.value.clone()));
                        }
                        _ => {}
                    }
                }
            }
        }
        for (slot_id, val) in writes_needed {
            self.events.push(MemEvent::SlotWrite { slot_id, value: val, span });
        }
        // Dangling-borrow detection: scan locals for Value::Ref with
        // target = Pointee::Heap(from). After the addr change, these refs
        // still hold the OLD addr, which is freed — they're dangling.
        let mut dangling: Vec<Span> = Vec::new();
        for frame in self.frames.iter() {
            for scope in frame.scopes.iter() {
                for local in scope.locals.iter() {
                    if let Value::Ref { target: crate::event::Pointee::Heap(a), .. } = local.value {
                        if a == from {
                            dangling.push(local.decl_span);
                        }
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

    /// **M07**: free a heap object — emit HeapFree, remove from state.
    fn free_heap(&mut self, addr: crate::event::HeapAddr, span: Span) {
        if self.heap.objects.shift_remove(&addr).is_some() {
            self.events.push(MemEvent::HeapFree { addr, span });
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
        let frame = self.frames.last()?;
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
        if self.halted {
            return Value::Unit;
        }
        if self.frames.len() >= RECURSION_LIMIT {
            self.emit_runtime_error(
                format!("recursion depth exceeded ({RECURSION_LIMIT} frames)"),
                call_span,
            );
            return Value::Unit;
        }

        let decl = self.fn_decls[&fn_binding];
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
            fn_name: decl.name.clone(),
            span: call_span,
        });

        // Push the frame with an outer (param) scope.
        self.frames.push(Frame {
            frame_id,
            scopes: vec![Scope { locals: Vec::new(), borrows: Vec::new(), heap_allocs: Vec::new() }],
        });

        // Emit per-param SlotAlloc + SlotWrite and push the locals.
        for (binding_id, slot_id, name, value, decl_span) in param_slots {
            let ty = self
                .lookup_var_ty(binding_id)
                .expect("param has BindingType::Var(_) after typeck");
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
            self.frames
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

        // M06: drop the body scope NOW (after ReturnValue). Emits BorrowEnd
        // and SlotDrop (M07+) events. Use the closing `}` of the body block
        // as the BorrowEnd span (1-char span at the end of the block).
        let body_close = closing_brace_span(decl.body.span);
        self.drop_current_scope(body_close);
        // Drop the param scope (LIFO). The param scope's "closing brace" is
        // the function decl's closing brace.
        self.drop_current_scope(closing_brace_span(decl.span));

        // Pop the frame and emit FrameLeave.
        let frame = self.frames.pop().expect("frame still active");
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
            .frames
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
            // Detect heap-owning current value (the slot's current Value).
            let heap_addr = match &local.value {
                Value::Box { addr } | Value::Vec { addr } | Value::String { addr } => Some(*addr),
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
    fn eval_fn_body(&mut self, block: &ast::Block) -> Value {
        if self.halted {
            return Value::Unit;
        }
        self.frames
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

    fn eval_block(&mut self, block: &ast::Block) -> Value {
        if self.halted {
            return Value::Unit;
        }
        // Push a new lexical scope for the block.
        self.frames
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

    fn eval_stmt(&mut self, stmt: &ast::Stmt) {
        let result = self.eval_stmt_inner(stmt);
        // **M06.1**: flush any deferred Notes (e.g. from deref-read) AFTER
        // the statement's main effects, so the explanatory message and the
        // result (e.g. SlotWrite of the new binding) land at adjacent cursor
        // steps with the result first.
        if !self.halted {
            self.flush_pending_notes();
        }
        let _ = result;
    }

    fn eval_stmt_inner(&mut self, stmt: &ast::Stmt) {
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
                self.frames
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
        for frame in self.frames.iter_mut().rev() {
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
        for frame in self.frames.iter().rev() {
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

    fn eval_expr(&mut self, expr: &ast::Expr) -> Value {
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
                let kind = match self.types.expr_types.get(span) {
                    Some(crate::Ty::Int(k)) => *k,
                    _ => IntKind::I32,
                };
                Value::Int { kind, bits: *v as i128 }
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
            ast::Expr::Borrow { inner, mutable, span } => {
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
                    // **M07**: `&v[i]` — target is the Vec's heap allocation.
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
                self.frames
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
            // **M07**: StrLit transient value.
            ast::Expr::StrLit(s, _) => Value::Str(s.clone()),
            ast::Expr::Path { .. } => panic!("Path expressions only valid as Call callees in M07"),
            // **M07**: method call — dispatch via eval_method_call.
            ast::Expr::MethodCall { receiver, name, args, span } => {
                self.eval_method_call(receiver, name, args, *span)
            }
            // **M07**: indexing — bounds-check + copy.
            ast::Expr::Index { receiver, index, span } => {
                self.eval_index(receiver, index, *span)
            }
        }
    }

    /// **M07**: evaluate a path-callee `Box::new(v)` / `Vec::new()` / `String::from("...")`.
    fn eval_path_call(
        &mut self,
        segments: &[String],
        args: &[ast::Expr],
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
                let size = 8; // M07: simplification — uniform 8 bytes for primitives.
                // M07: embed value in the display label so the heap panel
                // shows e.g. "Box<i32> = 5_i32" instead of just "Box<i32>".
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
                // M07: initial capacity = 2 (pedagogical default; pushes fit
                // in-place until they exceed). Matches the user-friendly
                // mental model where the heap addr stays stable until a
                // genuine realloc happens.
                let elem_ty = Ty::Int(IntKind::I32);
                let initial_cap: usize = 2;
                let addr = self.alloc_heap(
                    HeapObject::Vec { elements: Vec::new(), capacity: initial_cap, elem_ty },
                    format!("Vec [] (cap={initial_cap})"),
                    (initial_cap * 4) as u32,
                    span,
                );
                Value::Vec { addr }
            }
            ["String", "from"] => {
                let s = match self.eval_expr(&args[0]) {
                    Value::Str(s) => s,
                    _ => panic!("typeck should have rejected non-StrLit arg to String::from"),
                };
                let size = s.len() as u32;
                let display = format!("String \"{s}\" (cap={})", s.len());
                let addr = self.alloc_heap(
                    HeapObject::Str { bytes: s.clone(), capacity: s.len() },
                    display,
                    size,
                    span,
                );
                Value::String { addr }
            }
            _ => panic!("typeck should have rejected unknown path"),
        }
    }

    /// **M07**: evaluate a method call (Vec::push, Vec::len, String::push_str).
    fn eval_method_call(
        &mut self,
        receiver: &ast::Expr,
        name: &str,
        args: &[ast::Expr],
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
            (Value::Vec { addr }, "len") => {
                let obj = self.heap.objects.get(addr).expect("vec exists");
                let len = if let HeapObject::Vec { elements, .. } = obj { elements.len() } else { 0 };
                Value::Int { kind: IntKind::U64, bits: len as i128 }
            }
            (Value::String { addr }, "push_str") => {
                let suffix = match self.eval_expr(&args[0]) {
                    Value::Str(s) => s,
                    _ => panic!("typeck should have rejected non-StrLit arg"),
                };
                self.string_push_str(*addr, &suffix, span);
                Value::Unit
            }
            _ => panic!("typeck should reject this method call"),
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
            // ── Phase 1: announce the realloc ─────────────────────────────
            let new_cap = if cur_cap == 0 { 1 } else { cur_cap * 2 };
            let other_blocks: Vec<u32> = self.heap.objects.keys()
                .filter(|a| **a != addr)
                .map(|a| a.0)
                .collect();
            let pre_msg = if other_blocks.is_empty() {
                format!(
                    "Vec capacity exceeded ({cur_cap} → {new_cap}); allocator will copy the bytes to a fresh location and free the old block"
                )
            } else {
                let others_str = other_blocks.iter().map(|a| format!("#{a}")).collect::<Vec<_>>().join(", ");
                format!(
                    "Vec capacity exceeded ({cur_cap} → {new_cap}); cannot grow in place because heap blocks [{others_str}] occupy the adjacent region — allocator will copy the bytes to a fresh location and free the old block"
                )
            };
            self.events.push(MemEvent::Note {
                kind: NoteKind::Info,
                message: pre_msg,
                span,
            });
            // ── Phase 2: realloc (alloc new + copy old contents, free old) ──
            // The new block holds the OLD elements at the NEW capacity. The
            // actual push hasn't happened yet — that's phase 3.
            let copy_obj = HeapObject::Vec {
                elements: old_elements,
                capacity: new_cap,
                elem_ty: elem_ty.clone(),
            };
            let new_size = (new_cap * 4) as u32;
            let new_addr = self.realloc_heap(addr, copy_obj, new_size, span);
            // ── Phase 3: the actual push, in place on the new buffer ──────
            if let Some(HeapObject::Vec { elements, .. }) = self.heap.objects.get_mut(&new_addr) {
                elements.push(value);
            }
            let push_display = self.heap.objects.get(&new_addr)
                .map(heap_object_display)
                .unwrap_or_default();
            self.events.push(MemEvent::HeapRealloc {
                from: new_addr,
                to: new_addr,
                new_size,
                new_display: push_display,
                span,
            });
        } else {
            // In-place push: just update contents and emit HeapRealloc with
            // from==to carrying the new display.
            if let Some(HeapObject::Vec { elements, .. }) = self.heap.objects.get_mut(&addr) {
                elements.push(value);
            }
            let new_display = self.heap.objects.get(&addr)
                .map(heap_object_display)
                .unwrap_or_default();
            self.events.push(MemEvent::HeapRealloc {
                from: addr,
                to: addr,
                new_size: (cur_cap * 4) as u32,
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
            let new_display = self.heap.objects.get(&addr)
                .map(heap_object_display)
                .unwrap_or_default();
            self.events.push(MemEvent::HeapRealloc {
                from: addr,
                to: addr,
                new_size: cur_cap as u32,
                new_display,
                span,
            });
        }
    }

    /// **M07**: evaluate `receiver[index]`. Receiver must be a Vec; index any Int.
    /// Returns a copy of the element (bounds-checked).
    fn eval_index(&mut self, receiver: &ast::Expr, index: &ast::Expr, span: Span) -> Value {
        let recv = self.eval_expr(receiver);
        if self.halted { return Value::Unit; }
        let idx_v = self.eval_expr(index);
        if self.halted { return Value::Unit; }
        let addr = match recv {
            Value::Vec { addr } => addr,
            _ => panic!("typeck should reject non-Vec index"),
        };
        let i = match idx_v {
            Value::Int { bits, .. } => bits,
            _ => panic!("typeck should reject non-Int index"),
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
        for frame in self.frames.iter().rev() {
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
        for frame in self.frames.iter().rev() {
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
        lhs_expr: &ast::Expr,
        rhs_expr: &ast::Expr,
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
/// labels and realloc Info notes).
fn heap_object_display(obj: &HeapObject) -> String {
    match obj {
        HeapObject::Box(v) => format!("Box = {}", render_value_for_note(v)),
        HeapObject::Vec { elements, capacity, .. } => {
            let elems: Vec<String> = elements.iter().map(render_value_for_note).collect();
            format!("Vec [{}] (cap={})", elems.join(", "), capacity)
        }
        HeapObject::Str { bytes, capacity } => {
            format!("String \"{bytes}\" (cap={capacity})")
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
        Value::Str(s) => format!("\"{s}\""),
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
