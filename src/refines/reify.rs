use super::types::SpecExpr;
use crate::syntax::{BinOp, CallArg, Expr, FnDecl, PExpr, ParamKind, UnOp};

pub(super) fn subst(e: &SpecExpr, name: &str, repl: &SpecExpr) -> SpecExpr {
    match e {
        SpecExpr::Var(n) if n == name => repl.clone(),
        SpecExpr::Var(_) | SpecExpr::Int(_) | SpecExpr::Bool(_) => e.clone(),
        SpecExpr::Array(xs) => SpecExpr::Array(xs.iter().map(|x| subst(x, name, repl)).collect()),
        SpecExpr::Call(n, args) => SpecExpr::Call(
            n.clone(),
            args.iter().map(|x| subst(x, name, repl)).collect(),
        ),
    }
}

pub(super) fn reify_expr(e: &Expr) -> Result<SpecExpr, String> {
    match e {
        Expr::Var(n) => Ok(SpecExpr::Var(n.clone())),
        Expr::Int(n) => Ok(SpecExpr::Int(*n)),
        Expr::Bool(b) => Ok(SpecExpr::Bool(*b)),
        Expr::Unit => Ok(SpecExpr::Call("unit".into(), vec![])),
        Expr::Array(xs) => Ok(SpecExpr::Array(
            xs.iter().map(reify_expr).collect::<Result<Vec<_>, _>>()?,
        )),
        Expr::Un(op, x) => Ok(SpecExpr::Call(
            match op {
                UnOp::Neg => "neg".into(),
                UnOp::Not => "not".into(),
            },
            vec![reify_expr(x)?],
        )),
        Expr::Bin(op, a, b) => Ok(SpecExpr::Call(
            binop_name(*op).to_string(),
            vec![reify_expr(a)?, reify_expr(b)?],
        )),
        Expr::Let(n, v, b) => {
            let v = reify_expr(v)?;
            let b = reify_expr(b)?;
            Ok(subst(&b, n, &v))
        }
        Expr::Call { name, args } => {
            let mut out = Vec::new();
            for a in args {
                match a {
                    CallArg::Expr(e) => out.push(reify_expr(e)?),
                    CallArg::Proof(_) => {}
                }
            }
            let n = match name.as_str() {
                "set" | "__set_in_place" | "__set_copy" => "set_pure",
                "push" | "__push_in_place" | "__push_copy" => "push_pure",
                other => other,
            };
            Ok(SpecExpr::Call(n.to_string(), out))
        }
        Expr::Index(a, i) => Ok(SpecExpr::Call(
            "get".into(),
            vec![reify_expr(a)?, reify_expr(i)?],
        )),
        Expr::If(_, _, _) | Expr::Seq(_, _) => {
            Err("if/seq bodies are not supported by the prototype refines checker".into())
        }
    }
}

pub(super) fn binop_name(op: BinOp) -> &'static str {
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

pub(super) fn reify_pexpr(p: &PExpr) -> Result<SpecExpr, String> {
    match p {
        PExpr::Int(n) => Ok(SpecExpr::Int(*n)),
        PExpr::Bool(b) => Ok(SpecExpr::Bool(*b)),
        PExpr::Var(n) => Ok(SpecExpr::Var(n.clone())),
        PExpr::Len(x) => Ok(SpecExpr::Call("len".into(), vec![reify_pexpr(x)?])),
        PExpr::Array(xs) => Ok(SpecExpr::Array(
            xs.iter().map(reify_pexpr).collect::<Result<Vec<_>, _>>()?,
        )),
        PExpr::Un(op, x) => Ok(SpecExpr::Call(
            match op {
                UnOp::Neg => "neg".into(),
                UnOp::Not => "not".into(),
            },
            vec![reify_pexpr(x)?],
        )),
        PExpr::Bin(op, a, b) | PExpr::Cmp(op, a, b) => Ok(SpecExpr::Call(
            binop_name(*op).to_string(),
            vec![reify_pexpr(a)?, reify_pexpr(b)?],
        )),
        PExpr::Call(name, args) => Ok(SpecExpr::Call(
            name.clone(),
            args.iter()
                .map(reify_pexpr)
                .collect::<Result<Vec<_>, _>>()?,
        )),
        PExpr::And(_, _)
        | PExpr::Or(_, _)
        | PExpr::Not(_)
        | PExpr::Imp(_, _)
        | PExpr::Forall(_, _, _)
        | PExpr::Exists(_, _, _) => {
            Err("complex propositions are not supported by the prototype refines checker".into())
        }
    }
}

/// `ensures result == expr` or `ensures expr` -> `expr`.
pub(super) fn spec_result(p: &PExpr) -> Result<SpecExpr, String> {
    match p {
        PExpr::Cmp(BinOp::Eq, lhs, rhs) => {
            if matches!(lhs.as_ref(), PExpr::Var(n) if n == "result") {
                reify_pexpr(rhs)
            } else if matches!(rhs.as_ref(), PExpr::Var(n) if n == "result") {
                reify_pexpr(lhs)
            } else {
                reify_pexpr(p)
            }
        }
        _ => reify_pexpr(p),
    }
}

pub(super) fn runtime_params(f: &FnDecl) -> Vec<&crate::syntax::Param> {
    f.params
        .iter()
        .filter(|p| matches!(p.kind, ParamKind::Runtime(_)))
        .collect()
}
