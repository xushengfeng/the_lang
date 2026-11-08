use super::common::*;
use super::nat::build_nat;
use crate::dependent::{sapp, sapp3, sapp4, sapp6, sconst, slam, spi, svar, Env, STerm};

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
