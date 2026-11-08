//! A small region-based memory model.
//!
//! There is no RC and no GC.  A region owns a set of blocks; freeing a region
//! frees all of its blocks at once.  `set` mutates in place when the caller has
//! proved uniqueness, and copies otherwise.
//!
//! The heap is deliberately abstract: `Loc` is `(region, block)`, not a machine
//! address.  This is the executable seed for the model in
//! `docs/18-region-memory.md`; it is not yet wired to the surface language or
//! the VM.

mod heap;
mod types;

pub use heap::*;
pub use types::*;

#[cfg(test)]
mod tests;
