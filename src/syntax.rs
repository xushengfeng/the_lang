//! Surface syntax: lexer, AST, parser, and pretty printing.
//!
//! The prototype deliberately keeps the surface small.  The design documents
//! describe the full language; this module is the vertical slice that lets us
//! validate proof erasure and proof-directed optimisation end to end.

mod ast;
mod lexer;
mod parser;
mod pretty;

pub use ast::*;
pub use lexer::*;
pub use parser::*;
pub use pretty::*;
