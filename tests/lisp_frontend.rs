use alethe::ownership::{check_program, check_source, check_source_lisp, run};
use alethe::syntax::parse_lisp;
use alethe::vm::{compile, run as vm_run, verify};

const ALT: &str = include_str!("../examples/callbacks.alt");
const LISP: &str = include_str!("../examples/callbacks.lisp");

#[test]
fn lisp_and_alt_lower_to_the_same_program_shape() {
    let alt = alethe::syntax::parse(ALT).unwrap();
    let lisp = parse_lisp(LISP).unwrap();
    assert_eq!(alt.decls.len(), lisp.decls.len());
    let alt_names: Vec<_> = alt
        .decls
        .iter()
        .map(|d| match d {
            alethe::syntax::Decl::Fn(f) => f.name.clone(),
            _ => String::new(),
        })
        .collect();
    let lisp_names: Vec<_> = lisp
        .decls
        .iter()
        .map(|d| match d {
            alethe::syntax::Decl::Fn(f) => f.name.clone(),
            _ => String::new(),
        })
        .collect();
    assert_eq!(alt_names, lisp_names);
}

#[test]
fn lisp_callbacks_run_in_the_interpreter() {
    let prog = check_source_lisp(LISP).unwrap();
    let result = run(&prog, "main").unwrap();
    assert_eq!(result.value.to_string(), "47");
}

#[test]
fn lisp_callbacks_run_on_the_vm() {
    let prog = check_program(&parse_lisp(LISP).unwrap()).unwrap();
    let bc = compile(&prog).unwrap();
    verify(&bc).unwrap();
    let result = vm_run(&bc, bc.main).unwrap();
    assert_eq!(result.value.to_string(), "47");
}

#[test]
fn alt_and_lisp_agree() {
    let alt = check_source(ALT).unwrap();
    let lisp = check_source_lisp(LISP).unwrap();
    let a = run(&alt, "main").unwrap();
    let b = run(&lisp, "main").unwrap();
    assert_eq!(a.value.to_string(), b.value.to_string());
}
