//! Ownership / uniqueness prototype.
//!
//! This is a separate layer from the dependent kernel: it checks the surface
//! language's `own` / `borrow` / inferred-unique modes, lowers `set`/`push` to
//! in-place or copy operations, and runs the result in a tiny interpreter.
//!
//! It is deliberately not a full type system.  The goal is to make the
//! ownership rules executable and testable before they are connected to the
//! proof kernel.

mod checker;
mod interp;
mod types;

pub use checker::*;
pub use interp::*;
pub use types::*;

#[cfg(test)]
mod tests;
