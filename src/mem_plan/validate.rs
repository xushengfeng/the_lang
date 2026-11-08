//! Static validation for MemIR.
//!
//! MemIR is intentionally small, so a lightweight type/scope check catches
//! most mistakes before evaluation: unknown variables/functions, wrong call
//! arity, non-Bool conditions, non-Array targets, and mismatched branch types.
//! It is not a full ownership/region checker; those facts are produced by the
//! planner and surface checker.

use std::collections::HashMap;

use super::ir::{MemExpr, MemProgram};
use crate::syntax::Ty;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemType {
    Int,
    Bool,
    Unit,
    Array,
    Fun(Vec<MemType>, Box<MemType>),
    Thunk(Box<MemType>),
    Unknown,
}

impl std::fmt::Display for MemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemType::Int => write!(f, "Int"),
            MemType::Bool => write!(f, "Bool"),
            MemType::Unit => write!(f, "Unit"),
            MemType::Array => write!(f, "Array"),
            MemType::Fun(params, ret) => {
                write!(f, "Fn(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{p}")?;
                }
                write!(f, ") -> {ret}")
            }
            MemType::Thunk(inner) => write!(f, "Thunk({inner})"),
            MemType::Unknown => write!(f, "?"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidateError {
    UnknownVar(String),
    UnknownFunction(String),
    DuplicateFunction(String),
    DuplicateParam(String),
    BadArity {
        function: String,
        expected: usize,
        got: usize,
    },
    NotBool {
        what: &'static str,
    },
    NotArray {
        what: &'static str,
    },
    NotInt {
        what: &'static str,
    },
    NotCallable,
    NotThunk,
    IfBranchMismatch {
        then_ty: MemType,
        else_ty: MemType,
    },
    TypeMismatch {
        what: &'static str,
        expected: MemType,
        got: MemType,
    },
    ReturnTypeMismatch {
        function: String,
        expected: MemType,
        got: MemType,
    },
}

impl std::fmt::Display for ValidateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidateError::UnknownVar(name) => write!(f, "unknown variable `{name}`"),
            ValidateError::UnknownFunction(name) => write!(f, "unknown function `{name}`"),
            ValidateError::DuplicateFunction(name) => write!(f, "duplicate function `{name}`"),
            ValidateError::DuplicateParam(name) => write!(f, "duplicate parameter `{name}`"),
            ValidateError::BadArity {
                function,
                expected,
                got,
            } => write!(
                f,
                "function `{function}` expects {expected} arguments, got {got}"
            ),
            ValidateError::NotBool { what } => write!(f, "{what} must be Bool"),
            ValidateError::NotArray { what } => write!(f, "{what} must be Array"),
            ValidateError::NotInt { what } => write!(f, "{what} must be Int"),
            ValidateError::NotCallable => write!(f, "expression is not callable"),
            ValidateError::NotThunk => write!(f, "expression is not a thunk"),
            ValidateError::IfBranchMismatch { then_ty, else_ty } => {
                write!(
                    f,
                    "if branches have different types: {then_ty} vs {else_ty}"
                )
            }
            ValidateError::TypeMismatch {
                what,
                expected,
                got,
            } => write!(f, "{what} expects {expected}, got {got}"),
            ValidateError::ReturnTypeMismatch {
                function,
                expected,
                got,
            } => write!(
                f,
                "function `{function}` returns {got}, expected {expected}"
            ),
        }
    }
}

impl std::error::Error for ValidateError {}

