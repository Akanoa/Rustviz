//! Lightweight type checking: validate annotations and propagate L1 value
//! types. Consumes a [`Resolution`] from [`crate::resolve`].

use indexmap::IndexMap;

use crate::parse::ast;
use crate::parse::error::ParseError;
use crate::parse::span::Span;
use crate::resolve::{BindingId, BindingKind, Resolution};

/// **M03.2**: integer-kind discriminator. Used by `Ty::Int` and `Value::Int`.
/// `USize` / `ISize` are pinned to 64-bit width for browser determinism
/// (per FR-011); their `min_value`/`max_value` match `U64`/`I64`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)] // Variant names are self-documenting (`I8`, `U16`, ...).
pub enum IntKind {
    I8, I16, I32, I64, I128,
    U8, U16, U32, U64, U128,
    ISize, USize,
}

impl IntKind {
    /// Lowest representable value as `i128` (wide-enough storage for all
    /// variants). For unsigned types this is `0`.
    pub fn min_value(self) -> i128 {
        match self {
            Self::I8 => i8::MIN as i128,
            Self::I16 => i16::MIN as i128,
            Self::I32 => i32::MIN as i128,
            Self::I64 => i64::MIN as i128,
            Self::I128 => i128::MIN,
            Self::U8 | Self::U16 | Self::U32 | Self::U64 | Self::U128 | Self::USize => 0,
            Self::ISize => i64::MIN as i128, // FR-011: isize ≡ i64.
        }
    }

    /// Highest representable value as `i128`.
    pub fn max_value(self) -> i128 {
        match self {
            Self::I8 => i8::MAX as i128,
            Self::I16 => i16::MAX as i128,
            Self::I32 => i32::MAX as i128,
            Self::I64 => i64::MAX as i128,
            Self::I128 => i128::MAX,
            Self::U8 => u8::MAX as i128,
            Self::U16 => u16::MAX as i128,
            Self::U32 => u32::MAX as i128,
            Self::U64 => u64::MAX as i128,
            Self::U128 => i128::MAX, // u128::MAX doesn't fit i128; pin to i128::MAX.
            Self::USize => u64::MAX as i128, // FR-011: usize ≡ u64.
            Self::ISize => i64::MAX as i128, // FR-011: isize ≡ i64.
        }
    }

    /// `true` iff `v` is in this type's representable range.
    pub fn contains(self, v: i128) -> bool {
        v >= self.min_value() && v <= self.max_value()
    }

    /// `true` for signed-integer kinds (i*, isize). `false` for unsigned.
    pub fn is_signed(self) -> bool {
        matches!(
            self,
            Self::I8 | Self::I16 | Self::I32 | Self::I64 | Self::I128 | Self::ISize
        )
    }

    /// Rust type-name verbatim (`"u8"`, `"i64"`, `"usize"`, …).
    pub fn name(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::I128 => "i128",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::U128 => "u128",
            Self::ISize => "isize",
            Self::USize => "usize",
        }
    }
}

/// **M03.2**: float-kind discriminator. Used by `Ty::Float` and `Value::Float`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)] // Variant names are self-documenting (`F32`, `F64`).
pub enum FloatKind {
    F32, F64,
}

#[cfg(test)]
mod intkind_tests {
    use super::*;

    #[test]
    fn u8_range() {
        assert_eq!(IntKind::U8.min_value(), 0);
        assert_eq!(IntKind::U8.max_value(), 255);
        assert!(IntKind::U8.contains(0));
        assert!(IntKind::U8.contains(255));
        assert!(!IntKind::U8.contains(256));
        assert!(!IntKind::U8.contains(-1));
    }

    #[test]
    fn i8_range() {
        assert_eq!(IntKind::I8.min_value(), -128);
        assert_eq!(IntKind::I8.max_value(), 127);
        assert!(IntKind::I8.contains(-128));
        assert!(IntKind::I8.contains(127));
        assert!(!IntKind::I8.contains(128));
        assert!(!IntKind::I8.contains(-129));
    }

