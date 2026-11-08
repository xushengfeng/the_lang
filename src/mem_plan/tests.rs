use super::*;
use crate::region::{Loc, MemValue, RegionHeap, RegionId};

fn arr(xs: &[i64]) -> SrcExpr {
    SrcExpr::Array(xs.iter().map(|n| SrcExpr::Int(*n)).collect())
}

fn var(name: &str) -> SrcExpr {
    SrcExpr::Var(name.to_string())
}

fn own(name: &str, value: SrcExpr, body: SrcExpr) -> SrcExpr {
    SrcExpr::Let {
        name: name.to_string(),
        mode: BindMode::Own,
        value: Box::new(value),
        body: Box::new(body),
    }
}

fn share(name: &str, value: SrcExpr, body: SrcExpr) -> SrcExpr {
    SrcExpr::Let {
        name: name.to_string(),
        mode: BindMode::Share,
        value: Box::new(value),
        body: Box::new(body),
    }
}

fn set(name: &str, index: usize, value: i64) -> SrcExpr {
    SrcExpr::Set {
        target: Box::new(var(name)),
        index,
        value: Box::new(SrcExpr::Int(value)),
    }
}

fn get(name: &str, index: usize) -> SrcExpr {
    SrcExpr::Get {
        target: Box::new(var(name)),
        index,
    }
}

fn run_src(src: &SrcExpr) -> MemRun {
    let plan = plan(src).unwrap();
    run(&plan).unwrap()
}

#[test]
fn unique_set_is_in_place() {
    let src = own("a", arr(&[1, 2, 3]), own("b", set("a", 1, 9), get("b", 1)));

    let result = run_src(&src);
    assert_eq!(result.value, MemValue::Int(9));
    assert_eq!(result.stats().allocations, 1);
    assert_eq!(result.stats().in_place, 1);
    assert_eq!(result.stats().copies, 0);
}

#[test]
fn shared_set_copies_result() {
    let src = own(
        "a",
        arr(&[1, 2, 3]),
        share("s", var("a"), own("b", set("s", 1, 9), get("b", 1))),
    );

    let result = run_src(&src);
    assert_eq!(result.value, MemValue::Int(9));
    assert_eq!(result.stats().allocations, 2);
    assert_eq!(result.stats().in_place, 0);
    assert_eq!(result.stats().copies, 1);
}

#[test]
fn shared_set_leaves_the_original_unchanged() {
    let src = own(
        "a",
        arr(&[1, 2, 3]),
        share("s", var("a"), own("b", set("s", 1, 9), get("a", 1))),
    );

    let result = run_src(&src);
    assert_eq!(result.value, MemValue::Int(2));
    assert_eq!(result.stats().copies, 1);
}

#[test]
fn own_of_a_shared_value_copies() {
    let src = own(
        "a",
        arr(&[1, 2, 3]),
        share("s", var("a"), own("b", var("s"), get("b", 1))),
    );

    let result = run_src(&src);
    assert_eq!(result.value, MemValue::Int(2));
    assert_eq!(result.stats().allocations, 2);
    assert_eq!(result.stats().copies, 1);
}

#[test]
fn use_after_consume_is_rejected() {
    let src = own("a", arr(&[1, 2, 3]), own("b", set("a", 1, 9), get("a", 1)));

    assert_eq!(plan(&src), Err(PlanError::UseAfterConsume("a".into())));
}

#[test]
fn region_frees_its_allocations() {
    let src = SrcExpr::Region(Box::new(own("a", arr(&[1, 2, 3]), get("a", 1))));

    let result = run_src(&src);
    assert_eq!(result.value, MemValue::Int(2));
    assert_eq!(result.stats().frees, 1);
    assert!(!result.heap.is_live(RegionId(1)));
    assert!(result.heap.is_live(result.root));
}

#[test]
fn region_may_not_return_an_array() {
    let src = SrcExpr::Region(Box::new(arr(&[1, 2, 3])));
    assert_eq!(plan(&src), Err(PlanError::RegionEscape));
}

#[test]
fn returned_array_is_observed_in_root_region() {
    let src = SrcExpr::Return(Box::new(arr(&[1, 2, 3])));

    let result = run_src(&src);
    assert!(result.heap.is_live(result.root));
    let obs = result.observe().unwrap();
    assert_eq!(obs.roots, vec![crate::region::ObsValue::LocRef(0)]);
    assert_eq!(
        obs.blocks,
        vec![vec![
            crate::region::ObsValue::Int(1),
            crate::region::ObsValue::Int(2),
            crate::region::ObsValue::Int(3),
        ]]
    );
}

