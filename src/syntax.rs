//! Surface syntax: lexer, AST, and parser.
//!
//! The prototype deliberately keeps the surface small.  The design documents
//! describe the full language; this module is the vertical slice that lets us
//! validate proof erasure and proof-directed optimisation end to end.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    Ident(String),
    Int(i64),

    Fn,
    Spec,
    Impl,
    Refines,
    Let,
    In,
    If,
    Then,
    Else,
    True,
    False,
    Theorem,
    Requires,
    Ensures,
    Own,
    Borrow,
    Proof,
    Forall,
    Exists,
    By,
    Intro,
    Exact,
    Assumption,
    Apply,
    Split,
    Left,
    Right,
    Rfl,
    Trivial,
    Decide,
    Contradiction,

    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semi,
    Colon,
    Arrow,
    FatArrow,
    Dot,
    Pipe,
    Eq,

    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    AmpAmp,
    PipePipe,
    Bang,

    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
    pub col: usize,
}

#[derive(Clone, Debug)]
pub struct ParseError {
    pub msg: String,
    pub line: usize,
    pub col: usize,
}

impl ParseError {
    fn new(msg: impl Into<String>, line: usize, col: usize) -> Self {
        Self {
            msg: msg.into(),
            line,
            col,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.msg)
    }
}

// ---------------------------------------------------------------------------
// AST
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Program {
    pub decls: Vec<Decl>,
}

#[derive(Clone, Debug)]
pub enum Decl {
    Fn(FnDecl),
    Spec(SpecDecl),
    Impl(ImplDecl),
    Theorem(ThmDecl),
}

#[derive(Clone, Debug)]
pub struct SpecDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Ty,
    pub requires: Vec<PExpr>,
    pub ensures: Vec<PExpr>,
}

#[derive(Clone, Debug)]
pub struct ImplDecl {
    pub f: FnDecl,
    pub refines: String,
}

#[derive(Clone, Debug)]
pub struct FnDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Ty,
    pub requires: Vec<PExpr>,
    pub ensures: Vec<PExpr>,
    pub body: Expr,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub mode: Mode,
    pub kind: ParamKind,
    pub refinement: Option<PExpr>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Val,
    Own,
    Borrow,
}

#[derive(Clone, Debug)]
pub enum ParamKind {
    Runtime(Ty),
    Proof(PExpr),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ty {
    Int,
    Bool,
    Unit,
    Array(Box<Ty>),
    Prop,
}

#[derive(Clone, Debug)]
pub struct ThmDecl {
    pub name: String,
    pub prop: PExpr,
    pub tactics: Vec<Tactic>,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Var(String),
    Int(i64),
    Bool(bool),
    Unit,
    Array(Vec<Expr>),
    Un(UnOp, Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Let(String, Box<Expr>, Box<Expr>),
    Call { name: String, args: Vec<CallArg> },
    Index(Box<Expr>, Box<Expr>),
    Seq(Box<Expr>, Box<Expr>),
}

#[derive(Clone, Debug)]
pub enum CallArg {
    Expr(Expr),
    Proof(Vec<Tactic>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Clone, Debug)]
pub enum Tactic {
    Intro(Option<String>),
    Exact(ProofExpr),
    Assumption,
    Apply(ProofExpr),
    Split,
    Left,
    Right,
    Rfl,
    Trivial,
    Decide,
    Contradiction,
}

#[derive(Clone, Debug)]
pub enum ProofExpr {
    Var(String),
    App(Box<ProofExpr>, Box<ProofExpr>),
}

#[derive(Clone, Debug)]
pub enum PExpr {
    Int(i64),
    Bool(bool),
    Var(String),
    Len(Box<PExpr>),
    Array(Vec<PExpr>),
    Un(UnOp, Box<PExpr>),
    Bin(BinOp, Box<PExpr>, Box<PExpr>),
    Cmp(BinOp, Box<PExpr>, Box<PExpr>),
    And(Box<PExpr>, Box<PExpr>),
    Or(Box<PExpr>, Box<PExpr>),
    Not(Box<PExpr>),
    Imp(Box<PExpr>, Box<PExpr>),
    Forall(String, SortAst, Box<PExpr>),
    Exists(String, SortAst, Box<PExpr>),
    Call(String, Vec<PExpr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortAst {
    Int,
    Bool,
    Array,
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

pub fn lex(src: &str) -> Result<Vec<Token>, ParseError> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut col = 1usize;
    let mut out = Vec::new();

    macro_rules! push {
        ($tok:expr, $l:expr, $c:expr) => {
            out.push(Token {
                tok: $tok,
                line: $l,
                col: $c,
            })
        };
    }

    while i < chars.len() {
        let c = chars[i];
        let l = line;
        let cc = col;
        if c == '\n' {
            i += 1;
            line += 1;
            col = 1;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            col += 1;
            continue;
        }
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
                col += 1;
            }
            continue;
        }
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            i += 2;
            col += 2;
            let mut closed = false;
            while i < chars.len() {
                if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '/' {
                    i += 2;
                    col += 2;
                    closed = true;
                    break;
                }
                if chars[i] == '\n' {
                    line += 1;
                    col = 1;
                } else {
                    col += 1;
                }
                i += 1;
            }
            if !closed {
                return Err(ParseError::new("unterminated block comment", l, cc));
            }
            continue;
        }

