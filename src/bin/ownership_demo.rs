//! Ownership prototype: `own` / `borrow` / inferred-unique.
//!
//! Run:
//!   cargo run --bin ownership_demo
//!   cargo run --bin ownership_demo -- examples/ownership.alt

use std::env;
use std::fs;

use alethe::ownership::{check_source, check_source_lisp, run};

fn is_lisp(path: &str) -> bool {
    path.ends_with(".lisp") || path.ends_with(".lsp") || path.ends_with(".sexp")
}

fn main() {
    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| "examples/ownership.alt".to_string());
    let src = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            std::process::exit(2);
        }
    };

    let checked = if is_lisp(&path) {
        check_source_lisp(&src)
    } else {
        check_source(&src)
    };
    let prog = match checked {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ownership check failed:\n{e}");
            std::process::exit(1);
        }
    };

    println!("ownership check passed");
    println!("functions:");
    for f in &prog.funcs {
        println!("  {}({}) -> {}", f.name, f.params.join(", "), f.ret);
    }

    let result = match run(&prog, "main") {
        Ok(r) => r,
        Err(e) => {
            eprintln!("runtime error: {e}");
            std::process::exit(1);
        }
    };

    println!("\nmain = {}", result.value);
    println!("stats:");
    println!("  allocations:   {}", result.stats.allocations);
    println!("  copies:        {}", result.stats.copies);
    println!("  in-place ops:  {}", result.stats.in_place);
    println!("  bounds checks: {}", result.stats.bounds_checks);
}
