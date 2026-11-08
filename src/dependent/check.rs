use super::debruijn::{apply_many, instantiate, shift, spine};
use super::env::Env;
use super::term::Term;

// ---------------------------------------------------------------------------
// Type checking
// ---------------------------------------------------------------------------

impl Env {
    pub fn infer(&self, ctx: &mut Vec<(String, Term)>, t: &Term) -> Result<Term, String> {
        match t {
            Term::Var(i) => ctx
                .get(ctx.len().wrapping_sub(1 + *i))
                .map(|(_, ty)| shift(ty, *i as isize + 1, 0))
                .ok_or_else(|| format!("unbound de Bruijn index {i}")),
            Term::Sort(u) => Ok(Term::Sort(u + 1)),
            Term::Const(name) => {
                if let Some(ty) = self.types.get(name) {
                    Ok(ty.clone())
                } else if let Some(d) = self.defs.get(name) {
                    Ok(d.ty.clone())
                } else {
                    Err(format!("unknown constant `{name}`"))
                }
            }
            Term::App(f, a) => {
                let ft = self.whnf(&self.infer(ctx, f)?);
                match ft {
                    Term::Pi(_, dom, cod) => {
                        self.check(ctx, a, &dom)?;
                        Ok(instantiate(&cod, a))
                    }
                    other => Err(format!(
                        "application of non-function {}",
                        self.pretty(&other)
                    )),
                }
            }
            Term::Lam(n, a, b) => {
                let at = self.infer(ctx, a)?;
                if !matches!(self.whnf(&at), Term::Sort(_)) {
                    return Err(format!(
                        "lambda domain is not a type: {} : {}",
                        self.pretty(a),
                        self.pretty(&at)
                    ));
                }
                ctx.push((n.clone(), (**a).clone()));
                let bt = self.infer(ctx, b);
                ctx.pop();
                Ok(Term::Pi(n.clone(), a.clone(), Box::new(bt?)))
            }
            Term::Pi(n, a, b) => {
                let at = self.infer(ctx, a)?;
                let au = match self.whnf(&at) {
                    Term::Sort(u) => u,
                    _ => {
                        return Err(format!(
                            "Pi domain is not a type: {} : {}",
                            self.pretty(a),
                            self.pretty(&at)
                        ))
                    }
                };
                ctx.push((n.clone(), (**a).clone()));
                let bt = self.infer(ctx, b);
                ctx.pop();
                let bt = bt?;
                let bu = match self.whnf(&bt) {
                    Term::Sort(u) => u,
                    _ => {
                        return Err(format!(
                            "Pi codomain is not a type: {} : {}",
                            self.pretty(b),
                            self.pretty(&bt)
                        ))
                    }
                };
                let level = if bu == 0 { 0 } else { au.max(bu) };
                Ok(Term::Sort(level))
            }
            Term::Let(n, a, v, b) => {
                let at = self.infer(ctx, a)?;
                if !matches!(self.whnf(&at), Term::Sort(_)) {
                    return Err(format!("let type is not a type: {}", self.pretty(&at)));
                }
                self.check(ctx, v, a)?;
                ctx.push((n.clone(), (**a).clone()));
                let bt = self.infer(ctx, b);
                ctx.pop();
                Ok(instantiate(&bt?, v))
            }
            Term::Nat(_) => Ok(Term::Const("Nat".to_string())),
        }
    }

