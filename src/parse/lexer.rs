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
        // for a float literal (M03.2). Optionally followed by a type suffix
        // (`5u8`, `2.5_f64`, etc.) — the suffix is parsed inline so the
        // typeck doesn't have to coerce.
        if b.is_ascii_digit() {
            while pos < len && src[pos as usize].is_ascii_digit() {
                pos += 1;
            }
            // M03.2: float literal? Peek for `.digit`.
            let is_float = pos + 1 < len
                && src[pos as usize] == b'.'
                && src[(pos + 1) as usize].is_ascii_digit();
            let body_end = if is_float {
                pos += 1; // consume `.`
                while pos < len && src[pos as usize].is_ascii_digit() {
                    pos += 1;
                }
                pos
            } else {
                pos
            };
            // M03.2: parse optional type suffix. Allows `5u8`, `5_u8`,
            // `2.5f32`, `2.5_f64`. Consume an optional `_` separator, then
            // the suffix identifier, then validate against the 14 type names.
            let suffix_start = pos;
            if pos < len && src[pos as usize] == b'_' {
                pos += 1;
            }
            let alpha_start = pos;
            while pos < len {
                let c = src[pos as usize];
                if c.is_ascii_alphanumeric() {
                    pos += 1;
                } else {
                    break;
                }
            }
            let suffix_str = std::str::from_utf8(&src[alpha_start as usize..pos as usize])
                .expect("ascii is valid UTF-8");
            let (int_suffix, float_suffix) = if suffix_str.is_empty() {
                (None, None)
            } else {
                let int_k = parse_int_suffix(suffix_str);
                let float_k = parse_float_suffix(suffix_str);
                if int_k.is_none() && float_k.is_none() {
                    return Err(ParseError {
                        message: format!("invalid suffix `{suffix_str}` after numeric literal"),
                        span: Span::new(suffix_start, pos, file),
                    });
                }
                (int_k, float_k)
            };
            // Check suffix matches literal kind: int suffix on int, float on float.
            if is_float && int_suffix.is_some() {
                return Err(ParseError {
                    message: format!(
                        "integer-type suffix `{suffix_str}` on a float literal"
                    ),
                    span: Span::new(suffix_start, pos, file),
                });
            }
            if !is_float && float_suffix.is_some() {
                return Err(ParseError {
                    message: format!(
                        "float-type suffix `{suffix_str}` on an integer literal"
                    ),
                    span: Span::new(suffix_start, pos, file),
                });
            }
            // Reject identifier characters immediately after we've decided
            // the literal is done (catches typos that aren't valid suffixes).
            if pos < len {
                let n = src[pos as usize];
                if n.is_ascii_alphabetic() || n == b'_' {
                    return Err(ParseError {
                        message: "invalid suffix after numeric literal".into(),
                        span: Span::new(pos, pos + 1, file),
                    });
                }
            }
            let body_str = std::str::from_utf8(&src[start as usize..body_end as usize])
                .expect("digits are valid UTF-8");
            let kind = if is_float {
                let val: f64 = body_str.parse().map_err(|_| ParseError {
                    message: format!("float literal `{body_str}` is invalid"),
                    span: Span::new(start, body_end, file),
                })?;
                TokenKind::Float(val, float_suffix)
            } else {
                let val: i64 = body_str.parse().map_err(|_| ParseError {
                    message: format!("integer literal `{body_str}` does not fit in i64"),
                    span: Span::new(start, body_end, file),
                })?;
                TokenKind::Int(val, int_suffix)
            };
            tokens.push(Token {
                kind,
                span: Span::new(start, pos, file),
            });
            continue;
        }

        // **M07**: string literal `"..."` with escapes `\n`, `\t`, `\\`, `\"`.
        // No raw strings, no multi-line, ASCII-only contents per L3 scope.
        if b == b'"' {
            pos += 1; // consume opening `"`
            let mut bytes = Vec::new();
            loop {
                if pos >= len {
                    return Err(ParseError {
                        message: "unterminated string literal".into(),
                        span: Span::new(start, pos, file),
                    });
                }
                let c = src[pos as usize];
                if c == b'"' {
                    pos += 1; // consume closing `"`
                    break;
                }
                if c == b'\\' {
                    if pos + 1 >= len {
                        return Err(ParseError {
                            message: "unterminated string literal".into(),
                            span: Span::new(start, pos + 1, file),
                        });
                    }
                    let esc = src[(pos + 1) as usize];
                    let resolved = match esc {
                        b'n' => b'\n',
                        b't' => b'\t',
                        b'\\' => b'\\',
                        b'"' => b'"',
                        other => {
                            return Err(ParseError {
                                message: format!(
                                    "invalid escape sequence `\\{}`",
                                    other as char
                                ),
                                span: Span::new(pos, pos + 2, file),
                            });
                        }
                    };
                    bytes.push(resolved);
                    pos += 2;
                    continue;
                }
                bytes.push(c);
                pos += 1;
            }
            let s = String::from_utf8(bytes).map_err(|_| ParseError {
                message: "string literal contains invalid UTF-8".into(),
                span: Span::new(start, pos, file),
            })?;
            tokens.push(Token {
                kind: TokenKind::Str(s),
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
                // **M07.4**: struct decl + impl block + self-receiver keywords.
                "struct" => TokenKind::Struct,
                "impl" => TokenKind::Impl,
                "self" => TokenKind::SelfKw,
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
                // M06: `&mut` requires no whitespace between `&` and `mut`,
                // AND the `mut` must be a complete identifier (not followed
                // by another alphanumeric/underscore). Otherwise it's a plain `&`.
                let is_mut = (pos + 3) < len
                    && src[(pos + 1) as usize] == b'm'
                    && src[(pos + 2) as usize] == b'u'
                    && src[(pos + 3) as usize] == b't'
                    && (
                        (pos + 4) >= len
                            || !(src[(pos + 4) as usize].is_ascii_alphanumeric()
                                || src[(pos + 4) as usize] == b'_')
                    );
                if is_mut {
                    (TokenKind::AmpMut, 4)
                } else {
                    (TokenKind::Amp, 1)
                }
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
            // **M07**: `::` as a single token (path separator).
            (b':', Some(b':')) => (TokenKind::ColonColon, 2),
            (b':', _) => (TokenKind::Colon, 1),
            // **M07**: `.`, `[`, `]` as single-char tokens. The numeric-literal
            // lexer arm consumes `.digit` greedily for floats; bare `.` reaches
            // here only as the postfix method-call separator.
            // **M07.1**: `..` (DotDot) two-char arm comes first — handles `1..3`,
            // `..3`, `1..`, `..` inside `[ ]`. The numeric arm already consumed
            // `1.0` greedily for floats, so a `.` here followed by `.` is always
            // the range operator.
            (b'.', Some(b'.')) => (TokenKind::DotDot, 2),
            (b'.', _) => (TokenKind::Dot, 1),
            (b'[', _) => (TokenKind::LBracket, 1),
            (b']', _) => (TokenKind::RBracket, 1),
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

/// **M03.2**: map an integer-type suffix string to its `IntKind`. `None` if
/// the string isn't one of the 12 integer type names.
fn parse_int_suffix(s: &str) -> Option<crate::typeck::IntKind> {
    use crate::typeck::IntKind::*;
    Some(match s {
        "i8" => I8, "i16" => I16, "i32" => I32, "i64" => I64, "i128" => I128,
        "u8" => U8, "u16" => U16, "u32" => U32, "u64" => U64, "u128" => U128,
        "isize" => ISize, "usize" => USize,
        _ => return None,
    })
}

/// **M03.2**: map a float-type suffix string to its `FloatKind`.
fn parse_float_suffix(s: &str) -> Option<crate::typeck::FloatKind> {
    use crate::typeck::FloatKind::*;
    match s {
        "f32" => Some(F32),
        "f64" => Some(F64),
        _ => None,
    }
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
