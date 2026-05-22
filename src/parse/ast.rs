//! AST types for Level 1 syntax. Every node carries a [`Span`].
//!
//! R-018 decision (open question resolved during implementation): `Type` is
//! represented generically as `Type::Path { segments, span }` or `Type::Unit`.
//! M01 only parses; M02 will resolve path segments to typed kinds (`i32`,
//! `bool`, etc.). Keeping the AST general avoids special-casing primitives
//! before name resolution exists.

use super::span::Span;

/// Top-level program: ordered list of items.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    /// Items in source order.
    pub items: Vec<Item>,
    /// Span covering the program (`0..0` for empty input).
    pub span: Span,
}

/// A top-level item.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// Function declaration.
    Fn(FnDecl),
}

/// A function declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    /// Function name.
    pub name: String,
    /// Parameter list (may be empty).
    pub params: Vec<Param>,
    /// Optional return type annotation (after `->`).
    pub return_ty: Option<Type>,
    /// Function body.
    pub body: Block,
    /// Span from `fn` keyword through closing `}` of the body.
    pub span: Span,
}

/// A function parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    /// Parameter name.
    pub name: String,
    /// Parameter type.
    pub ty: Type,
    /// Span covering the parameter (name through type).
    pub span: Span,
}

/// A type annotation.
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// Path-like type (e.g. `i32`, `std::option::Option`).
    Path {
        /// Path segments split on `::`.
        segments: Vec<String>,
        /// Span covering the path.
        span: Span,
    },
    /// Unit type `()`.
    Unit {
        /// Span covering `()`.
        span: Span,
    },
    /// **M06**: `&T` or `&mut T` reference type.
    Ref {
        /// Pointed-to type.
        inner: Box<Type>,
        /// `true` for `&mut T`, `false` for `&T`.
        mutable: bool,
        /// Span covering the `&` (or `&mut`) plus the inner type.
        span: Span,
    },
}

/// A block: zero or more statements followed by an optional tail expression.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// Statements in source order.
    pub stmts: Vec<Stmt>,
    /// Optional trailing expression without a `;` — if `Some`, this block's
    /// value is the tail expression's value.
    pub tail: Option<Box<Expr>>,
    /// Span from `{` through `}`.
    pub span: Span,
}

/// A statement.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `let` or `let mut` binding with required initializer.
    Let(LetStmt),
    /// An expression followed by `;` (its value is discarded).
    Expr(Expr),
}

/// A `let` binding.
#[derive(Debug, Clone, PartialEq)]
pub struct LetStmt {
    /// `true` if declared with `let mut`.
    pub mutable: bool,
    /// Binding name.
    pub name: String,
    /// Optional type annotation.
    pub ty: Option<Type>,
    /// Initializer expression (required in M01 — no uninitialized bindings).
    pub init: Expr,
    /// Span from `let` through `;`.
    pub span: Span,
}

/// An expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Integer literal. The optional kind suffix (`5u8`, `5_i64`, …) is
    /// `Some` when the source provided one; typeck then uses it directly
    /// instead of coercing from default.
    LitInt(i64, Option<crate::typeck::IntKind>, Span),
    /// **M03.2**: Float literal. Stores `f64`; narrowed to `f32` by typeck
    /// when the surrounding annotation says `f32` or when a `f32` suffix
    /// is present.
    LitFloat(f64, Option<crate::typeck::FloatKind>, Span),
    /// Boolean literal.
    LitBool(bool, Span),
    /// Identifier reference (resolved in M02).
    Ident(String, Span),
    /// Unary operation.
    Unary {
        /// Operator.
        op: UnOp,
        /// Operand.
        expr: Box<Expr>,
        /// Span from operator through operand.
        span: Span,
    },
    /// Binary operation.
    Binary {
        /// Operator.
        op: BinOp,
        /// Left-hand side.
        lhs: Box<Expr>,
        /// Right-hand side.
        rhs: Box<Expr>,
        /// Span from start of LHS through end of RHS.
        span: Span,
    },
    /// Function call.
    Call {
        /// Callee expression.
        callee: Box<Expr>,
        /// Argument list.
        args: Vec<Expr>,
        /// Span from start of callee through `)`.
        span: Span,
    },
    /// Parenthesized expression (preserved for round-trip readability).
    Paren {
        /// Inner expression.
        inner: Box<Expr>,
        /// Span from `(` through `)`.
        span: Span,
    },
    /// Block as an expression (its tail expression is the value).
    Block(Box<Block>),
    /// `if cond { then } else? { else }`.
    If {
        /// Condition.
        cond: Box<Expr>,
        /// Then branch.
        then_block: Box<Block>,
        /// Optional else branch.
        else_block: Option<Box<Block>>,
        /// Span from `if` keyword through end of last branch.
        span: Span,
    },
    /// **M06**: `&place` or `&mut place`. The `inner` must be a place
    /// expression (currently only `Expr::Ident(_, _)` in L2).
    Borrow {
        /// The expression being borrowed.
        inner: Box<Expr>,
        /// `true` for `&mut`, `false` for `&`.
        mutable: bool,
        /// Span from `&` (or `&mut`) through end of `inner`.
        span: Span,
    },
}

impl Expr {
    /// Source span of this expression.
    pub fn span(&self) -> Span {
        match self {
            Self::LitInt(_, _, s)
            | Self::LitFloat(_, _, s)
            | Self::LitBool(_, s)
            | Self::Ident(_, s) => *s,
            Self::Unary { span, .. }
            | Self::Binary { span, .. }
            | Self::Call { span, .. }
            | Self::Paren { span, .. }
            | Self::If { span, .. }
            | Self::Borrow { span, .. } => *span,
            Self::Block(b) => b.span,
        }
    }
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    /// `-x`
    Neg,
    /// `!x`
    Not,
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    /// `||` — lowest precedence.
    Or,
    /// `&&`
    And,
    /// `==`
    Eq,
    /// `!=`
    Neq,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*` — highest precedence (along with `/` and `%`).
    Mul,
    /// `/`
    Div,
    /// `%`
    Rem,
}
