use alethe::dependent::Env;
use alethe::syntax::{parse, parse_lisp, Decl, KTerm, Program, ProofDecl};

fn check_program(prog: &Program) -> Result<usize, String> {
    let mut env = Env::new().map_err(|e| e.to_string())?;
    let mut n = 0;
    for decl in &prog.decls {
        if let Decl::Proof(p) = decl {
            env.add_surface_proof(p)?;
            n += 1;
        }
    }
    Ok(n)
}

#[test]
fn lisp_proofs_are_kernel_checked() {
    let prog = parse_lisp(include_str!("../examples/proofs.lisp")).unwrap();
    assert_eq!(check_program(&prog).unwrap(), 5);
}

#[test]
fn alt_proofs_are_kernel_checked() {
    let prog = parse(include_str!("../examples/proofs.alt")).unwrap();
    assert_eq!(check_program(&prog).unwrap(), 5);
}

#[test]
fn bogus_proof_is_rejected() {
    let bad = ProofDecl {
        name: "bad".into(),
        ty: KTerm::App(
            Box::new(KTerm::App(
                Box::new(KTerm::App(
                    Box::new(KTerm::Name("Eq".into())),
                    Box::new(KTerm::Name("Nat".into())),
                )),
                Box::new(KTerm::Nat(0)),
            )),
            Box::new(KTerm::Nat(1)),
        ),
        body: KTerm::App(
            Box::new(KTerm::App(
                Box::new(KTerm::Name("Eq.refl".into())),
                Box::new(KTerm::Name("Nat".into())),
            )),
            Box::new(KTerm::Nat(0)),
        ),
    };
    let prog = Program {
        decls: vec![Decl::Proof(bad)],
    };
    let err = check_program(&prog).unwrap_err();
    assert!(err.contains("bad"), "{err}");
}

#[test]
fn proof_can_reuse_an_earlier_theorem() {
    let src = r#"
        (proof id_theorem
          (forall ((p Prop)) (forall ((h p)) p))
          (lambda ((p Prop)) (lambda ((h p)) h)))
        (proof id_again
          (forall ((p Prop)) (forall ((h p)) p))
          id_theorem)
    "#;
    let prog = parse_lisp(src).unwrap();
    assert_eq!(check_program(&prog).unwrap(), 2);
}
