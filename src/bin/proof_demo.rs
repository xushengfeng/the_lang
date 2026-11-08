//! Check surface `proof` declarations against the dependent kernel.
//!
//! The surface term language is explicit: no holes, metavariables, or tactic
//! elaboration.  The bridge resolves names and the kernel remains the judge.

use std::env;
use std::fs;

use alethe::dependent::Env;
use alethe::syntax::{parse, parse_lisp, Decl};

fn is_lisp(path: &str) -> bool {
    path.ends_with(".lisp") || path.ends_with(".lsp") || path.ends_with(".sexp")
}

fn main() {
    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| "examples/proofs.lisp".to_string());
    let src = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            std::process::exit(2);
        }
    };
    let parsed = if is_lisp(&path) {
        parse_lisp(&src)
    } else {
        parse(&src)
    };
    let prog = match parsed {
        Ok(p) => p,
        Err(e) => {
            eprintln!("parse error: {e}");
            std::process::exit(1);
        }
    };

    let mut kernel = match Env::new() {
        Ok(env) => env,
        Err(e) => {
            eprintln!("kernel construction failed:\n{e}");
            std::process::exit(1);
        }
    };

    let mut checked = 0usize;
    for decl in &prog.decls {
        match decl {
            Decl::Proof(p) => match kernel.add_surface_proof(p) {
                Ok(()) => {
                    checked += 1;
                    let ty = kernel
                        .type_of(&p.name)
                        .map(|t| format!(" : {t}"))
                        .unwrap_or_default();
                    println!("✓ {}{}", p.name, ty);
                }
                Err(e) => {
                    println!("✗ {}:\n{e}", p.name);
                    std::process::exit(1);
                }
            },
            Decl::Theorem(t) => {
                println!(
                    "· theorem {}: {} tactic(s) parsed; use `proof` for kernel-checked terms",
                    t.name,
                    t.tactics.len()
                );
            }
            Decl::Fn(_) | Decl::Spec(_) | Decl::Impl(_) => {}
        }
    }

    if checked == 0 {
        eprintln!("no `proof` declarations found in {path}");
        std::process::exit(1);
    }
    println!("\n{checked} proof(s) checked by the kernel");
}