// -----------------------------------------------------------------------
// Surface bridge
// -----------------------------------------------------------------------

use crate::ownership::check_source;
use crate::syntax::Ty;

fn heap_with_array(xs: &[i64]) -> (RegionHeap, RegionId, Loc) {
    let mut heap = RegionHeap::new();
    let root = heap.new_region();
    let loc = heap
        .alloc(root, xs.iter().map(|n| MemValue::Int(*n)).collect())
        .unwrap();
    (heap, root, loc)
}

fn ints(xs: &[i64]) -> Vec<MemValue> {
    xs.iter().map(|n| MemValue::Int(*n)).collect()
}

#[test]
fn checked_fill_is_in_place() {
    let src = "fn fill(own a: Array<Int>, x: Int) -> Array<Int> { set(a, 0, x) }";
    let prog = check_source(src).unwrap();
    let f = prog.get("fill").unwrap();

    let (mut heap, root, a) = heap_with_array(&[1, 2, 3]);
    let result = run_fn_in(f, &[MemValue::Loc(a), MemValue::Int(9)], &mut heap, root).unwrap();
    let result = match result {
        MemValue::Loc(loc) => loc,
        other => panic!("expected a location, got {other:?}"),
    };

    assert_eq!(result, a);
    assert_eq!(heap.block(a).unwrap(), &ints(&[9, 2, 3]));
    assert_eq!(heap.stats().in_place, 1);
    assert_eq!(heap.stats().copies, 0);
    assert_eq!(heap.stats().allocations, 1);
}

#[test]
fn checked_fill_shared_copies() {
    let src = "fn fill_shared(a: Array<Int>, x: Int) -> Array<Int> { set(a, 0, x) }";
    let prog = check_source(src).unwrap();
    let f = prog.get("fill_shared").unwrap();

    let (mut heap, root, a) = heap_with_array(&[1, 2, 3]);
    let result = run_fn_in(f, &[MemValue::Loc(a), MemValue::Int(9)], &mut heap, root).unwrap();
    let result = match result {
        MemValue::Loc(loc) => loc,
        other => panic!("expected a location, got {other:?}"),
    };

    assert_ne!(result, a);
    assert_eq!(heap.block(a).unwrap(), &ints(&[1, 2, 3]));
    assert_eq!(heap.block(result).unwrap(), &ints(&[9, 2, 3]));
    assert_eq!(heap.stats().in_place, 0);
    assert_eq!(heap.stats().copies, 1);
    assert_eq!(heap.stats().allocations, 2);
}

#[test]
fn checked_local_index_runs() {
    let src = "fn third() -> Int { let a = [1, 2, 3]; a[2] }";
    let prog = check_source(src).unwrap();
    let f = prog.get("third").unwrap();

    let mut heap = RegionHeap::new();
    let root = heap.new_region();
    let result = run_fn_in(f, &[], &mut heap, root).unwrap();

    assert_eq!(result, MemValue::Int(3));
    assert_eq!(heap.stats().allocations, 1);
}

#[test]
fn checked_push_is_in_place() {
    let src = "fn grow(own a: Array<Int>, x: Int) -> Array<Int> { push(a, x) }";
    let prog = check_source(src).unwrap();
    let f = prog.get("grow").unwrap();

    let (mut heap, root, a) = heap_with_array(&[1, 2, 3]);
    let result = run_fn_in(f, &[MemValue::Loc(a), MemValue::Int(9)], &mut heap, root).unwrap();
    let result = match result {
        MemValue::Loc(loc) => loc,
        other => panic!("expected a location, got {other:?}"),
    };

    assert_eq!(result, a);
    assert_eq!(heap.block(a).unwrap(), &ints(&[1, 2, 3, 9]));
    assert_eq!(heap.stats().in_place, 1);
}

#[test]
fn checked_unsupported_expression_is_rejected() {
    let src = "fn add(a: Int) -> Int { a + 1 }";
    let prog = check_source(src).unwrap();
    let f = prog.get("add").unwrap();

    assert_eq!(
        plan_checked_fn(f),
        Err(BridgeError::Unsupported("binary expression".into()))
    );
}

