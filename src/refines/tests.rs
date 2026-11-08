use super::*;
use crate::ownership::check_source;
use crate::refines_proof::ProofStore;
use crate::syntax::parse;

const SRC: &str = r#"
    spec fn fill_spec(own a: Array<Int>, x: Int) -> Array<Int>
      requires Unique(a)
      ensures set_pure(a, 0, x)

    impl fn fill(own a: Array<Int>, x: Int) -> Array<Int>
      refines fill_spec
    {
      set(a, 0, x)
    }

    spec fn fill_shared_spec(a: Array<Int>, x: Int) -> Array<Int>
      ensures set_pure(a, 0, x)

    impl fn fill_shared(a: Array<Int>, x: Int) -> Array<Int>
      refines fill_shared_spec
    {
      set(a, 0, x)
    }

    impl fn fill_bad(own a: Array<Int>, x: Int) -> Array<Int>
      refines fill_spec
    {
      set(a, 0, 0)
    }
"#;

#[test]
fn refines_accepts_and_rejects() {
    let prog = parse(SRC).unwrap();
    check_source(SRC).unwrap();
    let reports = check_program(&prog, None).unwrap();
    let get = |n: &str| reports.iter().find(|r| r.impl_name == n).unwrap();
    assert!(get("fill").ok);
    assert!(get("fill_shared").ok);
    assert!(!get("fill_bad").ok);
}

#[test]
fn refines_with_kernel_certificates() {
    let prog = parse(SRC).unwrap();
    let store = ProofStore::build().unwrap();
    let reports = check_program(&prog, Some(&store)).unwrap();
    let get = |n: &str| reports.iter().find(|r| r.impl_name == n).unwrap();
    let fill = get("fill");
    assert!(fill.ok);
    assert_eq!(fill.kernel_proof.as_deref(), Some("fill_refines_spec"));
    let shared = get("fill_shared");
    assert!(shared.ok);
    assert_eq!(
        shared.kernel_proof.as_deref(),
        Some("fill_shared_refines_spec")
    );
    assert!(!get("fill_bad").ok);
}

#[test]
fn unique_requires_own() {
    let src = r#"
        spec fn s(a: Array<Int>) -> Array<Int>
          requires Unique(a)
          ensures set_pure(a, 0, 1)
        impl fn i(a: Array<Int>) -> Array<Int>
          refines s
        { set(a, 0, 1) }
    "#;
    let prog = parse(src).unwrap();
    let reports = check_program(&prog, None).unwrap();
    assert!(!reports[0].ok);
    assert!(reports[0].message.contains("not `own`"));
}
