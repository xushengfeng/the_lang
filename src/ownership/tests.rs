use super::*;

const FILL: &str = r#"
    fn fill(own a: Array<Int>, x: Int) -> Array<Int> { set(a, 0, x) }
"#;

#[test]
fn own_is_in_place() {
    let src = format!(
        "{FILL}
         fn main() -> Int {{
           let a = [1, 2, 3];
           let b = fill(a, 9);
           print(b);
           b[0]
         }}"
    );
    let prog = check_source(&src).unwrap();
    let r = run(&prog, "main").unwrap();
    assert_eq!(r.stats.in_place, 1);
    assert_eq!(r.stats.copies, 0);
    assert_eq!(r.stats.allocations, 1);
}

#[test]
fn shared_is_copy() {
    let src = r#"
        fn fill_shared(a: Array<Int>, x: Int) -> Array<Int> { set(a, 0, x) }
        fn main() -> Int {
          let a = [1, 2, 3];
          let b = fill_shared(a, 9);
          print(a);
          print(b);
          b[0]
        }
    "#;
    let prog = check_source(src).unwrap();
    let r = run(&prog, "main").unwrap();
    assert_eq!(r.stats.in_place, 0);
    assert_eq!(r.stats.copies, 1);
}

#[test]
fn rejects_use_after_move() {
    let src = format!(
        "{FILL}
         fn bad(own a: Array<Int>, x: Int) -> Int {{
           let b = fill(a, x);
           len(a)
         }}"
    );
    let err = check_source(&src).unwrap_err();
    assert!(err.contains("use of moved value"), "{err}");
}

#[test]
fn rejects_borrow_escape() {
    let src = r#"
        fn leak(borrow a: Array<Int>) -> Array<Int> { a }
    "#;
    let err = check_source(src).unwrap_err();
    assert!(err.contains("escapes"), "{err}");
}

#[test]
fn rejects_move_borrow() {
    let src = format!(
        "{FILL}
         fn bad(borrow a: Array<Int>, x: Int) -> Int {{
           let b = fill(a, x);
           len(b)
         }}"
    );
    let err = check_source(&src).unwrap_err();
    assert!(err.contains("cannot pass borrow"), "{err}");
}

#[test]
fn callbacks_and_closures_run() {
    let src = r#"
        fn apply_twice(f: Fn(Int) -> Int, x: Int) -> Int {
          f(f(x))
        }
        fn add_one(x: Int) -> Int { x + 1 }
        fn make_adder(n: Int) -> Fn(Int) -> Int {
          fn(y: Int) -> Int { y + n }
        }
        fn main() -> Int {
          let double = fn(y: Int) -> Int { y * 2 };
          let a = apply_twice(double, 5);
          let b = apply_twice(add_one, 1);
          let add10 = make_adder(10);
          let c = make_adder(3)(4);
          a + b + add10(7) + c
        }
    "#;
    let prog = check_source(src).unwrap();
    let r = run(&prog, "main").unwrap();
    assert_eq!(r.value.to_string(), "47");
}

#[test]
fn rejects_lambda_capturing_borrow() {
    let src = r#"
        fn bad(borrow a: Array<Int>) -> Fn(Int) -> Int {
          fn(i: Int) -> Int { a[i] }
        }
    "#;
    let err = check_source(src).unwrap_err();
    assert!(err.contains("may not capture borrow"), "{err}");
}

#[test]
fn rejects_stored_borrow() {
    let src = r#"
        fn bad(borrow a: Array<Int>) -> Array<Array<Int>> { [a] }
    "#;
    let err = check_source(src).unwrap_err();
    assert!(err.contains("escapes"), "{err}");
}