#[test]
fn checked_arg_type_is_checked() {
    let src = "fn fill(own a: Array<Int>, x: Int) -> Array<Int> { set(a, 0, x) }";
    let prog = check_source(src).unwrap();
    let f = prog.get("fill").unwrap();

    let mut heap = RegionHeap::new();
    let root = heap.new_region();
    let result = run_fn_in(f, &[MemValue::Int(1), MemValue::Int(9)], &mut heap, root);

    assert_eq!(
        result,
        Err(EvalError::ArgType {
            index: 0,
            expected: Ty::Array(Box::new(Ty::Int)),
        })
    );
}

#[test]
fn checked_len_runs() {
    let src = "fn size(a: Array<Int>) -> Int { len(a) }";
    let prog = check_source(src).unwrap();
    let f = prog.get("size").unwrap();

    let (mut heap, root, a) = heap_with_array(&[1, 2, 3]);
    let result = run_fn_in(f, &[MemValue::Loc(a)], &mut heap, root).unwrap();

    assert_eq!(result, MemValue::Int(3));
}

#[test]
fn checked_seq_runs() {
    let src = "fn second() -> Int { let a = [1, 2, 3]; a[0]; a[1] }";
    let prog = check_source(src).unwrap();
    let f = prog.get("second").unwrap();

    let mut heap = RegionHeap::new();
    let root = heap.new_region();
    let result = run_fn_in(f, &[], &mut heap, root).unwrap();

    assert_eq!(result, MemValue::Int(2));
}

#[test]
fn checked_call_is_in_place_when_argument_is_owned() {
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
    let prog = check_source(src).unwrap();
    let mem = plan_checked_program(&prog).unwrap();
    let result = run_program(&mem, "main").unwrap();

    assert_eq!(result.value, MemValue::Int(9));
    assert_eq!(result.stats().allocations, 1);
    assert_eq!(result.stats().in_place, 1);
    assert_eq!(result.stats().copies, 0);
    assert_eq!(result.stats().frees, 1);
}

#[test]
fn checked_call_copies_a_returned_array() {
    let src = r#"
        fn make(x: Int) -> Array<Int> {
            [x, x]
        }
        fn main() -> Int {
            let a = make(7);
            a[0]
        }
    "#;
    let prog = check_source(src).unwrap();
    let mem = plan_checked_program(&prog).unwrap();
    let result = run_program(&mem, "main").unwrap();

    assert_eq!(result.value, MemValue::Int(7));
    assert_eq!(result.stats().allocations, 2);
    assert_eq!(result.stats().copies, 1);
    assert_eq!(result.stats().frees, 1);
}

#[test]
fn checked_call_can_return_a_caller_array_without_copy() {
    let src = r#"
        fn id(a: Array<Int>) -> Array<Int> {
            a
        }
        fn main() -> Int {
            let a = [1, 2, 3];
            let b = id(a);
            b[0]
        }
    "#;
    let prog = check_source(src).unwrap();
    let mem = plan_checked_program(&prog).unwrap();
    let result = run_program(&mem, "main").unwrap();

    assert_eq!(result.value, MemValue::Int(1));
    assert_eq!(result.stats().allocations, 1);
    assert_eq!(result.stats().copies, 0);
    assert_eq!(result.stats().frees, 1);
}

#[test]
fn checked_if_selects_the_branch() {
    let src = "fn choose(c: Bool) -> Int { if c then 1 else 2 }";
    let prog = check_source(src).unwrap();
    let f = prog.get("choose").unwrap();

    let mut heap = RegionHeap::new();
    let root = heap.new_region();
    let then_value = run_fn_in(f, &[MemValue::Bool(true)], &mut heap, root).unwrap();
    let else_value = run_fn_in(f, &[MemValue::Bool(false)], &mut heap, root).unwrap();

    assert_eq!(then_value, MemValue::Int(1));
    assert_eq!(else_value, MemValue::Int(2));
}

#[test]
fn checked_if_allocates_only_the_selected_branch() {
    let src = r#"
        fn choose(c: Bool) -> Array<Int> {
            if c then [1, 2] else [3, 4]
        }
    "#;
    let prog = check_source(src).unwrap();
    let f = prog.get("choose").unwrap();

    let mut heap = RegionHeap::new();
    let root = heap.new_region();
    let result = run_fn_in(f, &[MemValue::Bool(true)], &mut heap, root).unwrap();
    let result = match result {
        MemValue::Loc(loc) => loc,
        other => panic!("expected a location, got {other:?}"),
    };

    assert_eq!(heap.block(result).unwrap(), &ints(&[1, 2]));
    assert_eq!(heap.stats().allocations, 1);
}

