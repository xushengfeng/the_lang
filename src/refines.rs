//! Minimal `refines` checker.
//!
//! This is a *prototype*: it checks
//!
//! 1. runtime signature compatibility;
//! 2. simple preconditions (`Unique(a)` requires `own`);
//! 3. the implementation body's spec expression equals the spec's
//!    `ensures` expression (after reification).
//!
//! It is not a kernel proof.  It is the minimal closed loop that lets
//! `spec` / `impl ... refines` participate in compile/verify/run.

mod check;
mod reify;
mod types;

pub use check::*;
pub use types::*;

#[cfg(test)]
mod tests;
