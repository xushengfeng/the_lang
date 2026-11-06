//! Minimal `refines` checker.
//!
//! This is a *prototype*: it checks
//!
//! 1. runtime signature compatibility;
//! 2. simple preconditions (`Unique(a)` requires `own`);
//! 3. the implementation body's spec expression equals the spec's
//!    `ensures` expression (after reification).
//!
//! It is not a kernel proof.  It is the minimal closed loop that lets
//! `spec` / `impl ... refines` participate in compile/verify/run.

use std::collections::HashMap;

use crate::refines_proof::ProofStore;
use crate::syntax::{
    BinOp, CallArg, Decl, Expr, FnDecl, ImplDecl, Mode, PExpr, ParamKind, Program, SpecDecl, UnOp,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpecExpr {
    Var(String),
    Int(i64),
    Bool(bool),
    Array(Vec<SpecExpr>),
    Call(String, Vec<SpecExpr>),
}

impl SpecExpr {
    fn pretty(&self) -> String {
        match self {
            SpecExpr::Var(n) => n.clone(),
            SpecExpr::Int(n) => n.to_string(),
            SpecExpr::Bool(b) => b.to_string(),
            SpecExpr::Array(xs) => format!(
                "[{}]",
                xs.iter().map(|x| x.pretty()).collect::<Vec<_>>().join(", ")
            ),
            SpecExpr::Call(n, args) => format!(
                "{}({})",
                n,
                args.iter()
                    .map(|x| x.pretty())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RefinesReport {
    pub impl_name: String,
    pub spec_name: String,
    pub ok: bool,
    pub message: String,
    pub kernel_proof: Option<String>,
}

fn subst(e: &SpecExpr, name: &str, repl: &SpecExpr) -> SpecExpr {
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

fn reify_expr(e: &Expr) -> Result<SpecExpr, String> {
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

fn binop_name(op: BinOp) -> &'static str {
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

fn reify_pexpr(p: &PExpr) -> Result<SpecExpr, String> {
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
fn spec_result(p: &PExpr) -> Result<SpecExpr, String> {
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

fn runtime_params(f: &FnDecl) -> Vec<&crate::syntax::Param> {
    f.params
        .iter()
        .filter(|p| matches!(p.kind, ParamKind::Runtime(_)))
        .collect()
}

fn check_signature(spec: &SpecDecl, imp: &ImplDecl) -> Result<(), String> {
    let sp: Vec<_> = spec
        .params
        .iter()
        .filter(|p| matches!(p.kind, ParamKind::Runtime(_)))
        .collect();
    let ip = runtime_params(&imp.f);
    if sp.len() != ip.len() {
        return Err(format!(
            "spec has {} runtime params, impl has {}",
            sp.len(),
            ip.len()
        ));
    }
    for (s, i) in sp.iter().zip(&ip) {
        if s.name != i.name {
            return Err(format!("param name mismatch: `{}` vs `{}`", s.name, i.name));
        }
        match (&s.kind, &i.kind) {
            (ParamKind::Runtime(st), ParamKind::Runtime(it)) => {
                if st != it {
                    return Err(format!("param `{}` type `{it}`, expected `{st}`", i.name));
                }
            }
            _ => return Err("internal: non-runtime param".into()),
        }
        if s.mode != i.mode {
            return Err(format!(
                "param `{}` mode `{:?}`, expected `{:?}`",
                i.name, i.mode, s.mode
            ));
        }
    }
    if spec.ret != imp.f.ret {
        return Err(format!(
            "return type `{}`, expected `{}`",
            imp.f.ret, spec.ret
        ));
    }
    Ok(())
}

fn check_requires(spec: &SpecDecl, imp: &ImplDecl) -> Result<(), String> {
    for r in &spec.requires {
        match r {
            PExpr::Bool(true) => {}
            PExpr::Call(name, args) if name == "Unique" => {
                let param = match args.as_slice() {
                    [PExpr::Var(n)] => n,
                    _ => return Err("Unique(...) expects a variable".into()),
                };
                let p = imp
                    .f
                    .params
                    .iter()
                    .find(|p| &p.name == param)
                    .ok_or_else(|| format!("impl has no param `{param}`"))?;
                if p.mode != Mode::Own {
                    return Err(format!(
                        "spec requires Unique({param}), but impl param `{param}` is not `own`"
                    ));
                }
            }
            other => {
                let _ = other;
                return Err("prototype refines only supports `requires Unique(x)`".into());
            }
        }
    }
    Ok(())
}

fn check_ensures(spec: &SpecDecl, imp: &ImplDecl) -> Result<(), String> {
    if spec.ensures.len() != 1 {
        return Err("prototype refines expects exactly one `ensures`".into());
    }
    let want = spec_result(&spec.ensures[0])?;
    let got = reify_expr(&imp.f.body)?;
    if want == got {
        Ok(())
    } else {
        Err(format!(
            "body reifies to `{}`, spec requires `{}`",
            got.pretty(),
            want.pretty()
        ))
    }
}

pub fn check_program(
    prog: &Program,
    proofs: Option<&ProofStore>,
) -> Result<Vec<RefinesReport>, String> {
    let mut specs: HashMap<String, &SpecDecl> = HashMap::new();
    for d in &prog.decls {
        if let Decl::Spec(s) = d {
            specs.insert(s.name.clone(), s);
        }
    }

    let mut reports = Vec::new();
    for d in &prog.decls {
        let Decl::Impl(imp) = d else {
            continue;
        };
        let Some(spec) = specs.get(&imp.refines) else {
            return Err(format!(
                "impl `{}` refines unknown spec `{}`",
                imp.f.name, imp.refines
            ));
        };

        let mut kernel_proof = None;
        let result = check_signature(spec, imp)
            .and_then(|_| check_requires(spec, imp))
            .and_then(|_| check_ensures(spec, imp))
            .and_then(|_| {
                if let Some(store) = proofs {
                    let cert = certificate_for(&imp.f.name, &spec.name).ok_or_else(|| {
                        format!(
                            "no kernel certificate mapping for `{}` refines `{}`",
                            imp.f.name, spec.name
                        )
                    })?;
                    store.check(cert)?;
                    kernel_proof = Some(cert.to_string());
                }
                Ok(())
            });
        let (ok, message) = match result {
            Ok(()) => (true, "ok".to_string()),
            Err(e) => (false, e),
        };
        reports.push(RefinesReport {
            impl_name: imp.f.name.clone(),
            spec_name: spec.name.clone(),
            ok,
            message,
            kernel_proof,
        });
    }
    Ok(reports)
}

/// Manual certificate mapping for the prototype.  The next step is to
/// generate this obligation from the surface spec.
fn certificate_for(impl_name: &str, spec_name: &str) -> Option<&'static str> {
    match (impl_name, spec_name) {
        ("fill", "fill_spec") => Some("fill_refines_spec"),
        ("fill_shared", "fill_shared_spec") => Some("fill_shared_refines_spec"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ownership::check_source;
    use crate::syntax::parse;

    const SRC: &str = r#"
        spec fn fill_spec(own a: Array<Int>, x: Int) -> Array<Int>
          requires Unique(a)
          ensures set_pure(a, 0, x)

        impl fn fill(own a: Array<Int>, x: Int) -> Array<Int>
          refines fill_spec
        {
          set(a, 0, x)
        }

        spec fn fill_shared_spec(a: Array<Int>, x: Int) -> Array<Int>
          ensures set_pure(a, 0, x)

        impl fn fill_shared(a: Array<Int>, x: Int) -> Array<Int>
          refines fill_shared_spec
        {
          set(a, 0, x)
        }

        impl fn fill_bad(own a: Array<Int>, x: Int) -> Array<Int>
          refines fill_spec
        {
          set(a, 0, 0)
        }
    "#;

    #[test]
    fn refines_accepts_and_rejects() {
        let prog = parse(SRC).unwrap();
        check_source(SRC).unwrap();
        let reports = check_program(&prog, None).unwrap();
        let get = |n: &str| reports.iter().find(|r| r.impl_name == n).unwrap();
        assert!(get("fill").ok);
        assert!(get("fill_shared").ok);
        assert!(!get("fill_bad").ok);
    }

    #[test]
    fn refines_with_kernel_certificates() {
        let prog = parse(SRC).unwrap();
        let store = ProofStore::build().unwrap();
        let reports = check_program(&prog, Some(&store)).unwrap();
        let get = |n: &str| reports.iter().find(|r| r.impl_name == n).unwrap();
        let fill = get("fill");
        assert!(fill.ok);
        assert_eq!(fill.kernel_proof.as_deref(), Some("fill_refines_spec"));
        let shared = get("fill_shared");
        assert!(shared.ok);
        assert_eq!(
            shared.kernel_proof.as_deref(),
            Some("fill_shared_refines_spec")
        );
        assert!(!get("fill_bad").ok);
    }

    #[test]
    fn unique_requires_own() {
        let src = r#"
            spec fn s(a: Array<Int>) -> Array<Int>
              requires Unique(a)
              ensures set_pure(a, 0, 1)
            impl fn i(a: Array<Int>) -> Array<Int>
              refines s
            { set(a, 0, 1) }
        "#;
        let prog = parse(src).unwrap();
        let reports = check_program(&prog, None).unwrap();
        assert!(!reports[0].ok);
        assert!(reports[0].message.contains("not `own`"));
    }
}
