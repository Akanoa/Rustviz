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
    /// **M07.4**: struct declaration `struct Name { f1: T1, f2: T2 }`. At
    /// least one field required (empty structs rejected at parse time).
    Struct(StructDecl),
    /// **M07.4**: impl block. M07.4 was inherent-only; M07.6 extended via
    /// `ImplBlock.trait_name: Option<String>` to support trait impls
    /// (`impl Trait for Type`).
    Impl(ImplBlock),
    /// **M07.6**: trait declaration `trait Name { fn item1; fn item2 { ... }; }`.
    /// Items are either required (signature-only) or default (with body).
    Trait(TraitDecl),
}

/// **M07.6**: trait declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct TraitDecl {
    /// Trait name (e.g. `"Show"`).
    pub name: String,
    /// Trait items in declaration order.
    pub items: Vec<TraitItem>,
    /// Span from `trait` keyword through closing `}`.
    pub span: Span,
}

/// **M07.6**: one item declared inside a trait — either a required method
/// (signature only, no body — impl must provide) or a default method
/// (with a body the impl can override or fall through to).
#[derive(Debug, Clone, PartialEq)]
pub enum TraitItem {
    /// Required method — signature only.
    Required {
        /// Method name.
        name: String,
        /// Param list (first param must be a self-receiver per `Param.kind`).
        params: Vec<Param>,
        /// Optional return type annotation.
        return_ty: Option<Type>,
        /// Span from `fn` keyword through `;`.
        span: Span,
    },
    /// Default method — has a body. Impl can override or fall through.
    Default {
        /// Full FnDecl with body.
        decl: FnDecl,
    },
}

/// **M07.4**: struct declaration with named fields.
#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    /// Type name (e.g. `"Point"`).
    pub name: String,
    /// **M07.5**: type parameters (`<T>`). Empty for non-generic structs.
    pub type_params: Vec<TypeParam>,
    /// Fields in declaration order. At least one — order drives byte layout
    /// AND drop order.
    pub fields: Vec<StructField>,
    /// Span from `struct` keyword through closing `}`.
    pub span: Span,
}

/// **M07.5** + **M07.6**: type parameter declared on a fn or struct.
/// M07.5 had `bound: Option<String>` (parser-stored, typeck-rejected);
/// M07.6 promotes to `bounds: Vec<String>` (multi-bound supported,
/// typeck-checked: each bound must reference an existing trait).
#[derive(Debug, Clone, PartialEq)]
pub struct TypeParam {
    /// Type parameter name (e.g. `"T"`).
    pub name: String,
    /// **M07.6**: trait bounds (`T: Foo + Bar` → `vec!["Foo", "Bar"]`).
    /// Empty for unbounded params (M07.5 case). Each bound must reference
    /// a registered trait (typeck phase 1 verifies).
    pub bounds: Vec<String>,
    /// Span covering the name.
    pub span: Span,
}

/// **M07.4**: one declared field of a struct.
#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    /// Field name.
    pub name: String,
    /// Field type.
    pub ty: Type,
    /// Span covering `name: ty`.
    pub span: Span,
}

/// **M07.4** + **M07.6**: impl block. M07.4 introduced inherent impls
/// (`trait_name = None`); M07.6 extends with trait impls
/// (`trait_name = Some("Show")` for `impl Show for Point { .. }`).
#[derive(Debug, Clone, PartialEq)]
pub struct ImplBlock {
    /// **M07.6**: `None` for inherent impls (M07.4 behavior);
    /// `Some(name)` for trait impls (`impl <name> for <ty_name>`).
    pub trait_name: Option<String>,
    /// Receiver type name (single-segment — `"Point"`).
    pub ty_name: String,
    /// Associated functions + methods. Each fn may have a self-receiver as
    /// its first `Param` (via `Param.kind`).
    pub items: Vec<FnDecl>,
    /// Span from `impl` keyword through closing `}`.
    pub span: Span,
}

