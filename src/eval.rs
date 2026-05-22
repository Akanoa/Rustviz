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
        let _ = eval.call_fn(id, Vec::new(), decl.span);
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
            events: Vec::new(),
            halted: false,
        })
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
            Some(BindingType::Var(ty)) => Some(*ty),
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
            scopes: vec![Scope { locals: Vec::new() }],
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

        // Evaluate the body. Returns Value::Unit on halt.
        let body_value = self.eval_block(&decl.body);

        if self.halted {
            // Frame did not return — no ReturnValue, no FrameLeave. Stream ends
            // at the runtime-error Note already pushed by the halt path.
            return Value::Unit;
        }

        // M03.1: emit ReturnValue between body completion and scope teardown.
        // Pedagogically: the value is now visible for one cursor tick before
        // any drops fire or the frame closes.
        let return_span = decl
            .body
            .tail
            .as_ref()
            .map(|t| t.span())
            .unwrap_or(decl.body.span);
        self.events.push(MemEvent::ReturnValue {
            frame_id,
            value: body_value.clone(),
            span: return_span,
        });

        // Drop the param scope (LIFO). For L1 / Copy types this emits no
        // events (gated in M03.1); M07+ non-Copy types still drop here.
        self.drop_current_scope();

        // Pop the frame and emit FrameLeave.
        let frame = self.frames.pop().expect("frame still active");
        self.events.push(MemEvent::FrameLeave {
            frame_id: frame.frame_id,
            return_value: body_value.clone(),
            span: decl.body.span,
        });

        body_value
    }

    fn drop_current_scope(&mut self) {
        let scope = self
            .frames
            .last_mut()
            .expect("frame active")
            .scopes
            .pop()
            .expect("scope active");
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

    fn eval_block(&mut self, block: &ast::Block) -> Value {
        if self.halted {
            return Value::Unit;
        }
        // Push a new lexical scope for the block.
        self.frames
            .last_mut()
            .expect("frame active")
            .scopes
            .push(Scope { locals: Vec::new() });

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
            self.drop_current_scope();
        }

        tail_value
    }

    fn eval_stmt(&mut self, stmt: &ast::Stmt) {
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
        }
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
        }
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
