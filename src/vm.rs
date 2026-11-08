//! A small stack VM for the ownership-checked surface language.
//!
//! Pipeline:
//!   source -> parser -> ownership checker/lowering -> bytecode -> VM
//!
//! Proof parameters and proof arguments are already removed by the ownership
//! checker, so the VM never sees a proof value.  `set`/`push` arrive as
//! `__set_in_place` / `__set_copy` and are compiled to bytecode variants.
//!
//! This is a reference backend, not the production backend.

mod compile;
mod ir;
mod run;
mod verify;

pub use compile::*;
pub use ir::*;
pub use run::*;
pub use verify::*;

#[cfg(test)]
mod tests;