    #[test]
    fn usize_matches_u64() {
        assert_eq!(IntKind::USize.min_value(), IntKind::U64.min_value());
        assert_eq!(IntKind::USize.max_value(), IntKind::U64.max_value());
    }

    #[test]
    fn isize_matches_i64() {
        assert_eq!(IntKind::ISize.min_value(), IntKind::I64.min_value());
        assert_eq!(IntKind::ISize.max_value(), IntKind::I64.max_value());
    }

    #[test]
    fn is_signed_exhaustive() {
        for k in [IntKind::I8, IntKind::I16, IntKind::I32, IntKind::I64, IntKind::I128, IntKind::ISize] {
            assert!(k.is_signed(), "{} should be signed", k.name());
        }
        for k in [IntKind::U8, IntKind::U16, IntKind::U32, IntKind::U64, IntKind::U128, IntKind::USize] {
            assert!(!k.is_signed(), "{} should be unsigned", k.name());
        }
    }

    #[test]
    fn names_match_rust() {
        assert_eq!(IntKind::U8.name(), "u8");
        assert_eq!(IntKind::I64.name(), "i64");
        assert_eq!(IntKind::USize.name(), "usize");
        assert_eq!(FloatKind::F32.name(), "f32");
        assert_eq!(FloatKind::F64.name(), "f64");
    }
}

impl FloatKind {
    /// Rust type-name verbatim (`"f32"` or `"f64"`).
    pub fn name(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }
}

/// L1 value types. **M03.2**: restructured into nested kind enums.
/// Function signatures live in [`FnSig`], not here, because functions are not
/// first-class values in L1.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Ty {
    /// Signed or unsigned integer. Width is carried by [`IntKind`].
    Int(IntKind),
    /// Floating-point. Width is carried by [`FloatKind`].
    Float(FloatKind),
    /// Boolean.
    Bool,
    /// Unit `()`.
    Unit,
}

impl Ty {
    /// Render this type as a user-facing string (`"u8"`, `"f64"`, `"bool"`, `"()"`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Int(k) => k.name(),
            Self::Float(k) => k.name(),
            Self::Bool => "bool",
            Self::Unit => "()",
        }
    }

    /// Whether values of this type are `Copy` (no destructor, bytes physically
    /// persist on the stack until storage is reused).
    ///
    /// L1's lattice — every integer width, both floats, `bool`, `()` — is
    /// entirely Copy. M07+ will add non-Copy heap-allocated variants
    /// (e.g. `Box`, `Vec`, `String`) that return `false`. The exhaustive
    /// `match` below ensures any new variant forces a deliberate
    /// classification — there is intentionally no `_` catch-all.
    pub fn is_copy(self) -> bool {
        match self {
            Self::Int(_) | Self::Float(_) | Self::Bool | Self::Unit => true,
        }
    }
}

/// Function signature: parameter types and return type.
#[derive(Debug, Clone, PartialEq)]
pub struct FnSig {
    /// Parameter types in declaration order.
    pub params: Vec<Ty>,
    /// Return type. `Ty::Unit` if the function has no `-> T` annotation.
    pub ret: Ty,
}

/// Type information attached to a binding.
#[derive(Debug, Clone, PartialEq)]
pub enum BindingType {
    /// Binding holds a value of this type (let / param).
    Var(Ty),
    /// Binding is a function with this signature.
    Fn(FnSig),
}

/// Output of [`typeck`]. Two side tables — one keyed by expression span, one
/// keyed by binding id.
#[derive(Debug, Clone, Default)]
pub struct TypeMap {
    /// Maps each value-producing `Expr` (by span) to its inferred [`Ty`].
    /// The callee Ident of a `Call` is intentionally absent (it's a function
    /// reference, not a value). Iteration order is tree-walk pre-order
    /// (research.md R-002).
    pub expr_types: IndexMap<Span, Ty>,
    /// Maps each `BindingId` to its [`BindingType`].
    pub binding_types: IndexMap<BindingId, BindingType>,
}