#[test]
fn direct_if_constructs_no_thunks() {
    let src = "fn choose(c: Bool) -> Int { if c then 1 else 2 }";
    let prog = check_source(src).unwrap();
    let f = prog.get("choose").unwrap();

    let mut heap = RegionHeap::new();
    let root = heap.new_region();
    let result = run_fn_in(f, &[MemValue::Bool(true)], &mut heap, root).unwrap();

    assert_eq!(result, MemValue::Int(1));
    assert_eq!(heap.stats().thunks, 0);
}

#[test]
fn fallback_if_constructs_both_thunks() {
    let plan = MemExpr::If {
        cond: Box::new(MemExpr::Bool(true)),
        then_branch: Box::new(MemExpr::Int(1)),
        else_branch: Box::new(MemExpr::Int(2)),
        optimized: false,
    };

    let result = run(&plan).unwrap();
    assert_eq!(result.value, MemValue::Int(1));
    assert_eq!(result.stats().thunks, 2);
    assert_eq!(result.stats().allocations, 0);
}

#[test]
fn fallback_if_executes_only_the_selected_branch() {
    let plan = MemExpr::If {
        cond: Box::new(MemExpr::Bool(true)),
        then_branch: Box::new(MemExpr::Alloc(vec![MemExpr::Int(1)])),
        else_branch: Box::new(MemExpr::Alloc(vec![MemExpr::Int(2)])),
        optimized: false,
    };

    let result = run(&plan).unwrap();
    assert!(matches!(result.value, MemValue::Loc(_)));
    assert_eq!(result.stats().thunks, 2);
    assert_eq!(result.stats().allocations, 1);
}

#[test]
fn fallback_if_captures_the_environment() {
    let plan = MemExpr::Let {
        name: "x".into(),
        value: Box::new(MemExpr::Int(7)),
        body: Box::new(MemExpr::If {
            cond: Box::new(MemExpr::Bool(false)),
            then_branch: Box::new(MemExpr::Var("x".into())),
            else_branch: Box::new(MemExpr::Int(0)),
            optimized: false,
        }),
    };

    let result = run(&plan).unwrap();
    assert_eq!(result.value, MemValue::Int(0));
    assert_eq!(result.stats().thunks, 2);
}

#[test]
fn lambda_identity_apply() {
    let plan = MemExpr::Apply {
        callee: Box::new(MemExpr::Lambda {
            params: vec!["x".into()],
            body: Box::new(MemExpr::Var("x".into())),
        }),
        args: vec![MemExpr::Int(7)],
    };

    let result = run(&plan).unwrap();
    assert_eq!(result.value, MemValue::Int(7));
    assert_eq!(result.stats().closures, 1);
}

#[test]
fn lambda_captures_the_environment() {
    let plan = MemExpr::Let {
        name: "n".into(),
        value: Box::new(MemExpr::Int(7)),
        body: Box::new(MemExpr::Apply {
            callee: Box::new(MemExpr::Lambda {
                params: vec![],
                body: Box::new(MemExpr::Var("n".into())),
            }),
            args: vec![],
        }),
    };

    let result = run(&plan).unwrap();
    assert_eq!(result.value, MemValue::Int(7));
    assert_eq!(result.stats().closures, 1);
}

#[test]
fn thunk_force() {
    let plan = MemExpr::Force(Box::new(MemExpr::Thunk(Box::new(MemExpr::Int(7)))));

    let result = run(&plan).unwrap();
    assert_eq!(result.value, MemValue::Int(7));
    assert_eq!(result.stats().thunks, 1);
}

#[test]
fn higher_order_lambda_apply() {
    // let id = \x -> x in (\f -> f 7) id
    let plan = MemExpr::Let {
        name: "id".into(),
        value: Box::new(MemExpr::Lambda {
            params: vec!["x".into()],
            body: Box::new(MemExpr::Var("x".into())),
        }),
        body: Box::new(MemExpr::Apply {
            callee: Box::new(MemExpr::Lambda {
                params: vec!["f".into()],
                body: Box::new(MemExpr::Apply {
                    callee: Box::new(MemExpr::Var("f".into())),
                    args: vec![MemExpr::Int(7)],
                }),
            }),
            args: vec![MemExpr::Var("id".into())],
        }),
    };

    let result = run(&plan).unwrap();
    assert_eq!(result.value, MemValue::Int(7));
    assert_eq!(result.stats().closures, 2);
}