pub fn validate_program(prog: &MemProgram) -> Result<(), ValidateError> {
    let mut seen = HashMap::new();
    for f in &prog.funcs {
        if seen.insert(f.name.clone(), ()).is_some() {
            return Err(ValidateError::DuplicateFunction(f.name.clone()));
        }
    }

    for f in &prog.funcs {
        let mut env = HashMap::new();
        for (name, ty) in f.params.iter().zip(&f.param_tys) {
            if env.insert(name.clone(), ty_to_mem(ty)).is_some() {
                return Err(ValidateError::DuplicateParam(name.clone()));
            }
        }
        let body_ty = infer(&f.body, prog, &env)?;
        let ret_ty = ty_to_mem(&f.ret);
        if body_ty != MemType::Unknown && ret_ty != MemType::Unknown && body_ty != ret_ty {
            return Err(ValidateError::ReturnTypeMismatch {
                function: f.name.clone(),
                expected: ret_ty,
                got: body_ty,
            });
        }
    }
    Ok(())
}

pub fn validate_expr(expr: &MemExpr, prog: &MemProgram) -> Result<(), ValidateError> {
    infer(expr, prog, &HashMap::new()).map(|_| ())
}

fn ty_to_mem(ty: &Ty) -> MemType {
    match ty {
        Ty::Int => MemType::Int,
        Ty::Bool => MemType::Bool,
        Ty::Unit => MemType::Unit,
        Ty::Array(_) => MemType::Array,
        Ty::Fn(params, ret) => MemType::Fun(
            params.iter().map(ty_to_mem).collect(),
            Box::new(ty_to_mem(ret)),
        ),
        Ty::Prop => MemType::Unknown,
    }
}

fn infer(
    expr: &MemExpr,
    prog: &MemProgram,
    env: &HashMap<String, MemType>,
) -> Result<MemType, ValidateError> {
    match expr {
        MemExpr::Int(_) => Ok(MemType::Int),
        MemExpr::Bool(_) => Ok(MemType::Bool),
        MemExpr::Unit => Ok(MemType::Unit),
        MemExpr::Var(name) => env
            .get(name)
            .cloned()
            .ok_or_else(|| ValidateError::UnknownVar(name.clone())),
        MemExpr::Call { name, args } => {
            let f = prog
                .get(name)
                .ok_or_else(|| ValidateError::UnknownFunction(name.clone()))?;
            if args.len() != f.params.len() {
                return Err(ValidateError::BadArity {
                    function: name.clone(),
                    expected: f.params.len(),
                    got: args.len(),
                });
            }
            for (index, (arg, ty)) in args.iter().zip(&f.param_tys).enumerate() {
                let got = infer(arg, prog, env)?;
                expect(got, ty_to_mem(ty), "call argument")?;
                let _ = index;
            }
            Ok(ty_to_mem(&f.ret))
        }
        MemExpr::FnRef(name) => {
            let f = prog
                .get(name)
                .ok_or_else(|| ValidateError::UnknownFunction(name.clone()))?;
            Ok(MemType::Fun(
                f.param_tys.iter().map(ty_to_mem).collect(),
                Box::new(ty_to_mem(&f.ret)),
            ))
        }
        MemExpr::Lambda { params, body } => {
            let mut inner = env.clone();
            for param in params {
                inner.insert(param.clone(), MemType::Unknown);
            }
            let body_ty = infer(body, prog, &inner)?;
            Ok(MemType::Fun(
                vec![MemType::Unknown; params.len()],
                Box::new(body_ty),
            ))
        }
        MemExpr::Apply { callee, args } => {
            let callee_ty = infer(callee, prog, env)?;
            let arg_tys = args
                .iter()
                .map(|arg| infer(arg, prog, env))
                .collect::<Result<Vec<_>, _>>()?;

            match callee_ty {
                MemType::Fun(params, ret) => {
                    if args.len() != params.len() {
                        return Err(ValidateError::BadArity {
                            function: "closure".into(),
                            expected: params.len(),
                            got: args.len(),
                        });
                    }
                    for (got, expected) in arg_tys.into_iter().zip(params) {
                        expect(got, expected, "closure argument")?;
                    }
                    Ok(*ret)
                }
                MemType::Unknown => Ok(MemType::Unknown),
                _ => Err(ValidateError::NotCallable),
            }
        }
        MemExpr::Thunk(body) => Ok(MemType::Thunk(Box::new(infer(body, prog, env)?))),
        MemExpr::Force(inner) => match infer(inner, prog, env)? {
            MemType::Thunk(inner) => Ok(*inner),
            MemType::Unknown => Ok(MemType::Unknown),
            _ => Err(ValidateError::NotThunk),
        },
        MemExpr::Alloc(items) => {
            for item in items {
                expect(infer(item, prog, env)?, MemType::Int, "array element")?;
            }
            Ok(MemType::Array)
        }
        MemExpr::Copy(inner) => {
            expect(infer(inner, prog, env)?, MemType::Array, "copy target")?;
            Ok(MemType::Array)
        }
        MemExpr::Let { name, value, body } => {
            let value_ty = infer(value, prog, env)?;
            let mut inner = env.clone();
            inner.insert(name.clone(), value_ty);
            infer(body, prog, &inner)
        }
        MemExpr::Set { target, value, .. } => {
            expect(infer(target, prog, env)?, MemType::Array, "set target")?;
            expect(infer(value, prog, env)?, MemType::Int, "set value")?;
            Ok(MemType::Array)
        }
        MemExpr::Push { target, value, .. } => {
            expect(infer(target, prog, env)?, MemType::Array, "push target")?;
            expect(infer(value, prog, env)?, MemType::Int, "push value")?;
            Ok(MemType::Array)
        }
        MemExpr::Get { target, .. } => {
            expect(infer(target, prog, env)?, MemType::Array, "get target")?;
            Ok(MemType::Int)
        }
        MemExpr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            expect(infer(cond, prog, env)?, MemType::Bool, "if condition")?;
            let then_ty = infer(then_branch, prog, env)?;
            let else_ty = infer(else_branch, prog, env)?;
            unify(then_ty, else_ty)
        }
        MemExpr::Len(inner) => {
            expect(infer(inner, prog, env)?, MemType::Array, "len target")?;
            Ok(MemType::Int)
        }
        MemExpr::Seq(first, second) => {
            infer(first, prog, env)?;
            infer(second, prog, env)
        }
        MemExpr::Region(body) => infer(body, prog, env),
        MemExpr::Return(inner) => infer(inner, prog, env),
    }
}