/// Type-check a resolved program.
///
/// On success, returns a `TypeMap` with `expr_types` covering every
/// value-producing expression. On failure, returns a single `ParseError`.
pub fn typeck(program: &ast::Program, resolution: &Resolution) -> Result<TypeMap, ParseError> {
    let mut t = Typechecker::new(program, resolution);

    // Phase 1: compute FnSig for every top-level fn item and seed binding_types.
    for item in &program.items {
        match item {
            ast::Item::Fn(decl) => {
                let sig = t.build_fn_sig(decl)?;
                let id = t
                    .lookup_binding(|d| matches!(d.kind, BindingKind::Fn) && d.name == decl.name)
                    .expect("fn binding present after resolve");
                t.types.binding_types.insert(id, BindingType::Fn(sig));
            }
        }
    }

    // Phase 2: typecheck each fn body.
    for item in &program.items {
        match item {
            ast::Item::Fn(decl) => t.typecheck_fn(decl)?,
        }
    }

    Ok(t.types)
}

struct Typechecker<'a> {
    resolution: &'a Resolution,
    types: TypeMap,
    /// Expected return type of the function currently being checked.
    current_fn_ret: Option<Ty>,
}

impl<'a> Typechecker<'a> {
    fn new(_program: &'a ast::Program, resolution: &'a Resolution) -> Self {
        Self {
            resolution,
            types: TypeMap::default(),
            current_fn_ret: None,
        }
    }

    fn lookup_binding(&self, mut pred: impl FnMut(&crate::resolve::BindingDecl) -> bool) -> Option<BindingId> {
        self.resolution
            .bindings
            .iter()
            .find_map(|(id, decl)| if pred(decl) { Some(*id) } else { None })
    }

    fn build_fn_sig(&self, decl: &ast::FnDecl) -> Result<FnSig, ParseError> {
        let mut params = Vec::with_capacity(decl.params.len());
        for param in &decl.params {
            params.push(ty_from_ast(&param.ty)?);
        }
        let ret = match &decl.return_ty {
            Some(t) => ty_from_ast(t)?,
            None => Ty::Unit,
        };
        Ok(FnSig { params, ret })
    }

    fn typecheck_fn(&mut self, decl: &ast::FnDecl) -> Result<(), ParseError> {
        let fn_id = self
            .lookup_binding(|d| matches!(d.kind, BindingKind::Fn) && d.name == decl.name)
            .expect("fn binding present");
        let sig = match self.types.binding_types.get(&fn_id) {
            Some(BindingType::Fn(s)) => s.clone(),
            _ => panic!("fn sig must be set in Phase 1"),
        };
        for (param, &param_ty) in decl.params.iter().zip(sig.params.iter()) {
            let pid = self
                .lookup_binding(|d| matches!(d.kind, BindingKind::Param) && d.name_span == param.span)
                .expect("param binding present");
            self.types
                .binding_types
                .insert(pid, BindingType::Var(param_ty));
        }
        let prev = self.current_fn_ret.replace(sig.ret);
        let body_ty = self.typecheck_block(&decl.body)?;
        if body_ty != sig.ret {
            return Err(ParseError {
                message: format!(
                    "function returns `{}`, but body has type `{}`",
                    sig.ret.name(),
                    body_ty.name()
                ),
                span: decl
                    .body
                    .tail
                    .as_ref()
                    .map(|t| t.span())
                    .unwrap_or(decl.body.span),
            });
        }
        self.current_fn_ret = prev;
        Ok(())
    }

    fn typecheck_block(&mut self, block: &ast::Block) -> Result<Ty, ParseError> {
        for stmt in &block.stmts {
            self.typecheck_stmt(stmt)?;
        }
        if let Some(tail) = &block.tail {
            self.typecheck_expr(tail)
        } else {
            Ok(Ty::Unit)
        }
    }

