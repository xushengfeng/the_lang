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
