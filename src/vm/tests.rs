use super::*;
use crate::ownership::{check_source, run as interp_run};

fn one_func(code: Vec<Instr>, ret: BType) -> BcProgram {
    BcProgram {
        funcs: vec![BcFunction {
            name: "bad".into(),
            arity: 0,
            param_tys: vec![],
            ret,
            nlocals: 1,
            code,
        }],
        main: 0,
    }
}

#[test]
fn vm_runs_ownership_demo() {
    let src = std::fs::read_to_string("examples/ownership.alt").unwrap();
    let checked = check_source(&src).unwrap();
    let bc = compile(&checked).unwrap();
    verify(&bc).unwrap();
    let r = run(&bc, bc.main).unwrap();
    assert_eq!(r.value.to_string(), "30");
    assert_eq!(r.stats.in_place, 2);
    assert_eq!(r.stats.copies, 2);
    assert_eq!(r.stats.allocations, 5);
    assert_eq!(r.stats.bounds_checks, 9);
}

#[test]
fn vm_matches_interpreter() {
    let src = std::fs::read_to_string("examples/ownership.alt").unwrap();
    let checked = check_source(&src).unwrap();
    let bc = compile(&checked).unwrap();
    let vm = run(&bc, bc.main).unwrap();
    let interp = interp_run(&checked, "main").unwrap();
    assert_eq!(vm.value.to_string(), interp.value.to_string());
    assert_eq!(vm.stats.allocations, interp.stats.allocations);
    assert_eq!(vm.stats.copies, interp.stats.copies);
    assert_eq!(vm.stats.in_place, interp.stats.in_place);
    assert_eq!(vm.stats.bounds_checks, interp.stats.bounds_checks);
}

#[test]
fn vm_runs_callbacks_and_closures() {
    let src = r#"
        fn apply_twice(f: Fn(Int) -> Int, x: Int) -> Int {
          f(f(x))
        }
        fn add_one(x: Int) -> Int { x + 1 }
        fn main() -> Int {
          let double = fn(y: Int) -> Int { y * 2 };
          let a = apply_twice(double, 5);
          let b = apply_twice(add_one, 1);
          a + b
        }
    "#;
    let checked = check_source(src).unwrap();
    let bc = compile(&checked).unwrap();
    verify(&bc).unwrap();
    let r = run(&bc, bc.main).unwrap();
    let interp = interp_run(&checked, "main").unwrap();
    assert_eq!(r.value.to_string(), "23");
    assert_eq!(r.value.to_string(), interp.value.to_string());
}

#[test]
fn vm_lifts_nested_closures() {
    let src = r#"
        fn make_adder(n: Int) -> Fn(Int) -> Int {
          fn(y: Int) -> Int { y + n }
        }
        fn main() -> Int { make_adder(10)(7) }
    "#;
    let checked = check_source(src).unwrap();
    let bc = compile(&checked).unwrap();
    verify(&bc).unwrap();
    let r = run(&bc, bc.main).unwrap();
    assert_eq!(r.value.to_string(), "17");
}

#[test]
fn verifier_rejects_stack_underflow() {
    let bc = one_func(vec![Instr::Add, Instr::Ret], BType::Int);
    let err = verify(&bc).unwrap_err();
    assert!(err.contains("underflow"), "{err}");
}

#[test]
fn verifier_rejects_wrong_return_type() {
    let bc = one_func(vec![Instr::PushBool(true), Instr::Ret], BType::Int);
    let err = verify(&bc).unwrap_err();
    assert!(err.contains("ret returns"), "{err}");
}

#[test]
fn verifier_rejects_bad_jump() {
    let bc = one_func(vec![Instr::Jump(99)], BType::Unit);
    let err = verify(&bc).unwrap_err();
    assert!(err.contains("out of range"), "{err}");
}

#[test]
fn verifier_rejects_uninitialized_local() {
    let bc = one_func(vec![Instr::Load(0), Instr::Ret], BType::Int);
    let err = verify(&bc).unwrap_err();
    assert!(err.contains("uninitialized"), "{err}");
}

#[test]
fn verifier_rejects_wrong_call_argument() {
    let callee = BcFunction {
        name: "callee".into(),
        arity: 1,
        param_tys: vec![BType::Int],
        ret: BType::Int,
        nlocals: 1,
        code: vec![Instr::Load(0), Instr::Ret],
    };
    let caller = BcFunction {
        name: "caller".into(),
        arity: 0,
        param_tys: vec![],
        ret: BType::Int,
        nlocals: 0,
        code: vec![
            Instr::PushBool(true),
            Instr::Call { func: 0, argc: 1 },
            Instr::Ret,
        ],
    };
    let bc = BcProgram {
        funcs: vec![callee, caller],
        main: 1,
    };
    let err = verify(&bc).unwrap_err();
    assert!(err.contains("argument"), "{err}");
}

#[test]
fn vm_erases_proof_parameters() {
    let src = r#"
        fn get(a: Array<Int>, i: Int, h: proof 0 <= i) -> Int { a[i] }
        fn main() -> Int {
          let a = [10, 20, 30];
          get(a, 1, by trivial)
        }
    "#;
    let checked = check_source(src).unwrap();
    let bc = compile(&checked).unwrap();
    let get = bc.get("get").unwrap();
    assert_eq!(get.arity, 2, "proof parameter must be erased");
    let r = run(&bc, bc.main).unwrap();
    assert_eq!(r.value.to_string(), "20");
}