    fn typecheck_stmt(&mut self, stmt: &ast::Stmt) -> Result<(), ParseError> {
        match stmt {
            ast::Stmt::Let(let_stmt) => {
                let init_ty = self.typecheck_expr(&let_stmt.init)?;
                let bind_ty = match &let_stmt.ty {
                    Some(annot) => {
                        let annot_ty = ty_from_ast(annot)?;
                        // M03.2: attempt to coerce a literal init to the annotated
                        // type before checking equality. Allows `let x: u8 = 5;`.
                        let init_ty = self
                            .try_coerce_to(&let_stmt.init, init_ty, annot_ty)
                            .unwrap_or(init_ty);
                        if annot_ty != init_ty {
                            return Err(ParseError {
                                message: format!(
                                    "expected `{}`, found `{}`",
                                    annot_ty.name(),
                                    init_ty.name()
                                ),
                                span: let_stmt.init.span(),
                            });
                        }
                        annot_ty
                    }
                    None => init_ty,
                };
                let id = self
                    .lookup_binding(|d| {
                        matches!(d.kind, BindingKind::Let { .. }) && d.name_span == let_stmt.span
                    })
                    .expect("let binding present");
                self.types.binding_types.insert(id, BindingType::Var(bind_ty));
            }
            ast::Stmt::Expr(expr) => {
                self.typecheck_expr(expr)?;
            }
        }
        Ok(())
    }

    /// Typecheck an expression and record its type in `expr_types`. Returns the type.
    fn typecheck_expr(&mut self, expr: &ast::Expr) -> Result<Ty, ParseError> {
        let ty = self.typecheck_expr_inner(expr)?;
        self.types.expr_types.insert(expr.span(), ty);
        Ok(ty)
    }

    fn typecheck_expr_inner(&mut self, expr: &ast::Expr) -> Result<Ty, ParseError> {
        match expr {
            ast::Expr::LitInt(_, _) => Ok(Ty::Int(IntKind::I32)),
            ast::Expr::LitFloat(_, _) => Ok(Ty::Float(FloatKind::F64)),
            ast::Expr::LitBool(_, _) => Ok(Ty::Bool),
            ast::Expr::Ident(_, span) => {
                let id = *self
                    .resolution
                    .uses
                    .get(span)
                    .expect("ident use resolved during resolve()");
                match self.types.binding_types.get(&id) {
                    Some(BindingType::Var(ty)) => Ok(*ty),
                    Some(BindingType::Fn(_)) => {
                        let name = self.resolution.bindings[&id].name.clone();
                        Err(ParseError {
                            message: format!(
                                "`{name}` is a function; functions are not first-class values in L1"
                            ),
                            span: *span,
                        })
                    }
                    None => panic!("binding {id:?} has no type"),
                }
            }
            ast::Expr::Unary { op, expr: inner, span } => {
                let inner_ty = self.typecheck_expr(inner)?;
                // M03.2: unary `-` works on any signed-integer kind or float.
                // Unsigned types reject (matches Rust's missing Neg impl).
                if let ast::UnOp::Neg = op {
                    match inner_ty {
                        Ty::Int(k) if k.is_signed() => return Ok(inner_ty),
                        Ty::Float(_) => return Ok(inner_ty),
                        Ty::Int(k) => {
                            return Err(ParseError {
                                message: format!(
                                    "cannot apply unary `-` to `{}` (unsigned types don't impl Neg)",
                                    k.name()
                                ),
                                span: *span,
                            });
                        }
                        _ => {
                            return Err(ParseError {
                                message: format!(
                                    "unary operator `-` requires a numeric operand, found `{}`",
                                    inner_ty.name()
                                ),
                                span: *span,
                            });
                        }
                    }
                }
                let expected = match op {
                    ast::UnOp::Neg => unreachable!("handled above"),
                    ast::UnOp::Not => Ty::Bool,
                };
                if inner_ty != expected {
                    return Err(ParseError {
                        message: format!(
                            "unary operator `{}` requires `{}`, found `{}`",
                            unop_str(*op),
                            expected.name(),
                            inner_ty.name()
                        ),
                        span: *span,
                    });
                }
                Ok(expected)
            }
            ast::Expr::Binary { op, lhs, rhs, span } => {
                self.typecheck_binary(*op, lhs, rhs, *span)
            }
            ast::Expr::Call { callee, args, span } => self.typecheck_call(callee, args, *span),
            ast::Expr::Paren { inner, .. } => self.typecheck_expr(inner),
            ast::Expr::Block(block) => self.typecheck_block(block),
            ast::Expr::If {
                cond,
                then_block,
                else_block,
                span,
            } => {
                let cond_ty = self.typecheck_expr(cond)?;
                if cond_ty != Ty::Bool {
                    return Err(ParseError {
                        message: format!(
                            "`if` condition must be `bool`, found `{}`",
                            cond_ty.name()
                        ),
                        span: cond.span(),
                    });
                }
                let then_ty = self.typecheck_block(then_block)?;
                match else_block {
                    Some(else_block) => {
                        let else_ty = self.typecheck_block(else_block)?;
                        if then_ty != else_ty {
                            return Err(ParseError {
                                message: format!(
                                    "branches of `if` have different types: `{}` vs `{}`",
                                    then_ty.name(),
                                    else_ty.name()
                                ),
                                span: *span,
                            });
                        }
                        Ok(then_ty)
                    }
                    None => {
                        if then_ty != Ty::Unit {
                            return Err(ParseError {
                                message: format!(
                                    "`if` without `else` has type `()`; cannot use as a value of type `{}`",
                                    then_ty.name()
                                ),
                                span: *span,
                            });
                        }
                        Ok(Ty::Unit)
                    }
                }
            }
        }
    }