        if c.is_ascii_digit() {
            let mut n: i64 = 0;
            while i < chars.len() && chars[i].is_ascii_digit() {
                n = n
                    .saturating_mul(10)
                    .saturating_add((chars[i] as u8 - b'0') as i64);
                i += 1;
                col += 1;
            }
            push!(Tok::Int(n), l, cc);
            continue;
        }

        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
                col += 1;
            }
            let s: String = chars[start..i].iter().collect();
            let tok = match s.as_str() {
                "fn" => Tok::Fn,
                "spec" => Tok::Spec,
                "impl" => Tok::Impl,
                "refines" => Tok::Refines,
                "let" => Tok::Let,
                "in" => Tok::In,
                "if" => Tok::If,
                "then" => Tok::Then,
                "else" => Tok::Else,
                "true" => Tok::True,
                "false" => Tok::False,
                "theorem" => Tok::Theorem,
                "requires" => Tok::Requires,
                "ensures" => Tok::Ensures,
                "own" => Tok::Own,
                "borrow" => Tok::Borrow,
                "proof" => Tok::Proof,
                "forall" => Tok::Forall,
                "exists" => Tok::Exists,
                "by" => Tok::By,
                "intro" => Tok::Intro,
                "exact" => Tok::Exact,
                "assumption" => Tok::Assumption,
                "apply" => Tok::Apply,
                "split" => Tok::Split,
                "left" => Tok::Left,
                "right" => Tok::Right,
                "rfl" => Tok::Rfl,
                "trivial" => Tok::Trivial,
                "decide" => Tok::Decide,
                "contradiction" => Tok::Contradiction,
                _ => Tok::Ident(s),
            };
            push!(tok, l, cc);
            continue;
        }

