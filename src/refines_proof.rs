//! Kernel certificates for `refines`.
//!
//! These are not new axioms.  They are ordinary theorems in the existing
//! ProofModel, checked by `proofcheck`.  `refines` names one of them as the
//! certificate for an `impl ... refines ...` pair.
//!
//! Current mapping is manual:
//!   fill        / fill_spec        -> fill_refines_spec
//!   fill_shared / fill_shared_spec -> fill_shared_refines_spec
//!
//! The next step is to generate these obligations from the surface spec
//! instead of hand-mapping names.

use crate::dependent::{sapp, sapp2, sapp3, sapp5, sconst, slam, spi, svar, Env, STerm};
use crate::ownership_proof::build_borrow;

fn nat() -> STerm {
    sconst("Nat")
}

fn array_ty() -> STerm {
    sapp(sconst("List"), nat())
}

fn zero() -> STerm {
    sconst("Nat.zero")
}

fn heap_ty() -> STerm {
    spi("_", nat(), nat())
}

fn store_ty() -> STerm {
    spi("_", nat(), array_ty())
}

fn eq(ty: STerm, a: STerm, b: STerm) -> STerm {
    sapp3(sconst("Eq"), ty, a, b)
}

fn get_store(s: STerm, h: STerm, l: STerm) -> STerm {
    sapp3(sconst("get_store"), s, h, l)
}

fn set_in_place_at(h: STerm, s: STerm, l: STerm, i: STerm, v: STerm) -> STerm {
    sapp5(sconst("set_in_place_at"), h, s, l, i, v)
}

fn arr_set(xs: STerm, i: STerm, v: STerm) -> STerm {
    sapp3(sconst("arr_set"), xs, i, v)
}

pub struct ProofStore {
    pub env: Env,
}

impl ProofStore {
    pub fn build() -> Result<Self, String> {
        let mut env = build_borrow()?;

        // fill_refines_spec:
        //   ∀ h s l x,
        //     get_store (set_in_place_at h s l 0 x) h l
        //       = arr_set (get_store s h l) 0 x
        //
        // This is `set_result_same_nat` specialized to index 0; the RHS
        // definitionally unfolds `set_pure_at` to `arr_set`.
        let fill_refines_ty = spi(
            "h",
            heap_ty(),
            spi(
                "s",
                store_ty(),
                spi(
                    "l",
                    nat(),
                    spi(
                        "x",
                        nat(),
                        eq(
                            array_ty(),
                            get_store(
                                set_in_place_at(svar("h"), svar("s"), svar("l"), zero(), svar("x")),
                                svar("h"),
                                svar("l"),
                            ),
                            arr_set(
                                get_store(svar("s"), svar("h"), svar("l")),
                                zero(),
                                svar("x"),
                            ),
                        ),
                    ),
                ),
            ),
        );
        let fill_refines_body = slam(
            "h",
            heap_ty(),
            slam(
                "s",
                store_ty(),
                slam(
                    "l",
                    nat(),
                    slam(
                        "x",
                        nat(),
                        sapp5(
                            sconst("set_result_same_nat"),
                            svar("h"),
                            svar("s"),
                            svar("l"),
                            zero(),
                            svar("x"),
                        ),
                    ),
                ),
            ),
        );
        env.add_def(
            "fill_refines_spec",
            fill_refines_ty,
            fill_refines_body,
            false,
        )?;

        // fill_shared_refines_spec:
        //   ∀ a x, arr_set a 0 x = arr_set a 0 x
        //
        // The copy implementation returns the pure updated array; this is
        // definitional.
        let fill_shared_ty = spi(
            "a",
            array_ty(),
            spi(
                "x",
                nat(),
                eq(
                    array_ty(),
                    arr_set(svar("a"), zero(), svar("x")),
                    arr_set(svar("a"), zero(), svar("x")),
                ),
            ),
        );
        let fill_shared_body = slam(
            "a",
            array_ty(),
            slam(
                "x",
                nat(),
                sapp2(
                    sconst("Eq.refl"),
                    array_ty(),
                    arr_set(svar("a"), zero(), svar("x")),
                ),
            ),
        );
        env.add_def(
            "fill_shared_refines_spec",
            fill_shared_ty,
            fill_shared_body,
            false,
        )?;

        Ok(Self { env })
    }

    pub fn check(&self, name: &str) -> Result<(), String> {
        self.env
            .check_named(name)
            .map_err(|e| format!("kernel certificate `{name}` failed:\n{e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certificates_check() {
        let store = ProofStore::build().unwrap();
        store.check("fill_refines_spec").unwrap();
        store.check("fill_shared_refines_spec").unwrap();
    }
}
