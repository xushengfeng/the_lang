//! Machine-checked ownership models.
//!
//! Two models:
//!   Bool2 : two locations, two array ids (minimal case split)
//!   Nat   : arbitrary locations/ids, frame proved by induction
//!
//! Both are checked by `proofcheck` (the kernel).  No runtime verification.

use alethe::dependent::term_size;
use alethe::ownership_proof::build_borrow;

fn show(env: &alethe::dependent::Env, title: &str, names: &[&str]) {
    println!("\n{title}");
    println!("{}", "=".repeat(title.len()));
    for name in names {
        match env.check_named(name) {
            Ok(()) => {
                let ty = env.type_of(name).map(|t| env.pretty(t)).unwrap_or_default();
                println!("  ✓ {name}");
                println!("    {ty}");
                if let Some(d) = env.defs.get(*name) {
                    println!("    proof nodes: {}", term_size(&d.body));
                }
            }
            Err(e) => {
                println!("  ✗ {name}:\n{e}");
                std::process::exit(1);
            }
        }
    }
}

fn main() {
    let env = match build_borrow() {
        Ok(env) => env,
        Err(e) => {
            eprintln!("model construction failed:\n{e}");
            std::process::exit(1);
        }
    };

    println!("ownership ProofModel (checked by proofcheck)");
    println!("============================================");
    println!("Bool2: Loc = Bool, Aid = Bool, Heap = Loc -> Aid");
    println!("Nat  : Loc = Nat,  Aid = Nat,  Heap = Loc -> Aid");
    println!("Store s : Aid -> Array;  get s h l = s (h l)");
    println!("Unique h := ∀ i j, i ≠ j -> h i ≠ h j");
    println!("set_pure returns the updated array; set_in_place updates the store.");
    println!("Perm p : Loc -> Nat; 0=free, 1=shared borrow, 2=mutable borrow.");

    show(
        &env,
        "Bool2 model (two locations)",
        &["set_result_same", "set_frame"],
    );

    show(
        &env,
        "Nat model (arbitrary locations, induction)",
        &["set_result_same_nat", "set_frame_nat"],
    );

    show(
        &env,
        "Borrow permissions (0=free, 1=shared, 2=mutable)",
        &[
            "borrow_not_mut",
            "borrow_not_free",
            "borrow_mut_not_free",
            "borrow_can_read",
            "borrow_mut_can_read",
            "borrow_cannot_write",
            "borrow_mut_can_write",
        ],
    );

    println!("\nmeaning");
    println!("  set_*_same: in-place and pure return the same updated array.");
    println!("  set_*_frame: under Unique, updating one location is invisible");
    println!("               to every other location.");
    println!("\nchecked at proof/compile time; the VM does not verify at runtime.");
}
