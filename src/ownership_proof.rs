//! Minimal formal model of unique ownership and in-place update.
//!
//! The model is deliberately tiny:
//!
//! * two locations, represented by `Bool.true` / `Bool.false`;
//! * a heap maps each location to an "array id" (`Bool`);
//! * a store maps each array id to an array (`List Nat`);
//! * `Unique(h)` means the two locations do not alias (different ids);
//! * `set_pure` returns the updated array and leaves the store unchanged;
//! * `set_in_place` updates the store at the first location.
//!
//! The checked theorems are:
//!
//! * `set_result_same`: both versions return the same array;
//! * `set_frame`: under `Unique(h)`, the second location observes the old
//!   array after an in-place update of the first.
//!
//! This is not the full heap model. It is the smallest model that makes the
//! aliasing/frame condition non-trivial and kernel-checkable.

use crate::dependent::{
    sapp, sapp2, sapp3, sapp4, sapp5, sapp6, sconst, slam, spi, svar, Env, STerm,
};

fn nat() -> STerm {
    sconst("Nat")
}

fn bool_ty() -> STerm {
    sconst("Bool")
}

fn btrue() -> STerm {
    sconst("Bool.true")
}

fn bfalse() -> STerm {
    sconst("Bool.false")
}

fn array_ty() -> STerm {
    sapp(sconst("List"), nat())
}

fn nil_array() -> STerm {
    sapp(sconst("List.nil"), nat())
}

fn cons_array(x: STerm, xs: STerm) -> STerm {
    sapp3(sconst("List.cons"), nat(), x, xs)
}

fn eq(ty: STerm, a: STerm, b: STerm) -> STerm {
    sapp3(sconst("Eq"), ty, a, b)
}

fn refl(ty: STerm, a: STerm) -> STerm {
    sapp2(sconst("Eq.refl"), ty, a)
}

fn neq(a: STerm, b: STerm) -> STerm {
    sapp2(sconst("NeqBool"), a, b)
}

fn store_ty() -> STerm {
    spi("_", bool_ty(), array_ty())
}

fn heap_ty() -> STerm {
    spi("_", bool_ty(), bool_ty())
}

fn update_store(s: STerm, b: STerm, v: STerm) -> STerm {
    sapp3(sconst("update_store"), s, b, v)
}

fn update_store_at(s: STerm, b: STerm, v: STerm, at: STerm) -> STerm {
    sapp(update_store(s, b, v), at)
}

fn arr_set(xs: STerm, i: STerm, v: STerm) -> STerm {
    sapp3(sconst("arr_set"), xs, i, v)
}

fn obs(s: STerm, h: STerm, l: STerm) -> STerm {
    sapp3(sconst("obs"), s, h, l)
}

fn set_in_place(h: STerm, s: STerm, i: STerm, v: STerm) -> STerm {
    sapp4(sconst("set_in_place"), h, s, i, v)
}

fn set_pure(h: STerm, s: STerm, i: STerm, v: STerm) -> STerm {
    sapp4(sconst("set_pure"), h, s, i, v)
}

fn unique(h: STerm) -> STerm {
    sapp(sconst("Unique"), h)
}

