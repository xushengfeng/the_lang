use std::collections::HashMap;

use super::debruijn::apply_many;
use super::term::*;

#[derive(Default)]
pub struct Env {
    pub types: HashMap<String, Term>,
    pub defs: HashMap<String, Def>,
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
