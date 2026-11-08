// ---------------------------------------------------------------------------
// AST
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Program {
    pub decls: Vec<Decl>,
}

#[derive(Clone, Debug)]
pub enum Decl {
    Fn(FnDecl),
    Spec(SpecDecl),
    Impl(ImplDecl),
    Theorem(ThmDecl),
}

#[derive(Clone, Debug)]
pub struct SpecDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Ty,
    pub requires: Vec<PExpr>,
    pub ensures: Vec<PExpr>,
}

#[derive(Clone, Debug)]
pub struct ImplDecl {
    pub f: FnDecl,
    pub refines: String,
}

#[derive(Clone, Debug)]
pub struct FnDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Ty,
    pub requires: Vec<PExpr>,
    pub ensures: Vec<PExpr>,
    pub body: Expr,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub mode: Mode,
    pub kind: ParamKind,
    pub refinement: Option<PExpr>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Val,
    Own,
    Borrow,
}

#[derive(Clone, Debug)]
pub enum ParamKind {
    Runtime(Ty),
    Proof(PExpr),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ty {
    Int,
    Bool,
    Unit,
    Array(Box<Ty>),
    /// Runtime function value: `Fn(T1, T2) -> R`.
    Fn(Vec<Ty>, Box<Ty>),
    Prop,
}

#[derive(Clone, Debug)]
pub struct LambdaParam {
    pub name: String,
    pub ty: Ty,
}

#[derive(Clone, Debug)]
pub struct ThmDecl {
    pub name: String,
    pub prop: PExpr,
    pub tactics: Vec<Tactic>,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Var(String),
    Int(i64),
    Bool(bool),
    Unit,
    Array(Vec<Expr>),
    Un(UnOp, Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Let(String, Box<Expr>, Box<Expr>),
    Call {
        name: String,
        args: Vec<CallArg>,
    },
    /// Reference to a top-level function used as a value.
    FnRef(String),
    /// Anonymous function that captures its environment.
    Lambda {
        params: Vec<LambdaParam>,
        ret: Ty,
        body: Box<Expr>,
    },
    /// Apply a function value (not a statically known function name).
    Apply {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Index(Box<Expr>, Box<Expr>),
    Seq(Box<Expr>, Box<Expr>),
}

#[derive(Clone, Debug)]
pub enum CallArg {
    Expr(Expr),
    Proof(Vec<Tactic>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Clone, Debug)]
pub enum Tactic {
    Intro(Option<String>),
    Exact(ProofExpr),
    Assumption,
    Apply(ProofExpr),
    Split,
    Left,
    Right,
    Rfl,
    Trivial,
    Decide,
    Contradiction,
}

#[derive(Clone, Debug)]
pub enum ProofExpr {
    Var(String),
    App(Box<ProofExpr>, Box<ProofExpr>),
}

#[derive(Clone, Debug)]
pub enum PExpr {
    Int(i64),
    Bool(bool),
    Var(String),
    Len(Box<PExpr>),
    Array(Vec<PExpr>),
    Un(UnOp, Box<PExpr>),
    Bin(BinOp, Box<PExpr>, Box<PExpr>),
    Cmp(BinOp, Box<PExpr>, Box<PExpr>),
    And(Box<PExpr>, Box<PExpr>),
    Or(Box<PExpr>, Box<PExpr>),
    Not(Box<PExpr>),
    Imp(Box<PExpr>, Box<PExpr>),
    Forall(String, SortAst, Box<PExpr>),
    Exists(String, SortAst, Box<PExpr>),
    Call(String, Vec<PExpr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortAst {
    Int,
    Bool,
    Array,
}