/// A function declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    /// Function name.
    pub name: String,
    /// **M07.5**: type parameters (`<T>`). Empty for non-generic fns.
    pub type_params: Vec<TypeParam>,
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
    /// **M07.4**: param classification — distinguishes regular params from
    /// the four self-receiver shapes that only appear inside `impl` blocks.
    /// Pre-M07.4 free-fn params are always `Normal`.
    pub kind: ParamKind,
    /// Span covering the parameter (name through type).
    pub span: Span,
}

/// **M07.4**: param classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    /// Regular `name: Type` param.
    Normal,
    /// `self` — owned receiver. Only valid at param index 0 of an impl-block fn.
    SelfOwned,
    /// `&self` — shared borrow receiver.
    SelfShared,
    /// `&mut self` — mutable borrow receiver.
    SelfMut,
}

/// A type annotation.
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// Path-like type (e.g. `i32`, `std::option::Option`, `Wrapper<i32>`).
    Path {
        /// Path segments split on `::`.
        segments: Vec<String>,
        /// **M07.5**: type-args list for `Wrapper<i32>` annotations. Empty
        /// for plain `i32`/`bool` etc. M07 generic built-ins (Box<T>, Vec<T>)
        /// continue to use `Type::Generic`; M07.5+ uses this field for
        /// user-defined generic types.
        type_args: Vec<Type>,
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
    /// **M07.7**: trait-object type `dyn TraitName`. The `&` / `&mut` wrap
    /// is handled by the outer `Type::Ref { inner: Type::DynTrait, .. }`
    /// pattern; bare `Type::DynTrait` only appears inside `Box<dyn _>`
    /// (the wrapping `Box<T>` machinery provides the indirection).
    DynTrait {
        /// Single-segment trait name (e.g. `"Show"`).
        trait_name: String,
        /// Span from `dyn` keyword through the trait name.
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
        /// **M07.5**: turbofish type-args (`id::<bool>`, `Wrapper::<i32>`).
        /// Empty for non-turbofish paths (e.g. `Vec::new`).
        type_args: Vec<Type>,
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
    /// **M07.4**: struct literal `Path { f1: e1, f2: e2 }`. Single-segment
    /// `path` in M07.4 (multi-segment typeck-rejected).
    StructLit {
        /// Path segments naming the struct type. `["Point"]` in M07.4.
        path: Vec<String>,
        /// **M07.5**: turbofish type-args (`Wrapper::<i32> { v: 5 }`).
        /// Empty for the inferred case (`Wrapper { v: 5 }`).
        type_args: Vec<Type>,
        /// Field initializers in source order.
        fields: Vec<StructLitField>,
        /// Span from path start through closing `}`.
        span: Span,
    },
    /// **M07.4**: field access `receiver.name`. Postfix; `name` is an
    /// identifier NOT followed by `(` (a trailing `(` would be `MethodCall`).
    FieldAccess {
        /// Receiver expression.
        receiver: Box<Expr>,
        /// Field name.
        name: String,
        /// Span from receiver start through `name`.
        span: Span,
    },
    /// **M07.7**: cast expression `inner as TargetType`. In M07.7 only used
    /// for `&p as &dyn Show` coercion; future numeric/string casts would
    /// reuse this AST node.
    Cast {
        /// The value being cast.
        inner: Box<Expr>,
        /// Destination type.
        target_ty: Type,
        /// Span from `inner` start through `target_ty`'s end.
        span: Span,
    },
}

/// **M07.4**: one field initializer inside a struct literal.
#[derive(Debug, Clone, PartialEq)]
pub struct StructLitField {
    /// Field name.
    pub name: String,
    /// Field value. `None` indicates field-shorthand `Point { x, y }` —
    /// resolves to the local binding of the same name at typeck/eval.
    pub value: Option<Expr>,
    /// Span covering `name: value` (or just `name` for shorthand).
    pub span: Span,
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
            | Self::ArrayLit { span, .. }
            | Self::StructLit { span, .. }
            | Self::FieldAccess { span, .. }
            | Self::Cast { span, .. } => *span,
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
