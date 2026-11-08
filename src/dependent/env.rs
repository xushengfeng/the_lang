use std::collections::HashMap;

use super::debruijn::apply_many;
use super::term::*;

#[derive(Default)]
pub struct Env {
    pub types: HashMap<String, Term>,
    pub defs: HashMap<String, Def>,
}

// ---------------------------------------------------------------------------
// Environment API
// ---------------------------------------------------------------------------

impl Env {
    pub fn add_type(&mut self, name: &str, ty: STerm) -> Result<(), String> {
        let ty = elab(&ty, &mut Vec::new())?;
        let mut ctx = Vec::new();
        let sort = self
            .infer(&mut ctx, &ty)
            .map_err(|e| format!("while adding type `{name}`:\n{e}"))?;
        if !matches!(self.whnf(&sort), Term::Sort(_)) {
            return Err(format!(
                "type of `{name}` is not a sort: {}",
                self.pretty(&sort)
            ));
        }
        self.types.insert(name.to_string(), ty);
        Ok(())
    }

    pub fn add_def(
        &mut self,
        name: &str,
        ty: STerm,
        body: STerm,
        reducible: bool,
    ) -> Result<(), String> {
        let ty = elab(&ty, &mut Vec::new())?;
        let body = elab(&body, &mut Vec::new())?;
        let mut ctx = Vec::new();
        let sort = self.infer(&mut ctx, &ty)?;
        if !matches!(self.whnf(&sort), Term::Sort(_)) {
            return Err(format!(
                "type of `{name}` is not a sort: {}",
                self.pretty(&sort)
            ));
        }
        self.check(&mut ctx, &body, &ty)
            .map_err(|e| format!("while checking `{name}`:\n{e}"))?;
        self.defs.insert(
            name.to_string(),
            Def {
                ty,
                body,
                reducible,
            },
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers for the demo
// ---------------------------------------------------------------------------

impl Env {
    pub fn nat(&self) -> Term {
        Term::Const("Nat".to_string())
    }

    pub fn list_nat(&self) -> Term {
        Term::App(
            Box::new(Term::Const("List".to_string())),
            Box::new(self.nat()),
        )
    }

    pub fn nat_lit(&self, n: u64) -> Term {
        Term::Nat(n)
    }

    pub fn nil(&self) -> Term {
        Term::App(
            Box::new(Term::Const("List.nil".to_string())),
            Box::new(self.nat()),
        )
    }

    pub fn cons(&self, x: Term, xs: Term) -> Term {
        Term::App(
            Box::new(Term::App(
                Box::new(Term::App(
                    Box::new(Term::Const("List.cons".to_string())),
                    Box::new(self.nat()),
                )),
                Box::new(x),
            )),
            Box::new(xs),
        )
    }

    pub fn list_of(&self, xs: &[u64]) -> Term {
        let mut out = self.nil();
        for x in xs.iter().rev() {
            out = self.cons(self.nat_lit(*x), out);
        }
        out
    }

    pub fn apply(&self, name: &str, args: &[Term]) -> Term {
        apply_many(Term::Const(name.to_string()), args)
    }

    pub fn type_of(&self, name: &str) -> Option<&Term> {
        self.defs
            .get(name)
            .map(|d| &d.ty)
            .or_else(|| self.types.get(name))
    }

    pub fn check_named(&self, name: &str) -> Result<(), String> {
        let d = self
            .defs
            .get(name)
            .ok_or_else(|| format!("unknown definition `{name}`"))?;
        let mut ctx = Vec::new();
        self.check(&mut ctx, &d.body, &d.ty)
    }
}

impl Env {
    /// Evaluate a closed `Nat` term to a host integer, if possible.
    pub fn eval_nat(&self, t: &Term) -> Option<u64> {
        match self.nf(t) {
            Term::Nat(n) => Some(n),
            Term::Const(n) if n == "Nat.zero" => Some(0),
            Term::App(f, a) => {
                if let Term::Const(n) = &*f {
                    if n == "Nat.succ" {
                        return self.eval_nat(&a).map(|x| x + 1);
                    }
                }
                None
            }
            _ => None,
        }
    }
}
