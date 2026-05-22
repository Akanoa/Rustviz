//! Lexer state machine. Converts a registered source file to a `Vec<Token>`.
//!
//! `&` rejection (US3) will land in this file as a new match arm; for M01 Phase 3
//! a stray `&` falls through to the generic "unexpected character" error.

use super::error::ParseError;
use super::span::{FileId, Span, SourceMap};
use super::token::{Token, TokenKind};

/// Lex a registered source file to a `Vec<Token>`.
///
/// On success, returns all tokens followed by a final `Eof` token at `src.len()`.
/// On failure, returns a single error.
pub fn lex(file: FileId, source_map: &SourceMap) -> Result<Vec<Token>, ParseError> {
    let src_file = source_map.get(file).ok_or_else(|| ParseError {
        message: format!("file id {} not found in source map", file.0),
        span: Span::point(0, file),
    })?;
    let src = src_file.src.as_bytes();
    let len = src.len() as u32;
    let mut tokens = Vec::new();
    let mut pos: u32 = 0;

    while pos < len {
        let b = src[pos as usize];

        // Whitespace
        if matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
            pos += 1;
            continue;
        }

        // Line comment: `//` to newline or EOF.
        if b == b'/' && pos + 1 < len && src[(pos + 1) as usize] == b'/' {
            pos += 2;
            while pos < len && src[pos as usize] != b'\n' {
                pos += 1;
            }
            continue;
        }

        let start = pos;

        // Numeric literal: digit sequence, optionally followed by `.digits`
        // for a float literal (M03.2). The `.digits` form requires the `.` to
        // be followed by at least one digit — otherwise the int literal ends
        // and the `.` becomes a separate token (only relevant once L1 has
        // method calls, which it doesn't yet).
        if b.is_ascii_digit() {
            while pos < len && src[pos as usize].is_ascii_digit() {
                pos += 1;
            }
            // M03.2: float literal? Peek for `.digit`.
            let is_float = pos + 1 < len
                && src[pos as usize] == b'.'
                && src[(pos + 1) as usize].is_ascii_digit();
            if is_float {
                pos += 1; // consume `.`
                while pos < len && src[pos as usize].is_ascii_digit() {
                    pos += 1;
                }
            }
            // Reject identifier characters immediately following.
            if pos < len {
                let n = src[pos as usize];
                if n.is_ascii_alphabetic() || n == b'_' {
                    return Err(ParseError {
                        message: "invalid suffix after numeric literal".into(),
                        span: Span::new(pos, pos + 1, file),
                    });
                }
            }
            let s = std::str::from_utf8(&src[start as usize..pos as usize])
                .expect("digits are valid UTF-8");
            let kind = if is_float {
                let val: f64 = s.parse().map_err(|_| ParseError {
                    message: format!("float literal `{s}` is invalid"),
                    span: Span::new(start, pos, file),
                })?;
                TokenKind::Float(val)
            } else {
                let val: i64 = s.parse().map_err(|_| ParseError {
                    message: format!("integer literal `{s}` does not fit in i64"),
                    span: Span::new(start, pos, file),
                })?;
                TokenKind::Int(val)
            };
            tokens.push(Token {
                kind,
                span: Span::new(start, pos, file),
            });
            continue;
        }

        // Identifier or keyword: `[A-Za-z_][A-Za-z0-9_]*`.
        if b.is_ascii_alphabetic() || b == b'_' {
            while pos < len {
                let c = src[pos as usize];
                if c.is_ascii_alphanumeric() || c == b'_' {
                    pos += 1;
                } else {
                    break;
                }
            }
            let s = std::str::from_utf8(&src[start as usize..pos as usize])
                .expect("ident bytes are ASCII");
            let kind = match s {
                "let" => TokenKind::Let,
                "mut" => TokenKind::Mut,
                "fn" => TokenKind::Fn,
                "if" => TokenKind::If,
                "else" => TokenKind::Else,
                "return" => TokenKind::Return,
                "true" => TokenKind::Bool(true),
                "false" => TokenKind::Bool(false),
                _ => TokenKind::Ident(s.to_owned()),
            };
            tokens.push(Token {
                kind,
                span: Span::new(start, pos, file),
            });
            continue;
        }

        // Operators and punctuation, including multi-char lookahead.
        let next = if pos + 1 < len {
            Some(src[(pos + 1) as usize])
        } else {
            None
        };
        let (kind, advance): (TokenKind, u32) = match (b, next) {
            (b'=', Some(b'=')) => (TokenKind::EqEq, 2),
            (b'!', Some(b'=')) => (TokenKind::BangEq, 2),
            (b'<', Some(b'=')) => (TokenKind::Le, 2),
            (b'>', Some(b'=')) => (TokenKind::Ge, 2),
            (b'&', Some(b'&')) => (TokenKind::AndAnd, 2),
            (b'&', _) => {
                // Single `&` (and `&mut`) — references are Level 2.
                // Pedagogical message points learners at the level concept;
                // M06 will replace this with `Amp`/`AmpMut` tokenization.
                return Err(ParseError {
                    message:
                        "references are a Level 2 feature, not yet supported in this version of rustviz"
                            .into(),
                    span: Span::new(start, start + 1, file),
                });
            }
            (b'|', Some(b'|')) => (TokenKind::OrOr, 2),
            (b'-', Some(b'>')) => (TokenKind::Arrow, 2),
            (b'+', _) => (TokenKind::Plus, 1),
            (b'-', _) => (TokenKind::Minus, 1),
            (b'*', _) => (TokenKind::Star, 1),
            (b'/', _) => (TokenKind::Slash, 1),
            (b'%', _) => (TokenKind::Percent, 1),
            (b'=', _) => (TokenKind::Eq, 1),
            (b'!', _) => (TokenKind::Bang, 1),
            (b'<', _) => (TokenKind::Lt, 1),
            (b'>', _) => (TokenKind::Gt, 1),
            (b'(', _) => (TokenKind::LParen, 1),
            (b')', _) => (TokenKind::RParen, 1),
            (b'{', _) => (TokenKind::LBrace, 1),
            (b'}', _) => (TokenKind::RBrace, 1),
            (b',', _) => (TokenKind::Comma, 1),
            (b';', _) => (TokenKind::Semi, 1),
            (b':', _) => (TokenKind::Colon, 1),
            _ => {
                // Unknown character. Compute next UTF-8 boundary so the span is
                // valid for multi-byte characters that landed here erroneously.
                let char_end = utf8_next_boundary(src, start as usize) as u32;
                let shown = if b.is_ascii_graphic() || b == b' ' {
                    (b as char).to_string()
                } else {
                    format!("\\x{b:02x}")
                };
                return Err(ParseError {
                    message: format!("unexpected character `{shown}`"),
                    span: Span::new(start, char_end, file),
                });
            }
        };
        tokens.push(Token {
            kind,
            span: Span::new(start, start + advance, file),
        });
        pos = start + advance;
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        span: Span::point(len, file),
    });
    Ok(tokens)
}

/// Byte index just after the UTF-8 character starting at `i`, capped at `src.len()`.
fn utf8_next_boundary(src: &[u8], i: usize) -> usize {
    let b = src[i];
    let width = if b < 0x80 {
        1
    } else if b < 0xC0 {
        1 // Continuation byte alone — malformed input; treat as 1 to avoid infinite loops.
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    };
    (i + width).min(src.len())
}