pub fn build() -> Result<Env, String> {
    let mut env = Env::new()?;

    // Logic helpers.
    env.add_def(
        "False",
        STerm::Sort(0),
        spi("P", STerm::Sort(0), svar("P")),
        true,
    )?;
    env.add_def(
        "True",
        STerm::Sort(0),
        spi("P", STerm::Sort(0), spi("p", svar("P"), svar("P"))),
        true,
    )?;
    env.add_def(
        "Not",
        spi("P", STerm::Sort(0), STerm::Sort(0)),
        slam("P", STerm::Sort(0), spi("_", svar("P"), sconst("False"))),
        true,
    )?;
    env.add_def(
        "NeqBool",
        spi("a", bool_ty(), spi("b", bool_ty(), STerm::Sort(0))),
        slam(
            "a",
            bool_ty(),
            slam(
                "b",
                bool_ty(),
                sapp(sconst("Not"), eq(bool_ty(), svar("a"), svar("b"))),
            ),
        ),
        true,
    )?;
    env.add_def(
        "false_elim",
        spi("P", STerm::Sort(0), spi("f", sconst("False"), svar("P"))),
        slam(
            "P",
            STerm::Sort(0),
            slam("f", sconst("False"), sapp(svar("f"), svar("P"))),
        ),
        true,
    )?;
    env.add_def(
        "true_intro",
        sconst("True"),
        slam("P", STerm::Sort(0), slam("p", svar("P"), svar("p"))),
        true,
    )?;
    env.add_def(
        "neq_self_false",
        spi(
            "a",
            bool_ty(),
            spi("h", neq(svar("a"), svar("a")), sconst("False")),
        ),
        slam(
            "a",
            bool_ty(),
            slam(
                "h",
                neq(svar("a"), svar("a")),
                sapp(svar("h"), refl(bool_ty(), svar("a"))),
            ),
        ),
        true,
    )?;

    // Array update: arr_set xs i v.
    let nat_rec = |motive: STerm, base: STerm, step: STerm, n: STerm| {
        sapp4(sconst("Nat.rec"), motive, base, step, n)
    };
    let list_rec = |motive: STerm, nil_case: STerm, cons_case: STerm, xs: STerm| {
        sapp5(sconst("List.rec"), nat(), motive, nil_case, cons_case, xs)
    };
    let arr_set_body = slam(
        "xs",
        array_ty(),
        list_rec(
            slam(
                "_",
                array_ty(),
                spi("_", nat(), spi("_", nat(), array_ty())),
            ),
            slam("i", nat(), slam("v", nat(), nil_array())),
            slam(
                "x",
                nat(),
                slam(
                    "xs",
                    array_ty(),
                    slam(
                        "ih",
                        spi("_", nat(), spi("_", nat(), array_ty())),
                        slam(
                            "i",
                            nat(),
                            slam(
                                "v",
                                nat(),
                                nat_rec(
                                    slam("_", nat(), array_ty()),
                                    cons_array(svar("v"), svar("xs")),
                                    slam(
                                        "n",
                                        nat(),
                                        slam(
                                            "_",
                                            array_ty(),
                                            cons_array(
                                                svar("x"),
                                                sapp2(svar("ih"), svar("n"), svar("v")),
                                            ),
                                        ),
                                    ),
                                    svar("i"),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
            svar("xs"),
        ),
    );
    env.add_def(
        "arr_set",
        spi(
            "xs",
            array_ty(),
            spi("i", nat(), spi("v", nat(), array_ty())),
        ),
        arr_set_body,
        true,
    )?;

    // Store update: update_store s b v = λ x. if x=b then v else s x.
    let bool_rec =
        |motive: STerm, t: STerm, f: STerm, b: STerm| sapp4(sconst("Bool.rec"), motive, t, f, b);
    let update_store_body = slam(
        "s",
        store_ty(),
        slam(
            "b",
            bool_ty(),
            slam(
                "v",
                array_ty(),
                bool_rec(
                    slam("_", bool_ty(), store_ty()),
                    slam(
                        "x",
                        bool_ty(),
                        bool_rec(
                            slam("_", bool_ty(), array_ty()),
                            svar("v"),
                            sapp(svar("s"), bfalse()),
                            svar("x"),
                        ),
                    ),
                    slam(
                        "x",
                        bool_ty(),
                        bool_rec(
                            slam("_", bool_ty(), array_ty()),
                            sapp(svar("s"), btrue()),
                            svar("v"),
                            svar("x"),
                        ),
                    ),
                    svar("b"),
                ),
            ),
        ),
    );
    env.add_def(
        "update_store",
        spi(
            "s",
            store_ty(),
            spi("b", bool_ty(), spi("v", array_ty(), store_ty())),
        ),
        update_store_body,
        true,
    )?;

    // obs s h l = s (h l)
    env.add_def(
        "obs",
        spi(
            "s",
            store_ty(),
            spi("h", heap_ty(), spi("l", bool_ty(), array_ty())),
        ),
        slam(
            "s",
            store_ty(),
            slam(
                "h",
                heap_ty(),
                slam("l", bool_ty(), sapp(svar("s"), sapp(svar("h"), svar("l")))),
            ),
        ),
        true,
    )?;

    // set_in_place h s i v = update_store s (h true) (arr_set (s (h true)) i v)
    let h0 = |h: STerm| sapp(h, btrue());
    let h1 = |h: STerm| sapp(h, bfalse());
    env.add_def(
        "set_in_place",
        spi(
            "h",
            heap_ty(),
            spi(
                "s",
                store_ty(),
                spi("i", nat(), spi("v", nat(), store_ty())),
            ),
        ),
        slam(
            "h",
            heap_ty(),
            slam(
                "s",
                store_ty(),
                slam(
                    "i",
                    nat(),
                    slam(
                        "v",
                        nat(),
                        update_store(
                            svar("s"),
                            h0(svar("h")),
                            arr_set(sapp(svar("s"), h0(svar("h"))), svar("i"), svar("v")),
                        ),
                    ),
                ),
            ),
        ),
        true,
    )?;

    // set_pure h s i v = arr_set (s (h true)) i v
    env.add_def(
        "set_pure",
        spi(
            "h",
            heap_ty(),
            spi(
                "s",
                store_ty(),
                spi("i", nat(), spi("v", nat(), array_ty())),
            ),
        ),
        slam(
            "h",
            heap_ty(),
            slam(
                "s",
                store_ty(),
                slam(
                    "i",
                    nat(),
                    slam(
                        "v",
                        nat(),
                        arr_set(sapp(svar("s"), h0(svar("h"))), svar("i"), svar("v")),
                    ),
                ),
            ),
        ),
        true,
    )?;

    // Unique h = NeqBool (h true) (h false)
    env.add_def(
        "Unique",
        spi("h", heap_ty(), STerm::Sort(0)),
        slam("h", heap_ty(), neq(h0(svar("h")), h1(svar("h")))),
        true,
    )?;

    // update_store_same : ∀ s v b, update_store s b v b = v
    let same_motive = slam(
        "b",
        bool_ty(),
        eq(
            array_ty(),
            update_store_at(svar("s"), svar("b"), svar("v"), svar("b")),
            svar("v"),
        ),
    );
    env.add_def(
        "update_store_same",
        spi(
            "s",
            store_ty(),
            spi(
                "v",
                array_ty(),
                spi(
                    "b",
                    bool_ty(),
                    eq(
                        array_ty(),
                        update_store_at(svar("s"), svar("b"), svar("v"), svar("b")),
                        svar("v"),
                    ),
                ),
            ),
        ),
        slam(
            "s",
            store_ty(),
            slam(
                "v",
                array_ty(),
                slam(
                    "b",
                    bool_ty(),
                    sapp4(
                        sconst("Bool.recProp"),
                        same_motive,
                        refl(array_ty(), svar("v")),
                        refl(array_ty(), svar("v")),
                        svar("b"),
                    ),
                ),
            ),
        ),
        false,
    )?;

    // update_store_other :
    //   ∀ a b s v, a ≠ b -> update_store s a v b = s b
    let other_ty = spi(
        "a",
        bool_ty(),
        spi(
            "b",
            bool_ty(),
            spi(
                "s",
                store_ty(),
                spi(
                    "v",
                    array_ty(),
                    spi(
                        "h",
                        neq(svar("a"), svar("b")),
                        eq(
                            array_ty(),
                            update_store_at(svar("s"), svar("a"), svar("v"), svar("b")),
                            sapp(svar("s"), svar("b")),
                        ),
                    ),
                ),
            ),
        ),
    );

    let motive_a = slam(
        "a",
        bool_ty(),
        spi(
            "b",
            bool_ty(),
            spi(
                "s",
                store_ty(),
                spi(
                    "v",
                    array_ty(),
                    spi(
                        "h",
                        neq(svar("a"), svar("b")),
                        eq(
                            array_ty(),
                            update_store_at(svar("s"), svar("a"), svar("v"), svar("b")),
                            sapp(svar("s"), svar("b")),
                        ),
                    ),
                ),
            ),
        ),
    );

    let motive_b_true = slam(
        "b",
        bool_ty(),
        spi(
            "s",
            store_ty(),
            spi(
                "v",
                array_ty(),
                spi(
                    "h",
                    neq(btrue(), svar("b")),
                    eq(
                        array_ty(),
                        update_store_at(svar("s"), btrue(), svar("v"), svar("b")),
                        sapp(svar("s"), svar("b")),
                    ),
                ),
            ),
        ),
    );
    let goal_true_true = eq(
        array_ty(),
        update_store_at(svar("s"), btrue(), svar("v"), btrue()),
        sapp(svar("s"), btrue()),
    );
    let case_true_true = slam(
        "s",
        store_ty(),
        slam(
            "v",
            array_ty(),
            slam(
                "h",
                neq(btrue(), btrue()),
                sapp2(
                    sconst("false_elim"),
                    goal_true_true,
                    sapp(svar("h"), refl(bool_ty(), btrue())),
                ),
            ),
        ),
    );
    let case_true_false = slam(
        "s",
        store_ty(),
        slam(
            "v",
            array_ty(),
            slam(
                "h",
                neq(btrue(), bfalse()),
                refl(array_ty(), sapp(svar("s"), bfalse())),
            ),
        ),
    );
    let case_true = slam(
        "b",
        bool_ty(),
        sapp4(
            sconst("Bool.recProp"),
            motive_b_true,
            case_true_true,
            case_true_false,
            svar("b"),
        ),
    );

    let motive_b_false = slam(
        "b",
        bool_ty(),
        spi(
            "s",
            store_ty(),
            spi(
                "v",
                array_ty(),
                spi(
                    "h",
                    neq(bfalse(), svar("b")),
                    eq(
                        array_ty(),
                        update_store_at(svar("s"), bfalse(), svar("v"), svar("b")),
                        sapp(svar("s"), svar("b")),
                    ),
                ),
            ),
        ),
    );
    let case_false_true = slam(
        "s",
        store_ty(),
        slam(
            "v",
            array_ty(),
            slam(
                "h",
                neq(bfalse(), btrue()),
                refl(array_ty(), sapp(svar("s"), btrue())),
            ),
        ),
    );
    let goal_false_false = eq(
        array_ty(),
        update_store_at(svar("s"), bfalse(), svar("v"), bfalse()),
        sapp(svar("s"), bfalse()),
    );
    let case_false_false = slam(
        "s",
        store_ty(),
        slam(
            "v",
            array_ty(),
            slam(
                "h",
                neq(bfalse(), bfalse()),
                sapp2(
                    sconst("false_elim"),
                    goal_false_false,
                    sapp(svar("h"), refl(bool_ty(), bfalse())),
                ),
            ),
        ),
    );
    let case_false = slam(
        "b",
        bool_ty(),
        sapp4(
            sconst("Bool.recProp"),
            motive_b_false,
            case_false_true,
            case_false_false,
            svar("b"),
        ),
    );

    env.add_def(
        "update_store_other",
        other_ty,
        slam(
            "a",
            bool_ty(),
            sapp4(
                sconst("Bool.recProp"),
                motive_a,
                case_true,
                case_false,
                svar("a"),
            ),
        ),
        false,
    )?;

    // set_result_same : obs (set_in_place h s i v) h true = set_pure h s i v
    let arr = arr_set(sapp(svar("s"), h0(svar("h"))), svar("i"), svar("v"));
    env.add_def(
        "set_result_same",
        spi(
            "h",
            heap_ty(),
            spi(
                "s",
                store_ty(),
                spi(
                    "i",
                    nat(),
                    spi(
                        "v",
                        nat(),
                        eq(
                            array_ty(),
                            obs(
                                set_in_place(svar("h"), svar("s"), svar("i"), svar("v")),
                                svar("h"),
                                btrue(),
                            ),
                            set_pure(svar("h"), svar("s"), svar("i"), svar("v")),
                        ),
                    ),
                ),
            ),
        ),
        slam(
            "h",
            heap_ty(),
            slam(
                "s",
                store_ty(),
                slam(
                    "i",
                    nat(),
                    slam(
                        "v",
                        nat(),
                        sapp3(sconst("update_store_same"), svar("s"), arr, h0(svar("h"))),
                    ),
                ),
            ),
        ),
        false,
    )?;

    // set_frame : Unique h -> obs (set_in_place h s i v) h false = obs s h false
    let arr = arr_set(sapp(svar("s"), h0(svar("h"))), svar("i"), svar("v"));
    env.add_def(
        "set_frame",
        spi(
            "h",
            heap_ty(),
            spi(
                "s",
                store_ty(),
                spi(
                    "i",
                    nat(),
                    spi(
                        "v",
                        nat(),
                        spi(
                            "u",
                            unique(svar("h")),
                            eq(
                                array_ty(),
                                obs(
                                    set_in_place(svar("h"), svar("s"), svar("i"), svar("v")),
                                    svar("h"),
                                    bfalse(),
                                ),
                                obs(svar("s"), svar("h"), bfalse()),
                            ),
                        ),
                    ),
                ),
            ),
        ),
        slam(
            "h",
            heap_ty(),
            slam(
                "s",
                store_ty(),
                slam(
                    "i",
                    nat(),
                    slam(
                        "v",
                        nat(),
                        slam(
                            "u",
                            unique(svar("h")),
                            sapp5(
                                sconst("update_store_other"),
                                h0(svar("h")),
                                h1(svar("h")),
                                svar("s"),
                                arr,
                                svar("u"),
                            ),
                        ),
                    ),
                ),
            ),
        ),
        false,
    )?;

    Ok(env)
}

// ---------------------------------------------------------------------------
// Generalization: Nat-indexed locations and array ids
// ---------------------------------------------------------------------------

fn nat_succ(a: STerm) -> STerm {
    sapp(sconst("Nat.succ"), a)
}

fn nat_zero() -> STerm {
    sconst("Nat.zero")
}

fn neq_nat(a: STerm, b: STerm) -> STerm {
    sapp2(sconst("NeqNat"), a, b)
}

fn store_nat_ty() -> STerm {
    spi("_", nat(), array_ty())
}

fn heap_nat_ty() -> STerm {
    spi("_", nat(), nat())
}

fn shift_store(s: STerm) -> STerm {
    sapp(sconst("shift_store"), s)
}

fn update_at(a: STerm, v: STerm, s: STerm, b: STerm) -> STerm {
    sapp4(sconst("update_at"), a, v, s, b)
}

fn unique_nat(h: STerm) -> STerm {
    sapp(sconst("UniqueNat"), h)
}

fn get_store(s: STerm, h: STerm, l: STerm) -> STerm {
    sapp3(sconst("get_store"), s, h, l)
}

fn set_in_place_at(h: STerm, s: STerm, l: STerm, i: STerm, v: STerm) -> STerm {
    sapp5(sconst("set_in_place_at"), h, s, l, i, v)
}

fn set_pure_at(h: STerm, s: STerm, l: STerm, i: STerm, v: STerm) -> STerm {
    sapp5(sconst("set_pure_at"), h, s, l, i, v)
}

/// Build the Nat-indexed model on top of `build()`.
///
/// `Store = Nat -> Array`, `Heap = Nat -> Nat`.
/// `update_at a v s` is the function that updates the store `s` at id `a`
/// with `v`; it shifts the store when recursing, so the frame lemma is a
/// genuine induction on the id, not a finite case split.
pub fn build_nat() -> Result<Env, String> {
    let mut env = build()?;

    // NeqNat a b = Not (Eq Nat a b)
    env.add_def(
        "NeqNat",
        spi("a", nat(), spi("b", nat(), STerm::Sort(0))),
        slam(
            "a",
            nat(),
            slam(
                "b",
                nat(),
                sapp(sconst("Not"), eq(nat(), svar("a"), svar("b"))),
            ),
        ),
        true,
    )?;

    // neq_sym : a ≠ b -> b ≠ a
    env.add_def(
        "neq_sym",
        spi(
            "a",
            nat(),
            spi(
                "b",
                nat(),
                spi(
                    "h",
                    neq_nat(svar("a"), svar("b")),
                    neq_nat(svar("b"), svar("a")),
                ),
            ),
        ),
        slam(
            "a",
            nat(),
            slam(
                "b",
                nat(),
                slam(
                    "h",
                    neq_nat(svar("a"), svar("b")),
                    slam(
                        "h_eq",
                        eq(nat(), svar("b"), svar("a")),
                        sapp(
                            svar("h"),
                            sapp4(sconst("Eq.symm"), nat(), svar("b"), svar("a"), svar("h_eq")),
                        ),
                    ),
                ),
            ),
        ),
        false,
    )?;

    // neq_succ_succ : succ a ≠ succ b -> a ≠ b
    env.add_def(
        "neq_succ_succ",
        spi(
            "a",
            nat(),
            spi(
                "b",
                nat(),
                spi(
                    "h",
                    neq_nat(nat_succ(svar("a")), nat_succ(svar("b"))),
                    neq_nat(svar("a"), svar("b")),
                ),
            ),
        ),
        slam(
            "a",
            nat(),
            slam(
                "b",
                nat(),
                slam(
                    "h",
                    neq_nat(nat_succ(svar("a")), nat_succ(svar("b"))),
                    slam(
                        "h_eq",
                        eq(nat(), svar("a"), svar("b")),
                        sapp(
                            svar("h"),
                            sapp3(sconst("congr_succ"), svar("a"), svar("b"), svar("h_eq")),
                        ),
                    ),
                ),
            ),
        ),
        false,
    )?;

    // shift_store s = λ x. s (succ x)
    env.add_def(
        "shift_store",
        spi("s", store_nat_ty(), store_nat_ty()),
        slam(
            "s",
            store_nat_ty(),
            slam("x", nat(), sapp(svar("s"), nat_succ(svar("x")))),
        ),
        true,
    )?;

    // update_at : Nat -> Array -> Store -> Nat -> Array
    let update_at_ty = spi(
        "a",
        nat(),
        spi(
            "v",
            array_ty(),
            spi("s", store_nat_ty(), spi("b", nat(), array_ty())),
        ),
    );
    let nat_rec = |motive: STerm, base: STerm, step: STerm, n: STerm| {
        sapp4(sconst("Nat.rec"), motive, base, step, n)
    };
    let update_at_body = slam(
        "a",
        nat(),
        slam(
            "v",
            array_ty(),
            slam(
                "s",
                store_nat_ty(),
                slam(
                    "b",
                    nat(),
                    sapp3(
                        nat_rec(
                            slam(
                                "_",
                                nat(),
                                spi(
                                    "_",
                                    array_ty(),
                                    spi("_", store_nat_ty(), spi("_", nat(), array_ty())),
                                ),
                            ),
                            // a = 0
                            slam(
                                "v",
                                array_ty(),
                                slam(
                                    "s",
                                    store_nat_ty(),
                                    slam(
                                        "b",
                                        nat(),
                                        nat_rec(
                                            slam("_", nat(), array_ty()),
                                            svar("v"),
                                            slam(
                                                "m",
                                                nat(),
                                                slam(
                                                    "_",
                                                    array_ty(),
                                                    sapp(svar("s"), nat_succ(svar("m"))),
                                                ),
                                            ),
                                            svar("b"),
                                        ),
                                    ),
                                ),
                            ),
                            // a = succ a'
                            slam(
                                "a'",
                                nat(),
                                slam(
                                    "ih",
                                    spi(
                                        "_",
                                        array_ty(),
                                        spi("_", store_nat_ty(), spi("_", nat(), array_ty())),
                                    ),
                                    slam(
                                        "v",
                                        array_ty(),
                                        slam(
                                            "s",
                                            store_nat_ty(),
                                            slam(
                                                "b",
                                                nat(),
                                                nat_rec(
                                                    slam("_", nat(), array_ty()),
                                                    sapp(svar("s"), nat_zero()),
                                                    slam(
                                                        "m",
                                                        nat(),
                                                        slam(
                                                            "_",
                                                            array_ty(),
                                                            sapp3(
                                                                svar("ih"),
                                                                svar("v"),
                                                                shift_store(svar("s")),
                                                                svar("m"),
                                                            ),
                                                        ),
                                                    ),
                                                    svar("b"),
                                                ),
                                            ),
                                        ),
                                    ),
                                ),
                            ),
                            svar("a"),
                        ),
                        svar("v"),
                        svar("s"),
                        svar("b"),
                    ),
                ),
            ),
        ),
    );
    env.add_def("update_at", update_at_ty, update_at_body, true)?;

    // update_at_same : ∀ a v s, update_at a v s a = v
    let same_motive = slam(
        "a",
        nat(),
        spi(
            "v",
            array_ty(),
            spi(
                "s",
                store_nat_ty(),
                eq(
                    array_ty(),
                    update_at(svar("a"), svar("v"), svar("s"), svar("a")),
                    svar("v"),
                ),
            ),
        ),
    );
    env.add_def(
        "update_at_same",
        spi(
            "a",
            nat(),
            spi(
                "v",
                array_ty(),
                spi(
                    "s",
                    store_nat_ty(),
                    eq(
                        array_ty(),
                        update_at(svar("a"), svar("v"), svar("s"), svar("a")),
                        svar("v"),
                    ),
                ),
            ),
        ),
        slam(
            "a",
            nat(),
            sapp4(
                sconst("Nat.recProp"),
                same_motive,
                slam(
                    "v",
                    array_ty(),
                    slam("s", store_nat_ty(), refl(array_ty(), svar("v"))),
                ),
                slam(
                    "a'",
                    nat(),
                    slam(
                        "ih",
                        spi(
                            "v",
                            array_ty(),
                            spi(
                                "s",
                                store_nat_ty(),
                                eq(
                                    array_ty(),
                                    update_at(svar("a'"), svar("v"), svar("s"), svar("a'")),
                                    svar("v"),
                                ),
                            ),
                        ),
                        slam(
                            "v",
                            array_ty(),
                            slam(
                                "s",
                                store_nat_ty(),
                                sapp2(svar("ih"), svar("v"), shift_store(svar("s"))),
                            ),
                        ),
                    ),
                ),
                svar("a"),
            ),
        ),
        false,
    )?;

    // update_at_other :
    //   ∀ a b v s, a ≠ b -> update_at a v s b = s b
    let other_ty = spi(
        "a",
        nat(),
        spi(
            "b",
            nat(),
            spi(
                "v",
                array_ty(),
                spi(
                    "s",
                    store_nat_ty(),
                    spi(
                        "h",
                        neq_nat(svar("a"), svar("b")),
                        eq(
                            array_ty(),
                            update_at(svar("a"), svar("v"), svar("s"), svar("b")),
                            sapp(svar("s"), svar("b")),
                        ),
                    ),
                ),
            ),
        ),
    );
    let motive_a = slam(
        "a",
        nat(),
        spi(
            "b",
            nat(),
            spi(
                "v",
                array_ty(),
                spi(
                    "s",
                    store_nat_ty(),
                    spi(
                        "h",
                        neq_nat(svar("a"), svar("b")),
                        eq(
                            array_ty(),
                            update_at(svar("a"), svar("v"), svar("s"), svar("b")),
                            sapp(svar("s"), svar("b")),
                        ),
                    ),
                ),
            ),
        ),
    );

    // a = 0
    let motive_b_zero = slam(
        "b",
        nat(),
        spi(
            "v",
            array_ty(),
            spi(
                "s",
                store_nat_ty(),
                spi(
                    "h",
                    neq_nat(nat_zero(), svar("b")),
                    eq(
                        array_ty(),
                        update_at(nat_zero(), svar("v"), svar("s"), svar("b")),
                        sapp(svar("s"), svar("b")),
                    ),
                ),
            ),
        ),
    );
    let goal_zero_zero = eq(
        array_ty(),
        update_at(nat_zero(), svar("v"), svar("s"), nat_zero()),
        sapp(svar("s"), nat_zero()),
    );
    let base_b_zero = slam(
        "v",
        array_ty(),
        slam(
            "s",
            store_nat_ty(),
            slam(
                "h",
                neq_nat(nat_zero(), nat_zero()),
                sapp2(
                    sconst("false_elim"),
                    goal_zero_zero,
                    sapp(svar("h"), refl(nat(), nat_zero())),
                ),
            ),
        ),
    );
    let step_b_zero = slam(
        "m",
        nat(),
        slam(
            "ih",
            spi(
                "v",
                array_ty(),
                spi(
                    "s",
                    store_nat_ty(),
                    spi(
                        "h",
                        neq_nat(nat_zero(), svar("m")),
                        eq(
                            array_ty(),
                            update_at(nat_zero(), svar("v"), svar("s"), svar("m")),
                            sapp(svar("s"), svar("m")),
                        ),
                    ),
                ),
            ),
            slam(
                "v",
                array_ty(),
                slam(
                    "s",
                    store_nat_ty(),
                    slam(
                        "h",
                        neq_nat(nat_zero(), nat_succ(svar("m"))),
                        refl(array_ty(), sapp(svar("s"), nat_succ(svar("m")))),
                    ),
                ),
            ),
        ),
    );
    let case_zero = slam(
        "b",
        nat(),
        sapp4(
            sconst("Nat.recProp"),
            motive_b_zero,
            base_b_zero,
            step_b_zero,
            svar("b"),
        ),
    );

    // a = succ a'
    let motive_b_succ = slam(
        "b",
        nat(),
        spi(
            "v",
            array_ty(),
            spi(
                "s",
                store_nat_ty(),
                spi(
                    "h",
                    neq_nat(nat_succ(svar("a'")), svar("b")),
                    eq(
                        array_ty(),
                        update_at(nat_succ(svar("a'")), svar("v"), svar("s"), svar("b")),
                        sapp(svar("s"), svar("b")),
                    ),
                ),
            ),
        ),
    );
    let base_b_succ = slam(
        "v",
        array_ty(),
        slam(
            "s",
            store_nat_ty(),
            slam(
                "h",
                neq_nat(nat_succ(svar("a'")), nat_zero()),
                refl(array_ty(), sapp(svar("s"), nat_zero())),
            ),
        ),
    );
    let step_b_succ = slam(
        "m",
        nat(),
        slam(
            "ih_b",
            spi(
                "v",
                array_ty(),
                spi(
                    "s",
                    store_nat_ty(),
                    spi(
                        "h",
                        neq_nat(nat_succ(svar("a'")), svar("m")),
                        eq(
                            array_ty(),
                            update_at(nat_succ(svar("a'")), svar("v"), svar("s"), svar("m")),
                            sapp(svar("s"), svar("m")),
                        ),
                    ),
                ),
            ),
            slam(
                "v",
                array_ty(),
                slam(
                    "s",
                    store_nat_ty(),
                    slam(
                        "h",
                        neq_nat(nat_succ(svar("a'")), nat_succ(svar("m"))),
                        sapp4(
                            svar("ih"),
                            svar("m"),
                            svar("v"),
                            shift_store(svar("s")),
                            sapp3(sconst("neq_succ_succ"), svar("a'"), svar("m"), svar("h")),
                        ),
                    ),
                ),
            ),
        ),
    );
    let case_succ = slam(
        "a'",
        nat(),
        slam(
            "ih",
            spi(
                "b",
                nat(),
                spi(
                    "v",
                    array_ty(),
                    spi(
                        "s",
                        store_nat_ty(),
                        spi(
                            "h",
                            neq_nat(svar("a'"), svar("b")),
                            eq(
                                array_ty(),
                                update_at(svar("a'"), svar("v"), svar("s"), svar("b")),
                                sapp(svar("s"), svar("b")),
                            ),
                        ),
                    ),
                ),
            ),
            slam(
                "b",
                nat(),
                sapp4(
                    sconst("Nat.recProp"),
                    motive_b_succ,
                    base_b_succ,
                    step_b_succ,
                    svar("b"),
                ),
            ),
        ),
    );
    env.add_def(
        "update_at_other",
        other_ty,
        slam(
            "a",
            nat(),
            sapp4(
                sconst("Nat.recProp"),
                motive_a,
                case_zero,
                case_succ,
                svar("a"),
            ),
        ),
        false,
    )?;

    // UniqueNat h = ∀ i j, i ≠ j -> h i ≠ h j
    env.add_def(
        "UniqueNat",
        spi("h", heap_nat_ty(), STerm::Sort(0)),
        slam(
            "h",
            heap_nat_ty(),
            spi(
                "i",
                nat(),
                spi(
                    "j",
                    nat(),
                    spi(
                        "hneq",
                        neq_nat(svar("i"), svar("j")),
                        neq_nat(sapp(svar("h"), svar("i")), sapp(svar("h"), svar("j"))),
                    ),
                ),
            ),
        ),
        true,
    )?;

    // get_store s h l = s (h l)
    env.add_def(
        "get_store",
        spi(
            "s",
            store_nat_ty(),
            spi("h", heap_nat_ty(), spi("l", nat(), array_ty())),
        ),
        slam(
            "s",
            store_nat_ty(),
            slam(
                "h",
                heap_nat_ty(),
                slam("l", nat(), sapp(svar("s"), sapp(svar("h"), svar("l")))),
            ),
        ),
        true,
    )?;

    // set_in_place_at h s l i v =
    //   update_at (h l) (arr_set (s (h l)) i v) s
    env.add_def(
        "set_in_place_at",
        spi(
            "h",
            heap_nat_ty(),
            spi(
                "s",
                store_nat_ty(),
                spi("l", nat(), spi("i", nat(), spi("v", nat(), store_nat_ty()))),
            ),
        ),
        slam(
            "h",
            heap_nat_ty(),
            slam(
                "s",
                store_nat_ty(),
                slam(
                    "l",
                    nat(),
                    slam(
                        "i",
                        nat(),
                        slam(
                            "v",
                            nat(),
                            sapp3(
                                sconst("update_at"),
                                sapp(svar("h"), svar("l")),
                                arr_set(
                                    sapp(svar("s"), sapp(svar("h"), svar("l"))),
                                    svar("i"),
                                    svar("v"),
                                ),
                                svar("s"),
                            ),
                        ),
                    ),
                ),
            ),
        ),
        true,
    )?;

    // set_pure_at h s l i v = arr_set (s (h l)) i v
    env.add_def(
        "set_pure_at",
        spi(
            "h",
            heap_nat_ty(),
            spi(
                "s",
                store_nat_ty(),
                spi("l", nat(), spi("i", nat(), spi("v", nat(), array_ty()))),
            ),
        ),
        slam(
            "h",
            heap_nat_ty(),
            slam(
                "s",
                store_nat_ty(),
                slam(
                    "l",
                    nat(),
                    slam(
                        "i",
                        nat(),
                        slam(
                            "v",
                            nat(),
                            arr_set(
                                sapp(svar("s"), sapp(svar("h"), svar("l"))),
                                svar("i"),
                                svar("v"),
                            ),
                        ),
                    ),
                ),
            ),
        ),
        true,
    )?;

    // set_result_same_nat :
    //   ∀ h s l i v,
    //     get_store (set_in_place_at h s l i v) h l = set_pure_at h s l i v
    let arr = arr_set(
        sapp(svar("s"), sapp(svar("h"), svar("l"))),
        svar("i"),
        svar("v"),
    );
    env.add_def(
        "set_result_same_nat",
        spi(
            "h",
            heap_nat_ty(),
            spi(
                "s",
                store_nat_ty(),
                spi(
                    "l",
                    nat(),
                    spi(
                        "i",
                        nat(),
                        spi(
                            "v",
                            nat(),
                            eq(
                                array_ty(),
                                get_store(
                                    set_in_place_at(
                                        svar("h"),
                                        svar("s"),
                                        svar("l"),
                                        svar("i"),
                                        svar("v"),
                                    ),
                                    svar("h"),
                                    svar("l"),
                                ),
                                set_pure_at(svar("h"), svar("s"), svar("l"), svar("i"), svar("v")),
                            ),
                        ),
                    ),
                ),
            ),
        ),
        slam(
            "h",
            heap_nat_ty(),
            slam(
                "s",
                store_nat_ty(),
                slam(
                    "l",
                    nat(),
                    slam(
                        "i",
                        nat(),
                        slam(
                            "v",
                            nat(),
                            sapp3(
                                sconst("update_at_same"),
                                sapp(svar("h"), svar("l")),
                                arr,
                                svar("s"),
                            ),
                        ),
                    ),
                ),
            ),
        ),
        false,
    )?;

    // set_frame_nat :
    //   ∀ h s l i v j,
    //     UniqueNat h -> j ≠ l ->
    //     get_store (set_in_place_at h s l i v) h j = get_store s h j
    let arr = arr_set(
        sapp(svar("s"), sapp(svar("h"), svar("l"))),
        svar("i"),
        svar("v"),
    );
    env.add_def(
        "set_frame_nat",
        spi(
            "h",
            heap_nat_ty(),
            spi(
                "s",
                store_nat_ty(),
                spi(
                    "l",
                    nat(),
                    spi(
                        "i",
                        nat(),
                        spi(
                            "v",
                            nat(),
                            spi(
                                "j",
                                nat(),
                                spi(
                                    "u",
                                    unique_nat(svar("h")),
                                    spi(
                                        "hneq",
                                        neq_nat(svar("j"), svar("l")),
                                        eq(
                                            array_ty(),
                                            get_store(
                                                set_in_place_at(
                                                    svar("h"),
                                                    svar("s"),
                                                    svar("l"),
                                                    svar("i"),
                                                    svar("v"),
                                                ),
                                                svar("h"),
                                                svar("j"),
                                            ),
                                            get_store(svar("s"), svar("h"), svar("j")),
                                        ),
                                    ),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
        ),
        slam(
            "h",
            heap_nat_ty(),
            slam(
                "s",
                store_nat_ty(),
                slam(
                    "l",
                    nat(),
                    slam(
                        "i",
                        nat(),
                        slam(
                            "v",
                            nat(),
                            slam(
                                "j",
                                nat(),
                                slam(
                                    "u",
                                    unique_nat(svar("h")),
                                    slam(
                                        "hneq",
                                        neq_nat(svar("j"), svar("l")),
                                        sapp5(
                                            sconst("update_at_other"),
                                            sapp(svar("h"), svar("l")),
                                            sapp(svar("h"), svar("j")),
                                            arr,
                                            svar("s"),
                                            sapp3(
                                                svar("u"),
                                                svar("l"),
                                                svar("j"),
                                                sapp3(
                                                    sconst("neq_sym"),
                                                    svar("j"),
                                                    svar("l"),
                                                    svar("hneq"),
                                                ),
                                            ),
                                        ),
                                    ),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
        ),
        false,
    )?;

    Ok(env)
}

// ---------------------------------------------------------------------------
// Borrow permissions
// ---------------------------------------------------------------------------

fn perm_ty() -> STerm {
    spi("_", nat(), nat())
}

fn perm_get(p: STerm, l: STerm) -> STerm {
    sapp(p, l)
}

fn free(p: STerm, l: STerm) -> STerm {
    eq(nat(), perm_get(p, l), nat_zero())
}

fn borrow(p: STerm, l: STerm) -> STerm {
    eq(nat(), perm_get(p, l), nat_succ(nat_zero()))
}

fn borrow_mut(p: STerm, l: STerm) -> STerm {
    eq(nat(), perm_get(p, l), nat_succ(nat_succ(nat_zero())))
}

fn can_read(p: STerm, l: STerm) -> STerm {
    neq_nat(perm_get(p, l), nat_zero())
}

fn can_write(p: STerm, l: STerm) -> STerm {
    borrow_mut(p, l)
}

/// Build the minimal borrow-permission model on top of `build_nat()`.
///
/// Permission codes:
///   0 = free, 1 = shared borrow, 2 = mutable borrow.
///
/// Checked:
///   borrow_not_mut, borrow_not_free, borrow_mut_not_free,
///   borrow_can_read, borrow_mut_can_read,
///   borrow_cannot_write, borrow_mut_can_write.
pub fn build_borrow() -> Result<Env, String> {
    let mut env = build_nat()?;

    let one = nat_succ(nat_zero());
    let two = nat_succ(one.clone());

    // Prop-level discriminator: P 0 = True, P (succ n) = False.
    let p_zero_succ = slam(
        "x",
        nat(),
        sapp4(
            sconst("Nat.rec"),
            slam("_", nat(), STerm::Sort(0)),
            sconst("True"),
            slam("_", nat(), slam("_", STerm::Sort(0), sconst("False"))),
            svar("x"),
        ),
    );
    // Prop-level discriminator: Q 0 = False, Q (succ n) = True.
    let q_succ_zero = slam(
        "x",
        nat(),
        sapp4(
            sconst("Nat.rec"),
            slam("_", nat(), STerm::Sort(0)),
            sconst("False"),
            slam("_", nat(), slam("_", STerm::Sort(0), sconst("True"))),
            svar("x"),
        ),
    );

    // neq_nat_zero_succ : 0 ≠ succ n
    env.add_def(
        "neq_nat_zero_succ",
        spi("n", nat(), neq_nat(nat_zero(), nat_succ(svar("n")))),
        slam(
            "n",
            nat(),
            slam(
                "h",
                eq(nat(), nat_zero(), nat_succ(svar("n"))),
                sapp6(
                    sconst("Eq.subst"),
                    nat(),
                    p_zero_succ,
                    nat_zero(),
                    nat_succ(svar("n")),
                    svar("h"),
                    sconst("true_intro"),
                ),
            ),
        ),
        false,
    )?;

    // neq_nat_succ_zero : succ n ≠ 0
    env.add_def(
        "neq_nat_succ_zero",
        spi("n", nat(), neq_nat(nat_succ(svar("n")), nat_zero())),
        slam(
            "n",
            nat(),
            slam(
                "h",
                eq(nat(), nat_succ(svar("n")), nat_zero()),
                sapp6(
                    sconst("Eq.subst"),
                    nat(),
                    q_succ_zero,
                    nat_succ(svar("n")),
                    nat_zero(),
                    svar("h"),
                    sconst("true_intro"),
                ),
            ),
        ),
        false,
    )?;

    // pred : Nat -> Nat
    env.add_def(
        "pred",
        spi("_", nat(), nat()),
        slam(
            "x",
            nat(),
            sapp4(
                sconst("Nat.rec"),
                slam("_", nat(), nat()),
                nat_zero(),
                slam("n", nat(), slam("_", nat(), svar("n"))),
                svar("x"),
            ),
        ),
        true,
    )?;

    // nat_succ_inj : succ a = succ b -> a = b
    env.add_def(
        "nat_succ_inj",
        spi(
            "a",
            nat(),
            spi(
                "b",
                nat(),
                spi(
                    "h",
                    eq(nat(), nat_succ(svar("a")), nat_succ(svar("b"))),
                    eq(nat(), svar("a"), svar("b")),
                ),
            ),
        ),
        slam(
            "a",
            nat(),
            slam(
                "b",
                nat(),
                slam(
                    "h",
                    eq(nat(), nat_succ(svar("a")), nat_succ(svar("b"))),
                    sapp6(
                        sconst("Eq.subst"),
                        nat(),
                        slam(
                            "x",
                            nat(),
                            eq(nat(), svar("a"), sapp(sconst("pred"), svar("x"))),
                        ),
                        nat_succ(svar("a")),
                        nat_succ(svar("b")),
                        svar("h"),
                        refl(nat(), svar("a")),
                    ),
                ),
            ),
        ),
        false,
    )?;

    // neq_succ_succ_forward : a ≠ b -> succ a ≠ succ b
    env.add_def(
        "neq_succ_succ_forward",
        spi(
            "a",
            nat(),
            spi(
                "b",
                nat(),
                spi(
                    "h",
                    neq_nat(svar("a"), svar("b")),
                    neq_nat(nat_succ(svar("a")), nat_succ(svar("b"))),
                ),
            ),
        ),
        slam(
            "a",
            nat(),
            slam(
                "b",
                nat(),
                slam(
                    "h",
                    neq_nat(svar("a"), svar("b")),
                    slam(
                        "h_eq",
                        eq(nat(), nat_succ(svar("a")), nat_succ(svar("b"))),
                        sapp(
                            svar("h"),
                            sapp3(sconst("nat_succ_inj"), svar("a"), svar("b"), svar("h_eq")),
                        ),
                    ),
                ),
            ),
        ),
        false,
    )?;

    // 1 ≠ 2
    env.add_def(
        "neq_nat_one_two",
        neq_nat(one.clone(), two.clone()),
        sapp3(
            sconst("neq_succ_succ_forward"),
            nat_zero(),
            one.clone(),
            sapp(sconst("neq_nat_zero_succ"), nat_zero()),
        ),
        false,
    )?;
    // 1 ≠ 0
    env.add_def(
        "neq_nat_one_zero",
        neq_nat(one.clone(), nat_zero()),
        sapp(sconst("neq_nat_succ_zero"), nat_zero()),
        false,
    )?;
    // 2 ≠ 0
    env.add_def(
        "neq_nat_two_zero",
        neq_nat(two.clone(), nat_zero()),
        sapp(sconst("neq_nat_succ_zero"), one.clone()),
        false,
    )?;

    // Free / Borrow / BorrowMut / CanRead / CanWrite
    env.add_def(
        "Free",
        spi("p", perm_ty(), spi("l", nat(), STerm::Sort(0))),
        slam("p", perm_ty(), slam("l", nat(), free(svar("p"), svar("l")))),
        true,
    )?;
    env.add_def(
        "Borrow",
        spi("p", perm_ty(), spi("l", nat(), STerm::Sort(0))),
        slam(
            "p",
            perm_ty(),
            slam("l", nat(), borrow(svar("p"), svar("l"))),
        ),
        true,
    )?;
    env.add_def(
        "BorrowMut",
        spi("p", perm_ty(), spi("l", nat(), STerm::Sort(0))),
        slam(
            "p",
            perm_ty(),
            slam("l", nat(), borrow_mut(svar("p"), svar("l"))),
        ),
        true,
    )?;
    env.add_def(
        "CanRead",
        spi("p", perm_ty(), spi("l", nat(), STerm::Sort(0))),
        slam(
            "p",
            perm_ty(),
            slam("l", nat(), can_read(svar("p"), svar("l"))),
        ),
        true,
    )?;
    env.add_def(
        "CanWrite",
        spi("p", perm_ty(), spi("l", nat(), STerm::Sort(0))),
        slam(
            "p",
            perm_ty(),
            slam("l", nat(), can_write(svar("p"), svar("l"))),
        ),
        true,
    )?;

    // read_with : Perm -> Loc -> Heap -> Store -> CanRead p l -> Array
    env.add_def(
        "read_with",
        spi(
            "p",
            perm_ty(),
            spi(
                "l",
                nat(),
                spi(
                    "h",
                    heap_nat_ty(),
                    spi(
                        "s",
                        store_nat_ty(),
                        spi("r", can_read(svar("p"), svar("l")), array_ty()),
                    ),
                ),
            ),
        ),
        slam(
            "p",
            perm_ty(),
            slam(
                "l",
                nat(),
                slam(
                    "h",
                    heap_nat_ty(),
                    slam(
                        "s",
                        store_nat_ty(),
                        slam(
                            "r",
                            can_read(svar("p"), svar("l")),
                            get_store(svar("s"), svar("h"), svar("l")),
                        ),
                    ),
                ),
            ),
        ),
        false,
    )?;

    // write_with :
    //   Perm -> Loc -> Heap -> Store -> Nat -> Nat -> BorrowMut p l -> Store
    env.add_def(
        "write_with",
        spi(
            "p",
            perm_ty(),
            spi(
                "l",
                nat(),
                spi(
                    "h",
                    heap_nat_ty(),
                    spi(
                        "s",
                        store_nat_ty(),
                        spi(
                            "i",
                            nat(),
                            spi(
                                "v",
                                nat(),
                                spi("m", borrow_mut(svar("p"), svar("l")), store_nat_ty()),
                            ),
                        ),
                    ),
                ),
            ),
        ),
        slam(
            "p",
            perm_ty(),
            slam(
                "l",
                nat(),
                slam(
                    "h",
                    heap_nat_ty(),
                    slam(
                        "s",
                        store_nat_ty(),
                        slam(
                            "i",
                            nat(),
                            slam(
                                "v",
                                nat(),
                                slam(
                                    "m",
                                    borrow_mut(svar("p"), svar("l")),
                                    set_in_place_at(
                                        svar("h"),
                                        svar("s"),
                                        svar("l"),
                                        svar("i"),
                                        svar("v"),
                                    ),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
        ),
        false,
    )?;

    // borrow_not_mut
    env.add_def(
        "borrow_not_mut",
        spi(
            "p",
            perm_ty(),
            spi(
                "l",
                nat(),
                spi(
                    "b",
                    borrow(svar("p"), svar("l")),
                    spi("m", borrow_mut(svar("p"), svar("l")), sconst("False")),
                ),
            ),
        ),
        slam(
            "p",
            perm_ty(),
            slam(
                "l",
                nat(),
                slam(
                    "b",
                    borrow(svar("p"), svar("l")),
                    slam(
                        "m",
                        borrow_mut(svar("p"), svar("l")),
                        sapp(
                            sconst("neq_nat_one_two"),
                            sapp6(
                                sconst("Eq.trans"),
                                nat(),
                                one.clone(),
                                perm_get(svar("p"), svar("l")),
                                two.clone(),
                                sapp4(
                                    sconst("Eq.symm"),
                                    nat(),
                                    perm_get(svar("p"), svar("l")),
                                    one.clone(),
                                    svar("b"),
                                ),
                                svar("m"),
                            ),
                        ),
                    ),
                ),
            ),
        ),
        false,
    )?;

    // borrow_not_free
    env.add_def(
        "borrow_not_free",
        spi(
            "p",
            perm_ty(),
            spi(
                "l",
                nat(),
                spi(
                    "b",
                    borrow(svar("p"), svar("l")),
                    spi("f", free(svar("p"), svar("l")), sconst("False")),
                ),
            ),
        ),
        slam(
            "p",
            perm_ty(),
            slam(
                "l",
                nat(),
                slam(
                    "b",
                    borrow(svar("p"), svar("l")),
                    slam(
                        "f",
                        free(svar("p"), svar("l")),
                        sapp(
                            sconst("neq_nat_one_zero"),
                            sapp6(
                                sconst("Eq.trans"),
                                nat(),
                                one.clone(),
                                perm_get(svar("p"), svar("l")),
                                nat_zero(),
                                sapp4(
                                    sconst("Eq.symm"),
                                    nat(),
                                    perm_get(svar("p"), svar("l")),
                                    one.clone(),
                                    svar("b"),
                                ),
                                svar("f"),
                            ),
                        ),
                    ),
                ),
            ),
        ),
        false,
    )?;

    // borrow_mut_not_free
    env.add_def(
        "borrow_mut_not_free",
        spi(
            "p",
            perm_ty(),
            spi(
                "l",
                nat(),
                spi(
                    "m",
                    borrow_mut(svar("p"), svar("l")),
                    spi("f", free(svar("p"), svar("l")), sconst("False")),
                ),
            ),
        ),
        slam(
            "p",
            perm_ty(),
            slam(
                "l",
                nat(),
                slam(
                    "m",
                    borrow_mut(svar("p"), svar("l")),
                    slam(
                        "f",
                        free(svar("p"), svar("l")),
                        sapp(
                            sconst("neq_nat_two_zero"),
                            sapp6(
                                sconst("Eq.trans"),
                                nat(),
                                two.clone(),
                                perm_get(svar("p"), svar("l")),
                                nat_zero(),
                                sapp4(
                                    sconst("Eq.symm"),
                                    nat(),
                                    perm_get(svar("p"), svar("l")),
                                    two.clone(),
                                    svar("m"),
                                ),
                                svar("f"),
                            ),
                        ),
                    ),
                ),
            ),
        ),
        false,
    )?;

    // borrow_can_read : Borrow p l -> CanRead p l
    env.add_def(
        "borrow_can_read",
        spi(
            "p",
            perm_ty(),
            spi(
                "l",
                nat(),
                spi(
                    "b",
                    borrow(svar("p"), svar("l")),
                    can_read(svar("p"), svar("l")),
                ),
            ),
        ),
        slam(
            "p",
            perm_ty(),
            slam(
                "l",
                nat(),
                slam(
                    "b",
                    borrow(svar("p"), svar("l")),
                    slam(
                        "f",
                        eq(nat(), perm_get(svar("p"), svar("l")), nat_zero()),
                        sapp(
                            sconst("neq_nat_one_zero"),
                            sapp6(
                                sconst("Eq.trans"),
                                nat(),
                                one.clone(),
                                perm_get(svar("p"), svar("l")),
                                nat_zero(),
                                sapp4(
                                    sconst("Eq.symm"),
                                    nat(),
                                    perm_get(svar("p"), svar("l")),
                                    one.clone(),
                                    svar("b"),
                                ),
                                svar("f"),
                            ),
                        ),
                    ),
                ),
            ),
        ),
        false,
    )?;

    // borrow_mut_can_read : BorrowMut p l -> CanRead p l
    env.add_def(
        "borrow_mut_can_read",
        spi(
            "p",
            perm_ty(),
            spi(
                "l",
                nat(),
                spi(
                    "m",
                    borrow_mut(svar("p"), svar("l")),
                    can_read(svar("p"), svar("l")),
                ),
            ),
        ),
        slam(
            "p",
            perm_ty(),
            slam(
                "l",
                nat(),
                slam(
                    "m",
                    borrow_mut(svar("p"), svar("l")),
                    slam(
                        "f",
                        eq(nat(), perm_get(svar("p"), svar("l")), nat_zero()),
                        sapp(
                            sconst("neq_nat_two_zero"),
                            sapp6(
                                sconst("Eq.trans"),
                                nat(),
                                two.clone(),
                                perm_get(svar("p"), svar("l")),
                                nat_zero(),
                                sapp4(
                                    sconst("Eq.symm"),
                                    nat(),
                                    perm_get(svar("p"), svar("l")),
                                    two.clone(),
                                    svar("m"),
                                ),
                                svar("f"),
                            ),
                        ),
                    ),
                ),
            ),
        ),
        false,
    )?;

    // borrow_cannot_write : Borrow p l -> CanWrite p l -> False
    env.add_def(
        "borrow_cannot_write",
        spi(
            "p",
            perm_ty(),
            spi(
                "l",
                nat(),
                spi(
                    "b",
                    borrow(svar("p"), svar("l")),
                    spi("w", can_write(svar("p"), svar("l")), sconst("False")),
                ),
            ),
        ),
        slam(
            "p",
            perm_ty(),
            slam(
                "l",
                nat(),
                slam(
                    "b",
                    borrow(svar("p"), svar("l")),
                    slam(
                        "w",
                        can_write(svar("p"), svar("l")),
                        sapp4(
                            sconst("borrow_not_mut"),
                            svar("p"),
                            svar("l"),
                            svar("b"),
                            svar("w"),
                        ),
                    ),
                ),
            ),
        ),
        false,
    )?;

    // borrow_mut_can_write : BorrowMut p l -> CanWrite p l
    env.add_def(
        "borrow_mut_can_write",
        spi(
            "p",
            perm_ty(),
            spi(
                "l",
                nat(),
                spi(
                    "m",
                    borrow_mut(svar("p"), svar("l")),
                    can_write(svar("p"), svar("l")),
                ),
            ),
        ),
        slam(
            "p",
            perm_ty(),
            slam(
                "l",
                nat(),
                slam("m", borrow_mut(svar("p"), svar("l")), svar("m")),
            ),
        ),
        false,
    )?;

    Ok(env)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_builds() {
        build().expect("ownership model should build");
    }

    #[test]
    fn frame_and_result_check() {
        let env = build().unwrap();
        env.check_named("set_result_same").unwrap();
        env.check_named("set_frame").unwrap();
    }

    #[test]
    fn nat_model_builds() {
        build_nat().expect("Nat-indexed ownership model should build");
    }

    #[test]
    fn nat_frame_and_result_check() {
        let env = build_nat().unwrap();
        env.check_named("set_result_same_nat").unwrap();
        env.check_named("set_frame_nat").unwrap();
    }

    #[test]
    fn borrow_model_builds() {
        build_borrow().expect("borrow model should build");
    }

    #[test]
    fn borrow_theorems_check() {
        let env = build_borrow().unwrap();
        for name in [
            "borrow_not_mut",
            "borrow_not_free",
            "borrow_mut_not_free",
            "borrow_can_read",
            "borrow_mut_can_read",
            "borrow_cannot_write",
            "borrow_mut_can_write",
        ] {
            env.check_named(name).unwrap();
        }
    }
}