        let next = chars.get(i + 1).copied();
        let (tok, len) = match (c, next) {
            ('-', Some('>')) => (Tok::Arrow, 2),
            ('=', Some('>')) => (Tok::FatArrow, 2),
            ('=', Some('=')) => (Tok::EqEq, 2),
            ('!', Some('=')) => (Tok::NotEq, 2),
            ('<', Some('=')) => (Tok::Le, 2),
            ('>', Some('=')) => (Tok::Ge, 2),
            ('&', Some('&')) => (Tok::AmpAmp, 2),
            ('|', Some('|')) => (Tok::PipePipe, 2),
            ('(', _) => (Tok::LParen, 1),
            (')', _) => (Tok::RParen, 1),
            ('{', _) => (Tok::LBrace, 1),
            ('}', _) => (Tok::RBrace, 1),
            ('[', _) => (Tok::LBracket, 1),
            (']', _) => (Tok::RBracket, 1),
            (',', _) => (Tok::Comma, 1),
            (';', _) => (Tok::Semi, 1),
            (':', _) => (Tok::Colon, 1),
            ('.', _) => (Tok::Dot, 1),
            ('|', _) => (Tok::Pipe, 1),
            ('=', _) => (Tok::Eq, 1),
            ('+', _) => (Tok::Plus, 1),
            ('-', _) => (Tok::Minus, 1),
            ('*', _) => (Tok::Star, 1),
            ('/', _) => (Tok::Slash, 1),
            ('%', _) => (Tok::Percent, 1),
            ('<', _) => (Tok::Lt, 1),
            ('>', _) => (Tok::Gt, 1),
            ('!', _) => (Tok::Bang, 1),
            _ => {
                return Err(ParseError::new(
                    format!("unexpected character `{c}`"),
                    l,
                    cc,
                ))
            }
        };
        push!(tok, l, cc);
        i += len;
        col += len;
    }

    out.push(Token {
        tok: Tok::Eof,
        line,
        col,
    });
    Ok(out)
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

