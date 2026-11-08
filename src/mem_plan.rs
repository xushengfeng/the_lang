//! A minimal memory planner and MemIR.
//!
//! Source terms carry `own` / `share` annotations.  The planner:
//!
//! * assigns every allocation to the current lexical region;
//! * moves unique values, downgrades shared ones, and copies when `own` meets
//!   a shared value;
//! * lowers `set` to in-place when the target is unique and to a copy
//!   otherwise;
//! * rejects use-after-consume and region escapes.
//!
//! The evaluator runs the resulting `MemExpr` on `region::RegionHeap`.
//! The surface bridge lowers `ownership::CheckedFn` / `CheckedProgram` into
//! MemIR; `validate` checks scope, types, and call arity before execution.
//!
//! This is the executable model for `docs/18-region-memory.md` and
//! `docs/19-architecture.md`, not a production backend.

pub mod bridge;
pub mod eval;
pub mod ir;
pub mod planner;
pub mod validate;

pub use bridge::*;
pub use eval::*;
pub use ir::*;
pub use planner::*;
pub use validate::*;

#[cfg(test)]
mod tests;
