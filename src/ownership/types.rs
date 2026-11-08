use crate::syntax::{Expr, Mode, Ty};

#[derive(Clone, Debug)]
pub struct ParamSig {
    pub name: String,
    pub ty: Ty,
    pub mode: Mode,
    pub is_proof: bool,
}

#[derive(Clone, Debug)]
pub struct FnSig {
    pub name: String,
    pub params: Vec<ParamSig>,
    pub ret: Ty,
}

#[derive(Clone, Debug)]
pub struct CheckedFn {
    pub name: String,
    pub params: Vec<String>,
    pub param_tys: Vec<Ty>,
    pub ret: Ty,
    pub body: Expr,
}

#[derive(Clone, Debug)]
pub struct CheckedProgram {
    pub funcs: Vec<CheckedFn>,
}

impl CheckedProgram {
    pub fn get(&self, name: &str) -> Option<&CheckedFn> {
        self.funcs.iter().find(|f| f.name == name)
    }
}
