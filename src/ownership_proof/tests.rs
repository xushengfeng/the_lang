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
