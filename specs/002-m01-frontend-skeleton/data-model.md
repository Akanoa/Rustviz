# Data Model — M01 Entities

The data of M01 is the set of types exposed by `src/parse/`. Spans on every node, owned ASTs, single error type.

## Entity: `FileId`

```rust
pub struct FileId(pub u32);
```

Newtype wrapping a 32-bit file identifier. Allocated by `SourceMap` on registration. `0` is reserved for "no file" / sentinel.

## Entity: `Span`

```rust
pub struct Span {
    pub start: u32,
    pub end: u32,
    pub file: FileId,
}
```

Half-open byte range `[start, end)` into a registered source file.

### Validation rules

- **VR-1**: `start <= end`.
- **VR-2**: `file` must refer to a `FileId` registered in the active `SourceMap` at error / display time.
- **VR-3**: Spans MUST point at the byte range whose source the AST node was derived from. No `Span(0, 0)` placeholders in successful parse output (SC-002).

## Entity: `SourceMap`

```rust
pub struct SourceMap {
    files: HashMap<FileId, SourceFile>,
    next_id: u32,
}

pub struct SourceFile {
    pub name: String,
    pub src: String,
    line_starts: Vec<u32>, // byte offset of each line's first character; index = line number (0-based)
}
```

### Operations

- `pub fn add(&mut self, name: String, src: String) -> FileId` — registers a file, returns its id.
- `pub fn get(&self, file: FileId) -> Option<&SourceFile>` — fetches the registered file.
- `pub fn line_col(&self, span: Span) -> Option<(u32, u32)>` — derives 1-based `(line, col)` from a span's `start` using `line_starts`. Computed on demand (FR-009).

### Validation rules

- **VR-4**: `line_starts[0] == 0`.
- **VR-5**: `line_starts` is strictly increasing.

## Entity: `Token`

```rust
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}
```

## Entity: `TokenKind`

```rust
pub enum TokenKind {
    // Literals
    Int(i64),
    Bool(bool),

    // Identifiers
    Ident(String),

    // Keywords
    Let, Mut, Fn, If, Else, Return, True, False,

    // Operators
    Plus, Minus, Star, Slash, Percent,
    Eq, EqEq, BangEq,
    Lt, Le, Gt, Ge,
    AndAnd, OrOr, Bang,
    Arrow, // ->

    // Punctuation
    LParen, RParen, LBrace, RBrace,
    Comma, Semi, Colon,

    // End
    Eof,
}
```

`True` and `False` are both keywords (recognized at lex time) and produce `Bool` literal tokens — implementation can collapse them: the lexer emits `TokenKind::Bool(true)` directly when it sees `true`/`false`.

### Validation rules

- **VR-6**: `&` and `&mut` MUST NOT appear in `TokenKind` for M01. Lexer rejects them per FR-005 / R-014. M06 will add `Amp` and `AmpMut` variants in place.
- **VR-7**: `TokenKind::Ident(s)` — `s` is non-empty, starts with `[A-Za-z_]`, continues with `[A-Za-z0-9_]`.

## Entity: AST

The AST is rooted at `Program`. Every node carries a `span` field.

```rust
pub struct Program {
    pub items: Vec<Item>,
    pub span: Span,
}

pub enum Item {
    Fn(FnDecl),
}

pub struct FnDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub return_ty: Option<Type>,
    pub body: Block,
    pub span: Span,
}

pub struct Param {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

pub enum Type {
    /// Primitive name lookup deferred to M02. M01 parses into Path; M02 resolves "i32"/"bool"/"()" etc.
    Path { segments: Vec<String>, span: Span },
    Unit { span: Span },
}

pub struct Block {
    pub stmts: Vec<Stmt>,
    pub tail: Option<Box<Expr>>,
    pub span: Span,
}

pub enum Stmt {
    Let(LetStmt),
    Expr(Expr),       // expression statement (followed by `;`)
}

pub struct LetStmt {
    pub mutable: bool,
    pub name: String,
    pub ty: Option<Type>,
    pub init: Expr,
    pub span: Span,
}

pub enum Expr {
    LitInt(i64, Span),
    LitBool(bool, Span),
    Ident(String, Span),
    Unary { op: UnOp, expr: Box<Expr>, span: Span },
    Binary { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr>, span: Span },
    Call { callee: Box<Expr>, args: Vec<Expr>, span: Span },
    Paren { inner: Box<Expr>, span: Span },
    Block(Box<Block>),
    If {
        cond: Box<Expr>,
        then_block: Box<Block>,
        else_block: Option<Box<Block>>,
        span: Span,
    },
}

pub enum UnOp { Neg, Not }
pub enum BinOp {
    Add, Sub, Mul, Div, Rem,
    Eq, Neq, Lt, Le, Gt, Ge,
    And, Or,
    Assign, // for L1: only inside `let x = ...`, not as an expression operator
}
```

### Validation rules (parser-enforced)

- **VR-8**: `Program.items` is in source order.
- **VR-9**: `FnDecl.body` is a `Block` (not an arbitrary expression).
- **VR-10**: `LetStmt.init` is present (M01 does not support `let x: T;` declarations without init).
- **VR-11**: `Block.tail` is `Some` iff the block's last syntactic element is an expression *without* a trailing `;`.
- **VR-12**: Operator precedence in `Expr::Binary` reflects Rust standard (FR-008). Concretely, in the constructed tree:
  - `*`, `/`, `%` bind tighter than `+`, `-`.
  - `+`, `-` bind tighter than `<`, `<=`, `>`, `>=`.
  - Comparisons bind tighter than `==`, `!=`.
  - Equality binds tighter than `&&`.
  - `&&` binds tighter than `||`.
  - `=` (in let-init) is the lowest precedence; right-associative.
- **VR-13**: `If.then_block` and (when present) `If.else_block` have matching tail-expression presence when the `if` is used in an expression context — i.e. if `if` appears where an expression is required, both branches must have a tail expression. M01 enforces this syntactically; type compatibility is M02.
- **VR-14**: Every node's `span` covers the source range from the node's first token to its last token, inclusive of the last token (i.e. `span.end` is the byte offset *after* the last token).

## Entity: `ParseError`

```rust
pub struct ParseError {
    pub message: String,
    pub span: Span,
}
```

Both lexer and parser failures produce `ParseError`. There is no error code / kind enum in M01.

### Validation rules

- **VR-15**: `message` is non-empty.
- **VR-16**: `span` satisfies VR-1 / VR-2.
- **VR-17**: For "unexpected EOF" errors, `span.start == span.end == src.len()` and points at the EOF position (zero-length span at end).
- **VR-18**: For the `&` rejection (R-014), `message` contains the substring `"Level 2"` and `span` points at the `&` byte.

## Relationships

```
SourceMap 1───* SourceFile
            │
            └─── FileId (newtype, used by Span)

Span ─── used by every Token, every AST node, ParseError

Token ─── consumed by Parser
Parser ─── produces ─── Program (AST)
Parser ─── may produce ─── ParseError
```

## State transitions

No stateful entities in M01. The `Lexer` and `Parser` are short-lived state-machine structs that produce immutable outputs.
