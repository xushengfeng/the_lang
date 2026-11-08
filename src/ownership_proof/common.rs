use crate::dependent::{sapp, sapp2, sapp3, sapp4, sapp5, sconst, spi, STerm};

pub(super) fn nat() -> STerm {
    sconst("Nat")
}

pub(super) fn bool_ty() -> STerm {
    sconst("Bool")
}

pub(super) fn btrue() -> STerm {
    sconst("Bool.true")
}

pub(super) fn bfalse() -> STerm {
    sconst("Bool.false")
}

pub(super) fn array_ty() -> STerm {
    sapp(sconst("List"), nat())
}

pub(super) fn nil_array() -> STerm {
    sapp(sconst("List.nil"), nat())
}

pub(super) fn cons_array(x: STerm, xs: STerm) -> STerm {
    sapp3(sconst("List.cons"), nat(), x, xs)
}

pub(super) fn eq(ty: STerm, a: STerm, b: STerm) -> STerm {
    sapp3(sconst("Eq"), ty, a, b)
}

pub(super) fn refl(ty: STerm, a: STerm) -> STerm {
    sapp2(sconst("Eq.refl"), ty, a)
}

pub(super) fn neq(a: STerm, b: STerm) -> STerm {
    sapp2(sconst("NeqBool"), a, b)
}

pub(super) fn store_ty() -> STerm {
    spi("_", bool_ty(), array_ty())
}

pub(super) fn heap_ty() -> STerm {
    spi("_", bool_ty(), bool_ty())
}

pub(super) fn update_store(s: STerm, b: STerm, v: STerm) -> STerm {
    sapp3(sconst("update_store"), s, b, v)
}

pub(super) fn update_store_at(s: STerm, b: STerm, v: STerm, at: STerm) -> STerm {
    sapp(update_store(s, b, v), at)
}

pub(super) fn arr_set(xs: STerm, i: STerm, v: STerm) -> STerm {
    sapp3(sconst("arr_set"), xs, i, v)
}

pub(super) fn obs(s: STerm, h: STerm, l: STerm) -> STerm {
    sapp3(sconst("obs"), s, h, l)
}

pub(super) fn set_in_place(h: STerm, s: STerm, i: STerm, v: STerm) -> STerm {
    sapp4(sconst("set_in_place"), h, s, i, v)
}

pub(super) fn set_pure(h: STerm, s: STerm, i: STerm, v: STerm) -> STerm {
    sapp4(sconst("set_pure"), h, s, i, v)
}

pub(super) fn unique(h: STerm) -> STerm {
    sapp(sconst("Unique"), h)
}

pub(super) fn nat_succ(a: STerm) -> STerm {
    sapp(sconst("Nat.succ"), a)
}

pub(super) fn nat_zero() -> STerm {
    sconst("Nat.zero")
}

pub(super) fn neq_nat(a: STerm, b: STerm) -> STerm {
    sapp2(sconst("NeqNat"), a, b)
}

pub(super) fn store_nat_ty() -> STerm {
    spi("_", nat(), array_ty())
}

pub(super) fn heap_nat_ty() -> STerm {
    spi("_", nat(), nat())
}

pub(super) fn shift_store(s: STerm) -> STerm {
    sapp(sconst("shift_store"), s)
}

pub(super) fn update_at(a: STerm, v: STerm, s: STerm, b: STerm) -> STerm {
    sapp4(sconst("update_at"), a, v, s, b)
}

pub(super) fn unique_nat(h: STerm) -> STerm {
    sapp(sconst("UniqueNat"), h)
}

pub(super) fn get_store(s: STerm, h: STerm, l: STerm) -> STerm {
    sapp3(sconst("get_store"), s, h, l)
}

pub(super) fn set_in_place_at(h: STerm, s: STerm, l: STerm, i: STerm, v: STerm) -> STerm {
    sapp5(sconst("set_in_place_at"), h, s, l, i, v)
}

pub(super) fn set_pure_at(h: STerm, s: STerm, l: STerm, i: STerm, v: STerm) -> STerm {
    sapp5(sconst("set_pure_at"), h, s, l, i, v)
}

pub(super) fn perm_ty() -> STerm {
    spi("_", nat(), nat())
}

pub(super) fn perm_get(p: STerm, l: STerm) -> STerm {
    sapp(p, l)
}

pub(super) fn free(p: STerm, l: STerm) -> STerm {
    eq(nat(), perm_get(p, l), nat_zero())
}

pub(super) fn borrow(p: STerm, l: STerm) -> STerm {
    eq(nat(), perm_get(p, l), nat_succ(nat_zero()))
}

pub(super) fn borrow_mut(p: STerm, l: STerm) -> STerm {
    eq(nat(), perm_get(p, l), nat_succ(nat_succ(nat_zero())))
}

pub(super) fn can_read(p: STerm, l: STerm) -> STerm {
    neq_nat(perm_get(p, l), nat_zero())
}

pub(super) fn can_write(p: STerm, l: STerm) -> STerm {
    borrow_mut(p, l)
}