#[test]
fn surface_callback_lambda() {
    let src = r#"
        fn call_with(f: Fn(Int) -> Int, x: Int) -> Int { f(x) }
        fn main() -> Int {
          let g = fn(y: Int) -> Int { y };
          call_with(g, 7)
        }
    "#;
    let prog = check_source(src).unwrap();
    let mem = plan_checked_program(&prog).unwrap();
    let result = run_program(&mem, "main").unwrap();
    assert_eq!(result.value, MemValue::Int(7));
    assert!(result.stats().closures >= 1);
}

#[test]
fn surface_callback_named_function() {
    let src = r#"
        fn id(x: Int) -> Int { x }
        fn call_with(f: Fn(Int) -> Int, x: Int) -> Int { f(x) }
        fn main() -> Int { call_with(id, 7) }
    "#;
    let prog = check_source(src).unwrap();
    let mem = plan_checked_program(&prog).unwrap();
    let result = run_program(&mem, "main").unwrap();
    assert_eq!(result.value, MemValue::Int(7));
}

#[test]
fn validator_rejects_unknown_var() {
    let program = MemProgram::default();
    assert_eq!(
        validate_expr(&MemExpr::Var("x".into()), &program),
        Err(ValidateError::UnknownVar("x".into()))
    );
}

#[test]
fn validator_rejects_unknown_function() {
    let program = MemProgram::default();
    let expr = MemExpr::Call {
        name: "missing".into(),
        args: vec![],
    };
    assert_eq!(
        validate_expr(&expr, &program),
        Err(ValidateError::UnknownFunction("missing".into()))
    );
}

#[test]
fn validator_rejects_wrong_call_arity() {
    let program = MemProgram::from_funcs(vec![MemFn {
        name: "f".into(),
        params: vec!["x".into()],
        param_tys: vec![Ty::Int],
        ret: Ty::Int,
        body: MemExpr::Int(0),
    }]);
    let expr = MemExpr::Call {
        name: "f".into(),
        args: vec![],
    };
    assert_eq!(
        validate_expr(&expr, &program),
        Err(ValidateError::BadArity {
            function: "f".into(),
            expected: 1,
            got: 0,
        })
    );
}

#[test]
fn validator_rejects_non_bool_if() {
    let expr = MemExpr::If {
        cond: Box::new(MemExpr::Int(1)),
        then_branch: Box::new(MemExpr::Int(1)),
        else_branch: Box::new(MemExpr::Int(2)),
        optimized: true,
    };
    assert!(matches!(
        validate_expr(&expr, &MemProgram::default()),
        Err(ValidateError::TypeMismatch { .. })
    ));
}

#[test]
fn validator_rejects_set_on_non_array() {
    let expr = MemExpr::Set {
        target: Box::new(MemExpr::Int(1)),
        index: 0,
        value: Box::new(MemExpr::Int(2)),
        in_place: true,
    };
    assert!(matches!(
        validate_expr(&expr, &MemProgram::default()),
        Err(ValidateError::TypeMismatch { .. })
    ));
}

#[test]
fn validator_rejects_return_type_mismatch() {
    let program = MemProgram::from_funcs(vec![MemFn {
        name: "f".into(),
        params: vec![],
        param_tys: vec![],
        ret: Ty::Int,
        body: MemExpr::Bool(true),
    }]);
    assert!(matches!(
        validate_program(&program),
        Err(ValidateError::ReturnTypeMismatch { .. })
    ));
}

#[test]
fn validator_rejects_duplicate_functions() {
    let f = || MemFn {
        name: "f".into(),
        params: vec![],
        param_tys: vec![],
        ret: Ty::Int,
        body: MemExpr::Int(0),
    };
    let program = MemProgram::from_funcs(vec![f(), f()]);
    assert_eq!(
        validate_program(&program),
        Err(ValidateError::DuplicateFunction("f".into()))
    );
}

#[test]
fn validator_accepts_checked_program() {
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
    let prog = check_source(src).unwrap();
    let mem = plan_checked_program(&prog).unwrap();
    assert_eq!(validate_program(&mem), Ok(()));
}
