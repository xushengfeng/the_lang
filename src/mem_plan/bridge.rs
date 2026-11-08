use std::collections::HashMap;

use super::eval::{eval, EvalError, MemRun, Value};
use super::ir::{MemExpr, MemFn, MemProgram};
use super::validate::{validate_program, ValidateError};
use crate::ownership::{CheckedFn, CheckedProgram};
use crate::region::{MemValue, RegionHeap, RegionId};
use crate::syntax::{CallArg, Expr, Ty};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BridgeError {
    Unsupported(String),
    NonConstantIndex,
    BadArity(&'static str),
    ProofArgument,
    Validation(ValidateError),
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BridgeError::Unsupported(what) => write!(f, "unsupported in MemIR: {what}"),
            BridgeError::NonConstantIndex => write!(f, "MemIR currently requires a constant index"),
            BridgeError::BadArity(op) => write!(f, "wrong number of arguments for `{op}`"),
            BridgeError::ProofArgument => write!(f, "proof arguments must be erased before MemIR"),
            BridgeError::Validation(e) => write!(f, "{e}"),
        }
    }
}

impl From<ValidateError> for BridgeError {
    fn from(e: ValidateError) -> Self {
        BridgeError::Validation(e)
    }
}

impl std::error::Error for BridgeError {}

// ---------------------------------------------------------------------------
// Surface bridge
// ---------------------------------------------------------------------------

/// Lower a checked surface function body to MemIR.
///
/// The ownership checker has already chosen `__set_in_place` / `__set_copy`
/// and `__push_in_place` / `__push_copy`, so this pass only maps operations;
/// it does not re-run the ownership analysis.
pub fn plan_checked_fn(f: &CheckedFn) -> Result<MemExpr, BridgeError> {
    lower_expr(&f.body)
}

/// Lower all checked functions to a multi-function MemProgram.
pub fn plan_checked_program(prog: &CheckedProgram) -> Result<MemProgram, BridgeError> {
    let mut funcs = Vec::with_capacity(prog.funcs.len());
    for f in &prog.funcs {
        funcs.push(MemFn {
            name: f.name.clone(),
            params: f.params.clone(),
            param_tys: f.param_tys.clone(),
            ret: f.ret.clone(),
            body: lower_expr(&f.body)?,
        });
    }
    let by_name = funcs
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.clone(), i))
        .collect();
    let program = MemProgram { funcs, by_name };
    validate_program(&program)?;
    Ok(program)
}

/// Run a program entry with explicit arguments in an existing heap/region.
pub fn run_program_in(
    prog: &MemProgram,
    entry: &str,
    args: &[MemValue],
    heap: &mut RegionHeap,
    root: RegionId,
) -> Result<MemValue, EvalError> {
    let f = prog
        .get(entry)
        .ok_or_else(|| EvalError::UnknownFunction(entry.to_string()))?;

    validate_program(prog)?;

    if args.len() != f.params.len() {
        return Err(EvalError::BadArgCount {
            expected: f.params.len(),
            got: args.len(),
        });
    }
    for (i, (arg, ty)) in args.iter().zip(&f.param_tys).enumerate() {
        if !data_matches_ty(arg, ty) {
            return Err(EvalError::ArgType {
                index: i,
                expected: ty.clone(),
            });
        }
    }

    let mut env = HashMap::new();
    for (name, arg) in f.params.iter().zip(args) {
        env.insert(name.clone(), Value::Data(arg.clone()));
    }
    eval(&f.body, heap, &mut env, root, prog)?.into_data()
}

/// Run a no-argument program entry, creating a fresh root region.
pub fn run_program(prog: &MemProgram, entry: &str) -> Result<MemRun, EvalError> {
    let mut heap = RegionHeap::new();
    let root = heap.new_region();
    let value = run_program_in(prog, entry, &[], &mut heap, root)?;
    Ok(MemRun { value, root, heap })
}

/// Run a checked surface function in an existing heap/region.
///
/// Array arguments must already live in `heap`.  The caller owns `root` and
/// can free it after the result is no longer needed.
pub fn run_fn_in(
    f: &CheckedFn,
    args: &[MemValue],
    heap: &mut RegionHeap,
    root: RegionId,
) -> Result<MemValue, EvalError> {
    if args.len() != f.params.len() {
        return Err(EvalError::BadArgCount {
            expected: f.params.len(),
            got: args.len(),
        });
    }
    for (i, (arg, ty)) in args.iter().zip(&f.param_tys).enumerate() {
        if !data_matches_ty(arg, ty) {
            return Err(EvalError::ArgType {
                index: i,
                expected: ty.clone(),
            });
        }
    }

    let body = plan_checked_fn(f)?;
    let mut env = HashMap::new();
    for (name, arg) in f.params.iter().zip(args) {
        env.insert(name.clone(), Value::Data(arg.clone()));
    }
    let program = MemProgram::default();
    eval(&body, heap, &mut env, root, &program)?.into_data()
}

