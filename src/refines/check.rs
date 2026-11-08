use std::collections::HashMap;

use super::reify::*;
use super::types::*;
use crate::refines_proof::ProofStore;
use crate::syntax::{Decl, ImplDecl, Mode, PExpr, ParamKind, Program, SpecDecl};

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
