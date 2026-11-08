use super::term::Term;

// ---------------------------------------------------------------------------
// De Bruijn operations
// ---------------------------------------------------------------------------

pub(super) fn shift(t: &Term, d: isize, cutoff: usize) -> Term {
    match t {
        Term::Var(i) => {
            if *i >= cutoff {
                Term::Var(((*i as isize) + d).max(0) as usize)
            } else {
                Term::Var(*i)
            }
        }
        Term::Sort(u) => Term::Sort(*u),
        Term::Const(n) => Term::Const(n.clone()),
        Term::App(f, a) => Term::App(Box::new(shift(f, d, cutoff)), Box::new(shift(a, d, cutoff))),
        Term::Lam(n, a, b) => Term::Lam(
            n.clone(),
            Box::new(shift(a, d, cutoff)),
            Box::new(shift(b, d, cutoff + 1)),
        ),
        Term::Pi(n, a, b) => Term::Pi(
            n.clone(),
            Box::new(shift(a, d, cutoff)),
            Box::new(shift(b, d, cutoff + 1)),
        ),
        Term::Let(n, a, v, b) => Term::Let(
            n.clone(),
            Box::new(shift(a, d, cutoff)),
            Box::new(shift(v, d, cutoff)),
            Box::new(shift(b, d, cutoff + 1)),
        ),
        Term::Nat(n) => Term::Nat(*n),
    }
}

pub(super) fn subst(t: &Term, j: usize, s: &Term) -> Term {
    match t {
        Term::Var(i) => {
            if *i == j {
                s.clone()
            } else if *i > j {
                Term::Var(*i - 1)
            } else {
                Term::Var(*i)
            }
        }
        Term::Sort(u) => Term::Sort(*u),
        Term::Const(n) => Term::Const(n.clone()),
        Term::App(f, a) => Term::App(Box::new(subst(f, j, s)), Box::new(subst(a, j, s))),
        Term::Lam(n, a, b) => Term::Lam(
            n.clone(),
            Box::new(subst(a, j, s)),
            Box::new(subst(b, j + 1, &shift(s, 1, 0))),
        ),
        Term::Pi(n, a, b) => Term::Pi(
            n.clone(),
            Box::new(subst(a, j, s)),
            Box::new(subst(b, j + 1, &shift(s, 1, 0))),
        ),
        Term::Let(n, a, v, b) => Term::Let(
            n.clone(),
            Box::new(subst(a, j, s)),
            Box::new(subst(v, j, s)),
            Box::new(subst(b, j + 1, &shift(s, 1, 0))),
        ),
        Term::Nat(n) => Term::Nat(*n),
    }
}

/// Instantiate the outermost binder of `t` with `s` (beta/let reduction).
/// `subst` already removes the binder and shifts the argument under inner
/// binders, so no extra shift is needed here.
pub fn instantiate(t: &Term, s: &Term) -> Term {
    subst(t, 0, s)
}

pub(super) fn spine(t: &Term) -> (Term, Vec<Term>) {
    let mut args = Vec::new();
    let mut cur = t.clone();
    while let Term::App(f, a) = cur {
        args.push(*a);
        cur = *f;
    }
    args.reverse();
    (cur, args)
}

pub(super) fn apply_many(f: Term, args: &[Term]) -> Term {
    let mut r = f;
    for a in args {
        r = Term::App(Box::new(r), Box::new(a.clone()));
    }
    r
}
