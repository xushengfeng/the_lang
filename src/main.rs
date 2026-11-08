use alethe::syntax;

use std::env;
use std::fs;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: alethe <file.alt>");
        process::exit(2);
    }
    let src = match fs::read_to_string(&args[1]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {}: {e}", args[1]);
            process::exit(2);
        }
    };
    let is_lisp =
        args[1].ends_with(".lisp") || args[1].ends_with(".lsp") || args[1].ends_with(".sexp");
    let parsed = if is_lisp {
        syntax::parse_lisp(&src)
    } else {
        syntax::parse(&src)
    };
    match parsed {
        Ok(prog) => {
            for d in &prog.decls {
                match d {
                    syntax::Decl::Fn(f) => {
                        println!(
                            "fn {}({} params) -> {} = {}",
                            f.name,
                            f.params.len(),
                            f.ret,
                            f.body
                        )
                    }
                    syntax::Decl::Spec(s) => {
                        println!("spec fn {}({} params) -> {}", s.name, s.params.len(), s.ret)
                    }
                    syntax::Decl::Impl(i) => println!(
                        "impl fn {}({} params) -> {} refines {}",
                        i.f.name,
                        i.f.params.len(),
                        i.f.ret,
                        i.refines
                    ),
                    syntax::Decl::Theorem(t) => {
                        println!("theorem {}: {} tactic(s)", t.name, t.tactics.len())
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("parse error: {e}");
            process::exit(1);
        }
    }
}
