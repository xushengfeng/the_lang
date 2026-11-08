//! Minimal formal models of unique ownership and in-place update.
//!
//! The models are deliberately tiny:
//!
//! * two locations, represented by `Bool.true` / `Bool.false`;
//! * a heap maps each location to an "array id" (`Bool`);
//! * a store maps each array id to an array (`List Nat`);
//! * `Unique(h)` means the two locations do not alias (different ids);
//! * `set_pure` returns the updated array and leaves the store unchanged;
//! * `set_in_place` updates the store at the first location.
//!
//! Checked theorems include:
//!
//! * `set_result_same`: both versions return the same array;
//! * `set_frame`: under `Unique(h)`, the second location observes the old
//!   array after an in-place update of the first;
//! * Nat-indexed generalizations of both;
//! * borrow / borrow_mut permission laws.
//!
//! This is not the full heap model. It is the smallest set of models that
//! makes aliasing/frame conditions non-trivial and kernel-checkable.

mod borrow;
mod common;
mod nat;
mod two_pos;

pub use borrow::build_borrow;
pub use nat::build_nat;
pub use two_pos::build;

#[cfg(test)]
mod tests;
