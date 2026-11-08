use super::ast::*;
use super::lexer::{lex, ParseError, Tok, Token};

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
                Tok::Proof => decls.push(Decl::Proof(self.parse_proof()?)),
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
                "Fn" => {
                    self.expect(Tok::LParen)?;
                    let mut params = Vec::new();
                    if !self.eat(&Tok::RParen) {
                        loop {
                            params.push(self.parse_type()?);
                            if self.eat(&Tok::Comma) {
                                continue;
                            }
                            self.expect(Tok::RParen)?;
                            break;
                        }
                    }
                    self.expect(Tok::Arrow)?;
                    let ret = self.parse_type()?;
                    Ok(Ty::Fn(params, Box::new(ret), Vec::new()))
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

    fn parse_proof(&mut self) -> Result<ProofDecl, ParseError> {
        self.expect(Tok::Proof)?;
        let name = self.ident()?;
        self.expect(Tok::Colon)?;
        let ty = self.parse_kterm()?;
        let body = if self.eat(&Tok::LBrace) {
            let body = self.parse_kterm()?;
            self.expect(Tok::RBrace)?;
            body
        } else {
            self.expect(Tok::Eq)?;
            self.parse_kterm()?
        };
        Ok(ProofDecl { name, ty, body })
    }

    fn parse_kterm(&mut self) -> Result<KTerm, ParseError> {
        let lhs = self.parse_kterm_app()?;
        if self.eat(&Tok::Arrow) {
            let rhs = self.parse_kterm()?;
            Ok(KTerm::Pi("_".into(), Box::new(lhs), Box::new(rhs)))
        } else {
            Ok(lhs)
        }
    }

    fn parse_kterm_app(&mut self) -> Result<KTerm, ParseError> {
        let mut head = self.parse_kterm_atom()?;
        while self.eat(&Tok::LParen) {
            let mut args = Vec::new();
            if !self.eat(&Tok::RParen) {
                loop {
                    args.push(self.parse_kterm()?);
                    if self.eat(&Tok::Comma) {
                        continue;
                    }
                    self.expect(Tok::RParen)?;
                    break;
                }
            }
            for arg in args {
                head = KTerm::App(Box::new(head), Box::new(arg));
            }
        }
        Ok(head)
    }

    fn parse_kterm_atom(&mut self) -> Result<KTerm, ParseError> {
        match self.bump() {
            Tok::Int(n) if n >= 0 => Ok(KTerm::Nat(n as u64)),
            Tok::Int(_) => self.err("kernel Nat literal must be non-negative"),
            Tok::LParen => {
                let term = self.parse_kterm()?;
                self.expect(Tok::RParen)?;
                Ok(term)
            }
            Tok::Ident(name) => {
                // Constructor forms: Pi(x, A, B), Lam(x, A, b), Let(x, A, v, b).
                if matches!(name.as_str(), "Pi" | "Lam" | "Let") && self.eat(&Tok::LParen) {
                    return self.parse_kterm_binder(&name);
                }
                if name == "Sort" && self.eat(&Tok::LParen) {
                    let n = match self.bump() {
                        Tok::Int(n) if n >= 0 => n as u32,
                        other => {
                            return self.err(format!("expected universe level, got {other:?}"))
                        }
                    };
                    self.expect(Tok::RParen)?;
                    return Ok(KTerm::Sort(n));
                }
                if name == "Prop" {
                    return Ok(KTerm::Sort(0));
                }
                if name == "Type" {
                    return Ok(KTerm::Sort(1));
                }
                // Dotted constant names: Nat.add, Eq.refl, ...
                let mut full = name;
                while self.eat(&Tok::Dot) {
                    let part = self.ident()?;
                    full.push('.');
                    full.push_str(&part);
                }
                Ok(KTerm::Name(full))
            }
            other => self.err(format!("expected kernel term, found {:?}", other)),
        }
    }

    fn parse_kterm_binder(&mut self, ctor: &str) -> Result<KTerm, ParseError> {
        let name = self.ident()?;
        self.expect(Tok::Comma)?;
        let ty = self.parse_kterm()?;
        self.expect(Tok::Comma)?;
        if ctor == "Let" {
            let value = self.parse_kterm()?;
            self.expect(Tok::Comma)?;
            let body = self.parse_kterm()?;
            self.expect(Tok::RParen)?;
            return Ok(KTerm::Let(
                name,
                Box::new(ty),
                Box::new(value),
                Box::new(body),
            ));
        }
        let body = self.parse_kterm()?;
        self.expect(Tok::RParen)?;
        Ok(match ctor {
            "Pi" => KTerm::Pi(name, Box::new(ty), Box::new(body)),
            "Lam" => KTerm::Lam(name, Box::new(ty), Box::new(body)),
            _ => unreachable!(),
        })
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
        loop {
            if self.eat(&Tok::LBracket) {
                let idx = self.parse_expr()?;
                self.expect(Tok::RBracket)?;
                e = Expr::Index(Box::new(e), Box::new(idx));
                continue;
            }
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
                e = match e {
                    Expr::Var(name) => Expr::Call { name, args },
                    callee => {
                        let mut exprs = Vec::with_capacity(args.len());
                        for arg in args {
                            match arg {
                                CallArg::Expr(e) => exprs.push(e),
                                CallArg::Proof(_) => {
                                    return Err(ParseError::new(
                                        "proof arguments require a named function call",
                                        self.toks[self.pos.saturating_sub(1)].line,
                                        self.toks[self.pos.saturating_sub(1)].col,
                                    ))
                                }
                            }
                        }
                        Expr::Apply {
                            callee: Box::new(callee),
                            args: exprs,
                        }
                    }
                };
                continue;
            }
            break;
        }
        Ok(e)
    }

    fn parse_lambda(&mut self) -> Result<Expr, ParseError> {
        self.expect(Tok::LParen)?;
        let mut params = Vec::new();
        if !self.eat(&Tok::RParen) {
            loop {
                let name = self.ident()?;
                self.expect(Tok::Colon)?;
                let ty = self.parse_type()?;
                params.push(LambdaParam { name, ty });
                if self.eat(&Tok::Comma) {
                    continue;
                }
                self.expect(Tok::RParen)?;
                break;
            }
        }
        self.expect(Tok::Arrow)?;
        let ret = self.parse_type()?;
        let body = if self.eat(&Tok::LBrace) {
            let body = self.parse_expr_seq()?;
            self.expect(Tok::RBrace)?;
            body
        } else {
            self.expect(Tok::Eq)?;
            self.parse_expr()?
        };
        Ok(Expr::Lambda {
            params,
            ret,
            body: Box::new(body),
            captures: Vec::new(),
        })
    }

    fn parse_atom(&mut self) -> Result<Expr, ParseError> {
        match self.bump() {
            Tok::Int(n) => Ok(Expr::Int(n)),
            Tok::True => Ok(Expr::Bool(true)),
            Tok::False => Ok(Expr::Bool(false)),
            Tok::Ident(name) => Ok(Expr::Var(name)),
            Tok::Fn => self.parse_lambda(),
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
                let body = if self.eat(&Tok::In) || self.eat(&Tok::Semi) {
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