fn data_matches_ty(value: &MemValue, ty: &Ty) -> bool {
    matches!(
        (value, ty),
        (MemValue::Int(_), Ty::Int)
            | (MemValue::Bool(_), Ty::Bool)
            | (MemValue::Unit, Ty::Unit)
            | (MemValue::Loc(_), Ty::Array(_))
    )
}

pub(crate) fn value_matches_ty(value: &Value, ty: &Ty) -> bool {
    match (value, ty) {
        (Value::Data(value), ty) => data_matches_ty(value, ty),
        (Value::Closure(_), Ty::Fn(..)) => true,
        _ => false,
    }
}

fn lower_expr(expr: &Expr) -> Result<MemExpr, BridgeError> {
    match expr {
        Expr::Int(n) => Ok(MemExpr::Int(*n)),
        Expr::Bool(b) => Ok(MemExpr::Bool(*b)),
        Expr::Unit => Ok(MemExpr::Unit),
        Expr::Var(name) => Ok(MemExpr::Var(name.clone())),
        Expr::Array(items) => {
            let mut planned = Vec::with_capacity(items.len());
            for item in items {
                planned.push(lower_expr(item)?);
            }
            Ok(MemExpr::Alloc(planned))
        }
        Expr::Let(name, value, body) => Ok(MemExpr::Let {
            name: name.clone(),
            value: Box::new(lower_expr(value)?),
            body: Box::new(lower_expr(body)?),
        }),
        Expr::Seq(first, second) => Ok(MemExpr::Seq(
            Box::new(lower_expr(first)?),
            Box::new(lower_expr(second)?),
        )),
        Expr::Index(base, index) => Ok(MemExpr::Get {
            target: Box::new(lower_expr(base)?),
            index: const_index(index)?,
        }),
        Expr::Call { name, args } => lower_call(name, args),
        Expr::FnRef(name) => Ok(MemExpr::FnRef(name.clone())),
        Expr::Lambda { params, body, .. } => Ok(MemExpr::Lambda {
            params: params.iter().map(|p| p.name.clone()).collect(),
            body: Box::new(lower_expr(body)?),
        }),
        Expr::Apply { callee, args } => Ok(MemExpr::Apply {
            callee: Box::new(lower_expr(callee)?),
            args: args.iter().map(lower_expr).collect::<Result<Vec<_>, _>>()?,
        }),
        Expr::If(cond, then_branch, else_branch) => Ok(MemExpr::If {
            cond: Box::new(lower_expr(cond)?),
            then_branch: Box::new(lower_expr(then_branch)?),
            else_branch: Box::new(lower_expr(else_branch)?),
            optimized: true,
        }),
        Expr::Un(..) => Err(BridgeError::Unsupported("unary expression".into())),
        Expr::Bin(..) => Err(BridgeError::Unsupported("binary expression".into())),
    }
}

fn lower_call(name: &str, args: &[CallArg]) -> Result<MemExpr, BridgeError> {
    let args = expr_args(args)?;
    match name {
        "__set_in_place" | "__set_copy" => {
            if args.len() != 3 {
                return Err(BridgeError::BadArity("set"));
            }
            Ok(MemExpr::Set {
                target: Box::new(lower_expr(args[0])?),
                index: const_index(args[1])?,
                value: Box::new(lower_expr(args[2])?),
                in_place: name == "__set_in_place",
            })
        }
        "__push_in_place" | "__push_copy" => {
            if args.len() != 2 {
                return Err(BridgeError::BadArity("push"));
            }
            Ok(MemExpr::Push {
                target: Box::new(lower_expr(args[0])?),
                value: Box::new(lower_expr(args[1])?),
                in_place: name == "__push_in_place",
            })
        }
        "len" => {
            if args.len() != 1 {
                return Err(BridgeError::BadArity("len"));
            }
            Ok(MemExpr::Len(Box::new(lower_expr(args[0])?)))
        }
        _ => Ok(MemExpr::Call {
            name: name.to_string(),
            args: args
                .into_iter()
                .map(lower_expr)
                .collect::<Result<Vec<_>, _>>()?,
        }),
    }
}

fn expr_args(args: &[CallArg]) -> Result<Vec<&Expr>, BridgeError> {
    args.iter()
        .map(|arg| match arg {
            CallArg::Expr(e) => Ok(e),
            CallArg::Proof(_) => Err(BridgeError::ProofArgument),
        })
        .collect()
}

fn const_index(expr: &Expr) -> Result<usize, BridgeError> {
    match expr {
        Expr::Int(n) if *n >= 0 => Ok(*n as usize),
        _ => Err(BridgeError::NonConstantIndex),
    }
}
