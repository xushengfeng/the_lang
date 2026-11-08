use super::common::*;
use crate::dependent::{sapp, sapp2, sapp3, sapp4, sapp5, sconst, slam, spi, svar, Env, STerm};

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
