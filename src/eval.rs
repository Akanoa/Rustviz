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
    /// Emitted events in source-execution order.
    events: Vec<MemEvent>,
    /// Set to true on runtime error to stop further evaluation.
    halted: bool,
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
            scopes: vec![Scope { locals: Vec::new(), borrows: Vec::new() }],
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
        for local in scope.locals.into_iter().rev() {
            // M03.1: Copy types have no destructor; their bytes persist on the
            // stack until the frame is reused. Skipping the SlotDrop event for
            // Copy-typed slots avoids visualizing physical-memory loss that
            // doesn't actually happen. Non-Copy types (M07+: Box, Vec, String)
            // still emit SlotDrop because their drop runs real destructor work.
            let ty = self
                .lookup_var_ty(local.binding_id)
                .expect("var ty after typeck");
            if !ty.is_copy() {
                self.events.push(MemEvent::SlotDrop {
                    slot_id: local.slot_id,
                    span: local.decl_span,
                });
            }
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
            .push(Scope { locals: Vec::new(), borrows: Vec::new() });
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
            .push(Scope { locals: Vec::new(), borrows: Vec::new() });

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
                        // Read r's Value::Ref to find target_slot.
                        let ref_value = self.eval_expr(inner);
                        if self.halted {
                            return;
                        }
                        let target_slot = match ref_value {
                            Value::Ref { target_slot, mutable: true, .. } => target_slot,
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
                let callee_binding = match callee.as_ref() {
                    ast::Expr::Ident(_, sp) => *self
                        .resolution
                        .uses
                        .get(sp)
                        .expect("callee resolved"),
                    _ => panic!("typeck should have rejected non-Ident callees"),
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
                let target_binding = match inner.as_ref() {
                    ast::Expr::Ident(_, sp) => *self
                        .resolution
                        .uses
                        .get(sp)
                        .expect("ident resolved"),
                    _ => panic!("typeck should have rejected non-Ident place"),
                };
                let target_slot = self
                    .lookup_local_slot(target_binding)
                    .expect("local slot exists for borrowed binding");
                let borrow_id = self.alloc_borrow_id();
                // Emit the borrow event.
                let event = if *mutable {
                    MemEvent::BorrowMut {
                        borrow_id,
                        target: crate::event::Pointee::Slot(target_slot),
                        span: *span,
                    }
                } else {
                    MemEvent::BorrowShared {
                        borrow_id,
                        target: crate::event::Pointee::Slot(target_slot),
                        span: *span,
                    }
                };
                self.events.push(event);
                // Track this borrow against the current scope for BorrowEnd.
                self.frames
                    .last_mut()
                    .expect("frame active")
                    .scopes
                    .last_mut()
                    .expect("scope active")
                    .borrows
                    .push(borrow_id);
                Value::Ref {
                    borrow_id,
                    target_slot,
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
                    Value::Ref { target_slot, .. } => {
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
                    _ => panic!("typeck should reject deref of non-reference"),
                }
            }
        }
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
