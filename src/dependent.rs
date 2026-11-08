//! A small dependently typed kernel, Lean-like in spirit.
//!
//! This is intentionally a *kernel*, not a full elaborator:
//!
//! * terms use de Bruijn indices;
//! * universes are `Prop` (`Sort 0`) and `Type u` (`Sort (u+1)`);
//! * the built-in inductive types are `Nat`, `List` and `Eq`;
//! * recursion is expressed through `Nat.rec` / `List.rec`;
//! * proofs are ordinary terms checked by the kernel.
//!
//! `Nat.recProp` and `List.recProp` are Prop-motive variants of the recursors.
//! They keep the prototype small without universe polymorphism.

mod builtins;
mod check;
mod debruijn;
mod env;
mod term;

pub use debruijn::instantiate;
pub use env::*;
pub use term::*;

#[cfg(test)]
mod tests;
