//! Minimal spec / impl / refines closed loop.
//!
//! source -> ownership check -> refines check -> bytecode -> verify -> VM

use std::env;
use std::fs;

use alethe::ownership::{check_source, check_source_lisp};
use alethe::refines::check_program as check_refines;
use alethe::refines_proof::ProofStore;
use alethe::syntax::{parse, parse_lisp};
use alethe::vm::{compile, run, verify};

fn is_lisp(path: &str) -> bool {
    path.ends_with(".lisp") || path.ends_with(".lsp") || path.ends_with(".sexp")
}

fn main() {
    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| "examples/refines.alt".to_string());
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
    let checked = if is_lisp(&path) {
        check_source_lisp(&src)
    } else {
        check_source(&src)
    };
    let checked = match checked {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ownership check failed:\n{e}");
            std::process::exit(1);
        }
    };

    let proofs = match ProofStore::build() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("proof store failed:\n{e}");
            std::process::exit(1);
        }
    };
    let reports = match check_refines(&prog, Some(&proofs)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("refines check failed:\n{e}");
            std::process::exit(1);
        }
    };
    println!("refines reports:");
    for r in &reports {
        if r.ok {
            match &r.kernel_proof {
                Some(p) => println!(
                    "  ✓ {} refines {}  (kernel: {})",
                    r.impl_name, r.spec_name, p
                ),
                None => println!("  ✓ {} refines {}", r.impl_name, r.spec_name),
            }
        } else {
            println!("  ✗ {} refines {}: {}", r.impl_name, r.spec_name, r.message);
        }
    }

    let bc = match compile(&checked) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("compile failed:\n{e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = verify(&bc) {
        eprintln!("bytecode verification failed:\n{e}");
        std::process::exit(1);
    }
    println!("bytecode verification passed");

    match run(&bc, bc.main) {
        Ok(r) => {
            println!("main = {}", r.value);
            println!("stats:");
            println!("  allocations:   {}", r.stats.allocations);
            println!("  copies:        {}", r.stats.copies);
            println!("  in-place ops:  {}", r.stats.in_place);
            println!("  bounds checks: {}", r.stats.bounds_checks);
        }
        Err(e) => {
            eprintln!("vm error: {e}");
            std::process::exit(1);
        }
    }
}
