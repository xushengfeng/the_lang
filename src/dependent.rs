//! A small dependently typed kernel, Lean-like in spirit.
//!
//! This is intentionally a *kernel*, not a full elaborator:
//!
//! * terms use de Bruijn indices;
//! * universes are `Prop` (`Sort 0`) and `Type u` (`Sort (u+1)`);
//! * the built-in inductive types are `Nat`, `List` and `Eq`;
//! * recursion is expressed through `Nat.rec` / `List.rec`;
//! * proofs are ordinary terms checked by the kernel.
//!
//! `Nat.recProp` and `List.recProp` are Prop-motive variants of the recursors.
//! They keep the prototype small without universe polymorphism.

use std::collections::HashMap;
use std::fmt;

// ---------------------------------------------------------------------------
// Kernel terms
// ---------------------------------------------------------------------------

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

#[derive(Default)]
pub struct Env {
    pub types: HashMap<String, Term>,
    pub defs: HashMap<String, Def>,
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

// ---------------------------------------------------------------------------
// De Bruijn operations
// ---------------------------------------------------------------------------

fn shift(t: &Term, d: isize, cutoff: usize) -> Term {
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

fn subst(t: &Term, j: usize, s: &Term) -> Term {
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

fn spine(t: &Term) -> (Term, Vec<Term>) {
    let mut args = Vec::new();
    let mut cur = t.clone();
    while let Term::App(f, a) = cur {
        args.push(*a);
        cur = *f;
    }
    args.reverse();
    (cur, args)
}

fn apply_many(f: Term, args: &[Term]) -> Term {
    let mut r = f;
    for a in args {
        r = Term::App(Box::new(r), Box::new(a.clone()));
    }
    r
}

// ---------------------------------------------------------------------------
// Environment construction
// ---------------------------------------------------------------------------

impl Env {
    pub fn new() -> Result<Self, String> {
        let mut env = Env::default();

        let ty = STerm::Sort(1);
        let prop = STerm::Sort(0);
        let nat = sconst("Nat");
        let list = |a: STerm| sapp(sconst("List"), a);
        let nat_succ = |a: STerm| sapp(sconst("Nat.succ"), a);
        let nat_add = |a: STerm, b: STerm| sapp2(sconst("Nat.add"), a, b);
        let eq = |a: STerm, x: STerm, y: STerm| sapp3(sconst("Eq"), a, x, y);
        let refl = |a: STerm, x: STerm| sapp2(sconst("Eq.refl"), a, x);

        // Nat
        env.add_type("Nat", ty.clone())?;
        env.add_type("Nat.zero", nat.clone())?;
        env.add_type("Nat.succ", spi("_", nat.clone(), nat.clone()))?;

        // List
        env.add_type("List", spi("α", ty.clone(), ty.clone()))?;
        env.add_type("List.nil", spi("α", ty.clone(), list(svar("α"))))?;
        env.add_type(
            "List.cons",
            spi(
                "α",
                ty.clone(),
                spi("_", svar("α"), spi("_", list(svar("α")), list(svar("α")))),
            ),
        )?;

        // Nat.rec : (motive : Nat -> Type) -> motive 0 ->
        //            ((n : Nat) -> motive n -> motive (succ n)) -> (n : Nat) -> motive n
        let nat_motive_ty = spi("_", nat.clone(), ty.clone());
        let nat_rec_ty = spi(
            "motive",
            nat_motive_ty.clone(),
            spi(
                "base",
                sapp(svar("motive"), sconst("Nat.zero")),
                spi(
                    "step",
                    spi(
                        "n",
                        nat.clone(),
                        spi(
                            "ih",
                            sapp(svar("motive"), svar("n")),
                            sapp(svar("motive"), nat_succ(svar("n"))),
                        ),
                    ),
                    spi("n", nat.clone(), sapp(svar("motive"), svar("n"))),
                ),
            ),
        );
        env.add_type("Nat.rec", nat_rec_ty)?;

        let nat_rec_prop_ty = spi(
            "motive",
            spi("_", nat.clone(), prop.clone()),
            spi(
                "base",
                sapp(svar("motive"), sconst("Nat.zero")),
                spi(
                    "step",
                    spi(
                        "n",
                        nat.clone(),
                        spi(
                            "ih",
                            sapp(svar("motive"), svar("n")),
                            sapp(svar("motive"), nat_succ(svar("n"))),
                        ),
                    ),
                    spi("n", nat.clone(), sapp(svar("motive"), svar("n"))),
                ),
            ),
        );
        env.add_type("Nat.recProp", nat_rec_prop_ty)?;

        // Bool and its recursor. This is a minimal data type used by the
        // ownership model; a production kernel would use general inductives.
        let bool_ty = sconst("Bool");
        env.add_type("Bool", ty.clone())?;
        env.add_type("Bool.true", bool_ty.clone())?;
        env.add_type("Bool.false", bool_ty.clone())?;

        let bool_rec_ty = spi(
            "motive",
            spi("_", bool_ty.clone(), ty.clone()),
            spi(
                "t",
                sapp(svar("motive"), sconst("Bool.true")),
                spi(
                    "f",
                    sapp(svar("motive"), sconst("Bool.false")),
                    spi("b", bool_ty.clone(), sapp(svar("motive"), svar("b"))),
                ),
            ),
        );
        env.add_type("Bool.rec", bool_rec_ty)?;

        let bool_rec_prop_ty = spi(
            "motive",
            spi("_", bool_ty.clone(), prop.clone()),
            spi(
                "t",
                sapp(svar("motive"), sconst("Bool.true")),
                spi(
                    "f",
                    sapp(svar("motive"), sconst("Bool.false")),
                    spi("b", bool_ty.clone(), sapp(svar("motive"), svar("b"))),
                ),
            ),
        );
        env.add_type("Bool.recProp", bool_rec_prop_ty)?;

        // List.rec : (α : Type) -> (motive : List α -> Type) -> motive nil ->
        //            ((x : α) -> (xs : List α) -> motive xs -> motive (cons x xs)) ->
        //            (xs : List α) -> motive xs
        let list_rec_ty = spi(
            "α",
            ty.clone(),
            spi(
                "motive",
                spi("_", list(svar("α")), ty.clone()),
                spi(
                    "nil",
                    sapp(svar("motive"), sapp(sconst("List.nil"), svar("α"))),
                    spi(
                        "cons",
                        spi(
                            "x",
                            svar("α"),
                            spi(
                                "xs",
                                list(svar("α")),
                                spi(
                                    "ih",
                                    sapp(svar("motive"), svar("xs")),
                                    sapp(
                                        svar("motive"),
                                        sapp3(
                                            sconst("List.cons"),
                                            svar("α"),
                                            svar("x"),
                                            svar("xs"),
                                        ),
                                    ),
                                ),
                            ),
                        ),
                        spi("xs", list(svar("α")), sapp(svar("motive"), svar("xs"))),
                    ),
                ),
            ),
        );
        env.add_type("List.rec", list_rec_ty)?;

        let list_rec_prop_ty = spi(
            "α",
            ty.clone(),
            spi(
                "motive",
                spi("_", list(svar("α")), prop.clone()),
                spi(
                    "nil",
                    sapp(svar("motive"), sapp(sconst("List.nil"), svar("α"))),
                    spi(
                        "cons",
                        spi(
                            "x",
                            svar("α"),
                            spi(
                                "xs",
                                list(svar("α")),
                                spi(
                                    "ih",
                                    sapp(svar("motive"), svar("xs")),
                                    sapp(
                                        svar("motive"),
                                        sapp3(
                                            sconst("List.cons"),
                                            svar("α"),
                                            svar("x"),
                                            svar("xs"),
                                        ),
                                    ),
                                ),
                            ),
                        ),
                        spi("xs", list(svar("α")), sapp(svar("motive"), svar("xs"))),
                    ),
                ),
            ),
        );
        env.add_type("List.recProp", list_rec_prop_ty)?;

        // Eq : (α : Type) -> α -> α -> Prop
        env.add_type(
            "Eq",
            spi(
                "α",
                ty.clone(),
                spi("a", svar("α"), spi("b", svar("α"), prop.clone())),
            ),
        )?;
        env.add_type(
            "Eq.refl",
            spi(
                "α",
                ty.clone(),
                spi("a", svar("α"), eq(svar("α"), svar("a"), svar("a"))),
            ),
        )?;

        // Eq.subst : (α : Type) -> (motive : α -> Prop) -> (a b : α) ->
        //            Eq α a b -> motive a -> motive b
        env.add_type(
            "Eq.subst",
            spi(
                "α",
                ty.clone(),
                spi(
                    "motive",
                    spi("_", svar("α"), prop.clone()),
                    spi(
                        "a",
                        svar("α"),
                        spi(
                            "b",
                            svar("α"),
                            spi(
                                "h",
                                eq(svar("α"), svar("a"), svar("b")),
                                spi(
                                    "pa",
                                    sapp(svar("motive"), svar("a")),
                                    sapp(svar("motive"), svar("b")),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
        )?;

        // Nat.add
        let nat_add_ty = spi("_", nat.clone(), spi("_", nat.clone(), nat.clone()));
        let nat_add_body = slam(
            "a",
            nat.clone(),
            slam(
                "b",
                nat.clone(),
                sapp4(
                    sconst("Nat.rec"),
                    slam("_", nat.clone(), nat.clone()),
                    svar("b"),
                    slam(
                        "n",
                        nat.clone(),
                        slam("ih", nat.clone(), nat_succ(svar("ih"))),
                    ),
                    svar("a"),
                ),
            ),
        );
        env.add_def("Nat.add", nat_add_ty, nat_add_body, true)?;

        // Eq.symm
        let eq_symm_ty = spi(
            "α",
            ty.clone(),
            spi(
                "a",
                svar("α"),
                spi(
                    "b",
                    svar("α"),
                    spi(
                        "h",
                        eq(svar("α"), svar("a"), svar("b")),
                        eq(svar("α"), svar("b"), svar("a")),
                    ),
                ),
            ),
        );
        let eq_symm_body = slam(
            "α",
            ty.clone(),
            slam(
                "a",
                svar("α"),
                slam(
                    "b",
                    svar("α"),
                    slam(
                        "h",
                        eq(svar("α"), svar("a"), svar("b")),
                        sapp6(
                            sconst("Eq.subst"),
                            svar("α"),
                            slam("x", svar("α"), eq(svar("α"), svar("x"), svar("a"))),
                            svar("a"),
                            svar("b"),
                            svar("h"),
                            refl(svar("α"), svar("a")),
                        ),
                    ),
                ),
            ),
        );
        env.add_def("Eq.symm", eq_symm_ty, eq_symm_body, true)?;

        // Eq.trans
        let eq_trans_ty = spi(
            "α",
            ty.clone(),
            spi(
                "a",
                svar("α"),
                spi(
                    "b",
                    svar("α"),
                    spi(
                        "c",
                        svar("α"),
                        spi(
                            "h1",
                            eq(svar("α"), svar("a"), svar("b")),
                            spi(
                                "h2",
                                eq(svar("α"), svar("b"), svar("c")),
                                eq(svar("α"), svar("a"), svar("c")),
                            ),
                        ),
                    ),
                ),
            ),
        );
        let eq_trans_body = slam(
            "α",
            ty.clone(),
            slam(
                "a",
                svar("α"),
                slam(
                    "b",
                    svar("α"),
                    slam(
                        "c",
                        svar("α"),
                        slam(
                            "h1",
                            eq(svar("α"), svar("a"), svar("b")),
                            slam(
                                "h2",
                                eq(svar("α"), svar("b"), svar("c")),
                                sapp6(
                                    sconst("Eq.subst"),
                                    svar("α"),
                                    slam("x", svar("α"), eq(svar("α"), svar("a"), svar("x"))),
                                    svar("b"),
                                    svar("c"),
                                    svar("h2"),
                                    svar("h1"),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
        );
        env.add_def("Eq.trans", eq_trans_ty, eq_trans_body, true)?;

        // congr_succ : ∀ a b, Eq Nat a b -> Eq Nat (succ a) (succ b)
        let congr_succ_ty = spi(
            "a",
            nat.clone(),
            spi(
                "b",
                nat.clone(),
                spi(
                    "h",
                    eq(nat.clone(), svar("a"), svar("b")),
                    eq(nat.clone(), nat_succ(svar("a")), nat_succ(svar("b"))),
                ),
            ),
        );
        let congr_succ_body = slam(
            "a",
            nat.clone(),
            slam(
                "b",
                nat.clone(),
                slam(
                    "h",
                    eq(nat.clone(), svar("a"), svar("b")),
                    sapp6(
                        sconst("Eq.subst"),
                        nat.clone(),
                        slam(
                            "x",
                            nat.clone(),
                            eq(nat.clone(), nat_succ(svar("a")), nat_succ(svar("x"))),
                        ),
                        svar("a"),
                        svar("b"),
                        svar("h"),
                        refl(nat.clone(), nat_succ(svar("a"))),
                    ),
                ),
            ),
        );
        env.add_def("congr_succ", congr_succ_ty, congr_succ_body, true)?;

        // sum_ref : List Nat -> Nat
        // sum_ref xs = List.rec _ (λ _ => Nat) 0 (λ x xs ih => ih + x) xs
        let sum_ref_ty = spi("_", list(nat.clone()), nat.clone());
        let sum_ref_body = slam(
            "xs",
            list(nat.clone()),
            sapp5(
                sconst("List.rec"),
                nat.clone(),
                slam("_", list(nat.clone()), nat.clone()),
                sconst("Nat.zero"),
                slam(
                    "x",
                    nat.clone(),
                    slam(
                        "xs",
                        list(nat.clone()),
                        slam("ih", nat.clone(), nat_add(svar("ih"), svar("x"))),
                    ),
                ),
                svar("xs"),
            ),
        );
        env.add_def("sum_ref", sum_ref_ty, sum_ref_body, true)?;

        // sum_fast_go : List Nat -> Nat -> Nat
        // go xs = List.rec _ (λ _ => Nat -> Nat) (λ acc => acc)
        //                    (λ x xs ih => λ acc => ih (x + acc)) xs
        let sum_fast_go_ty = spi("_", list(nat.clone()), spi("_", nat.clone(), nat.clone()));
        let sum_fast_go_body = slam(
            "xs",
            list(nat.clone()),
            sapp5(
                sconst("List.rec"),
                nat.clone(),
                slam("_", list(nat.clone()), spi("_", nat.clone(), nat.clone())),
                slam("acc", nat.clone(), svar("acc")),
                slam(
                    "x",
                    nat.clone(),
                    slam(
                        "xs",
                        list(nat.clone()),
                        slam(
                            "ih",
                            spi("_", nat.clone(), nat.clone()),
                            slam(
                                "acc",
                                nat.clone(),
                                sapp(svar("ih"), nat_add(svar("x"), svar("acc"))),
                            ),
                        ),
                    ),
                ),
                svar("xs"),
            ),
        );
        env.add_def("sum_fast_go", sum_fast_go_ty, sum_fast_go_body, true)?;

        // sum_fast xs = sum_fast_go xs 0
        let sum_fast_ty = spi("_", list(nat.clone()), nat.clone());
        let sum_fast_body = slam(
            "xs",
            list(nat.clone()),
            sapp(sapp(sconst("sum_fast_go"), svar("xs")), sconst("Nat.zero")),
        );
        env.add_def("sum_fast", sum_fast_ty, sum_fast_body, true)?;

        // add_assoc : ∀ a b c, (a + b) + c = a + (b + c)
        let add_assoc_ty = spi(
            "a",
            nat.clone(),
            spi(
                "b",
                nat.clone(),
                spi(
                    "c",
                    nat.clone(),
                    eq(
                        nat.clone(),
                        nat_add(nat_add(svar("a"), svar("b")), svar("c")),
                        nat_add(svar("a"), nat_add(svar("b"), svar("c"))),
                    ),
                ),
            ),
        );
        let add_assoc_motive = slam(
            "a",
            nat.clone(),
            spi(
                "b",
                nat.clone(),
                spi(
                    "c",
                    nat.clone(),
                    eq(
                        nat.clone(),
                        nat_add(nat_add(svar("a"), svar("b")), svar("c")),
                        nat_add(svar("a"), nat_add(svar("b"), svar("c"))),
                    ),
                ),
            ),
        );
        let add_assoc_body = slam(
            "a",
            nat.clone(),
            sapp4(
                sconst("Nat.recProp"),
                add_assoc_motive.clone(),
                slam(
                    "b",
                    nat.clone(),
                    slam(
                        "c",
                        nat.clone(),
                        refl(nat.clone(), nat_add(svar("b"), svar("c"))),
                    ),
                ),
                slam(
                    "a",
                    nat.clone(),
                    slam(
                        "ih",
                        sapp(add_assoc_motive.clone(), svar("a")),
                        slam(
                            "b",
                            nat.clone(),
                            slam(
                                "c",
                                nat.clone(),
                                sapp3(
                                    sconst("congr_succ"),
                                    nat_add(nat_add(svar("a"), svar("b")), svar("c")),
                                    nat_add(svar("a"), nat_add(svar("b"), svar("c"))),
                                    sapp2(svar("ih"), svar("b"), svar("c")),
                                ),
                            ),
                        ),
                    ),
                ),
                svar("a"),
            ),
        );
        env.add_def("add_assoc", add_assoc_ty, add_assoc_body, false)?;

        // add_zero : ∀ n, n + 0 = n
        let add_zero_ty = spi(
            "n",
            nat.clone(),
            eq(
                nat.clone(),
                nat_add(svar("n"), sconst("Nat.zero")),
                svar("n"),
            ),
        );
        let add_zero_motive = slam(
            "n",
            nat.clone(),
            eq(
                nat.clone(),
                nat_add(svar("n"), sconst("Nat.zero")),
                svar("n"),
            ),
        );
        let add_zero_body = slam(
            "n",
            nat.clone(),
            sapp4(
                sconst("Nat.recProp"),
                add_zero_motive.clone(),
                refl(nat.clone(), sconst("Nat.zero")),
                slam(
                    "n",
                    nat.clone(),
                    slam(
                        "ih",
                        sapp(add_zero_motive.clone(), svar("n")),
                        sapp3(
                            sconst("congr_succ"),
                            nat_add(svar("n"), sconst("Nat.zero")),
                            svar("n"),
                            svar("ih"),
                        ),
                    ),
                ),
                svar("n"),
            ),
        );
        env.add_def("add_zero", add_zero_ty, add_zero_body, false)?;

        // go_eq : ∀ xs acc, sum_fast_go xs acc = sum_ref xs + acc
        let go_eq_prop = |xs: STerm, acc: STerm| {
            eq(
                nat.clone(),
                sapp(sapp(sconst("sum_fast_go"), xs.clone()), acc.clone()),
                nat_add(sapp(sconst("sum_ref"), xs), acc),
            )
        };
        let go_eq_ty = spi(
            "xs",
            list(nat.clone()),
            spi("acc", nat.clone(), go_eq_prop(svar("xs"), svar("acc"))),
        );
        let go_eq_motive = slam(
            "xs",
            list(nat.clone()),
            spi("acc", nat.clone(), go_eq_prop(svar("xs"), svar("acc"))),
        );
        let go_eq_body = slam(
            "xs",
            list(nat.clone()),
            sapp5(
                sconst("List.recProp"),
                nat.clone(),
                go_eq_motive.clone(),
                slam("acc", nat.clone(), refl(nat.clone(), svar("acc"))),
                slam(
                    "x",
                    nat.clone(),
                    slam(
                        "xs",
                        list(nat.clone()),
                        slam(
                            "ih",
                            spi("acc", nat.clone(), go_eq_prop(svar("xs"), svar("acc"))),
                            slam(
                                "acc",
                                nat.clone(),
                                sapp6(
                                    sconst("Eq.trans"),
                                    nat.clone(),
                                    sapp(
                                        sapp(sconst("sum_fast_go"), svar("xs")),
                                        nat_add(svar("x"), svar("acc")),
                                    ),
                                    nat_add(
                                        sapp(sconst("sum_ref"), svar("xs")),
                                        nat_add(svar("x"), svar("acc")),
                                    ),
                                    nat_add(
                                        nat_add(sapp(sconst("sum_ref"), svar("xs")), svar("x")),
                                        svar("acc"),
                                    ),
                                    sapp(svar("ih"), nat_add(svar("x"), svar("acc"))),
                                    sapp4(
                                        sconst("Eq.symm"),
                                        nat.clone(),
                                        nat_add(
                                            nat_add(sapp(sconst("sum_ref"), svar("xs")), svar("x")),
                                            svar("acc"),
                                        ),
                                        nat_add(
                                            sapp(sconst("sum_ref"), svar("xs")),
                                            nat_add(svar("x"), svar("acc")),
                                        ),
                                        sapp3(
                                            sconst("add_assoc"),
                                            sapp(sconst("sum_ref"), svar("xs")),
                                            svar("x"),
                                            svar("acc"),
                                        ),
                                    ),
                                ),
                            ),
                        ),
                    ),
                ),
                svar("xs"),
            ),
        );
        env.add_def("go_eq", go_eq_ty, go_eq_body, false)?;

        // sum_fast_eq : ∀ xs, sum_fast xs = sum_ref xs
        let sum_fast_eq_ty = spi(
            "xs",
            list(nat.clone()),
            eq(
                nat.clone(),
                sapp(sconst("sum_fast"), svar("xs")),
                sapp(sconst("sum_ref"), svar("xs")),
            ),
        );
        let sum_fast_eq_body = slam(
            "xs",
            list(nat.clone()),
            sapp6(
                sconst("Eq.trans"),
                nat.clone(),
                sapp(sconst("sum_fast"), svar("xs")),
                nat_add(sapp(sconst("sum_ref"), svar("xs")), sconst("Nat.zero")),
                sapp(sconst("sum_ref"), svar("xs")),
                sapp2(sconst("go_eq"), svar("xs"), sconst("Nat.zero")),
                sapp(sconst("add_zero"), sapp(sconst("sum_ref"), svar("xs"))),
            ),
        );
        env.add_def("sum_fast_eq", sum_fast_eq_ty, sum_fast_eq_body, false)?;

        Ok(env)
    }

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

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let env = Env::default();
        write!(f, "{}", env.pretty(self))
    }
}

// ---------------------------------------------------------------------------
// Demo helpers and tests
// ---------------------------------------------------------------------------

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

/// Number of AST nodes, useful as a crude proof-size metric.
pub fn term_size(t: &Term) -> usize {
    match t {
        Term::Var(_) | Term::Sort(_) | Term::Const(_) | Term::Nat(_) => 1,
        Term::App(f, a) => 1 + term_size(f) + term_size(a),
        Term::Lam(_, a, b) | Term::Pi(_, a, b) => 1 + term_size(a) + term_size(b),
        Term::Let(_, a, v, b) => 1 + term_size(a) + term_size(v) + term_size(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_builds() {
        Env::new().expect("kernel environment should build");
    }

    #[test]
    fn theorem_checks() {
        let env = Env::new().unwrap();
        env.check_named("sum_fast_eq").unwrap();
    }

    #[test]
    fn both_sums_compute_the_same() {
        let env = Env::new().unwrap();
        let list = env.list_of(&[1, 2, 3, 4, 5]);
        let a = env.apply("sum_ref", std::slice::from_ref(&list));
        let b = env.apply("sum_fast", std::slice::from_ref(&list));
        assert_eq!(env.eval_nat(&a), Some(15));
        assert_eq!(env.eval_nat(&b), Some(15));
    }

    #[test]
    fn bogus_proof_is_rejected() {
        let mut env = Env::new().unwrap();
        let nat = sconst("Nat");
        let list = sapp(sconst("List"), nat.clone());
        let eq = |a: STerm, b: STerm| sapp3(sconst("Eq"), nat.clone(), a, b);
        let sum_ref = |xs: STerm| sapp(sconst("sum_ref"), xs);
        let sum_fast = |xs: STerm| sapp(sconst("sum_fast"), xs);
        let ty = spi(
            "xs",
            list.clone(),
            eq(sum_ref(svar("xs")), sum_fast(svar("xs"))),
        );
        let body = slam(
            "xs",
            list,
            sapp2(sconst("Eq.refl"), nat.clone(), sum_ref(svar("xs"))),
        );
        assert!(env.add_def("bogus", ty, body, false).is_err());
    }
}