    fn typecheck_binary(
        &mut self,
        op: ast::BinOp,
        lhs: &ast::Expr,
        rhs: &ast::Expr,
        span: Span,
    ) -> Result<Ty, ParseError> {
        let lhs_ty = self.typecheck_expr(lhs)?;
        let rhs_ty = self.typecheck_expr(rhs)?;
        use ast::BinOp::*;
        match op {
            Add | Sub | Mul | Div | Rem => {
                // M03.2: any same-Int-kind arithmetic is allowed, plus literal
                // coercion to bring an untyped literal into agreement with the
                // other operand.
                let (lhs_ty, rhs_ty) = self.unify_numeric_operands(lhs, rhs, lhs_ty, rhs_ty);
                let unified = match (lhs_ty, rhs_ty) {
                    (Ty::Int(a), Ty::Int(b)) if a == b => Ty::Int(a),
                    (Ty::Float(a), Ty::Float(b)) if a == b => Ty::Float(a),
                    _ => {
                        return Err(ParseError {
                            message: format!(
                                "binary operator `{}` requires both operands to be the same numeric type, found `{}` and `{}`",
                                binop_str(op),
                                lhs_ty.name(),
                                rhs_ty.name()
                            ),
                            span,
                        });
                    }
                };
                Ok(unified)
            }
            Lt | Le | Gt | Ge => {
                let (lhs_ty, rhs_ty) = self.unify_numeric_operands(lhs, rhs, lhs_ty, rhs_ty);
                let ok = matches!((lhs_ty, rhs_ty),
                    (Ty::Int(a), Ty::Int(b)) if a == b)
                    || matches!((lhs_ty, rhs_ty),
                        (Ty::Float(a), Ty::Float(b)) if a == b);
                if !ok {
                    return Err(ParseError {
                        message: format!(
                            "comparison operator `{}` requires both operands to be the same numeric type, found `{}` and `{}`",
                            binop_str(op),
                            lhs_ty.name(),
                            rhs_ty.name()
                        ),
                        span,
                    });
                }
                Ok(Ty::Bool)
            }
            Eq | Neq => {
                if lhs_ty != rhs_ty {
                    return Err(ParseError {
                        message: format!(
                            "equality operator `{}` requires both operands to be the same type, found `{}` and `{}`",
                            binop_str(op),
                            lhs_ty.name(),
                            rhs_ty.name()
                        ),
                        span,
                    });
                }
                if lhs_ty == Ty::Unit {
                    return Err(ParseError {
                        message: format!(
                            "equality operator `{}` cannot compare values of type `()`",
                            binop_str(op)
                        ),
                        span,
                    });
                }
                Ok(Ty::Bool)
            }
            And | Or => {
                if lhs_ty != Ty::Bool || rhs_ty != Ty::Bool {
                    return Err(ParseError {
                        message: format!(
                            "logical operator `{}` requires both operands to be `bool`, found `{}` and `{}`",
                            binop_str(op),
                            lhs_ty.name(),
                            rhs_ty.name()
                        ),
                        span,
                    });
                }
                Ok(Ty::Bool)
            }
        }
    }

