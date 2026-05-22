//! Pratt parser for expressions; recursive descent for items, statements, blocks.

use super::ast::{BinOp, Block, Expr, FnDecl, Item, LetStmt, Param, Program, Stmt, Type, UnOp};
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
    };
    parser.parse_program()
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
    file: FileId,
    eof_pos: u32,
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
        } else {
            Err(self.error_expected("an item (e.g. `fn`)"))
        }
    }

    fn parse_fn_decl(&mut self) -> Result<FnDecl, ParseError> {
        let fn_kw = self.expect(&TokenKind::Fn, "`fn`")?;
        let name = self.expect_ident("function name")?;
        self.expect(&TokenKind::LParen, "`(`")?;
        let mut params = Vec::new();
        while !self.at(&TokenKind::RParen) {
            params.push(self.parse_param()?);
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
            params,
            return_ty,
            body,
            span,
        })
    }

    fn parse_param(&mut self) -> Result<Param, ParseError> {
        let name_span = self.peek().span;
        let name = self.expect_ident("parameter name")?;
        self.expect(&TokenKind::Colon, "`:`")?;
        let ty = self.parse_type()?;
        let span = name_span.merge(type_span(&ty));
        Ok(Param { name, ty, span })
    }

    fn parse_type(&mut self) -> Result<Type, ParseError> {
        if self.at(&TokenKind::LParen) {
            let lparen = self.bump();
            let rparen = self.expect(&TokenKind::RParen, "`)`")?;
            return Ok(Type::Unit {
                span: lparen.span.merge(rparen.span),
            });
        }
        // M01 supports single-segment paths only; `::` tokenizer support is M02+.
        let segment_span = self.peek().span;
        let segment = self.expect_ident("type")?;
        Ok(Type::Path {
            segments: vec![segment],
            span: segment_span,
        })
    }

    fn parse_block(&mut self) -> Result<Block, ParseError> {
        let lbrace = self.expect(&TokenKind::LBrace, "`{`")?;
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
            if self.bump_if(&TokenKind::Semi).is_some() {
                stmts.push(Stmt::Expr(expr));
            } else if self.at(&TokenKind::RBrace) {
                tail = Some(Box::new(expr));
                break;
            } else {
                return Err(self.error_expected("`;` or `}`"));
            }
        }
        let rbrace = self.expect(&TokenKind::RBrace, "`}`")?;
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
            // Postfix call: `expr(args, ...)`.
            if self.at(&TokenKind::LParen) {
                self.bump(); // `(`
                let mut args = Vec::new();
                while !self.at(&TokenKind::RParen) {
                    args.push(self.parse_expr(0)?);
                    if self.bump_if(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
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

    fn parse_atom(&mut self) -> Result<Expr, ParseError> {
        let tok = self.peek().clone();
        match &tok.kind {
            TokenKind::Int(v) => {
                self.bump();
                Ok(Expr::LitInt(*v, tok.span))
            }
            TokenKind::Float(v) => {
                self.bump();
                Ok(Expr::LitFloat(*v, tok.span))
            }
            TokenKind::Bool(b) => {
                self.bump();
                Ok(Expr::LitBool(*b, tok.span))
            }
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.bump();
                Ok(Expr::Ident(name, tok.span))
            }
            TokenKind::LParen => {
                let lparen = self.bump();
                let inner = self.parse_expr(0)?;
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
                let cond = self.parse_expr(0)?;
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
    }
}

fn type_span(ty: &Type) -> Span {
    match ty {
        Type::Path { span, .. } | Type::Unit { span } => *span,
    }
}
