use std::collections::HashMap;

use crate::syntax::Ty;

// ---------------------------------------------------------------------------
// Source
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindMode {
    Own,
    Share,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SrcExpr {
    Int(i64),
    Var(String),
    Array(Vec<SrcExpr>),
    Let {
        name: String,
        mode: BindMode,
        value: Box<SrcExpr>,
        body: Box<SrcExpr>,
    },
    Set {
        target: Box<SrcExpr>,
        index: usize,
        value: Box<SrcExpr>,
    },
    Get {
        target: Box<SrcExpr>,
        index: usize,
    },
    Region(Box<SrcExpr>),
    Return(Box<SrcExpr>),
}

// ---------------------------------------------------------------------------
// Planned IR
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemExpr {
    Int(i64),
    Bool(bool),
    Unit,
    Var(String),
    Call {
        name: String,
        args: Vec<MemExpr>,
    },
    /// Reference to a top-level function as a value.
    FnRef(String),
    Lambda {
        params: Vec<String>,
        body: Box<MemExpr>,
        /// Captured outer variables.  The evaluator clones the environment;
        /// validation uses this set to keep closures lexical.
        captures: Vec<String>,
    },
    Apply {
        callee: Box<MemExpr>,
        args: Vec<MemExpr>,
    },
    Thunk(Box<MemExpr>),
    Force(Box<MemExpr>),
    Alloc(Vec<MemExpr>),
    Copy(Box<MemExpr>),
    Let {
        name: String,
        value: Box<MemExpr>,
        body: Box<MemExpr>,
    },
    Set {
        target: Box<MemExpr>,
        index: usize,
        value: Box<MemExpr>,
        in_place: bool,
    },
    Push {
        target: Box<MemExpr>,
        value: Box<MemExpr>,
        in_place: bool,
    },
    Get {
        target: Box<MemExpr>,
        index: usize,
    },
    If {
        cond: Box<MemExpr>,
        then_branch: Box<MemExpr>,
        else_branch: Box<MemExpr>,
        /// `true`: primitive branch, zero thunk allocation.
        /// `false`: construct both branch thunks, force the selected one.
        optimized: bool,
    },
    Len(Box<MemExpr>),
    Seq(Box<MemExpr>, Box<MemExpr>),
    Region(Box<MemExpr>),
    Return(Box<MemExpr>),
}

// ---------------------------------------------------------------------------
// Multi-function program
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct MemFn {
    pub name: String,
    pub params: Vec<String>,
    pub param_tys: Vec<Ty>,
    pub ret: Ty,
    pub body: MemExpr,
}

#[derive(Clone, Debug, Default)]
pub struct MemProgram {
    pub funcs: Vec<MemFn>,
    pub(crate) by_name: HashMap<String, usize>,
}

impl MemProgram {
    pub fn from_funcs(funcs: Vec<MemFn>) -> Self {
        let by_name = funcs
            .iter()
            .enumerate()
            .map(|(i, f)| (f.name.clone(), i))
            .collect();
        Self { funcs, by_name }
    }

    pub fn get(&self, name: &str) -> Option<&MemFn> {
        self.by_name.get(name).map(|&i| &self.funcs[i])
    }
}
