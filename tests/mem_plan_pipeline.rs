use alethe::mem_plan::{plan_checked_program, run, run_program, MemExpr};
use alethe::ownership::check_source;
use alethe::region::MemValue;

#[test]
fn surface_fill_is_in_place() {
    let src = r#"
        fn fill(own a: Array<Int>, x: Int) -> Array<Int> {
            set(a, 0, x)
        }
        fn main() -> Int {
            let a = [1, 2, 3];
            let b = fill(a, 9);
            b[0]
        }
    "#;
    let checked = check_source(src).unwrap();
    let program = plan_checked_program(&checked).unwrap();
    let result = run_program(&program, "main").unwrap();

    assert_eq!(result.value, MemValue::Int(9));
    assert_eq!(result.stats().in_place, 1);
    assert_eq!(result.stats().copies, 0);
}

#[test]
fn surface_call_copies_a_returned_array() {
    let src = r#"
        fn make(x: Int) -> Array<Int> {
            [x, x]
        }
        fn main() -> Int {
            let a = make(7);
            a[0]
        }
    "#;
    let checked = check_source(src).unwrap();
    let program = plan_checked_program(&checked).unwrap();
    let result = run_program(&program, "main").unwrap();

    assert_eq!(result.value, MemValue::Int(7));
    assert_eq!(result.stats().copies, 1);
    assert_eq!(result.stats().frees, 1);
}

#[test]
fn direct_if_allocates_no_thunks() {
    let plan = MemExpr::If {
        cond: Box::new(MemExpr::Bool(true)),
        then_branch: Box::new(MemExpr::Int(1)),
        else_branch: Box::new(MemExpr::Int(2)),
        optimized: true,
    };
    let result = run(&plan).unwrap();

    assert_eq!(result.value, MemValue::Int(1));
    assert_eq!(result.stats().thunks, 0);
}

#[test]
fn fallback_if_uses_two_thunks() {
    let plan = MemExpr::If {
        cond: Box::new(MemExpr::Bool(false)),
        then_branch: Box::new(MemExpr::Int(1)),
        else_branch: Box::new(MemExpr::Int(2)),
        optimized: false,
    };
    let result = run(&plan).unwrap();

    assert_eq!(result.value, MemValue::Int(2));
    assert_eq!(result.stats().thunks, 2);
}

#[test]
fn lambda_apply_and_thunk_force() {
    let apply = MemExpr::Apply {
        callee: Box::new(MemExpr::Lambda {
            params: vec!["x".into()],
            body: Box::new(MemExpr::Var("x".into())),
            captures: vec![],
        }),
        args: vec![MemExpr::Int(7)],
    };
    let applied = run(&apply).unwrap();
    assert_eq!(applied.value, MemValue::Int(7));
    assert_eq!(applied.stats().closures, 1);

    let force = MemExpr::Force(Box::new(MemExpr::Thunk(Box::new(MemExpr::Int(9)))));
    let forced = run(&force).unwrap();
    assert_eq!(forced.value, MemValue::Int(9));
    assert_eq!(forced.stats().thunks, 1);
}
