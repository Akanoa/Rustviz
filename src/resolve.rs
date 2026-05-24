//! Name resolution: assign a stable [`BindingId`] to every introduced binding
//! and resolve every identifier use site.

use std::collections::HashMap;

use indexmap::IndexMap;

use crate::parse::ast;
use crate::parse::error::ParseError;
use crate::parse::span::Span;

/// Unique, stable identifier for an introduced binding (fn, let, param).
///
/// Allocated sequentially during the resolve pass. Shadowing creates a NEW
/// `BindingId` — `let x = 5; let x = true;` produces two distinct ids.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BindingId(pub u32);

/// How a binding was introduced.
#[derive(Debug, Clone, PartialEq)]
pub enum BindingKind {
    /// Top-level function declaration (forward-visible across the program).
    Fn,
    /// Let-binding inside a block. `mutable` is true for `let mut`.
    Let {
        /// `true` if declared with `let mut`.
        mutable: bool,
    },
    /// Function parameter.
    Param,
}

/// Where a binding was declared, with diagnostic-friendly span info.
///
/// `decl_span` and `name_span` may coincide for let/param bindings — the M01 AST
/// doesn't store a separate span for just the binding name, so we use the AST
/// node's span for both. This is a minor diagnostic compromise (highlighting
/// the whole declaration rather than just the name) but doesn't affect
/// correctness.
#[derive(Debug, Clone, PartialEq)]
pub struct BindingDecl {
    /// Source name of the binding.
    pub name: String,
    /// How it was introduced.
    pub kind: BindingKind,
    /// Span of the introducing form (the `let` stmt's span, the param's span, or the fn decl's span).
    pub decl_span: Span,
    /// Span of the binding name (currently equal to `decl_span` per the note above).
    pub name_span: Span,
}

/// Output of [`resolve`]. Maps every identifier use site to its [`BindingId`]
/// and records the declaration metadata for each id.
#[derive(Debug, Clone, Default)]
pub struct Resolution {
    /// Maps each `Expr::Ident` use site (by span) to its declaration's `BindingId`.
    /// Iteration order is tree-walk pre-order (see research.md R-002).
    pub uses: IndexMap<Span, BindingId>,
    /// Maps each `BindingId` to its declaration metadata.
    pub bindings: IndexMap<BindingId, BindingDecl>,
}

/// Resolve all identifier uses in `program` to `BindingId`s.
///
/// On success, returns a `Resolution` with `uses` covering every `Expr::Ident`
/// in the program and `bindings` cataloguing every introduced binding.
/// On failure, returns a single `ParseError` (stop-at-first-error).
pub fn resolve(program: &ast::Program) -> Result<Resolution, ParseError> {
    let mut r = Resolver::new();
    // The outermost scope holds top-level fn items, forward-declared before any body is walked.
    r.push_scope();
    for item in &program.items {
        match item {
            ast::Item::Fn(decl) => {
                // Top-level fn names just shadow each other on duplicate (we don't reject;
                // it's odd but Rust's actual behavior is to reject and we follow the
                // lightweight M02 stance of permissive shadowing — see spec edge cases).
                r.declare(decl.name.clone(), BindingKind::Fn, decl.span, decl.span);
            }
            // **M07.4**: structs and impl blocks don't introduce value-level
            // bindings (the type lives in typeck's `StructRegistry`). No
            // declare() needed here.
            ast::Item::Struct(_) | ast::Item::Impl(_) => {}
        }
    }
    for item in &program.items {
        match item {
            ast::Item::Fn(decl) => r.resolve_fn(decl)?,
            // **M07.4**: structs have no resolvable bodies — fields are
            // types, which name-resolve to top-level type bindings (handled
            // in typeck). Impl blocks' fn items are resolved here just like
            // top-level fns, but in a fresh param scope each.
            ast::Item::Struct(_) => {}
            ast::Item::Impl(block) => {
                for fn_decl in &block.items {
                    r.resolve_fn(fn_decl)?;
                }
            }
        }
    }
    r.pop_scope();
    Ok(r.resolution)
}

struct Resolver {
    /// Innermost scope last.
    scopes: Vec<HashMap<String, BindingId>>,
    next_id: u32,
    resolution: Resolution,
}

