//! Kernel milestone: two implementations of `sum`, one checked equivalence proof.
//!
//! This binary deliberately stays at the kernel level: no surface parser, no
//! optimiser, no erasure.  It shows the trust anchor working:
//!
//! * `sum_ref` is structural recursion over `List Nat`;
//! * `sum_fast` is tail-recursive with an accumulator;
//! * `sum_fast_eq` proves `∀ xs, sum_fast xs = sum_ref xs`;
//! * a bogus proof is rejected by the kernel.

use alethe::dependent::{sapp, sapp2, sapp3, sconst, slam, spi, svar, term_size, Env, STerm};

fn main() {
    let mut env = match Env::new() {
        Ok(env) => env,
        Err(e) => {
            eprintln!("kernel construction failed:\n{e}");
            std::process::exit(1);
        }
    };

    println!("Alethe kernel demo");
    println!("==================");
    println!(
        "built: {} primitive type constructors, {} definitions\n",
        env.types.len(),
        env.defs.len()
    );

    println!("definitions");
    for name in ["sum_ref", "sum_fast"] {
        if let Some(ty) = env.type_of(name) {
            println!("  {name} : {}", env.pretty(ty));
        }
    }

    println!("\ncomputation (kernel evaluation)");
    let list = env.list_of(&[1, 2, 3, 4, 5]);
    let r = env.apply("sum_ref", &[list.clone()]);
    let f = env.apply("sum_fast", &[list.clone()]);
    println!(
        "  sum_ref  [1,2,3,4,5] = {}",
        env.eval_nat(&r)
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".into())
    );
    println!(
        "  sum_fast [1,2,3,4,5] = {}",
        env.eval_nat(&f)
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".into())
    );

    println!("\nproofs (kernel checked)");
    for name in ["add_assoc", "add_zero", "go_eq", "sum_fast_eq"] {
        match env.check_named(name) {
            Ok(()) => println!("  ✓ {name}"),
            Err(e) => {
                println!("  ✗ {name}:\n{e}");
                std::process::exit(1);
            }
        }
    }
    if let Some(ty) = env.type_of("sum_fast_eq") {
        println!("\n  sum_fast_eq : {}", env.pretty(ty));
    }
    let nodes: usize = ["add_assoc", "add_zero", "go_eq", "sum_fast_eq"]
        .iter()
        .filter_map(|n| env.defs.get(*n))
        .map(|d| term_size(&d.body))
        .sum();
    println!("  proof term nodes: {nodes}");

    println!("\nrejection test");
    let nat = sconst("Nat");
    let list_ty = sapp(sconst("List"), nat.clone());
    let eq = |a: STerm, b: STerm| sapp3(sconst("Eq"), nat.clone(), a, b);
    let sum_ref = |xs: STerm| sapp(sconst("sum_ref"), xs);
    let sum_fast = |xs: STerm| sapp(sconst("sum_fast"), xs);
    let bogus_ty = spi(
        "xs",
        list_ty.clone(),
        eq(sum_ref(svar("xs")), sum_fast(svar("xs"))),
    );
    let bogus_body = slam(
        "xs",
        list_ty,
        sapp2(sconst("Eq.refl"), nat.clone(), sum_ref(svar("xs"))),
    );
    match env.add_def("bogus", bogus_ty, bogus_body, false) {
        Ok(()) => {
            println!("  ✗ BUG: bogus proof accepted");
            std::process::exit(1);
        }
        Err(e) => {
            let first = e.lines().take(3).collect::<Vec<_>>().join("\n  ");
            println!("  ✓ bogus proof rejected:\n  {first}");
        }
    }

    println!("\nnote");
    println!("  proofs are kernel terms; runtime erasure is a separate pass.");
    println!("  sum_fast uses an accumulator; sum_fast_eq is the refinement proof.");
}
