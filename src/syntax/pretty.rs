use std::fmt;

use super::ast::*;

// ---------------------------------------------------------------------------
// Pretty printing (for --dump-ast)
// ---------------------------------------------------------------------------

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Var(s) => write!(f, "{s}"),
            Expr::Int(n) => write!(f, "{n}"),
            Expr::Bool(b) => write!(f, "{b}"),
            Expr::Unit => write!(f, "()"),
            Expr::Array(xs) => {
                write!(f, "[")?;
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{x}")?;
                }
                write!(f, "]")
            }
            Expr::Un(op, e) => write!(f, "{}{}", unop_str(*op), e),
            Expr::Bin(op, l, r) => write!(f, "({l} {} {r})", binop_str(*op)),
            Expr::If(c, t, e) => write!(f, "if {c} then {t} else {e}"),
            Expr::Let(n, v, b) => write!(f, "let {n} = {v} in {b}"),
            Expr::Call { name, args } => {
                write!(f, "{name}(")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    match a {
                        CallArg::Expr(e) => write!(f, "{e}")?,
                        CallArg::Proof(ts) => write!(f, "by {} tactic(s)", ts.len())?,
                    }
                }
                write!(f, ")")
            }
            Expr::Index(b, i) => write!(f, "{b}[{i}]"),
            Expr::Seq(a, b) => write!(f, "{a}; {b}"),
        }
    }
}

pub fn unop_str(op: UnOp) -> &'static str {
    match op {
        UnOp::Neg => "-",
        UnOp::Not => "!",
    }
}

pub fn binop_str(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Int => write!(f, "Int"),
            Ty::Bool => write!(f, "Bool"),
            Ty::Unit => write!(f, "Unit"),
            Ty::Prop => write!(f, "Prop"),
            Ty::Array(t) => write!(f, "Array<{t}>"),
        }
    }
}