impl Resolver {
    fn new() -> Self {
        Self {
            scopes: Vec::new(),
            next_id: 0,
            resolution: Resolution::default(),
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare(
        &mut self,
        name: String,
        kind: BindingKind,
        decl_span: Span,
        name_span: Span,
    ) -> BindingId {
        let id = BindingId(self.next_id);
        self.next_id += 1;
        self.scopes
            .last_mut()
            .expect("at least one scope is active when declaring")
            .insert(name.clone(), id);
        self.resolution.bindings.insert(
            id,
            BindingDecl {
                name,
                kind,
                decl_span,
                name_span,
            },
        );
        id
    }

    fn lookup(&self, name: &str) -> Option<BindingId> {
        for scope in self.scopes.iter().rev() {
            if let Some(&id) = scope.get(name) {
                return Some(id);
            }
        }
        None
    }

    fn resolve_fn(&mut self, decl: &ast::FnDecl) -> Result<(), ParseError> {
        // A function-body scope: holds params + the outer "block" of the body.
        // We push one scope for params and let the body's parse_block call push
        // a second scope for the block's interior. Resolution sees params in
        // the outer scope when walking the inner block.
        self.push_scope();
        for param in &decl.params {
            if self
                .scopes
                .last()
                .expect("param scope active")
                .contains_key(&param.name)
            {
                return Err(ParseError {
                    message: format!("duplicate parameter `{}`", param.name),
                    span: param.span,
                });
            }
            self.declare(
                param.name.clone(),
                BindingKind::Param,
                param.span,
                param.span,
            );
        }
        self.resolve_block(&decl.body)?;
        self.pop_scope();
        Ok(())
    }

    fn resolve_block(&mut self, block: &ast::Block) -> Result<(), ParseError> {
        self.push_scope();
        for stmt in &block.stmts {
            self.resolve_stmt(stmt)?;
        }
        if let Some(tail) = &block.tail {
            self.resolve_expr(tail)?;
        }
        self.pop_scope();
        Ok(())
    }

    fn resolve_stmt(&mut self, stmt: &ast::Stmt) -> Result<(), ParseError> {
        match stmt {
            ast::Stmt::Let(let_stmt) => {
                // Init is resolved BEFORE the new binding is declared, so the RHS
                // sees the outer-scope binding (spec FR-005 / shadowing-chain edge case).
                self.resolve_expr(&let_stmt.init)?;
                let mutable = let_stmt.mutable;
                self.declare(
                    let_stmt.name.clone(),
                    BindingKind::Let { mutable },
                    let_stmt.span,
                    let_stmt.span,
                );
            }
            ast::Stmt::Expr(expr) => {
                self.resolve_expr(expr)?;
            }
            // **M06.1**: resolve both sides of an assignment. No new binding.
            ast::Stmt::Assign { lhs, rhs, .. } => {
                self.resolve_expr(lhs)?;
                self.resolve_expr(rhs)?;
            }
        }
        Ok(())
    }

    fn resolve_expr(&mut self, expr: &ast::Expr) -> Result<(), ParseError> {
        match expr {
            ast::Expr::LitInt(_, _, _)
            | ast::Expr::LitFloat(_, _, _)
            | ast::Expr::LitBool(_, _)
            | ast::Expr::StrLit(_, _)
            | ast::Expr::Path { .. } => {}
            ast::Expr::Ident(name, span) => match self.lookup(name) {
                Some(id) => {
                    self.resolution.uses.insert(*span, id);
                }
                None => {
                    return Err(ParseError {
                        message: format!("use of undeclared variable `{name}`"),
                        span: *span,
                    });
                }
            },
            ast::Expr::Unary { expr, .. } => self.resolve_expr(expr)?,
            ast::Expr::Borrow { inner, .. } => self.resolve_expr(inner)?,
            ast::Expr::Deref { inner, .. } => self.resolve_expr(inner)?,
            ast::Expr::MethodCall { receiver, args, .. } => {
                self.resolve_expr(receiver)?;
                for arg in args {
                    self.resolve_expr(arg)?;
                }
            }
            ast::Expr::Index { receiver, index, .. } => {
                self.resolve_expr(receiver)?;
                self.resolve_expr(index)?;
            }
            ast::Expr::Range { start, end, .. } => {
                if let Some(s) = start {
                    self.resolve_expr(s)?;
                }
                if let Some(e) = end {
                    self.resolve_expr(e)?;
                }
            }
            ast::Expr::ArrayLit { elements, .. } => {
                for e in elements {
                    self.resolve_expr(e)?;
                }
            }
            // **M07.4**: struct literal — resolve each non-shorthand field value;
            // shorthand fields (value: None) are resolved here too as if they
            // were `name: name` (each shorthand resolves the bare `name` as an
            // identifier use in the current scope). This lets a missing local
            // surface the standard "use of undeclared variable" error pointing
            // at the shorthand's span.
            ast::Expr::StructLit { fields, .. } => {
                for field in fields {
                    match &field.value {
                        Some(expr) => self.resolve_expr(expr)?,
                        None => {
                            // Shorthand: resolve `name` in the current scope.
                            match self.lookup(&field.name) {
                                Some(id) => {
                                    self.resolution.uses.insert(field.span, id);
                                }
                                None => {
                                    return Err(ParseError {
                                        message: format!(
                                            "no local named `{}` in scope for field-shorthand",
                                            field.name
                                        ),
                                        span: field.span,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            // **M07.4**: field access — recurse on receiver only; field name
            // is a symbol-lookup deferred to typeck (it's not a binding).
            ast::Expr::FieldAccess { receiver, .. } => self.resolve_expr(receiver)?,
            ast::Expr::Binary { lhs, rhs, .. } => {
                self.resolve_expr(lhs)?;
                self.resolve_expr(rhs)?;
            }
            ast::Expr::Call { callee, args, .. } => {
                self.resolve_expr(callee)?;
                for arg in args {
                    self.resolve_expr(arg)?;
                }
            }
            ast::Expr::Paren { inner, .. } => self.resolve_expr(inner)?,
            ast::Expr::Block(block) => self.resolve_block(block)?,
            ast::Expr::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.resolve_expr(cond)?;
                self.resolve_block(then_block)?;
                if let Some(else_block) = else_block {
                    self.resolve_block(else_block)?;
                }
            }
        }
        Ok(())
    }
}
