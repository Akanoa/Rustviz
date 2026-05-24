//! Pratt parser for expressions; recursive descent for items, statements, blocks.

use super::ast::{
    BinOp, Block, Expr, FnDecl, ImplBlock, Item, LetStmt, Param, ParamKind, Program, Stmt,
    StructDecl, StructField, StructLitField, Type, TypeParam, UnOp,
};
use super::error::ParseError;
use super::span::{FileId, Span, SourceMap};
use super::token::{Token, TokenKind};

/// Parse a token stream produced by [`super::lexer::lex`] into a [`Program`].
pub fn parse_tokens(
    tokens: Vec<Token>,
    file: FileId,
    source_map: &SourceMap,
) -> Result<Program, ParseError> {
    let eof_pos = source_map.get(file).map(|f| f.src.len() as u32).unwrap_or(0);
    let mut parser = Parser {
        tokens,
        cursor: 0,
        file,
        eof_pos,
        restrict_struct_lit: false,
    };
    parser.parse_program()
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
    file: FileId,
    eof_pos: u32,
    /// **M07.4**: when true, `parse_atom` does NOT treat `Ident { ... }` as
    /// a struct literal — instead it stops at the Ident and lets the outer
    /// caller see the `{` (which is then interpreted as the start of a
    /// block, e.g. the `then` block of an `if` or `while`). Mirrors Rust's
    /// "no struct literals in cond positions" rule.
    ///
    /// Set true when parsing the cond of `if`/`while`; reset to false
    /// inside `( ... )`, struct literals' fields, blocks `{ ... }`, and
    /// any other context where a struct literal is grammatically
    /// unambiguous.
    restrict_struct_lit: bool,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.cursor]
    }

    fn bump(&mut self) -> Token {
        let tok = self.tokens[self.cursor].clone();
        if !matches!(tok.kind, TokenKind::Eof) {
            self.cursor += 1;
        }
        tok
    }

    fn at(&self, want: &TokenKind) -> bool {
        std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(want)
    }

    fn bump_if(&mut self, want: &TokenKind) -> Option<Token> {
        if self.at(want) {
            Some(self.bump())
        } else {
            None
        }
    }

    fn expect(&mut self, want: &TokenKind, label: &str) -> Result<Token, ParseError> {
        if self.at(want) {
            Ok(self.bump())
        } else {
            Err(self.error_expected(label))
        }
    }

    fn error_expected(&self, label: &str) -> ParseError {
        let found = self.peek();
        let span = match found.kind {
            TokenKind::Eof => Span::point(self.eof_pos, self.file),
            _ => found.span,
        };
        ParseError {
            message: format!("expected {}, found {}", label, found.kind.describe()),
            span,
        }
    }

    fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut items = Vec::new();
        while !matches!(self.peek().kind, TokenKind::Eof) {
            items.push(self.parse_item()?);
        }
        let span = if items.is_empty() {
            Span::new(0, 0, self.file)
        } else {
            let first = items_first_span(&items);
            let last = items_last_span(&items);
            first.merge(last)
        };
        Ok(Program { items, span })
    }

    fn parse_item(&mut self) -> Result<Item, ParseError> {
        if self.at(&TokenKind::Fn) {
            Ok(Item::Fn(self.parse_fn_decl()?))
        } else if self.at(&TokenKind::Struct) {
            Ok(Item::Struct(self.parse_struct_decl()?))
        } else if self.at(&TokenKind::Impl) {
            Ok(Item::Impl(self.parse_impl_block()?))
        } else {
            Err(self.error_expected("an item (`fn`, `struct`, or `impl`)"))
        }
    }

    /// **M07.5**: parse optional `<T>` / `<T, U>` type-parameter list. Bound
    /// syntax `<T: Foo>` is parser-accepted (bound parsed and discarded
    /// here — typeck rejects with the M07.6-pointer error). Multi-param
    /// lists are parser-accepted too; typeck rejects.
    fn parse_type_params(&mut self) -> Result<Vec<TypeParam>, ParseError> {
        if !self.at(&TokenKind::Lt) {
            return Ok(Vec::new());
        }
        self.bump(); // `<`
        let mut params = Vec::new();
        while !self.at(&TokenKind::Gt) {
            let name_span = self.peek().span;
            let name = self.expect_ident("type parameter name")?;
            // **M07.5**: bound syntax `T: Foo` accepted at parse; typeck
            // rejects with an M07.6-pointer message. Bound captured on
            // TypeParam so typeck can see it.
            let bound = if self.bump_if(&TokenKind::Colon).is_some() {
                Some(self.expect_ident("trait name in bound")?)
            } else {
                None
            };
            params.push(TypeParam { name, bound, span: name_span });
            if self.bump_if(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::Gt, "`>`")?;
        Ok(params)
    }

    /// **M07.4**: `struct Name { f1: T1, f2: T2 }`. At least one field
    /// required — empty structs rejected at parse time with a clear message.
    /// **M07.5**: optional `<T>` between name and `{`.
    fn parse_struct_decl(&mut self) -> Result<StructDecl, ParseError> {
        let kw = self.expect(&TokenKind::Struct, "`struct`")?;
        let name = self.expect_ident("struct name")?;
        let type_params = self.parse_type_params()?;
        self.expect(&TokenKind::LBrace, "`{`")?;
        let mut fields = Vec::new();
        let mut field_names: std::collections::HashSet<String> = std::collections::HashSet::new();
        while !self.at(&TokenKind::RBrace) {
            let name_span = self.peek().span;
            let field_name = self.expect_ident("field name")?;
            if !field_names.insert(field_name.clone()) {
                return Err(ParseError {
                    message: format!("duplicate field `{field_name}` in struct `{name}`"),
                    span: name_span,
                });
            }
            self.expect(&TokenKind::Colon, "`:`")?;
            let ty = self.parse_type()?;
            let field_span = name_span.merge(type_span(&ty));
            fields.push(StructField {
                name: field_name,
                ty,
                span: field_span,
            });
            if self.bump_if(&TokenKind::Comma).is_none() {
                break;
            }
        }
        let rbrace = self.expect(&TokenKind::RBrace, "`}`")?;
        if fields.is_empty() {
            return Err(ParseError {
                message: "structs in M07.4 must have at least one field".into(),
                span: kw.span.merge(rbrace.span),
            });
        }
        Ok(StructDecl {
            name,
            type_params,
            fields,
            span: kw.span.merge(rbrace.span),
        })
    }

    /// **M07.4**: `impl Type { fn ...; fn ...; }`. M07.4 supports inherent
    /// impls only (no traits), and a single-segment type path.
    fn parse_impl_block(&mut self) -> Result<ImplBlock, ParseError> {
        let kw = self.expect(&TokenKind::Impl, "`impl`")?;
        let ty_name = self.expect_ident("type name after `impl`")?;
        self.expect(&TokenKind::LBrace, "`{`")?;
        let mut items = Vec::new();
        while !self.at(&TokenKind::RBrace) {
            items.push(self.parse_fn_decl()?);
        }
        let rbrace = self.expect(&TokenKind::RBrace, "`}`")?;
        Ok(ImplBlock {
            ty_name,
            items,
            span: kw.span.merge(rbrace.span),
        })
    }

    fn parse_fn_decl(&mut self) -> Result<FnDecl, ParseError> {
        let fn_kw = self.expect(&TokenKind::Fn, "`fn`")?;
        let name = self.expect_ident("function name")?;
        let type_params = self.parse_type_params()?;
        self.expect(&TokenKind::LParen, "`(`")?;
        let mut params = Vec::new();
        let mut idx = 0usize;
        while !self.at(&TokenKind::RParen) {
            params.push(self.parse_param(idx)?);
            idx += 1;
            if self.bump_if(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::RParen, "`)`")?;
        let return_ty = if self.bump_if(&TokenKind::Arrow).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = self.parse_block()?;
        let span = fn_kw.span.merge(body.span);
        Ok(FnDecl {
            name,
            type_params,
            params,
            return_ty,
            body,
            span,
        })
    }

    /// Parse one parameter. `index` is the parameter's position (0-based) so
    /// we can recognize self-receivers ONLY at position 0 and reject them
    /// elsewhere. The parser doesn't know whether we're inside an `impl`
    /// block — phase-1 typeck rejects self-receivers inside free `fn`
    /// declarations.
    fn parse_param(&mut self, index: usize) -> Result<Param, ParseError> {
        // **M07.4**: self-receiver patterns at the first param position.
        //   `self`, `&self`, `&mut self`
        // The ty placeholder is `Type::Path { segments: ["__SelfPlaceholder"] }`
        // which phase-1 typeck replaces with the enclosing impl block's
        // real `Ty::Struct` (or `Ty::Ref { Ty::Struct, .. }`).
        if matches!(
            self.peek().kind,
            TokenKind::SelfKw | TokenKind::Amp | TokenKind::AmpMut
        ) {
            // Determine if this is a self-receiver shape.
            let kind_opt = match self.peek().kind {
                TokenKind::SelfKw => Some(ParamKind::SelfOwned),
                TokenKind::Amp
                    if matches!(
                        self.tokens.get(self.cursor + 1).map(|t| &t.kind),
                        Some(TokenKind::SelfKw)
                    ) =>
                {
                    Some(ParamKind::SelfShared)
                }
                TokenKind::AmpMut
                    if matches!(
                        self.tokens.get(self.cursor + 1).map(|t| &t.kind),
                        Some(TokenKind::SelfKw)
                    ) =>
                {
                    Some(ParamKind::SelfMut)
                }
                _ => None,
            };
            if let Some(kind) = kind_opt {
                if index != 0 {
                    let span_tok = self.peek().clone();
                    return Err(ParseError {
                        message: "`self` parameter must be the first parameter".into(),
                        span: span_tok.span,
                    });
                }
                // Consume the borrow prefix (if any) and the `self` keyword.
                let start_span = self.peek().span;
                if matches!(kind, ParamKind::SelfShared | ParamKind::SelfMut) {
                    self.bump(); // `&` or `&mut`
                }
                let self_tok = self.expect(&TokenKind::SelfKw, "`self`")?;
                let end_span = self_tok.span;
                let placeholder_inner = Type::Path {
                    segments: vec!["__SelfPlaceholder".to_owned()],
                    type_args: Vec::new(),
                    span: end_span,
                };
                let ty = match kind {
                    ParamKind::SelfOwned => placeholder_inner,
                    ParamKind::SelfShared => Type::Ref {
                        inner: Box::new(placeholder_inner),
                        mutable: false,
                        span: start_span.merge(end_span),
                    },
                    ParamKind::SelfMut => Type::Ref {
                        inner: Box::new(placeholder_inner),
                        mutable: true,
                        span: start_span.merge(end_span),
                    },
                    ParamKind::Normal => unreachable!(),
                };
                return Ok(Param {
                    name: "self".to_owned(),
                    ty,
                    kind,
                    span: start_span.merge(end_span),
                });
            }
            // `&` not followed by `self` at param position — fall through;
            // the upcoming `expect_ident` will produce a clear error.
        }
        let name_span = self.peek().span;
        let name = self.expect_ident("parameter name")?;
        self.expect(&TokenKind::Colon, "`:`")?;
        let ty = self.parse_type()?;
        let span = name_span.merge(type_span(&ty));
        Ok(Param {
            name,
            ty,
            kind: ParamKind::Normal,
            span,
        })
    }

    fn parse_type(&mut self) -> Result<Type, ParseError> {
        if self.at(&TokenKind::LParen) {
            let lparen = self.bump();
            let rparen = self.expect(&TokenKind::RParen, "`)`")?;
            return Ok(Type::Unit {
                span: lparen.span.merge(rparen.span),
            });
        }
        // **M07.3**: array type annotation `[T; N]`. Distinct from `&[T]`
        // (slice — that's caught by the `&`-prefix path below). The size
        // must be a non-negative integer literal (no const expressions).
        if self.at(&TokenKind::LBracket) {
            let lbracket = self.bump();
            let inner = self.parse_type()?;
            self.expect(&TokenKind::Semi, "`;`")?;
            let size_tok = self.peek().clone();
            let size = match size_tok.kind {
                TokenKind::Int(n, _) if n >= 0 => {
                    self.bump();
                    n as u64
                }
                _ => {
                    return Err(ParseError {
                        message: "array size must be a non-negative integer literal".into(),
                        span: size_tok.span,
                    });
                }
            };
            let rbracket = self.expect(&TokenKind::RBracket, "`]`")?;
            return Ok(Type::Array {
                inner: Box::new(inner),
                size,
                span: lbracket.span.merge(rbracket.span),
            });
        }
        // M06: `&T` and `&mut T` reference types.
        // **M07.1**: `&[T]` and `&mut [T]` slice types — when `[` follows the
        // `&` (or `&mut`), this is a slice annotation, not a regular ref.
        if self.at(&TokenKind::Amp) || self.at(&TokenKind::AmpMut) {
            let amp = self.bump();
            let mutable = matches!(amp.kind, TokenKind::AmpMut);
            // M07.1: slice annotation `&[T]` / `&mut [T]`.
            if self.at(&TokenKind::LBracket) {
                self.bump(); // `[`
                let inner = self.parse_type()?;
                let rbracket = self.expect(&TokenKind::RBracket, "`]`")?;
                return Ok(Type::Slice {
                    inner: Box::new(inner),
                    mutable,
                    span: amp.span.merge(rbracket.span),
                });
            }
            let inner = self.parse_type()?;
            let inner_span = type_span(&inner);
            return Ok(Type::Ref {
                inner: Box::new(inner),
                mutable,
                span: amp.span.merge(inner_span),
            });
        }
        // M01 supports single-segment paths only; `::` tokenizer support is M02+.
        let segment_span = self.peek().span;
        let segment = self.expect_ident("type")?;
        // **M07** + **M07.5**: optional generic args `<T>` after the type name.
        // Box/Vec route to `Type::Generic` (existing M07 typeck dispatch
        // expects that shape). User-defined generic types (`Wrapper<i32>`)
        // route to `Type::Path { type_args }` (M07.5).
        if self.at(&TokenKind::Lt) {
            let lt = self.bump();
            let mut args = Vec::new();
            loop {
                args.push(self.parse_type()?);
                if self.bump_if(&TokenKind::Comma).is_none() {
                    break;
                }
            }
            let gt = self.expect(&TokenKind::Gt, "`>`")?;
            if segment == "Box" || segment == "Vec" {
                return Ok(Type::Generic {
                    segments: vec![segment],
                    args,
                    span: segment_span.merge(gt.span).merge(lt.span),
                });
            }
            // M07.5: user-defined generic type.
            return Ok(Type::Path {
                segments: vec![segment],
                type_args: args,
                span: segment_span.merge(gt.span).merge(lt.span),
            });
        }
        Ok(Type::Path {
            segments: vec![segment],
            type_args: Vec::new(),
            span: segment_span,
        })
    }

    fn parse_block(&mut self) -> Result<Block, ParseError> {
        let lbrace = self.expect(&TokenKind::LBrace, "`{`")?;
        // **M07.4**: block bodies are NOT cond positions — re-enable struct
        // literals (in case we're parsing the then-block of an `if`).
        let prev_restrict = std::mem::replace(&mut self.restrict_struct_lit, false);
        let mut stmts = Vec::new();
        let mut tail: Option<Box<Expr>> = None;
        loop {
            if self.at(&TokenKind::RBrace) {
                break;
            }
            if self.at(&TokenKind::Let) {
                stmts.push(Stmt::Let(self.parse_let_stmt()?));
                continue;
            }
            let expr = self.parse_expr(0)?;
            // M06.1: if the parsed expression is followed by `=`, treat the
            // whole thing as an assignment statement. lhs validity is checked
            // at typeck (place-expression rule).
            if self.bump_if(&TokenKind::Eq).is_some() {
                let rhs = self.parse_expr(0)?;
                let semi = self.expect(&TokenKind::Semi, "`;`")?;
                let span = expr.span().merge(semi.span);
                stmts.push(Stmt::Assign { lhs: expr, rhs, span });
            } else if self.bump_if(&TokenKind::Semi).is_some() {
                stmts.push(Stmt::Expr(expr));
            } else if self.at(&TokenKind::RBrace) {
                tail = Some(Box::new(expr));
                break;
            } else {
                return Err(self.error_expected("`;`, `=`, or `}`"));
            }
        }
        let rbrace = self.expect(&TokenKind::RBrace, "`}`")?;
        self.restrict_struct_lit = prev_restrict;
        let span = lbrace.span.merge(rbrace.span);
        Ok(Block { stmts, tail, span })
    }

    fn parse_let_stmt(&mut self) -> Result<LetStmt, ParseError> {
        let let_kw = self.expect(&TokenKind::Let, "`let`")?;
        let mutable = self.bump_if(&TokenKind::Mut).is_some();
        let name = self.expect_ident("binding name")?;
        let ty = if self.bump_if(&TokenKind::Colon).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&TokenKind::Eq, "`=`")?;
        let init = self.parse_expr(0)?;
        let semi = self.expect(&TokenKind::Semi, "`;`")?;
        Ok(LetStmt {
            mutable,
            name,
            ty,
            init,
            span: let_kw.span.merge(semi.span),
        })
    }

    /// Pratt parser. Consumes binary operators whose binding power exceeds `min_bp`.
    fn parse_expr(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_atom()?;
        loop {
            // **M07**: postfix method call `expr.method(args)`.
            // **M07.4**: same `Dot Ident` prefix can also mean field access
            // `expr.name` when NOT followed by `(`. One-token lookahead
            // disambiguates here.
            if self.at(&TokenKind::Dot) {
                self.bump(); // `.`
                let name_span = self.peek().span;
                let name = self.expect_ident("field or method name")?;
                if self.at(&TokenKind::LParen) {
                    self.bump(); // `(`
                    // **M07.4**: call args are NOT cond positions —
                    // re-enable struct literals.
                    let prev_restrict =
                        std::mem::replace(&mut self.restrict_struct_lit, false);
                    let mut args = Vec::new();
                    while !self.at(&TokenKind::RParen) {
                        args.push(self.parse_expr(0)?);
                        if self.bump_if(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                    self.restrict_struct_lit = prev_restrict;
                    let rparen = self.expect(&TokenKind::RParen, "`)`")?;
                    let span = lhs.span().merge(rparen.span);
                    lhs = Expr::MethodCall {
                        receiver: Box::new(lhs),
                        name,
                        args,
                        span,
                    };
                } else {
                    // **M07.4**: field access — no trailing `(`.
                    let span = lhs.span().merge(name_span);
                    lhs = Expr::FieldAccess {
                        receiver: Box::new(lhs),
                        name,
                        span,
                    };
                }
                continue;
            }
            // **M07**: postfix indexing `expr[index]`.
            // **M07.1**: indexing also accepts a range `[ start? .. end? ]`.
            // The four range forms (`a..b`, `..b`, `a..`, `..`) all parse here
            // and produce an `Expr::Index { index: Expr::Range { .. }, .. }`.
            if self.at(&TokenKind::LBracket) {
                let lbracket = self.bump(); // `[`
                // **M07.4**: index arg is NOT cond position — re-enable
                // struct literals.
                let prev_restrict = std::mem::replace(&mut self.restrict_struct_lit, false);
                let index = self.parse_index_inner(lbracket.span)?;
                self.restrict_struct_lit = prev_restrict;
                let rbracket = self.expect(&TokenKind::RBracket, "`]`")?;
                let span = lhs.span().merge(rbracket.span);
                lhs = Expr::Index {
                    receiver: Box::new(lhs),
                    index: Box::new(index),
                    span,
                };
                continue;
            }
            // Postfix call: `expr(args, ...)`.
            if self.at(&TokenKind::LParen) {
                self.bump(); // `(`
                // **M07.4**: call args are NOT cond positions.
                let prev_restrict = std::mem::replace(&mut self.restrict_struct_lit, false);
                let mut args = Vec::new();
                while !self.at(&TokenKind::RParen) {
                    args.push(self.parse_expr(0)?);
                    if self.bump_if(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
                self.restrict_struct_lit = prev_restrict;
                let rparen = self.expect(&TokenKind::RParen, "`)`")?;
                let span = lhs.span().merge(rparen.span);
                lhs = Expr::Call {
                    callee: Box::new(lhs),
                    args,
                    span,
                };
                continue;
            }
            // Infix binary operator.
            let (op, bp) = match &self.peek().kind {
                TokenKind::OrOr => (BinOp::Or, 10),
                TokenKind::AndAnd => (BinOp::And, 20),
                TokenKind::EqEq => (BinOp::Eq, 30),
                TokenKind::BangEq => (BinOp::Neq, 30),
                TokenKind::Lt => (BinOp::Lt, 40),
                TokenKind::Le => (BinOp::Le, 40),
                TokenKind::Gt => (BinOp::Gt, 40),
                TokenKind::Ge => (BinOp::Ge, 40),
                TokenKind::Plus => (BinOp::Add, 50),
                TokenKind::Minus => (BinOp::Sub, 50),
                TokenKind::Star => (BinOp::Mul, 60),
                TokenKind::Slash => (BinOp::Div, 60),
                TokenKind::Percent => (BinOp::Rem, 60),
                _ => break,
            };
            if bp <= min_bp {
                break;
            }
            self.bump();
            let rhs = self.parse_expr(bp)?;
            let span = lhs.span().merge(rhs.span());
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    /// **M07.1**: parse the contents of an index bracket `[ ... ]`. This is the
    /// only context where a range expression (`..`, `a..b`, `..b`, `a..`) is
    /// accepted in M07.1. Returns either a scalar `Expr` (existing M07
    /// single-element index behavior) or an `Expr::Range { .. }`.
    ///
    /// `lbracket_span` is the span of the consumed `[`, used to give the
    /// `..` (no-start, no-end) form a meaningful span.
    fn parse_index_inner(&mut self, lbracket_span: Span) -> Result<Expr, ParseError> {
        // Case 1: starts with `..` — no `start`.
        if self.at(&TokenKind::DotDot) {
            let dotdot = self.bump();
            // `..` immediately followed by `]` → full range `..`.
            if self.at(&TokenKind::RBracket) {
                return Ok(Expr::Range {
                    start: None,
                    end: None,
                    span: lbracket_span.merge(dotdot.span),
                });
            }
            // `..end`.
            let end = self.parse_expr(0)?;
            let span = lbracket_span.merge(end.span());
            return Ok(Expr::Range {
                start: None,
                end: Some(Box::new(end)),
                span,
            });
        }
        // Case 2: parse a primary expression (potential `start`), then decide.
        let first = self.parse_expr(0)?;
        if self.at(&TokenKind::DotDot) {
            self.bump(); // `..`
            // `start..` immediately followed by `]` → open-ended range.
            if self.at(&TokenKind::RBracket) {
                let span = first.span();
                return Ok(Expr::Range {
                    start: Some(Box::new(first)),
                    end: None,
                    span,
                });
            }
            // `start..end`.
            let end = self.parse_expr(0)?;
            let span = first.span().merge(end.span());
            return Ok(Expr::Range {
                start: Some(Box::new(first)),
                end: Some(Box::new(end)),
                span,
            });
        }
        // Scalar-index path (existing M07 behavior).
        Ok(first)
    }

    fn parse_atom(&mut self) -> Result<Expr, ParseError> {
        // **M07.3**: array literal `[e1, e2, ..., eN]`. Detected at the
        // atom level (before any other atom rule fires) — the existing
        // postfix `[` rule for `expr[i]` indexing runs in `parse_expr`
        // after `parse_atom`, so there's no grammar conflict.
        if self.at(&TokenKind::LBracket) {
            let lbracket = self.bump();
            // **M07.4**: array-literal elements are NOT cond positions.
            let prev_restrict = std::mem::replace(&mut self.restrict_struct_lit, false);
            let mut elements = Vec::new();
            if !self.at(&TokenKind::RBracket) {
                loop {
                    elements.push(self.parse_expr(0)?);
                    if self.bump_if(&TokenKind::Comma).is_none() {
                        break;
                    }
                    // Trailing comma allowed: `[1, 2, 3,]` parses.
                    if self.at(&TokenKind::RBracket) {
                        break;
                    }
                }
            }
            self.restrict_struct_lit = prev_restrict;
            let rbracket = self.expect(&TokenKind::RBracket, "`]`")?;
            return Ok(Expr::ArrayLit {
                elements,
                span: lbracket.span.merge(rbracket.span),
            });
        }
        let tok = self.peek().clone();
        match &tok.kind {
            TokenKind::Int(v, suffix) => {
                self.bump();
                Ok(Expr::LitInt(*v, *suffix, tok.span))
            }
            TokenKind::Float(v, suffix) => {
                self.bump();
                Ok(Expr::LitFloat(*v, *suffix, tok.span))
            }
            TokenKind::Bool(b) => {
                self.bump();
                Ok(Expr::LitBool(*b, tok.span))
            }
            TokenKind::Ident(name) => {
                let name = name.clone();
                let start_tok = self.bump();
                // **M07**: if followed by `::Ident`, parse as multi-segment Path.
                // **M07.5**: turbofish — after a `::`, if `Lt` follows (no
                // intervening segment), parse type-args. Pattern:
                //   `id::<bool>(...)` — turbofish call
                //   `Wrapper::<i32> { ... }` — turbofish struct lit
                if self.at(&TokenKind::ColonColon) {
                    let mut segments = vec![name];
                    let mut end_span = start_tok.span;
                    let mut type_args: Vec<Type> = Vec::new();
                    while self.bump_if(&TokenKind::ColonColon).is_some() {
                        // M07.5 turbofish: `::<` immediately after the last
                        // segment we've consumed.
                        if self.at(&TokenKind::Lt) {
                            self.bump(); // `<`
                            // M07.5 turbofish: re-enable struct literals
                            // inside type args (defensive — unlikely but
                            // keeps the cond-position flag consistent).
                            let prev_restrict =
                                std::mem::replace(&mut self.restrict_struct_lit, false);
                            loop {
                                type_args.push(self.parse_type()?);
                                if self.bump_if(&TokenKind::Comma).is_none() {
                                    break;
                                }
                            }
                            self.restrict_struct_lit = prev_restrict;
                            let gt = self.expect(&TokenKind::Gt, "`>`")?;
                            end_span = gt.span;
                            // Turbofish ends the path; no more `::Ident` after.
                            break;
                        }
                        let seg_tok = self.peek().clone();
                        let seg = self.expect_ident("path segment")?;
                        segments.push(seg);
                        end_span = seg_tok.span;
                    }
                    // M07.5: turbofish struct literal `Wrapper::<i32> { v: 5 }`.
                    if !type_args.is_empty()
                        && self.at(&TokenKind::LBrace)
                        && !self.restrict_struct_lit
                    {
                        return self.parse_struct_lit_with_args(
                            segments,
                            type_args,
                            start_tok.span,
                        );
                    }
                    Ok(Expr::Path {
                        segments,
                        type_args,
                        span: start_tok.span.merge(end_span),
                    })
                } else if self.at(&TokenKind::LBrace) && !self.restrict_struct_lit {
                    // **M07.4**: struct literal `Name { f1: e1, f2: e2 }`.
                    // Disabled in cond positions (`if c { ... }`,
                    // `while c { ... }`) where a `{` after an ident MUST
                    // be the start of the then-block per Rust's grammar.
                    self.parse_struct_lit(vec![name], start_tok.span)
                } else {
                    Ok(Expr::Ident(name, tok.span))
                }
            }
            // **M07**: string literal.
            TokenKind::Str(s) => {
                let s = s.clone();
                self.bump();
                Ok(Expr::StrLit(s, tok.span))
            }
            TokenKind::LParen => {
                let lparen = self.bump();
                // **M07.4**: parentheses re-enable struct literals
                // regardless of the outer cond-position restriction
                // (matches Rust: `if (Point { .. }).x > 0` is fine).
                let prev_restrict = std::mem::replace(&mut self.restrict_struct_lit, false);
                let inner = self.parse_expr(0)?;
                self.restrict_struct_lit = prev_restrict;
                let rparen = self.expect(&TokenKind::RParen, "`)`")?;
                Ok(Expr::Paren {
                    inner: Box::new(inner),
                    span: lparen.span.merge(rparen.span),
                })
            }
            TokenKind::LBrace => {
                let block = self.parse_block()?;
                Ok(Expr::Block(Box::new(block)))
            }
            TokenKind::If => {
                let if_kw = self.bump();
                // **M07.4**: gate struct-literal parsing inside the cond.
                // `if c { ... }` must NOT parse `c { ... }` as a struct
                // literal — the `{` is the then-block. Save and restore
                // around the cond expression.
                let prev_restrict = std::mem::replace(&mut self.restrict_struct_lit, true);
                let cond = self.parse_expr(0)?;
                self.restrict_struct_lit = prev_restrict;
                let then_block = self.parse_block()?;
                let else_block = if self.bump_if(&TokenKind::Else).is_some() {
                    Some(Box::new(self.parse_block()?))
                } else {
                    None
                };
                let end_span = else_block
                    .as_ref()
                    .map(|b| b.span)
                    .unwrap_or(then_block.span);
                Ok(Expr::If {
                    cond: Box::new(cond),
                    then_block: Box::new(then_block),
                    else_block,
                    span: if_kw.span.merge(end_span),
                })
            }
            TokenKind::Minus => self.parse_unary(UnOp::Neg),
            TokenKind::Bang => self.parse_unary(UnOp::Not),
            TokenKind::Amp | TokenKind::AmpMut => self.parse_borrow_expr(),
            TokenKind::Star => self.parse_deref_expr(),
            // **M07.4**: `self` in expression position resolves to the
            // bound `self` parameter of the enclosing method. Treated as
            // a regular identifier from here on — resolve.rs looks up
            // "self" in the current scope.
            TokenKind::SelfKw => {
                self.bump();
                Ok(Expr::Ident("self".to_owned(), tok.span))
            }
            _ => Err(self.error_expected("expression")),
        }
    }

    fn parse_unary(&mut self, op: UnOp) -> Result<Expr, ParseError> {
        let op_tok = self.bump();
        // Unary binds tighter than any binary op (max bp here is 60 for `*`).
        let expr = self.parse_expr(70)?;
        let span = op_tok.span.merge(expr.span());
        Ok(Expr::Unary {
            op,
            expr: Box::new(expr),
            span,
        })
    }

    /// **M06**: `&expr` or `&mut expr`. Place-expression check happens at
    /// typeck (parser accepts any expression after the `&` for now).
    fn parse_borrow_expr(&mut self) -> Result<Expr, ParseError> {
        let amp = self.bump();
        let mutable = matches!(amp.kind, TokenKind::AmpMut);
        // Borrows bind tighter than binary ops, like other unary prefixes.
        let inner = self.parse_expr(70)?;
        let span = amp.span.merge(inner.span());
        Ok(Expr::Borrow {
            inner: Box::new(inner),
            mutable,
            span,
        })
    }

    /// **M06.1**: `*expr` — prefix deref. Same precedence as other unaries
    /// (bp 70). Disambiguation from binary `*` is by parser position: this
    /// path runs at expression-start; the binary path runs after an operand.
    fn parse_deref_expr(&mut self) -> Result<Expr, ParseError> {
        let star = self.bump();
        let inner = self.parse_expr(70)?;
        let span = star.span.merge(inner.span());
        Ok(Expr::Deref {
            inner: Box::new(inner),
            span,
        })
    }

    /// **M07.4**: parse `{ name: expr, name, ... }` after a struct's path
    /// has already been consumed. `path_start_span` covers the path; the
    /// resulting `Expr::StructLit` span runs from path start through `}`.
    fn parse_struct_lit(
        &mut self,
        path: Vec<String>,
        path_start_span: Span,
    ) -> Result<Expr, ParseError> {
        self.parse_struct_lit_with_args(path, Vec::new(), path_start_span)
    }

    /// **M07.5**: `Wrapper::<i32> { v: 5 }` — same as `parse_struct_lit` but
    /// the path carries turbofish `type_args`. Used for explicit-annotation
    /// generic struct literals.
    fn parse_struct_lit_with_args(
        &mut self,
        path: Vec<String>,
        type_args: Vec<Type>,
        path_start_span: Span,
    ) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::LBrace, "`{`")?;
        // **M07.4**: struct-literal field VALUES are arbitrary expressions
        // and not in cond-position — re-enable struct literals (in case
        // we're nested inside an `if` cond context that disabled them).
        let prev_restrict = std::mem::replace(&mut self.restrict_struct_lit, false);
        let mut fields = Vec::new();
        while !self.at(&TokenKind::RBrace) {
            let name_span = self.peek().span;
            let field_name = self.expect_ident("field name")?;
            let (value, end_span) = if self.bump_if(&TokenKind::Colon).is_some() {
                let value = self.parse_expr(0)?;
                let end = value.span();
                (Some(value), end)
            } else {
                // Shorthand: just the bound local name, no `: value`.
                (None, name_span)
            };
            fields.push(StructLitField {
                name: field_name,
                value,
                span: name_span.merge(end_span),
            });
            if self.bump_if(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.restrict_struct_lit = prev_restrict;
        let rbrace = self.expect(&TokenKind::RBrace, "`}`")?;
        Ok(Expr::StructLit {
            path,
            type_args,
            fields,
            span: path_start_span.merge(rbrace.span),
        })
    }

    fn expect_ident(&mut self, label: &str) -> Result<String, ParseError> {
        let tok = self.peek().clone();
        if let TokenKind::Ident(s) = &tok.kind {
            let name = s.clone();
            self.bump();
            Ok(name)
        } else {
            Err(self.error_expected(label))
        }
    }
}

fn items_first_span(items: &[Item]) -> Span {
    item_span(items.first().expect("non-empty"))
}

fn items_last_span(items: &[Item]) -> Span {
    item_span(items.last().expect("non-empty"))
}

fn item_span(item: &Item) -> Span {
    match item {
        Item::Fn(f) => f.span,
        Item::Struct(s) => s.span,
        Item::Impl(i) => i.span,
    }
}

fn type_span(ty: &Type) -> Span {
    match ty {
        Type::Path { span, .. }
        | Type::Unit { span }
        | Type::Ref { span, .. }
        | Type::Generic { span, .. }
        | Type::Slice { span, .. }
        | Type::Array { span, .. } => *span,
    }
}
