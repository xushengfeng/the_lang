//! Minimal machine-checked model: unique ownership and in-place update.
//!
//! Model:
//!   two locations (Bool.true / Bool.false)
//!   heap  : location -> aid
//!   store : aid -> Array
//!   Unique h := h true != h false
//!   set_pure      returns the updated array, leaves the store unchanged
//!   set_in_place  updates the store at the first location
//!
//! Checked:
//!   set_result_same : both versions return the same array
//!   set_frame       : under Unique h, the second location is unchanged

use alethe::dependent::term_size;
use alethe::ownership_proof::build;

fn main() {
    let env = match build() {
        Ok(env) => env,
        Err(e) => {
            eprintln!("model construction failed:\n{e}");
            std::process::exit(1);
        }
    };

    println!("minimal ownership model (two locations)");
    println!("========================================");
    println!("heap  : Bool -> Bool       -- location -> array id");
    println!("store : Bool -> List Nat   -- array id -> array");
    println!("Unique h := h true != h false\n");

    for name in ["Unique", "set_pure", "set_in_place"] {
        if let Some(ty) = env.type_of(name) {
            println!("{name} : {}", env.pretty(ty));
        }
    }

    println!("\nchecked theorems");
    for name in ["set_result_same", "set_frame"] {
        match env.check_named(name) {
            Ok(()) => {
                let ty = env.type_of(name).map(|t| env.pretty(t)).unwrap_or_default();
                println!("  ✓ {name}");
                println!("    {ty}");
                if let Some(d) = env.defs.get(name) {
                    println!("    proof nodes: {}", term_size(&d.body));
                }
            }
            Err(e) => {
                println!("  ✗ {name}:\n{e}");
                std::process::exit(1);
            }
        }
    }

    println!("\nmeaning");
    println!("  set_result_same: in-place and pure return the same updated array.");
    println!("  set_frame:       if the two locations do not alias, updating the");
    println!("                   first is invisible to the second.");
}
