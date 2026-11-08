use crate::ownership::{Stats, Value};

#[derive(Clone, Debug)]
pub enum Instr {
    PushInt(i64),
    PushBool(bool),
    PushUnit,
    Load(usize),
    Store(usize),
    Add,
    Sub,
    Mul,
    Div { checked: bool },
    Mod { checked: bool },
    Neg,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Not,
    Jump(usize),
    JumpIfFalse(usize),
    Call { func: usize, argc: usize },
    Ret,
    MakeArray(usize),
    ArrayGet { checked: bool },
    ArraySet { in_place: bool },
    ArrayPush { in_place: bool },
    Len,
    Print,
    Pop,
    Halt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BType {
    Int,
    Bool,
    Unit,
    Array(Box<BType>),
}

impl BType {
    pub fn from_ty(ty: &crate::syntax::Ty) -> Result<Self, String> {
        match ty {
            crate::syntax::Ty::Int => Ok(BType::Int),
            crate::syntax::Ty::Bool => Ok(BType::Bool),
            crate::syntax::Ty::Unit => Ok(BType::Unit),
            crate::syntax::Ty::Array(t) => Ok(BType::Array(Box::new(Self::from_ty(t)?))),
            crate::syntax::Ty::Prop => Err("Prop must be erased before bytecode".to_string()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct BcFunction {
    pub name: String,
    pub arity: usize,
    pub param_tys: Vec<BType>,
    pub ret: BType,
    pub nlocals: usize,
    pub code: Vec<Instr>,
}

#[derive(Clone, Debug)]
pub struct BcProgram {
    pub funcs: Vec<BcFunction>,
    pub main: usize,
}

impl BcProgram {
    pub fn get(&self, name: &str) -> Option<&BcFunction> {
        self.funcs.iter().find(|f| f.name == name)
    }
}

pub struct VmResult {
    pub value: Value,
    pub stats: Stats,
}