/// `Unknown` is a wildcard, including inside function and thunk types.  This
/// lets a typed callback parameter accept a lambda whose parameter types were
/// not recorded in MemIR.
fn compatible(got: &MemType, expected: &MemType) -> bool {
    match (got, expected) {
        (MemType::Unknown, _) | (_, MemType::Unknown) => true,
        (MemType::Int, MemType::Int)
        | (MemType::Bool, MemType::Bool)
        | (MemType::Unit, MemType::Unit)
        | (MemType::Array, MemType::Array) => true,
        (MemType::Fun(gp, gr), MemType::Fun(ep, er)) => {
            gp.len() == ep.len()
                && gp.iter().zip(ep).all(|(g, e)| compatible(g, e))
                && compatible(gr, er)
        }
        (MemType::Thunk(g), MemType::Thunk(e)) => compatible(g, e),
        _ => false,
    }
}

fn expect(got: MemType, expected: MemType, what: &'static str) -> Result<(), ValidateError> {
    if compatible(&got, &expected) {
        Ok(())
    } else {
        Err(ValidateError::TypeMismatch {
            what,
            expected,
            got,
        })
    }
}

fn unify(then_ty: MemType, else_ty: MemType) -> Result<MemType, ValidateError> {
    if then_ty == else_ty {
        Ok(then_ty)
    } else if then_ty == MemType::Unknown {
        Ok(else_ty)
    } else if else_ty == MemType::Unknown || compatible(&then_ty, &else_ty) {
        Ok(then_ty)
    } else {
        Err(ValidateError::IfBranchMismatch { then_ty, else_ty })
    }
}