pub struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(toks: Vec<Token>) -> Self {
        Self { toks, pos: 0 }
    }

    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }

    #[allow(dead_code)]
    fn peek_at(&self, n: usize) -> &Tok {
        let i = (self.pos + n).min(self.toks.len() - 1);
        &self.toks[i].tok
    }

    fn line_col(&self) -> (usize, usize) {
        let t = &self.toks[self.pos];
        (t.line, t.col)
    }

    fn err<T>(&self, msg: impl Into<String>) -> Result<T, ParseError> {
        let (l, c) = self.line_col();
        Err(ParseError::new(msg, l, c))
    }

    fn bump(&mut self) -> Tok {
        let t = self.toks[self.pos].tok.clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if std::mem::discriminant(self.peek()) == std::mem::discriminant(t) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, t: Tok) -> Result<(), ParseError> {
        if self.eat(&t) {
            Ok(())
        } else {
            self.err(format!("expected {:?}, found {:?}", t, self.peek()))
        }
    }

    fn ident(&mut self) -> Result<String, ParseError> {
        match self.bump() {
            Tok::Ident(s) => Ok(s),
            other => self.err(format!("expected identifier, found {:?}", other)),
        }
    }

    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut decls = Vec::new();
        loop {
            match self.peek() {
                Tok::Eof => break,
                Tok::Fn => decls.push(Decl::Fn(self.parse_fn()?)),
                Tok::Spec => decls.push(Decl::Spec(self.parse_spec()?)),
                Tok::Impl => decls.push(Decl::Impl(self.parse_impl()?)),
                Tok::Theorem => decls.push(Decl::Theorem(self.parse_theorem()?)),
                other => return self.err(format!("expected declaration, found {:?}", other)),
            }
        }
        Ok(Program { decls })
    }

    fn parse_fn(&mut self) -> Result<FnDecl, ParseError> {
        self.expect(Tok::Fn)?;
        let name = self.ident()?;
        let params = self.parse_params()?;
        self.expect(Tok::Arrow)?;
        let ret = self.parse_type()?;

        let mut requires = Vec::new();
        while self.eat(&Tok::Requires) {
            requires.push(self.parse_pexpr()?);
        }
        let mut ensures = Vec::new();
        while self.eat(&Tok::Ensures) {
            ensures.push(self.parse_pexpr()?);
        }

        let body = if self.eat(&Tok::LBrace) {
            let body = self.parse_expr_seq()?;
            self.expect(Tok::RBrace)?;
            body
        } else {
            self.expect(Tok::Eq)?;
            self.parse_expr()?
        };

        Ok(FnDecl {
            name,
            params,
            ret,
            requires,
            ensures,
            body,
        })
    }

    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        self.expect(Tok::LParen)?;
        let mut params = Vec::new();
        if !self.eat(&Tok::RParen) {
            loop {
                params.push(self.parse_param()?);
                if self.eat(&Tok::Comma) {
                    continue;
                }
                self.expect(Tok::RParen)?;
                break;
            }
        }
        Ok(params)
    }

    fn parse_spec(&mut self) -> Result<SpecDecl, ParseError> {
        self.expect(Tok::Spec)?;
        self.expect(Tok::Fn)?;
        let name = self.ident()?;
        let params = self.parse_params()?;
        self.expect(Tok::Arrow)?;
        let ret = self.parse_type()?;
        let mut requires = Vec::new();
        while self.eat(&Tok::Requires) {
            requires.push(self.parse_pexpr()?);
        }
        let mut ensures = Vec::new();
        while self.eat(&Tok::Ensures) {
            ensures.push(self.parse_pexpr()?);
        }
        Ok(SpecDecl {
            name,
            params,
            ret,
            requires,
            ensures,
        })
    }

    fn parse_impl(&mut self) -> Result<ImplDecl, ParseError> {
        self.expect(Tok::Impl)?;
        self.expect(Tok::Fn)?;
        let name = self.ident()?;
        let params = self.parse_params()?;
        self.expect(Tok::Arrow)?;
        let ret = self.parse_type()?;
        let mut requires = Vec::new();
        while self.eat(&Tok::Requires) {
            requires.push(self.parse_pexpr()?);
        }
        let mut ensures = Vec::new();
        while self.eat(&Tok::Ensures) {
            ensures.push(self.parse_pexpr()?);
        }
        self.expect(Tok::Refines)?;
        let refines = self.ident()?;
        let body = if self.eat(&Tok::LBrace) {
            let body = self.parse_expr_seq()?;
            self.expect(Tok::RBrace)?;
            body
        } else {
            self.expect(Tok::Eq)?;
            self.parse_expr()?
        };
        Ok(ImplDecl {
            f: FnDecl {
                name,
                params,
                ret,
                requires,
                ensures,
                body,
            },
            refines,
        })
    }

    fn parse_param(&mut self) -> Result<Param, ParseError> {
        let mode = if self.eat(&Tok::Own) {
            Mode::Own
        } else if self.eat(&Tok::Borrow) {
            Mode::Borrow
        } else {
            Mode::Val
        };
        let name = self.ident()?;
        self.expect(Tok::Colon)?;

        let mut refinement = None;
        let kind = if self.eat(&Tok::Proof) {
            ParamKind::Proof(self.parse_pexpr()?)
        } else if self.eat(&Tok::LBrace) {
            // {x: T | P}; the binder name is documentation in the prototype.
            let _binder = self.ident()?;
            self.expect(Tok::Colon)?;
            let base = self.parse_type()?;
            self.expect(Tok::Pipe)?;
            refinement = Some(self.parse_pexpr()?);
            self.expect(Tok::RBrace)?;
            ParamKind::Runtime(base)
        } else {
            ParamKind::Runtime(self.parse_type()?)
        };
        Ok(Param {
            name,
            mode,
            kind,
            refinement,
        })
    }

    fn parse_type(&mut self) -> Result<Ty, ParseError> {
        match self.bump() {
            Tok::Ident(s) => match s.as_str() {
                "Int" => Ok(Ty::Int),
                "Bool" => Ok(Ty::Bool),
                "Unit" => Ok(Ty::Unit),
                "Prop" => Ok(Ty::Prop),
                "Array" => {
                    self.expect(Tok::Lt)?;
                    let inner = self.parse_type()?;
                    self.expect(Tok::Gt)?;
                    Ok(Ty::Array(Box::new(inner)))
                }
                other => self.err(format!("unknown type `{other}`")),
            },
            Tok::LParen => {
                let t = self.parse_type()?;
                self.expect(Tok::RParen)?;
                Ok(t)
            }
            other => self.err(format!("expected type, found {:?}", other)),
        }
    }

    fn parse_sort(&mut self) -> Result<SortAst, ParseError> {
        match self.bump() {
            Tok::Ident(s) => match s.as_str() {
                "Int" => Ok(SortAst::Int),
                "Bool" => Ok(SortAst::Bool),
                "Array" => {
                    if self.eat(&Tok::Lt) {
                        let _ = self.parse_type()?;
                        self.expect(Tok::Gt)?;
                    }
                    Ok(SortAst::Array)
                }
                other => self.err(format!("unknown sort `{other}`")),
            },
            other => self.err(format!("expected sort, found {:?}", other)),
        }
    }

    fn parse_theorem(&mut self) -> Result<ThmDecl, ParseError> {
        self.expect(Tok::Theorem)?;
        let name = self.ident()?;
        self.expect(Tok::Colon)?;
        let prop = self.parse_pexpr()?;
        self.expect(Tok::LBrace)?;
        let tactics = self.parse_tactics_until_rbrace()?;
        self.expect(Tok::RBrace)?;
        Ok(ThmDecl {
            name,
            prop,
            tactics,
        })
    }

    fn parse_tactics_until_rbrace(&mut self) -> Result<Vec<Tactic>, ParseError> {
        let mut out = Vec::new();
        while !matches!(self.peek(), Tok::RBrace | Tok::Eof) {
            out.push(self.parse_tactic()?);
            let _ = self.eat(&Tok::Semi);
        }
        Ok(out)
    }

    fn parse_tactic(&mut self) -> Result<Tactic, ParseError> {
        let t = self.bump();
        match t {
            Tok::Intro => {
                let name = if let Tok::Ident(_) = self.peek() {
                    Some(self.ident()?)
                } else {
                    None
                };
                Ok(Tactic::Intro(name))
            }
            Tok::Exact => Ok(Tactic::Exact(self.parse_proof_expr()?)),
            Tok::Assumption => Ok(Tactic::Assumption),
            Tok::Apply => Ok(Tactic::Apply(self.parse_proof_expr()?)),
            Tok::Split => Ok(Tactic::Split),
            Tok::Left => Ok(Tactic::Left),
            Tok::Right => Ok(Tactic::Right),
            Tok::Rfl => Ok(Tactic::Rfl),
            Tok::Trivial => Ok(Tactic::Trivial),
            Tok::Decide => Ok(Tactic::Decide),
            Tok::Contradiction => Ok(Tactic::Contradiction),
            other => self.err(format!("expected tactic, found {:?}", other)),
        }
    }

    fn parse_proof_expr(&mut self) -> Result<ProofExpr, ParseError> {
        let mut lhs = match self.bump() {
            Tok::Ident(s) => ProofExpr::Var(s),
            Tok::LParen => {
                let e = self.parse_proof_expr()?;
                self.expect(Tok::RParen)?;
                e
            }
            other => return self.err(format!("expected proof term, found {:?}", other)),
        };
        loop {
            match self.peek().clone() {
                Tok::Ident(s) => {
                    self.bump();
                    lhs = ProofExpr::App(Box::new(lhs), Box::new(ProofExpr::Var(s)));
                }
                Tok::LParen => {
                    self.bump();
                    let rhs = self.parse_proof_expr()?;
                    self.expect(Tok::RParen)?;
                    lhs = ProofExpr::App(Box::new(lhs), Box::new(rhs));
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn parse_proof_block(&mut self) -> Result<Vec<Tactic>, ParseError> {
        if self.eat(&Tok::LBrace) {
            let ts = self.parse_tactics_until_rbrace()?;
            self.expect(Tok::RBrace)?;
            Ok(ts)
        } else {
            Ok(vec![self.parse_tactic()?])
        }
    }

    fn parse_expr_seq(&mut self) -> Result<Expr, ParseError> {
        let first = self.parse_expr()?;
        if self.eat(&Tok::Semi) {
            if matches!(self.peek(), Tok::RBrace | Tok::Eof) {
                Ok(first)
            } else {
                let rest = self.parse_expr_seq()?;
                Ok(Expr::Seq(Box::new(first), Box::new(rest)))
            }
        } else {
            Ok(first)
        }
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_binary(0)
    }

    fn parse_binary(&mut self, min_prec: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_unary()?;
        loop {
            let (op, prec) = match self.peek() {
                Tok::PipePipe => (BinOp::Or, 1),
                Tok::AmpAmp => (BinOp::And, 2),
                Tok::EqEq => (BinOp::Eq, 3),
                Tok::NotEq => (BinOp::Ne, 3),
                Tok::Lt => (BinOp::Lt, 3),
                Tok::Le => (BinOp::Le, 3),
                Tok::Gt => (BinOp::Gt, 3),
                Tok::Ge => (BinOp::Ge, 3),
                Tok::Plus => (BinOp::Add, 4),
                Tok::Minus => (BinOp::Sub, 4),
                Tok::Star => (BinOp::Mul, 5),
                Tok::Slash => (BinOp::Div, 5),
                Tok::Percent => (BinOp::Mod, 5),
                _ => break,
            };
            if prec < min_prec {
                break;
            }
            self.bump();
            let rhs = self.parse_binary(prec + 1)?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        if self.eat(&Tok::Minus) {
            let e = self.parse_unary()?;
            Ok(Expr::Un(UnOp::Neg, Box::new(e)))
        } else if self.eat(&Tok::Bang) {
            let e = self.parse_unary()?;
            Ok(Expr::Un(UnOp::Not, Box::new(e)))
        } else {
            self.parse_postfix()
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut e = self.parse_atom()?;
        while self.eat(&Tok::LBracket) {
            let idx = self.parse_expr()?;
            self.expect(Tok::RBracket)?;
            e = Expr::Index(Box::new(e), Box::new(idx));
        }
        Ok(e)
    }

    fn parse_atom(&mut self) -> Result<Expr, ParseError> {
        match self.bump() {
            Tok::Int(n) => Ok(Expr::Int(n)),
            Tok::True => Ok(Expr::Bool(true)),
            Tok::False => Ok(Expr::Bool(false)),
            Tok::Ident(name) => {
                if self.eat(&Tok::LParen) {
                    let mut args = Vec::new();
                    if !self.eat(&Tok::RParen) {
                        loop {
                            let arg = if self.eat(&Tok::By) {
                                CallArg::Proof(self.parse_proof_block()?)
                            } else {
                                CallArg::Expr(self.parse_expr()?)
                            };
                            args.push(arg);
                            if self.eat(&Tok::Comma) {
                                continue;
                            }
                            self.expect(Tok::RParen)?;
                            break;
                        }
                    }
                    Ok(Expr::Call { name, args })
                } else {
                    Ok(Expr::Var(name))
                }
            }
            Tok::LParen => {
                if self.eat(&Tok::RParen) {
                    Ok(Expr::Unit)
                } else {
                    let e = self.parse_expr()?;
                    self.expect(Tok::RParen)?;
                    Ok(e)
                }
            }
            Tok::LBracket => {
                let mut items = Vec::new();
                if !self.eat(&Tok::RBracket) {
                    loop {
                        items.push(self.parse_expr()?);
                        if self.eat(&Tok::Comma) {
                            continue;
                        }
                        self.expect(Tok::RBracket)?;
                        break;
                    }
                }
                Ok(Expr::Array(items))
            }
            Tok::If => {
                let cond = self.parse_expr()?;
                self.expect(Tok::Then)?;
                let then = self.parse_expr()?;
                self.expect(Tok::Else)?;
                let els = self.parse_expr()?;
                Ok(Expr::If(Box::new(cond), Box::new(then), Box::new(els)))
            }
            Tok::Let => {
                let name = self.ident()?;
                self.expect(Tok::Eq)?;
                let value = self.parse_expr()?;
                let body = if self.eat(&Tok::In) {
                    self.parse_expr_seq()?
                } else if self.eat(&Tok::Semi) {
                    self.parse_expr_seq()?
                } else {
                    return self.err("expected `in` or `;` after let binding");
                };
                Ok(Expr::Let(name, Box::new(value), Box::new(body)))
            }
            other => self.err(format!("expected expression, found {:?}", other)),
        }
    }

    // -----------------------------------------------------------------------
    // Proposition expressions
    // -----------------------------------------------------------------------

    fn parse_pexpr(&mut self) -> Result<PExpr, ParseError> {
        self.parse_p_imp()
    }

    fn parse_p_imp(&mut self) -> Result<PExpr, ParseError> {
        let lhs = self.parse_p_or()?;
        if self.eat(&Tok::Arrow) {
            let rhs = self.parse_p_imp()?;
            Ok(PExpr::Imp(Box::new(lhs), Box::new(rhs)))
        } else {
            Ok(lhs)
        }
    }

    fn parse_p_or(&mut self) -> Result<PExpr, ParseError> {
        let mut lhs = self.parse_p_and()?;
        while self.eat(&Tok::PipePipe) {
            let rhs = self.parse_p_and()?;
            lhs = PExpr::Or(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_p_and(&mut self) -> Result<PExpr, ParseError> {
        let mut lhs = self.parse_p_not()?;
        while self.eat(&Tok::AmpAmp) {
            let rhs = self.parse_p_not()?;
            lhs = PExpr::And(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_p_not(&mut self) -> Result<PExpr, ParseError> {
        if self.eat(&Tok::Bang) {
            let e = self.parse_p_not()?;
            Ok(PExpr::Not(Box::new(e)))
        } else {
            self.parse_p_cmp()
        }
    }

    fn parse_p_cmp(&mut self) -> Result<PExpr, ParseError> {
        let lhs = self.parse_p_add()?;
        let op = match self.peek() {
            Tok::EqEq => Some(BinOp::Eq),
            Tok::NotEq => Some(BinOp::Ne),
            Tok::Lt => Some(BinOp::Lt),
            Tok::Le => Some(BinOp::Le),
            Tok::Gt => Some(BinOp::Gt),
            Tok::Ge => Some(BinOp::Ge),
            _ => None,
        };
        if let Some(op) = op {
            self.bump();
            let rhs = self.parse_p_add()?;
            Ok(PExpr::Cmp(op, Box::new(lhs), Box::new(rhs)))
        } else {
            Ok(lhs)
        }
    }

    fn parse_p_add(&mut self) -> Result<PExpr, ParseError> {
        let mut lhs = self.parse_p_mul()?;
        loop {
            let op = match self.peek() {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_p_mul()?;
            lhs = PExpr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_p_mul(&mut self) -> Result<PExpr, ParseError> {
        let mut lhs = self.parse_p_atom()?;
        loop {
            let op = match self.peek() {
                Tok::Star => BinOp::Mul,
                Tok::Slash => BinOp::Div,
                Tok::Percent => BinOp::Mod,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_p_atom()?;
            lhs = PExpr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_p_atom(&mut self) -> Result<PExpr, ParseError> {
        match self.bump() {
            Tok::Int(n) => Ok(PExpr::Int(n)),
            Tok::True => Ok(PExpr::Bool(true)),
            Tok::False => Ok(PExpr::Bool(false)),
            Tok::Ident(name) => {
                if name == "len" && self.eat(&Tok::LParen) {
                    let inner = self.parse_p_imp()?;
                    self.expect(Tok::RParen)?;
                    Ok(PExpr::Len(Box::new(inner)))
                } else if self.eat(&Tok::LParen) {
                    let mut args = Vec::new();
                    if !self.eat(&Tok::RParen) {
                        loop {
                            args.push(self.parse_p_imp()?);
                            if self.eat(&Tok::Comma) {
                                continue;
                            }
                            self.expect(Tok::RParen)?;
                            break;
                        }
                    }
                    Ok(PExpr::Call(name, args))
                } else {
                    Ok(PExpr::Var(name))
                }
            }
            Tok::LParen => {
                let e = self.parse_p_imp()?;
                self.expect(Tok::RParen)?;
                Ok(e)
            }
            Tok::LBracket => {
                let mut items = Vec::new();
                if !self.eat(&Tok::RBracket) {
                    loop {
                        items.push(self.parse_p_imp()?);
                        if self.eat(&Tok::Comma) {
                            continue;
                        }
                        self.expect(Tok::RBracket)?;
                        break;
                    }
                }
                Ok(PExpr::Array(items))
            }
            Tok::Minus => {
                let e = self.parse_p_atom()?;
                Ok(PExpr::Un(UnOp::Neg, Box::new(e)))
            }
            Tok::Forall => {
                let name = self.ident()?;
                self.expect(Tok::Colon)?;
                let sort = self.parse_sort()?;
                self.expect(Tok::Dot)?;
                let body = self.parse_p_imp()?;
                Ok(PExpr::Forall(name, sort, Box::new(body)))
            }
            Tok::Exists => {
                let name = self.ident()?;
                self.expect(Tok::Colon)?;
                let sort = self.parse_sort()?;
                self.expect(Tok::Dot)?;
                let body = self.parse_p_imp()?;
                Ok(PExpr::Exists(name, sort, Box::new(body)))
            }
            other => self.err(format!("expected proposition atom, found {:?}", other)),
        }
    }
}

pub fn parse(src: &str) -> Result<Program, ParseError> {
    let toks = lex(src)?;
    let mut p = Parser::new(toks);
    p.parse_program()
}

// ---------------------------------------------------------------------------
// Pretty printing (for --dump-ast)
// ---------------------------------------------------------------------------

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Var(s) => write!(f, "{s}"),
            Expr::Int(n) => write!(f, "{n}"),
            Expr::Bool(b) => write!(f, "{b}"),
            Expr::Unit => write!(f, "()"),
            Expr::Array(xs) => {
                write!(f, "[")?;
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{x}")?;
                }
                write!(f, "]")
            }
            Expr::Un(op, e) => write!(f, "{}{}", unop_str(*op), e),
            Expr::Bin(op, l, r) => write!(f, "({l} {} {r})", binop_str(*op)),
            Expr::If(c, t, e) => write!(f, "if {c} then {t} else {e}"),
            Expr::Let(n, v, b) => write!(f, "let {n} = {v} in {b}"),
            Expr::Call { name, args } => {
                write!(f, "{name}(")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    match a {
                        CallArg::Expr(e) => write!(f, "{e}")?,
                        CallArg::Proof(ts) => write!(f, "by {} tactic(s)", ts.len())?,
                    }
                }
                write!(f, ")")
            }
            Expr::Index(b, i) => write!(f, "{b}[{i}]"),
            Expr::Seq(a, b) => write!(f, "{a}; {b}"),
        }
    }
}

pub fn unop_str(op: UnOp) -> &'static str {
    match op {
        UnOp::Neg => "-",
        UnOp::Not => "!",
    }
}

pub fn binop_str(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Int => write!(f, "Int"),
            Ty::Bool => write!(f, "Bool"),
            Ty::Unit => write!(f, "Unit"),
            Ty::Prop => write!(f, "Prop"),
            Ty::Array(t) => write!(f, "Array<{t}>"),
        }
    }
}