    pub fn check(
        &self,
        ctx: &mut Vec<(String, Term)>,
        t: &Term,
        expected: &Term,
    ) -> Result<(), String> {
        let actual = self.infer(ctx, t)?;
        if self.def_eq(&actual, expected) {
            Ok(())
        } else {
            Err(format!(
                "type mismatch\n  expected: {}\n  actual:   {}",
                self.pretty(expected),
                self.pretty(&actual)
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Reduction and definitional equality
// ---------------------------------------------------------------------------

impl Env {
    pub fn whnf(&self, t: &Term) -> Term {
        self.whnf_fuel(t, 20_000)
    }

    fn whnf_fuel(&self, t: &Term, fuel: usize) -> Term {
        if fuel == 0 {
            return t.clone();
        }
        match t {
            Term::Let(_, _, v, b) => {
                let r = instantiate(b, v);
                self.whnf_fuel(&r, fuel - 1)
            }
            Term::App(_, _) => {
                let (head, args) = spine(t);
                if let Term::Lam(_, _, body) = &head {
                    if let Some((first, rest)) = args.split_first() {
                        let r = apply_many(instantiate(body, first), rest);
                        return self.whnf_fuel(&r, fuel - 1);
                    }
                }
                if let Term::Const(name) = &head {
                    if let Some(d) = self.defs.get(name) {
                        if d.reducible {
                            let r = apply_many(d.body.clone(), &args);
                            return self.whnf_fuel(&r, fuel - 1);
                        }
                    }
                    match name.as_str() {
                        "Nat.rec" | "Nat.recProp" if args.len() >= 4 => {
                            if let Some(r) =
                                self.reduce_nat_rec(name, &args[0], &args[1], &args[2], &args[3])
                            {
                                let r = apply_many(r, &args[4..]);
                                return self.whnf_fuel(&r, fuel - 1);
                            }
                        }
                        "Bool.rec" | "Bool.recProp" if args.len() >= 4 => {
                            let b = self.whnf_fuel(&args[3], fuel - 1);
                            let r = match &b {
                                Term::Const(n) if n == "Bool.true" => Some(args[1].clone()),
                                Term::Const(n) if n == "Bool.false" => Some(args[2].clone()),
                                _ => None,
                            };
                            if let Some(r) = r {
                                let r = apply_many(r, &args[4..]);
                                return self.whnf_fuel(&r, fuel - 1);
                            }
                        }
                        "List.rec" | "List.recProp" if args.len() >= 5 => {
                            if let Some(r) = self.reduce_list_rec(
                                name, &args[0], &args[1], &args[2], &args[3], &args[4],
                            ) {
                                let r = apply_many(r, &args[5..]);
                                return self.whnf_fuel(&r, fuel - 1);
                            }
                        }
                        "Eq.subst" if args.len() >= 6 => {
                            let alpha = &args[0];
                            let a = &args[2];
                            let b = &args[3];
                            let h = &args[4];
                            let pa = &args[5];
                            if self.is_refl(h, alpha, a) || self.def_eq(a, b) {
                                let r = apply_many(pa.clone(), &args[6..]);
                                return self.whnf_fuel(&r, fuel - 1);
                            }
                        }
                        _ => {}
                    }
                }
                apply_many(head, &args)
            }
            Term::Const(name) => {
                if let Some(d) = self.defs.get(name) {
                    if d.reducible {
                        return self.whnf_fuel(&d.body, fuel - 1);
                    }
                }
                t.clone()
            }
            Term::Nat(n) => {
                if *n == 0 {
                    Term::Const("Nat.zero".to_string())
                } else {
                    Term::App(
                        Box::new(Term::Const("Nat.succ".to_string())),
                        Box::new(Term::Nat(n - 1)),
                    )
                }
            }
            _ => t.clone(),
        }
    }

    fn is_refl(&self, h: &Term, alpha: &Term, a: &Term) -> bool {
        let (head, args) = spine(h);
        if let Term::Const(name) = head {
            if name == "Eq.refl" && args.len() == 2 {
                return self.def_eq(&args[0], alpha) && self.def_eq(&args[1], a);
            }
        }
        false
    }

    fn reduce_nat_rec(
        &self,
        rec_name: &str,
        motive: &Term,
        base: &Term,
        step: &Term,
        n: &Term,
    ) -> Option<Term> {
        let n = self.whnf(n);
        match n {
            Term::Const(name) if name == "Nat.zero" => Some(base.clone()),
            Term::App(f, k) if matches!(&*f, Term::Const(name) if name == "Nat.succ") => {
                let rec = apply_many(
                    Term::Const(rec_name.to_string()),
                    &[motive.clone(), base.clone(), step.clone(), (*k).clone()],
                );
                Some(Term::App(
                    Box::new(Term::App(Box::new(step.clone()), k)),
                    Box::new(rec),
                ))
            }
            _ => None,
        }
    }

    fn reduce_list_rec(
        &self,
        rec_name: &str,
        alpha: &Term,
        motive: &Term,
        nil_case: &Term,
        cons_case: &Term,
        xs: &Term,
    ) -> Option<Term> {
        let xs = self.whnf(xs);
        let (head, args) = spine(&xs);
        if let Term::Const(name) = &head {
            if name == "List.nil" && args.len() == 1 {
                return Some(nil_case.clone());
            }
            if name == "List.cons" && args.len() == 3 {
                let x = &args[1];
                let rest = &args[2];
                let rec = apply_many(
                    Term::Const(rec_name.to_string()),
                    &[
                        alpha.clone(),
                        motive.clone(),
                        nil_case.clone(),
                        cons_case.clone(),
                        rest.clone(),
                    ],
                );
                return Some(apply_many(
                    cons_case.clone(),
                    &[x.clone(), rest.clone(), rec],
                ));
            }
        }
        None
    }

    pub fn def_eq(&self, a: &Term, b: &Term) -> bool {
        self.def_eq_fuel(a, b, 20_000)
    }

    fn def_eq_fuel(&self, a: &Term, b: &Term, fuel: usize) -> bool {
        if fuel == 0 {
            return a == b;
        }
        let a = self.whnf_fuel(a, fuel);
        let b = self.whnf_fuel(b, fuel);
        match (&a, &b) {
            (Term::Var(i), Term::Var(j)) => i == j,
            (Term::Sort(i), Term::Sort(j)) => i == j,
            (Term::Const(i), Term::Const(j)) => i == j,
            (Term::Nat(i), Term::Nat(j)) => i == j,
            (Term::App(f1, a1), Term::App(f2, a2)) => {
                self.def_eq_fuel(f1, f2, fuel - 1) && self.def_eq_fuel(a1, a2, fuel - 1)
            }
            (Term::Lam(_, a1, b1), Term::Lam(_, a2, b2))
            | (Term::Pi(_, a1, b1), Term::Pi(_, a2, b2)) => {
                self.def_eq_fuel(a1, a2, fuel - 1) && self.def_eq_fuel(b1, b2, fuel - 1)
            }
            (Term::Let(_, a1, v1, b1), Term::Let(_, a2, v2, b2)) => {
                self.def_eq_fuel(a1, a2, fuel - 1)
                    && self.def_eq_fuel(v1, v2, fuel - 1)
                    && self.def_eq_fuel(b1, b2, fuel - 1)
            }
            _ => false,
        }
    }

    pub fn nf(&self, t: &Term) -> Term {
        let t = self.whnf(t);
        match t {
            Term::App(f, a) => Term::App(Box::new(self.nf(&f)), Box::new(self.nf(&a))),
            Term::Lam(n, a, b) => Term::Lam(n, Box::new(self.nf(&a)), Box::new(self.nf(&b))),
            Term::Pi(n, a, b) => Term::Pi(n, Box::new(self.nf(&a)), Box::new(self.nf(&b))),
            other => other,
        }
    }

    pub fn pretty(&self, t: &Term) -> String {
        self.pretty_in(t, &mut Vec::new())
    }

    fn pretty_in(&self, t: &Term, names: &mut Vec<String>) -> String {
        match t {
            Term::Var(i) => names
                .get(names.len().wrapping_sub(1 + *i))
                .cloned()
                .unwrap_or_else(|| format!("#{i}")),
            Term::Sort(0) => "Prop".to_string(),
            Term::Sort(1) => "Type".to_string(),
            Term::Sort(u) => format!("Type {u}"),
            Term::Const(n) => n.clone(),
            Term::App(f, a) => {
                format!(
                    "({} {})",
                    self.pretty_in(f, names),
                    self.pretty_in(a, names)
                )
            }
            Term::Lam(n, a, b) => {
                let a = self.pretty_in(a, names);
                names.push(n.clone());
                let b = self.pretty_in(b, names);
                names.pop();
                format!("λ {n} : {a}. {b}")
            }
            Term::Pi(n, a, b) => {
                let a = self.pretty_in(a, names);
                names.push(n.clone());
                let b = self.pretty_in(b, names);
                names.pop();
                format!("Π {n} : {a}. {b}")
            }
            Term::Let(n, a, v, b) => {
                let a = self.pretty_in(a, names);
                let v = self.pretty_in(v, names);
                names.push(n.clone());
                let b = self.pretty_in(b, names);
                names.pop();
                format!("let {n} : {a} := {v} in {b}")
            }
            Term::Nat(n) => n.to_string(),
        }
    }
}
