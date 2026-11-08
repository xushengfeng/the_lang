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
    /// A kernel-level theorem: an explicit dependent type and proof term.
    Proof(ProofDecl),
}

/// Surface syntax for a kernel term.  Names are resolved by the bridge: a name
/// bound by an enclosing `Lam`/`Pi`/`Let` is a variable, otherwise a constant.
#[derive(Clone, Debug)]
pub enum KTerm {
    Name(String),
    Sort(u32),
    Nat(u64),
    App(Box<KTerm>, Box<KTerm>),
    Lam(String, Box<KTerm>, Box<KTerm>),
    Pi(String, Box<KTerm>, Box<KTerm>),
    Let(String, Box<KTerm>, Box<KTerm>, Box<KTerm>),
}

#[derive(Clone, Debug)]
pub struct ProofDecl {
    pub name: String,
    pub ty: KTerm,
    pub body: KTerm,
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

#[derive(Clone, Debug)]
pub enum Ty {
    Int,
    Bool,
    Unit,
    Array(Box<Ty>),
    /// Runtime function value: `Fn(T1, T2) -> R`.
    ///
    /// The third component is a checker-only capture set: names of borrows
    /// captured by a closure value.  It is ignored by type equality, so a
    /// closure with borrow captures still has the same surface function type;
    /// escape checks inspect it explicitly.
    Fn(Vec<Ty>, Box<Ty>, Vec<String>),
    Prop,
}

impl PartialEq for Ty {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Ty::Int, Ty::Int)
            | (Ty::Bool, Ty::Bool)
            | (Ty::Unit, Ty::Unit)
            | (Ty::Prop, Ty::Prop) => true,
            (Ty::Array(a), Ty::Array(b)) => a == b,
            (Ty::Fn(pa, ra, _), Ty::Fn(pb, rb, _)) => pa == pb && ra == rb,
            _ => false,
        }
    }
}

impl Eq for Ty {}

impl Ty {
    /// Borrows captured by a closure value, or by a closure nested in an
    /// array element type.
    pub fn borrow_captures(&self) -> &[String] {
        match self {
            Ty::Fn(_, _, captures) => captures,
            _ => &[],
        }
    }

    pub fn captures_borrow(&self) -> bool {
        match self {
            Ty::Array(t) => t.captures_borrow(),
            Ty::Fn(_, _, captures) => !captures.is_empty(),
            _ => false,
        }
    }

    /// Merge checker-only capture sets when both branches of an `if` have the
    /// same surface type.
    pub fn merge_captures(&self, other: &Ty) -> Ty {
        match (self, other) {
            (Ty::Array(a), Ty::Array(b)) => Ty::Array(Box::new(a.merge_captures(b))),
            (Ty::Fn(pa, ra, ca), Ty::Fn(_, _, cb)) => {
                let mut captures = ca.clone();
                for c in cb {
                    if !captures.contains(c) {
                        captures.push(c.clone());
                    }
                }
                Ty::Fn(pa.clone(), ra.clone(), captures)
            }
            _ => self.clone(),
        }
    }
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
        /// Captured outer variables, filled in by the ownership checker.
        captures: Vec<String>,
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
