//! Stack VM demo: source -> ownership check/lowering -> bytecode -> VM.
//!
//! Run:
//!   cargo run --bin vm_demo
//!   cargo run --bin vm_demo -- examples/ownership.alt --dump

use std::env;
use std::fs;

use alethe::ownership::check_source;
use alethe::vm::{compile, disassemble, run};

fn main() {
    let mut args = env::args().skip(1);
    let path = args
        .next()
        .unwrap_or_else(|| "examples/ownership.alt".to_string());
    let dump = args.any(|a| a == "--dump");

    let src = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            std::process::exit(2);
        }
    };
    let checked = match check_source(&src) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ownership check failed:\n{e}");
            std::process::exit(1);
        }
    };
    let bc = match compile(&checked) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("compile failed:\n{e}");
            std::process::exit(1);
        }
    };

    if dump {
        println!("{}", disassemble(&bc));
    }

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
