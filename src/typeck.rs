//! Lightweight type checking: validate annotations and propagate L1 value
//! types. Consumes a [`Resolution`] from [`crate::resolve`].

use indexmap::IndexMap;

use crate::parse::ast;
use crate::parse::error::ParseError;
use crate::parse::span::Span;
use crate::resolve::{BindingId, BindingKind, Resolution};

/// L1 value types. Three primitives — function signatures live in [`FnSig`],
/// not here, because functions are not first-class values in L1.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Ty {
    /// 32-bit signed integer.
    I32,
    /// Boolean.
    Bool,
    /// Unit `()`.
    Unit,
}

impl Ty {
    /// Render this type as a user-facing string (`"i32"`, `"bool"`, `"()"`).
    pub fn name(self) -> &'static str {
        match self {
            Self::I32 => "i32",
            Self::Bool => "bool",
            Self::Unit => "()",
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
            ast::Expr::LitInt(_, _) => Ok(Ty::I32),
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
                let expected = match op {
                    ast::UnOp::Neg => Ty::I32,
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
                if lhs_ty != Ty::I32 || rhs_ty != Ty::I32 {
                    return Err(ParseError {
                        message: format!(
                            "binary operator `{}` requires both operands to be `i32`, found `{}` and `{}`",
                            binop_str(op),
                            lhs_ty.name(),
                            rhs_ty.name()
                        ),
                        span,
                    });
                }
                Ok(Ty::I32)
            }
            Lt | Le | Gt | Ge => {
                if lhs_ty != Ty::I32 || rhs_ty != Ty::I32 {
                    return Err(ParseError {
                        message: format!(
                            "comparison operator `{}` requires both operands to be `i32`, found `{}` and `{}`",
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
                "i32" => Ok(Ty::I32),
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
