//! Bridge from surface kernel terms to the dependent core.
//!
//! The surface `proof` declaration is intentionally explicit: it contains a
//! full type and a full proof term, with no metavariables or unification.  The
//! bridge only resolves names (bound vs. global) and hands the terms to the
//! kernel, which remains the sole judge.

use std::collections::HashSet;

use super::env::Env;
use super::term::STerm;
use crate::syntax::{KTerm, ProofDecl};

impl Env {
    /// Check and register a surface `proof` declaration.
    pub fn add_surface_proof(&mut self, decl: &ProofDecl) -> Result<(), String> {
        let mut bound = HashSet::new();
        let ty =
            lower(&decl.ty, &mut bound).map_err(|e| format!("proof `{}` type: {e}", decl.name))?;
        let mut bound = HashSet::new();
        let body = lower(&decl.body, &mut bound)
            .map_err(|e| format!("proof `{}` term: {e}", decl.name))?;

        self.add_def(&decl.name, ty, body, false)
            .map_err(|e| format!("proof `{}`:\n{e}", decl.name))
    }
}

fn lower(term: &KTerm, bound: &mut HashSet<String>) -> Result<STerm, String> {
    Ok(match term {
        KTerm::Name(name) => {
            if bound.contains(name) {
                STerm::Var(name.clone())
            } else {
                STerm::Const(name.clone())
            }
        }
        KTerm::Sort(level) => STerm::Sort(*level),
        KTerm::Nat(n) => STerm::Nat(*n),
        KTerm::App(f, a) => STerm::App(Box::new(lower(f, bound)?), Box::new(lower(a, bound)?)),
        KTerm::Lam(name, ty, body) => {
            let ty = lower(ty, bound)?;
            let inserted = bound.insert(name.clone());
            let body = lower(body, bound);
            if inserted {
                bound.remove(name);
            }
            STerm::Lam(name.clone(), Box::new(ty), Box::new(body?))
        }
        KTerm::Pi(name, ty, body) => {
            let ty = lower(ty, bound)?;
            let inserted = bound.insert(name.clone());
            let body = lower(body, bound);
            if inserted {
                bound.remove(name);
            }
            STerm::Pi(name.clone(), Box::new(ty), Box::new(body?))
        }
        KTerm::Let(name, ty, value, body) => {
            let ty = lower(ty, bound)?;
            let value = lower(value, bound)?;
            let inserted = bound.insert(name.clone());
            let body = lower(body, bound);
            if inserted {
                bound.remove(name);
            }
            STerm::Let(name.clone(), Box::new(ty), Box::new(value), Box::new(body?))
        }
    })
}
