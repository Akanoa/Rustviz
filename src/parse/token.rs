//! Token and TokenKind.
//!
//! `&` and `&mut` are intentionally absent — the lexer rejects them with a
//! pedagogical "Level 2" message in M01. M06 will add `Amp` and `AmpMut`
//! variants here in place.

use super::span::Span;

/// A lexed token: kind + source span.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// Lexical kind.
    pub kind: TokenKind,
    /// Source span this token covers.
    pub span: Span,
}

/// All token kinds the M01 lexer produces.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// Integer literal parsed to `i64`.
    Int(i64),
    /// Boolean literal `true` or `false`.
    Bool(bool),
    /// Identifier (non-keyword).
    Ident(String),

    /// `let`
    Let,
    /// `mut`
    Mut,
    /// `fn`
    Fn,
    /// `if`
    If,
    /// `else`
    Else,
    /// `return`
    Return,

    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,
    /// `=`
    Eq,
    /// `==`
    EqEq,
    /// `!=`
    BangEq,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `&&`
    AndAnd,
    /// `||`
    OrOr,
    /// `!`
    Bang,
    /// `->`
    Arrow,

    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `,`
    Comma,
    /// `;`
    Semi,
    /// `:`
    Colon,

    /// End of input sentinel.
    Eof,
}

impl TokenKind {
    /// Human-readable name suitable for error messages (e.g. `"`;`"`, `"identifier"`).
    pub fn describe(&self) -> &'static str {
        match self {
            Self::Int(_) => "integer literal",
            Self::Bool(_) => "boolean literal",
            Self::Ident(_) => "identifier",
            Self::Let => "`let`",
            Self::Mut => "`mut`",
            Self::Fn => "`fn`",
            Self::If => "`if`",
            Self::Else => "`else`",
            Self::Return => "`return`",
            Self::Plus => "`+`",
            Self::Minus => "`-`",
            Self::Star => "`*`",
            Self::Slash => "`/`",
            Self::Percent => "`%`",
            Self::Eq => "`=`",
            Self::EqEq => "`==`",
            Self::BangEq => "`!=`",
            Self::Lt => "`<`",
            Self::Le => "`<=`",
            Self::Gt => "`>`",
            Self::Ge => "`>=`",
            Self::AndAnd => "`&&`",
            Self::OrOr => "`||`",
            Self::Bang => "`!`",
            Self::Arrow => "`->`",
            Self::LParen => "`(`",
            Self::RParen => "`)`",
            Self::LBrace => "`{`",
            Self::RBrace => "`}`",
            Self::Comma => "`,`",
            Self::Semi => "`;`",
            Self::Colon => "`:`",
            Self::Eof => "end of input",
        }
    }
}