    fn typecheck_call(
        &mut self,
        callee: &ast::Expr,
        args: &[ast::Expr],
        call_span: Span,
    ) -> Result<Ty, ParseError> {
        // L1 only supports direct function calls (callee must be an Ident).
        let (callee_name, callee_span) = match callee {
            ast::Expr::Ident(name, sp) => (name.clone(), *sp),
            _ => {
                return Err(ParseError {
                    message: "L1 only supports direct function calls (callee must be a function name)".into(),
                    span: callee.span(),
                });
            }
        };
        let id = *self
            .resolution
            .uses
            .get(&callee_span)
            .expect("callee ident resolved");
        let sig = match self.types.binding_types.get(&id) {
            Some(BindingType::Fn(s)) => s.clone(),
            Some(BindingType::Var(_)) => {
                return Err(ParseError {
                    message: format!("`{callee_name}` is not a function"),
                    span: callee_span,
                });
            }
            None => panic!("binding {id:?} has no type"),
        };
        // NB: do NOT record `expr_types[callee_span]` — the callee is a function
        // reference, not a value (data-model.md VR-11).
        if args.len() != sig.params.len() {
            return Err(ParseError {
                message: format!(
                    "function `{callee_name}` expects {} argument(s), found {}",
                    sig.params.len(),
                    args.len()
                ),
                span: call_span,
            });
        }
        for (i, (arg, &param_ty)) in args.iter().zip(sig.params.iter()).enumerate() {
            let arg_ty = self.typecheck_expr(arg)?;
            if arg_ty != param_ty {
                return Err(ParseError {
                    message: format!(
                        "argument {}: expected `{}`, found `{}`",
                        i + 1,
                        param_ty.name(),
                        arg_ty.name()
                    ),
                    span: arg.span(),
                });
            }
        }
        Ok(sig.ret)
    }

    /// **M03.2**: attempt to coerce a literal expression's type to `target`.
    /// Currently handles `Expr::LitInt(n)` → `Ty::Int(k)` when `k.contains(n)`,
    /// and `Expr::Unary { Neg, LitInt }` → `Ty::Int(k)` when signed `k` fits
    /// the negated literal. Returns `Some(target)` on successful coercion
    /// (and updates the recorded expression type), `None` otherwise.
    fn try_coerce_to(&mut self, expr: &ast::Expr, current: Ty, target: Ty) -> Option<Ty> {
        if current == target {
            return Some(target);
        }
        match (expr, target) {
            (ast::Expr::LitInt(n, span), Ty::Int(k)) => {
                if k.contains(*n as i128) {
                    self.types.expr_types.insert(*span, Ty::Int(k));
                    Some(Ty::Int(k))
                } else {
                    None
                }
            }
            // Integer literal annotated as float: `let x: f64 = 5;` is valid Rust.
            (ast::Expr::LitInt(_, span), Ty::Float(k)) => {
                self.types.expr_types.insert(*span, Ty::Float(k));
                Some(Ty::Float(k))
            }
            // Float literal coerces between f32/f64 freely (narrowing happens at eval).
            (ast::Expr::LitFloat(_, span), Ty::Float(k)) => {
                self.types.expr_types.insert(*span, Ty::Float(k));
                Some(Ty::Float(k))
            }
            (ast::Expr::Unary { op: ast::UnOp::Neg, expr: inner, span }, Ty::Int(k))
                if k.is_signed() =>
            {
                if let ast::Expr::LitInt(n, inner_span) = inner.as_ref() {
                    let negated = -(*n as i128);
                    if k.contains(negated) {
                        self.types.expr_types.insert(*inner_span, Ty::Int(k));
                        self.types.expr_types.insert(*span, Ty::Int(k));
                        return Some(Ty::Int(k));
                    }
                }
                None
            }
            // Unary `-` on a float literal: coerce the float to the target kind.
            (ast::Expr::Unary { op: ast::UnOp::Neg, expr: inner, span }, Ty::Float(k)) => {
                if let ast::Expr::LitFloat(_, inner_span) = inner.as_ref() {
                    self.types.expr_types.insert(*inner_span, Ty::Float(k));
                    self.types.expr_types.insert(*span, Ty::Float(k));
                    return Some(Ty::Float(k));
                }
                // Also allow unary `-` on an int literal annotated as float.
                if let ast::Expr::LitInt(_, inner_span) = inner.as_ref() {
                    self.types.expr_types.insert(*inner_span, Ty::Float(k));
                    self.types.expr_types.insert(*span, Ty::Float(k));
                    return Some(Ty::Float(k));
                }
                None
            }
            _ => None,
        }
    }

