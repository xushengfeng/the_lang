use super::common::*;
use super::two_pos::build;
use crate::dependent::{sapp, sapp2, sapp3, sapp4, sapp5, sconst, slam, spi, svar, Env, STerm};

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
