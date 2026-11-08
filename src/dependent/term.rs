use std::fmt;

use super::env::Env;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Term {
    Var(usize),
    Sort(u32),
    Const(String),
    App(Box<Term>, Box<Term>),
    Lam(String, Box<Term>, Box<Term>),
    Pi(String, Box<Term>, Box<Term>),
    Let(String, Box<Term>, Box<Term>, Box<Term>),
    /// Nat literal sugar. `whnf` expands it to `Nat.zero` / `Nat.succ`.
    Nat(u64),
}

#[derive(Clone, Debug)]
pub struct Def {
    pub ty: Term,
    pub body: Term,
    /// Reducible definitions are unfolded by `whnf`; opaque ones only expose
    /// their type. Lemmas are opaque, computation definitions are reducible.
    pub reducible: bool,
}

// ---------------------------------------------------------------------------
// Named surface terms, elaborated to de Bruijn
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum STerm {
    Var(String),
    Const(String),
    Sort(u32),
    App(Box<STerm>, Box<STerm>),
    Lam(String, Box<STerm>, Box<STerm>),
    Pi(String, Box<STerm>, Box<STerm>),
    Let(String, Box<STerm>, Box<STerm>, Box<STerm>),
    Nat(u64),
}

pub fn svar(name: &str) -> STerm {
    STerm::Var(name.to_string())
}

pub fn sconst(name: &str) -> STerm {
    STerm::Const(name.to_string())
}

pub fn sapp(f: STerm, a: STerm) -> STerm {
    STerm::App(Box::new(f), Box::new(a))
}

pub fn sapp2(f: STerm, a: STerm, b: STerm) -> STerm {
    sapp(sapp(f, a), b)
}

pub fn sapp3(f: STerm, a: STerm, b: STerm, c: STerm) -> STerm {
    sapp(sapp2(f, a, b), c)
}

pub fn sapp4(f: STerm, a: STerm, b: STerm, c: STerm, d: STerm) -> STerm {
    sapp(sapp3(f, a, b, c), d)
}

pub fn sapp5(f: STerm, a: STerm, b: STerm, c: STerm, d: STerm, e: STerm) -> STerm {
    sapp(sapp4(f, a, b, c, d), e)
}

pub fn sapp6(f: STerm, a: STerm, b: STerm, c: STerm, d: STerm, e: STerm, g: STerm) -> STerm {
    sapp(sapp5(f, a, b, c, d, e), g)
}

pub fn slam(name: &str, ty: STerm, body: STerm) -> STerm {
    STerm::Lam(name.to_string(), Box::new(ty), Box::new(body))
}

pub fn spi(name: &str, ty: STerm, body: STerm) -> STerm {
    STerm::Pi(name.to_string(), Box::new(ty), Box::new(body))
}

pub fn snat(n: u64) -> STerm {
    STerm::Nat(n)
}

pub fn elab(st: &STerm, ctx: &mut Vec<String>) -> Result<Term, String> {
    match st {
        STerm::Var(name) => ctx
            .iter()
            .rev()
            .position(|x| x == name)
            .map(Term::Var)
            .ok_or_else(|| format!("unbound variable `{name}`")),
        STerm::Const(name) => Ok(Term::Const(name.clone())),
        STerm::Sort(u) => Ok(Term::Sort(*u)),
        STerm::App(f, a) => Ok(Term::App(Box::new(elab(f, ctx)?), Box::new(elab(a, ctx)?))),
        STerm::Lam(n, ty, body) => {
            let ty = elab(ty, ctx)?;
            ctx.push(n.clone());
            let body = elab(body, ctx);
            ctx.pop();
            Ok(Term::Lam(n.clone(), Box::new(ty), Box::new(body?)))
        }
        STerm::Pi(n, ty, body) => {
            let ty = elab(ty, ctx)?;
            ctx.push(n.clone());
            let body = elab(body, ctx);
            ctx.pop();
            Ok(Term::Pi(n.clone(), Box::new(ty), Box::new(body?)))
        }
        STerm::Let(n, ty, val, body) => {
            let ty = elab(ty, ctx)?;
            let val = elab(val, ctx)?;
            ctx.push(n.clone());
            let body = elab(body, ctx);
            ctx.pop();
            Ok(Term::Let(
                n.clone(),
                Box::new(ty),
                Box::new(val),
                Box::new(body?),
            ))
        }
        STerm::Nat(n) => Ok(Term::Nat(*n)),
    }
}

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let env = Env::default();
        write!(f, "{}", env.pretty(self))
    }
}

/// Number of AST nodes, useful as a crude proof-size metric.
pub fn term_size(t: &Term) -> usize {
    match t {
        Term::Var(_) | Term::Sort(_) | Term::Const(_) | Term::Nat(_) => 1,
        Term::App(f, a) => 1 + term_size(f) + term_size(a),
        Term::Lam(_, a, b) | Term::Pi(_, a, b) => 1 + term_size(a) + term_size(b),
        Term::Let(_, a, v, b) => 1 + term_size(a) + term_size(v) + term_size(b),
    }
}