    /// **M03.2**: try to bring the two operands of a binary op to a common
    /// numeric type by coercing whichever side is a literal. If neither side
    /// is a literal (or coercion fails), returns the types unchanged — the
    /// caller will then issue a cross-type typeck error.
    fn unify_numeric_operands(
        &mut self,
        lhs: &ast::Expr,
        rhs: &ast::Expr,
        lhs_ty: Ty,
        rhs_ty: Ty,
    ) -> (Ty, Ty) {
        if lhs_ty == rhs_ty {
            return (lhs_ty, rhs_ty);
        }
        if let Some(new_rhs) = self.try_coerce_to(rhs, rhs_ty, lhs_ty) {
            return (lhs_ty, new_rhs);
        }
        if let Some(new_lhs) = self.try_coerce_to(lhs, lhs_ty, rhs_ty) {
            return (new_lhs, rhs_ty);
        }
        (lhs_ty, rhs_ty)
    }
}

fn ty_from_ast(t: &ast::Type) -> Result<Ty, ParseError> {
    match t {
        ast::Type::Unit { .. } => Ok(Ty::Unit),
        ast::Type::Path { segments, span } => {
            if segments.len() != 1 {
                return Err(ParseError {
                    message: "multi-segment type paths are not supported in L1".into(),
                    span: *span,
                });
            }
            match segments[0].as_str() {
                "i8" => Ok(Ty::Int(IntKind::I8)),
                "i16" => Ok(Ty::Int(IntKind::I16)),
                "i32" => Ok(Ty::Int(IntKind::I32)),
                "i64" => Ok(Ty::Int(IntKind::I64)),
                "i128" => Ok(Ty::Int(IntKind::I128)),
                "u8" => Ok(Ty::Int(IntKind::U8)),
                "u16" => Ok(Ty::Int(IntKind::U16)),
                "u32" => Ok(Ty::Int(IntKind::U32)),
                "u64" => Ok(Ty::Int(IntKind::U64)),
                "u128" => Ok(Ty::Int(IntKind::U128)),
                "isize" => Ok(Ty::Int(IntKind::ISize)),
                "usize" => Ok(Ty::Int(IntKind::USize)),
                "f32" => Ok(Ty::Float(FloatKind::F32)),
                "f64" => Ok(Ty::Float(FloatKind::F64)),
                "bool" => Ok(Ty::Bool),
                other => Err(ParseError {
                    message: format!("unknown type `{other}`"),
                    span: *span,
                }),
            }
        }
    }
}

fn binop_str(op: ast::BinOp) -> &'static str {
    use ast::BinOp::*;
    match op {
        Add => "+",
        Sub => "-",
        Mul => "*",
        Div => "/",
        Rem => "%",
        Lt => "<",
        Le => "<=",
        Gt => ">",
        Ge => ">=",
        Eq => "==",
        Neq => "!=",
        And => "&&",
        Or => "||",
    }
}

fn unop_str(op: ast::UnOp) -> &'static str {
    match op {
        ast::UnOp::Neg => "-",
        ast::UnOp::Not => "!",
    }
}
