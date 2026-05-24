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
    /// **M07**: generic type path `Vec<i32>`, `Box<i32>`. Multi-segment paths
    /// with type arguments. `String` parses as `Type::Path { segments: ["String"] }`.
    Generic {
        /// Path segments (e.g. `["Vec"]` for `Vec<T>`).
        segments: Vec<String>,
        /// Generic type arguments (e.g. `[i32]` for `Vec<i32>`).
        args: Vec<Type>,
        /// Span covering `segments<args>`.
        span: Span,
    },
    /// **M07.1**: slice type `&[T]` or `&mut [T]`. The leading `&` is
    /// absorbed into the slice type (matches Rust's "[T] only appears
    /// behind a reference"). M07.1 typeck rejects `mutable: true`.
    Slice {
        /// Element type.
        inner: Box<Type>,
        /// `true` for `&mut [T]`, `false` for `&[T]`.
        mutable: bool,
        /// Span from `&` (or `&mut`) through `]`.
        span: Span,
    },
    /// **M07.3**: array type annotation `[T; N]` where N is an integer
    /// literal. No const expressions, no const generics in M07.3.
    Array {
        /// Element type.
        inner: Box<Type>,
        /// Compile-time-known size N.
        size: u64,
        /// Span from `[` through `]`.
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
    /// **M06.1**: assignment statement `lhs = rhs;`. The `lhs` must be a
    /// place expression — typeck restricts it to `Expr::Ident(_, _)` (direct
    /// assignment to a `mut` binding) or `Expr::Deref(Expr::Ident(_, _))`
    /// (write through a `&mut` reference). Emits a `MemEvent::SlotWrite`
    /// event at the resolved target slot.
    Assign {
        /// Place expression on the left side.
        lhs: Expr,
        /// Value expression on the right side.
        rhs: Expr,
        /// Span from start of `lhs` through `;`.
        span: Span,
    },
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
    /// **M06.1**: `*expr` — read through a reference (rvalue), or write
    /// through a reference when used as the lhs of `Stmt::Assign` (lvalue).
    /// Typeck requires `inner`'s type to be `Ty::Ref { .. }`.
    Deref {
        /// The expression being dereferenced.
        inner: Box<Expr>,
        /// Span from `*` through end of inner.
        span: Span,
    },
    /// **M07**: string literal `"..."`. Used as argument to `String::from(...)`
    /// and `String::push_str(...)`.
    StrLit(String, Span),
    /// **M07**: multi-segment path `Vec::new`, `Box::new`, `String::from`.
    /// Single-segment idents stay as `Expr::Ident`.
    Path {
        /// Path segments (≥ 2).
        segments: Vec<String>,
        /// Span covering the path.
        span: Span,
    },
    /// **M07**: method call `receiver.method(args)`. Dispatched in typeck
    /// against the hardcoded `(receiver_ty, name)` table (no traits).
    MethodCall {
        /// Receiver expression.
        receiver: Box<Expr>,
        /// Method name.
        name: String,
        /// Method arguments.
        args: Vec<Expr>,
        /// Span from receiver start through `)`.
        span: Span,
    },
    /// **M07**: indexing `receiver[index]`. Rvalue-only in M07.
    Index {
        /// Receiver expression (must be `Ty::Vec(T)`).
        receiver: Box<Expr>,
        /// Index expression (must be `Ty::Int(_)`).
        index: Box<Expr>,
        /// Span from receiver start through `]`.
        span: Span,
    },
    /// **M07.1**: range expression `a..b`, `..b`, `a..`, `..`. M07.1 parses
    /// this only inside `Expr::Index.index`; typeck rejects standalone uses.
    Range {
        /// Start bound (inclusive). `None` defaults to 0 at eval time.
        start: Option<Box<Expr>>,
        /// End bound (exclusive). `None` defaults to receiver length at eval time.
        end: Option<Box<Expr>>,
        /// Span covering the whole range expression, including any bounds.
        span: Span,
    },
    /// **M07.3**: array literal `[e1, e2, ..., eN]`. Size N is implicit
    /// (= `elements.len()`). Empty literal `[]` is allowed at parse time
    /// but typeck-rejected unless paired with an explicit type annotation
    /// (can't infer element type from zero elements).
    ArrayLit {
        /// Element expressions in source order.
        elements: Vec<Expr>,
        /// Span from `[` through `]`.
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
            | Self::Borrow { span, .. }
            | Self::Deref { span, .. }
            | Self::Path { span, .. }
            | Self::MethodCall { span, .. }
            | Self::Index { span, .. }
            | Self::Range { span, .. }
            | Self::ArrayLit { span, .. } => *span,
            Self::StrLit(_, s) => *s,
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
