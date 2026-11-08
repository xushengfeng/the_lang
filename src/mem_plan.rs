//! A minimal memory planner and MemIR.
//!
//! Source terms carry `own` / `share` annotations.  The planner:
//!
//! * assigns every allocation to the current lexical region;
//! * moves unique values, downgrades shared ones, and copies when `own` meets
//!   a shared value;
//! * lowers `set` to in-place when the target is unique and to a copy
//!   otherwise;
//! * rejects use-after-consume and region escapes.
//!
//! The evaluator runs the resulting `MemExpr` on `region::RegionHeap`.
//! It is deliberately small: arrays of `Int`, no functions, no loops, no
//! closures.  This is the first executable slice of `docs/18-region-memory.md`.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ownership::{CheckedFn, CheckedProgram};
use crate::region::{Loc, MemError, MemStats, MemValue, Observation, RegionHeap, RegionId};
use crate::syntax::{CallArg, Expr, Ty};

// ---------------------------------------------------------------------------
// Source
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindMode {
    Own,
    Share,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SrcExpr {
    Int(i64),
    Var(String),
    Array(Vec<SrcExpr>),
    Let {
        name: String,
        mode: BindMode,
        value: Box<SrcExpr>,
        body: Box<SrcExpr>,
    },
    Set {
        target: Box<SrcExpr>,
        index: usize,
        value: Box<SrcExpr>,
    },
    Get {
        target: Box<SrcExpr>,
        index: usize,
    },
    Region(Box<SrcExpr>),
    Return(Box<SrcExpr>),
}

// ---------------------------------------------------------------------------
// Planned IR
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemExpr {
    Int(i64),
    Bool(bool),
    Unit,
    Var(String),
    Call {
        name: String,
        args: Vec<MemExpr>,
    },
    Lambda {
        params: Vec<String>,
        body: Box<MemExpr>,
    },
    Apply {
        callee: Box<MemExpr>,
        args: Vec<MemExpr>,
    },
    Thunk(Box<MemExpr>),
    Force(Box<MemExpr>),
    Alloc(Vec<MemExpr>),
    Copy(Box<MemExpr>),
    Let {
        name: String,
        value: Box<MemExpr>,
        body: Box<MemExpr>,
    },
    Set {
        target: Box<MemExpr>,
        index: usize,
        value: Box<MemExpr>,
        in_place: bool,
    },
    Push {
        target: Box<MemExpr>,
        value: Box<MemExpr>,
        in_place: bool,
    },
    Get {
        target: Box<MemExpr>,
        index: usize,
    },
    If {
        cond: Box<MemExpr>,
        then_branch: Box<MemExpr>,
        else_branch: Box<MemExpr>,
        /// `true`: primitive branch, zero thunk allocation.
        /// `false`: construct both branch thunks, force the selected one.
        optimized: bool,
    },
    Len(Box<MemExpr>),
    Seq(Box<MemExpr>, Box<MemExpr>),
    Region(Box<MemExpr>),
    Return(Box<MemExpr>),
}

// ---------------------------------------------------------------------------
// Multi-function program
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct MemFn {
    pub name: String,
    pub params: Vec<String>,
    pub param_tys: Vec<Ty>,
    pub body: MemExpr,
}

#[derive(Clone, Debug, Default)]
pub struct MemProgram {
    pub funcs: Vec<MemFn>,
    by_name: HashMap<String, usize>,
}

impl MemProgram {
    pub fn get(&self, name: &str) -> Option<&MemFn> {
        self.by_name.get(name).map(|&i| &self.funcs[i])
    }
}

// ---------------------------------------------------------------------------
// Runtime values
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Value {
    Data(MemValue),
    Closure(Rc<Closure>),
    Thunk(Rc<Thunk>),
}

#[derive(Clone, Debug)]
struct Closure {
    params: Vec<String>,
    body: MemExpr,
    env: HashMap<String, Value>,
}

#[derive(Clone, Debug)]
struct Thunk {
    body: MemExpr,
    env: HashMap<String, Value>,
}

impl Value {
    fn into_data(self) -> Result<MemValue, EvalError> {
        match self {
            Value::Data(value) => Ok(value),
            _ => Err(EvalError::NotData),
        }
    }

    fn as_loc(&self) -> Result<Loc, EvalError> {
        match self {
            Value::Data(MemValue::Loc(loc)) => Ok(*loc),
            _ => Err(EvalError::NotALoc),
        }
    }

    fn as_bool(&self) -> Result<bool, EvalError> {
        match self {
            Value::Data(MemValue::Bool(b)) => Ok(*b),
            _ => Err(EvalError::NotABool),
        }
    }
}

// ---------------------------------------------------------------------------
// Planner
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValType {
    Int,
    Array,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ownership {
    Unique,
    Shared,
    Consumed,
}

#[derive(Clone, Copy, Debug)]
struct VarState {
    ty: ValType,
    ownership: Ownership,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanError {
    UnknownVar(String),
    UseAfterConsume(String),
    NotAnArray,
    NotAnInt,
    BadArrayElement,
    RegionEscape,
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::UnknownVar(name) => write!(f, "unknown variable `{name}`"),
            PlanError::UseAfterConsume(name) => write!(f, "use after consume `{name}`"),
            PlanError::NotAnArray => write!(f, "expected an array"),
            PlanError::NotAnInt => write!(f, "expected an Int"),
            PlanError::BadArrayElement => write!(f, "array elements must be Int"),
            PlanError::RegionEscape => {
                write!(
                    f,
                    "a region body may not return an array (escape analysis not implemented)"
                )
            }
        }
    }
}

impl std::error::Error for PlanError {}

#[derive(Default)]
struct PlanEnv {
    vars: HashMap<String, VarState>,
}

impl PlanEnv {
    fn get(&self, name: &str) -> Result<VarState, PlanError> {
        let state = self
            .vars
            .get(name)
            .ok_or_else(|| PlanError::UnknownVar(name.to_string()))?;
        if state.ownership == Ownership::Consumed {
            return Err(PlanError::UseAfterConsume(name.to_string()));
        }
        Ok(*state)
    }

    fn consume(&mut self, name: &str) -> Result<(), PlanError> {
        let state = self
            .vars
            .get_mut(name)
            .ok_or_else(|| PlanError::UnknownVar(name.to_string()))?;
        if state.ownership == Ownership::Consumed {
            return Err(PlanError::UseAfterConsume(name.to_string()));
        }
        state.ownership = Ownership::Consumed;
        Ok(())
    }

    fn share(&mut self, name: &str) -> Result<(), PlanError> {
        let state = self
            .vars
            .get_mut(name)
            .ok_or_else(|| PlanError::UnknownVar(name.to_string()))?;
        if state.ownership == Ownership::Consumed {
            return Err(PlanError::UseAfterConsume(name.to_string()));
        }
        state.ownership = Ownership::Shared;
        Ok(())
    }
}

struct Planned {
    expr: MemExpr,
    ty: ValType,
    ownership: Ownership,
}

pub fn plan(src: &SrcExpr) -> Result<MemExpr, PlanError> {
    let mut env = PlanEnv::default();
    Ok(plan_expr(src, &mut env)?.expr)
}

fn plan_expr(src: &SrcExpr, env: &mut PlanEnv) -> Result<Planned, PlanError> {
    match src {
        SrcExpr::Int(n) => Ok(Planned {
            expr: MemExpr::Int(*n),
            ty: ValType::Int,
            ownership: Ownership::Shared,
        }),
        SrcExpr::Var(name) => {
            let state = env.get(name)?;
            Ok(Planned {
                expr: MemExpr::Var(name.clone()),
                ty: state.ty,
                ownership: state.ownership,
            })
        }
        SrcExpr::Array(items) => {
            let mut planned = Vec::with_capacity(items.len());
            for item in items {
                let p = plan_expr(item, env)?;
                if p.ty != ValType::Int {
                    return Err(PlanError::BadArrayElement);
                }
                planned.push(p.expr);
            }
            Ok(Planned {
                expr: MemExpr::Alloc(planned),
                ty: ValType::Array,
                ownership: Ownership::Unique,
            })
        }
        SrcExpr::Let {
            name,
            mode,
            value,
            body,
        } => {
            let value_plan = plan_expr(value, env)?;
            let binding = match mode {
                BindMode::Own => match (value_plan.ty, value_plan.ownership) {
                    (ValType::Array, Ownership::Unique) => {
                        if let SrcExpr::Var(old) = value.as_ref() {
                            env.consume(old)?;
                        }
                        VarState {
                            ty: ValType::Array,
                            ownership: Ownership::Unique,
                        }
                    }
                    (ValType::Array, Ownership::Shared) => {
                        // `own` of a shared value: copy, leave the original shared.
                        VarState {
                            ty: ValType::Array,
                            ownership: Ownership::Unique,
                        }
                    }
                    (ValType::Array, Ownership::Consumed) => {
                        unreachable!("planned value cannot be consumed")
                    }
                    _ => VarState {
                        ty: value_plan.ty,
                        ownership: Ownership::Shared,
                    },
                },
                BindMode::Share => {
                    if value_plan.ty == ValType::Array {
                        if let SrcExpr::Var(old) = value.as_ref() {
                            if env.get(old)?.ownership == Ownership::Unique {
                                env.share(old)?;
                            }
                        }
                    }
                    VarState {
                        ty: value_plan.ty,
                        ownership: Ownership::Shared,
                    }
                }
            };

            let value_expr = match (mode, value_plan.ty, value_plan.ownership) {
                (BindMode::Own, ValType::Array, Ownership::Shared) => {
                    MemExpr::Copy(Box::new(value_plan.expr))
                }
                _ => value_plan.expr,
            };

            let old = env.vars.insert(name.clone(), binding);
            let body_result = plan_expr(body, env);
            match old {
                Some(old) => {
                    env.vars.insert(name.clone(), old);
                }
                None => {
                    env.vars.remove(name);
                }
            }
            let body_plan = body_result?;
            Ok(Planned {
                expr: MemExpr::Let {
                    name: name.clone(),
                    value: Box::new(value_expr),
                    body: Box::new(body_plan.expr),
                },
                ty: body_plan.ty,
                ownership: body_plan.ownership,
            })
        }
        SrcExpr::Set {
            target,
            index,
            value,
        } => {
            let target_plan = plan_expr(target, env)?;
            if target_plan.ty != ValType::Array {
                return Err(PlanError::NotAnArray);
            }
            let value_plan = plan_expr(value, env)?;
            if value_plan.ty != ValType::Int {
                return Err(PlanError::NotAnInt);
            }

            let in_place = target_plan.ownership == Ownership::Unique;
            if in_place {
                if let SrcExpr::Var(name) = target.as_ref() {
                    env.consume(name)?;
                }
            }

            Ok(Planned {
                expr: MemExpr::Set {
                    target: Box::new(target_plan.expr),
                    index: *index,
                    value: Box::new(value_plan.expr),
                    in_place,
                },
                ty: ValType::Array,
                ownership: Ownership::Unique,
            })
        }
        SrcExpr::Get { target, index } => {
            let target_plan = plan_expr(target, env)?;
            if target_plan.ty != ValType::Array {
                return Err(PlanError::NotAnArray);
            }
            Ok(Planned {
                expr: MemExpr::Get {
                    target: Box::new(target_plan.expr),
                    index: *index,
                },
                ty: ValType::Int,
                ownership: Ownership::Shared,
            })
        }
        SrcExpr::Region(body) => {
            let body_plan = plan_expr(body, env)?;
            if body_plan.ty == ValType::Array {
                return Err(PlanError::RegionEscape);
            }
            Ok(Planned {
                expr: MemExpr::Region(Box::new(body_plan.expr)),
                ty: body_plan.ty,
                ownership: body_plan.ownership,
            })
        }
        SrcExpr::Return(inner) => {
            let inner_plan = plan_expr(inner, env)?;
            Ok(Planned {
                expr: MemExpr::Return(Box::new(inner_plan.expr)),
                ty: inner_plan.ty,
                ownership: inner_plan.ownership,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Evaluator
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BridgeError {
    Unsupported(String),
    NonConstantIndex,
    BadArity(&'static str),
    ProofArgument,
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BridgeError::Unsupported(what) => write!(f, "unsupported in MemIR: {what}"),
            BridgeError::NonConstantIndex => write!(f, "MemIR currently requires a constant index"),
            BridgeError::BadArity(op) => write!(f, "wrong number of arguments for `{op}`"),
            BridgeError::ProofArgument => write!(f, "proof arguments must be erased before MemIR"),
        }
    }
}

impl std::error::Error for BridgeError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvalError {
    Mem(MemError),
    Bridge(BridgeError),
    UnknownVar(String),
    UnknownFunction(String),
    NotALoc,
    NotABool,
    NotData,
    NotAClosure,
    NotAThunk,
    BadClosureArity { expected: usize, got: usize },
    ClosureEscapesCallee,
    EscapingClosure,
    BadArgCount { expected: usize, got: usize },
    ArgType { index: usize, expected: Ty },
    EscapingRegion(RegionId),
}

impl From<MemError> for EvalError {
    fn from(e: MemError) -> Self {
        EvalError::Mem(e)
    }
}

impl From<BridgeError> for EvalError {
    fn from(e: BridgeError) -> Self {
        EvalError::Bridge(e)
    }
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalError::Mem(e) => write!(f, "{e}"),
            EvalError::Bridge(e) => write!(f, "{e}"),
            EvalError::UnknownVar(name) => write!(f, "unknown variable `{name}`"),
            EvalError::UnknownFunction(name) => write!(f, "unknown function `{name}`"),
            EvalError::NotALoc => write!(f, "expected an array location"),
            EvalError::NotABool => write!(f, "expected a Bool condition"),
            EvalError::NotData => write!(f, "expected a data value, got a function/thunk"),
            EvalError::NotAClosure => write!(f, "expected a closure"),
            EvalError::NotAThunk => write!(f, "expected a thunk"),
            EvalError::BadClosureArity { expected, got } => {
                write!(f, "closure expected {expected} arguments, got {got}")
            }
            EvalError::ClosureEscapesCallee => {
                write!(f, "closure/thunk may not escape its callee region")
            }
            EvalError::EscapingClosure => {
                write!(f, "closure/thunk may not escape its region")
            }
            EvalError::BadArgCount { expected, got } => {
                write!(f, "expected {expected} arguments, got {got}")
            }
            EvalError::ArgType { index, expected } => {
                write!(f, "argument {index} does not have type `{expected:?}`")
            }
            EvalError::EscapingRegion(r) => {
                write!(f, "value escapes region {}", r.0)
            }
        }
    }
}

impl std::error::Error for EvalError {}

pub struct MemRun {
    pub value: MemValue,
    pub root: RegionId,
    pub heap: RegionHeap,
}

impl MemRun {
    pub fn observe(&self) -> Result<Observation, MemError> {
        self.heap.observe(&[self.value.clone()])
    }

    pub fn stats(&self) -> MemStats {
        self.heap.stats()
    }
}

pub fn run(plan: &MemExpr) -> Result<MemRun, EvalError> {
    let mut heap = RegionHeap::new();
    let root = heap.new_region();
    let mut env = HashMap::new();
    let program = MemProgram::default();
    let value = eval(plan, &mut heap, &mut env, root, &program)?.into_data()?;
    Ok(MemRun { value, root, heap })
}

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
            body: lower_expr(&f.body)?,
        });
    }
    let by_name = funcs
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.clone(), i))
        .collect();
    Ok(MemProgram { funcs, by_name })
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
    match (value, ty) {
        (MemValue::Int(_), Ty::Int) => true,
        (MemValue::Bool(_), Ty::Bool) => true,
        (MemValue::Unit, Ty::Unit) => true,
        (MemValue::Loc(_), Ty::Array(_)) => true,
        _ => false,
    }
}

fn value_matches_ty(value: &Value, ty: &Ty) -> bool {
    match value {
        Value::Data(value) => data_matches_ty(value, ty),
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

fn eval(
    expr: &MemExpr,
    heap: &mut RegionHeap,
    env: &mut HashMap<String, Value>,
    region: RegionId,
    program: &MemProgram,
) -> Result<Value, EvalError> {
    match expr {
        MemExpr::Int(n) => Ok(Value::Data(MemValue::Int(*n))),
        MemExpr::Bool(b) => Ok(Value::Data(MemValue::Bool(*b))),
        MemExpr::Unit => Ok(Value::Data(MemValue::Unit)),
        MemExpr::Var(name) => env
            .get(name)
            .cloned()
            .ok_or_else(|| EvalError::UnknownVar(name.clone())),
        MemExpr::Call { name, args } => eval_call(name, args, heap, env, region, program),
        MemExpr::Lambda { params, body } => {
            heap.record_closures(1);
            Ok(Value::Closure(Rc::new(Closure {
                params: params.clone(),
                body: (**body).clone(),
                env: env.clone(),
            })))
        }
        MemExpr::Apply { callee, args } => {
            let callee = eval(callee, heap, env, region, program)?;
            let mut values = Vec::with_capacity(args.len());
            for arg in args {
                values.push(eval(arg, heap, env, region, program)?);
            }
            apply_value(callee, values, heap, region, program)
        }
        MemExpr::Thunk(body) => {
            heap.record_thunks(1);
            Ok(Value::Thunk(Rc::new(Thunk {
                body: (**body).clone(),
                env: env.clone(),
            })))
        }
        MemExpr::Force(inner) => {
            let value = eval(inner, heap, env, region, program)?;
            force_value(value, heap, region, program)
        }
        MemExpr::Alloc(items) => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                values.push(eval(item, heap, env, region, program)?.into_data()?);
            }
            Ok(Value::Data(MemValue::Loc(heap.alloc(region, values)?)))
        }
        MemExpr::Copy(inner) => {
            let loc = eval(inner, heap, env, region, program)?.as_loc()?;
            Ok(Value::Data(MemValue::Loc(heap.copy(loc, region)?)))
        }
        MemExpr::Let { name, value, body } => {
            let value = eval(value, heap, env, region, program)?;
            let old = env.insert(name.clone(), value);
            let result = eval(body, heap, env, region, program);
            match old {
                Some(old) => {
                    env.insert(name.clone(), old);
                }
                None => {
                    env.remove(name);
                }
            }
            result
        }
        MemExpr::Set {
            target,
            index,
            value,
            in_place,
        } => {
            let target = eval(target, heap, env, region, program)?.as_loc()?;
            let value = eval(value, heap, env, region, program)?.into_data()?;
            let loc = if *in_place {
                heap.set_in_place(target, *index, value)?
            } else {
                heap.set_copy(target, *index, value)?
            };
            Ok(Value::Data(MemValue::Loc(loc)))
        }
        MemExpr::Push {
            target,
            value,
            in_place,
        } => {
            let target = eval(target, heap, env, region, program)?.as_loc()?;
            let value = eval(value, heap, env, region, program)?.into_data()?;
            let loc = if *in_place {
                heap.push_in_place(target, value)?
            } else {
                heap.push_copy(target, value)?
            };
            Ok(Value::Data(MemValue::Loc(loc)))
        }
        MemExpr::Get { target, index } => {
            let target = eval(target, heap, env, region, program)?.as_loc()?;
            Ok(Value::Data(heap.load(target, *index)?))
        }
        MemExpr::If {
            cond,
            then_branch,
            else_branch,
            optimized,
        } => {
            let cond = eval(cond, heap, env, region, program)?.as_bool()?;

            if *optimized {
                return if cond {
                    eval(then_branch, heap, env, region, program)
                } else {
                    eval(else_branch, heap, env, region, program)
                };
            }

            // Fallback: construct both branch thunks, then force the selected
            // one.  The thunks are real values now, but they do not escape.
            let then_thunk = Value::Thunk(Rc::new(Thunk {
                body: (**then_branch).clone(),
                env: env.clone(),
            }));
            let else_thunk = Value::Thunk(Rc::new(Thunk {
                body: (**else_branch).clone(),
                env: env.clone(),
            }));
            heap.record_thunks(2);
            if cond {
                force_value(then_thunk, heap, region, program)
            } else {
                force_value(else_thunk, heap, region, program)
            }
        }
        MemExpr::Len(inner) => {
            let loc = eval(inner, heap, env, region, program)?.as_loc()?;
            Ok(Value::Data(MemValue::Int(heap.len(loc)? as i64)))
        }
        MemExpr::Region(body) => {
            let inner = heap.new_region();
            let result = match eval(body, heap, env, inner, program) {
                Ok(v) => v,
                Err(e) => {
                    let _ = heap.free_region(inner);
                    return Err(e);
                }
            };
            let data = match &result {
                Value::Data(value) => value,
                _ => {
                    let _ = heap.free_region(inner);
                    return Err(EvalError::EscapingClosure);
                }
            };
            if value_refers_to_region(heap, data, inner)? {
                let _ = heap.free_region(inner);
                return Err(EvalError::EscapingRegion(inner));
            }
            heap.free_region(inner)?;
            Ok(result)
        }
        MemExpr::Seq(first, second) => {
            eval(first, heap, env, region, program)?;
            eval(second, heap, env, region, program)
        }
        MemExpr::Return(inner) => eval(inner, heap, env, region, program),
    }
}

fn eval_call(
    name: &str,
    args: &[MemExpr],
    heap: &mut RegionHeap,
    caller_env: &mut HashMap<String, Value>,
    caller_region: RegionId,
    program: &MemProgram,
) -> Result<Value, EvalError> {
    let callee = program
        .get(name)
        .ok_or_else(|| EvalError::UnknownFunction(name.to_string()))?;

    if args.len() != callee.params.len() {
        return Err(EvalError::BadArgCount {
            expected: callee.params.len(),
            got: args.len(),
        });
    }

    let mut values = Vec::with_capacity(args.len());
    for arg in args {
        values.push(eval(arg, heap, caller_env, caller_region, program)?);
    }
    for (i, (value, ty)) in values.iter().zip(&callee.param_tys).enumerate() {
        if !value_matches_ty(value, ty) {
            return Err(EvalError::ArgType {
                index: i,
                expected: ty.clone(),
            });
        }
    }

    let callee_region = heap.new_region();
    let mut callee_env = HashMap::new();
    for (param, value) in callee.params.iter().zip(values) {
        callee_env.insert(param.clone(), value);
    }

    let result = match eval(&callee.body, heap, &mut callee_env, callee_region, program) {
        Ok(value) => value,
        Err(e) => {
            let _ = heap.free_region(callee_region);
            return Err(e);
        }
    };

    // A value allocated by the callee is copied into the caller's region.
    // This is the copy fallback; region promotion can replace it later.
    let result = match result {
        Value::Data(MemValue::Loc(loc)) if loc.region == callee_region => {
            match heap.copy(loc, caller_region) {
                Ok(new_loc) => Value::Data(MemValue::Loc(new_loc)),
                Err(e) => {
                    let _ = heap.free_region(callee_region);
                    return Err(EvalError::Mem(e));
                }
            }
        }
        Value::Data(_) => result,
        _ => {
            let _ = heap.free_region(callee_region);
            return Err(EvalError::ClosureEscapesCallee);
        }
    };

    heap.free_region(callee_region)?;
    Ok(result)
}

fn apply_value(
    callee: Value,
    args: Vec<Value>,
    heap: &mut RegionHeap,
    region: RegionId,
    program: &MemProgram,
) -> Result<Value, EvalError> {
    match callee {
        Value::Closure(closure) => {
            if args.len() != closure.params.len() {
                return Err(EvalError::BadClosureArity {
                    expected: closure.params.len(),
                    got: args.len(),
                });
            }
            let mut env = closure.env.clone();
            for (param, arg) in closure.params.iter().zip(args) {
                env.insert(param.clone(), arg);
            }
            eval(&closure.body, heap, &mut env, region, program)
        }
        Value::Thunk(thunk) => {
            if !args.is_empty() {
                return Err(EvalError::BadClosureArity {
                    expected: 0,
                    got: args.len(),
                });
            }
            force_thunk(thunk.as_ref(), heap, region, program)
        }
        Value::Data(_) => Err(EvalError::NotAClosure),
    }
}

fn force_value(
    value: Value,
    heap: &mut RegionHeap,
    region: RegionId,
    program: &MemProgram,
) -> Result<Value, EvalError> {
    match value {
        Value::Thunk(thunk) => force_thunk(thunk.as_ref(), heap, region, program),
        _ => Err(EvalError::NotAThunk),
    }
}

fn force_thunk(
    thunk: &Thunk,
    heap: &mut RegionHeap,
    region: RegionId,
    program: &MemProgram,
) -> Result<Value, EvalError> {
    let mut env = thunk.env.clone();
    eval(&thunk.body, heap, &mut env, region, program)
}

fn value_refers_to_region(
    heap: &RegionHeap,
    value: &MemValue,
    region: RegionId,
) -> Result<bool, MemError> {
    fn go(
        heap: &RegionHeap,
        value: &MemValue,
        region: RegionId,
        seen: &mut HashSet<Loc>,
    ) -> Result<bool, MemError> {
        match value {
            MemValue::Loc(loc) => {
                if loc.region == region {
                    return Ok(true);
                }
                if !seen.insert(*loc) {
                    return Ok(false);
                }
                for item in heap.block(*loc)? {
                    if go(heap, item, region, seen)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    go(heap, value, region, &mut HashSet::new())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn arr(xs: &[i64]) -> SrcExpr {
        SrcExpr::Array(xs.iter().map(|n| SrcExpr::Int(*n)).collect())
    }

    fn var(name: &str) -> SrcExpr {
        SrcExpr::Var(name.to_string())
    }

    fn own(name: &str, value: SrcExpr, body: SrcExpr) -> SrcExpr {
        SrcExpr::Let {
            name: name.to_string(),
            mode: BindMode::Own,
            value: Box::new(value),
            body: Box::new(body),
        }
    }

    fn share(name: &str, value: SrcExpr, body: SrcExpr) -> SrcExpr {
        SrcExpr::Let {
            name: name.to_string(),
            mode: BindMode::Share,
            value: Box::new(value),
            body: Box::new(body),
        }
    }

    fn set(name: &str, index: usize, value: i64) -> SrcExpr {
        SrcExpr::Set {
            target: Box::new(var(name)),
            index,
            value: Box::new(SrcExpr::Int(value)),
        }
    }

    fn get(name: &str, index: usize) -> SrcExpr {
        SrcExpr::Get {
            target: Box::new(var(name)),
            index,
        }
    }

    fn run_src(src: &SrcExpr) -> MemRun {
        let plan = plan(src).unwrap();
        run(&plan).unwrap()
    }

    #[test]
    fn unique_set_is_in_place() {
        let src = own("a", arr(&[1, 2, 3]), own("b", set("a", 1, 9), get("b", 1)));

        let result = run_src(&src);
        assert_eq!(result.value, MemValue::Int(9));
        assert_eq!(result.stats().allocations, 1);
        assert_eq!(result.stats().in_place, 1);
        assert_eq!(result.stats().copies, 0);
    }

    #[test]
    fn shared_set_copies_result() {
        let src = own(
            "a",
            arr(&[1, 2, 3]),
            share("s", var("a"), own("b", set("s", 1, 9), get("b", 1))),
        );

        let result = run_src(&src);
        assert_eq!(result.value, MemValue::Int(9));
        assert_eq!(result.stats().allocations, 2);
        assert_eq!(result.stats().in_place, 0);
        assert_eq!(result.stats().copies, 1);
    }

    #[test]
    fn shared_set_leaves_the_original_unchanged() {
        let src = own(
            "a",
            arr(&[1, 2, 3]),
            share("s", var("a"), own("b", set("s", 1, 9), get("a", 1))),
        );

        let result = run_src(&src);
        assert_eq!(result.value, MemValue::Int(2));
        assert_eq!(result.stats().copies, 1);
    }

    #[test]
    fn own_of_a_shared_value_copies() {
        let src = own(
            "a",
            arr(&[1, 2, 3]),
            share("s", var("a"), own("b", var("s"), get("b", 1))),
        );

        let result = run_src(&src);
        assert_eq!(result.value, MemValue::Int(2));
        assert_eq!(result.stats().allocations, 2);
        assert_eq!(result.stats().copies, 1);
    }

    #[test]
    fn use_after_consume_is_rejected() {
        let src = own("a", arr(&[1, 2, 3]), own("b", set("a", 1, 9), get("a", 1)));

        assert_eq!(plan(&src), Err(PlanError::UseAfterConsume("a".into())));
    }

    #[test]
    fn region_frees_its_allocations() {
        let src = SrcExpr::Region(Box::new(own("a", arr(&[1, 2, 3]), get("a", 1))));

        let result = run_src(&src);
        assert_eq!(result.value, MemValue::Int(2));
        assert_eq!(result.stats().frees, 1);
        assert!(!result.heap.is_live(RegionId(1)));
        assert!(result.heap.is_live(result.root));
    }

    #[test]
    fn region_may_not_return_an_array() {
        let src = SrcExpr::Region(Box::new(arr(&[1, 2, 3])));
        assert_eq!(plan(&src), Err(PlanError::RegionEscape));
    }

    #[test]
    fn returned_array_is_observed_in_root_region() {
        let src = SrcExpr::Return(Box::new(arr(&[1, 2, 3])));

        let result = run_src(&src);
        assert!(result.heap.is_live(result.root));
        let obs = result.observe().unwrap();
        assert_eq!(obs.roots, vec![crate::region::ObsValue::LocRef(0)]);
        assert_eq!(
            obs.blocks,
            vec![vec![
                crate::region::ObsValue::Int(1),
                crate::region::ObsValue::Int(2),
                crate::region::ObsValue::Int(3),
            ]]
        );
    }

    // -----------------------------------------------------------------------
    // Surface bridge
    // -----------------------------------------------------------------------

    use crate::ownership::check_source;
    use crate::syntax::Ty;

    fn heap_with_array(xs: &[i64]) -> (RegionHeap, RegionId, Loc) {
        let mut heap = RegionHeap::new();
        let root = heap.new_region();
        let loc = heap
            .alloc(root, xs.iter().map(|n| MemValue::Int(*n)).collect())
            .unwrap();
        (heap, root, loc)
    }

    fn ints(xs: &[i64]) -> Vec<MemValue> {
        xs.iter().map(|n| MemValue::Int(*n)).collect()
    }

    #[test]
    fn checked_fill_is_in_place() {
        let src = "fn fill(own a: Array<Int>, x: Int) -> Array<Int> { set(a, 0, x) }";
        let prog = check_source(src).unwrap();
        let f = prog.get("fill").unwrap();

        let (mut heap, root, a) = heap_with_array(&[1, 2, 3]);
        let result = run_fn_in(f, &[MemValue::Loc(a), MemValue::Int(9)], &mut heap, root).unwrap();
        let result = match result {
            MemValue::Loc(loc) => loc,
            other => panic!("expected a location, got {other:?}"),
        };

        assert_eq!(result, a);
        assert_eq!(heap.block(a).unwrap(), &ints(&[9, 2, 3]));
        assert_eq!(heap.stats().in_place, 1);
        assert_eq!(heap.stats().copies, 0);
        assert_eq!(heap.stats().allocations, 1);
    }

    #[test]
    fn checked_fill_shared_copies() {
        let src = "fn fill_shared(a: Array<Int>, x: Int) -> Array<Int> { set(a, 0, x) }";
        let prog = check_source(src).unwrap();
        let f = prog.get("fill_shared").unwrap();

        let (mut heap, root, a) = heap_with_array(&[1, 2, 3]);
        let result = run_fn_in(f, &[MemValue::Loc(a), MemValue::Int(9)], &mut heap, root).unwrap();
        let result = match result {
            MemValue::Loc(loc) => loc,
            other => panic!("expected a location, got {other:?}"),
        };

        assert_ne!(result, a);
        assert_eq!(heap.block(a).unwrap(), &ints(&[1, 2, 3]));
        assert_eq!(heap.block(result).unwrap(), &ints(&[9, 2, 3]));
        assert_eq!(heap.stats().in_place, 0);
        assert_eq!(heap.stats().copies, 1);
        assert_eq!(heap.stats().allocations, 2);
    }

    #[test]
    fn checked_local_index_runs() {
        let src = "fn third() -> Int { let a = [1, 2, 3]; a[2] }";
        let prog = check_source(src).unwrap();
        let f = prog.get("third").unwrap();

        let mut heap = RegionHeap::new();
        let root = heap.new_region();
        let result = run_fn_in(f, &[], &mut heap, root).unwrap();

        assert_eq!(result, MemValue::Int(3));
        assert_eq!(heap.stats().allocations, 1);
    }

    #[test]
    fn checked_push_is_in_place() {
        let src = "fn grow(own a: Array<Int>, x: Int) -> Array<Int> { push(a, x) }";
        let prog = check_source(src).unwrap();
        let f = prog.get("grow").unwrap();

        let (mut heap, root, a) = heap_with_array(&[1, 2, 3]);
        let result = run_fn_in(f, &[MemValue::Loc(a), MemValue::Int(9)], &mut heap, root).unwrap();
        let result = match result {
            MemValue::Loc(loc) => loc,
            other => panic!("expected a location, got {other:?}"),
        };

        assert_eq!(result, a);
        assert_eq!(heap.block(a).unwrap(), &ints(&[1, 2, 3, 9]));
        assert_eq!(heap.stats().in_place, 1);
    }

    #[test]
    fn checked_unsupported_expression_is_rejected() {
        let src = "fn add(a: Int) -> Int { a + 1 }";
        let prog = check_source(src).unwrap();
        let f = prog.get("add").unwrap();

        assert_eq!(
            plan_checked_fn(f),
            Err(BridgeError::Unsupported("binary expression".into()))
        );
    }

    #[test]
    fn checked_arg_type_is_checked() {
        let src = "fn fill(own a: Array<Int>, x: Int) -> Array<Int> { set(a, 0, x) }";
        let prog = check_source(src).unwrap();
        let f = prog.get("fill").unwrap();

        let mut heap = RegionHeap::new();
        let root = heap.new_region();
        let result = run_fn_in(f, &[MemValue::Int(1), MemValue::Int(9)], &mut heap, root);

        assert_eq!(
            result,
            Err(EvalError::ArgType {
                index: 0,
                expected: Ty::Array(Box::new(Ty::Int)),
            })
        );
    }

    #[test]
    fn checked_len_runs() {
        let src = "fn size(a: Array<Int>) -> Int { len(a) }";
        let prog = check_source(src).unwrap();
        let f = prog.get("size").unwrap();

        let (mut heap, root, a) = heap_with_array(&[1, 2, 3]);
        let result = run_fn_in(f, &[MemValue::Loc(a)], &mut heap, root).unwrap();

        assert_eq!(result, MemValue::Int(3));
    }

    #[test]
    fn checked_seq_runs() {
        let src = "fn second() -> Int { let a = [1, 2, 3]; a[0]; a[1] }";
        let prog = check_source(src).unwrap();
        let f = prog.get("second").unwrap();

        let mut heap = RegionHeap::new();
        let root = heap.new_region();
        let result = run_fn_in(f, &[], &mut heap, root).unwrap();

        assert_eq!(result, MemValue::Int(2));
    }

    #[test]
    fn checked_call_is_in_place_when_argument_is_owned() {
        let src = r#"
            fn fill(own a: Array<Int>, x: Int) -> Array<Int> {
                set(a, 0, x)
            }
            fn main() -> Int {
                let a = [1, 2, 3];
                let b = fill(a, 9);
                b[0]
            }
        "#;
        let prog = check_source(src).unwrap();
        let mem = plan_checked_program(&prog).unwrap();
        let result = run_program(&mem, "main").unwrap();

        assert_eq!(result.value, MemValue::Int(9));
        assert_eq!(result.stats().allocations, 1);
        assert_eq!(result.stats().in_place, 1);
        assert_eq!(result.stats().copies, 0);
        assert_eq!(result.stats().frees, 1);
    }

    #[test]
    fn checked_call_copies_a_returned_array() {
        let src = r#"
            fn make(x: Int) -> Array<Int> {
                [x, x]
            }
            fn main() -> Int {
                let a = make(7);
                a[0]
            }
        "#;
        let prog = check_source(src).unwrap();
        let mem = plan_checked_program(&prog).unwrap();
        let result = run_program(&mem, "main").unwrap();

        assert_eq!(result.value, MemValue::Int(7));
        assert_eq!(result.stats().allocations, 2);
        assert_eq!(result.stats().copies, 1);
        assert_eq!(result.stats().frees, 1);
    }

    #[test]
    fn checked_call_can_return_a_caller_array_without_copy() {
        let src = r#"
            fn id(a: Array<Int>) -> Array<Int> {
                a
            }
            fn main() -> Int {
                let a = [1, 2, 3];
                let b = id(a);
                b[0]
            }
        "#;
        let prog = check_source(src).unwrap();
        let mem = plan_checked_program(&prog).unwrap();
        let result = run_program(&mem, "main").unwrap();

        assert_eq!(result.value, MemValue::Int(1));
        assert_eq!(result.stats().allocations, 1);
        assert_eq!(result.stats().copies, 0);
        assert_eq!(result.stats().frees, 1);
    }

    #[test]
    fn checked_if_selects_the_branch() {
        let src = "fn choose(c: Bool) -> Int { if c then 1 else 2 }";
        let prog = check_source(src).unwrap();
        let f = prog.get("choose").unwrap();

        let mut heap = RegionHeap::new();
        let root = heap.new_region();
        let then_value = run_fn_in(f, &[MemValue::Bool(true)], &mut heap, root).unwrap();
        let else_value = run_fn_in(f, &[MemValue::Bool(false)], &mut heap, root).unwrap();

        assert_eq!(then_value, MemValue::Int(1));
        assert_eq!(else_value, MemValue::Int(2));
    }

    #[test]
    fn checked_if_allocates_only_the_selected_branch() {
        let src = r#"
            fn choose(c: Bool) -> Array<Int> {
                if c then [1, 2] else [3, 4]
            }
        "#;
        let prog = check_source(src).unwrap();
        let f = prog.get("choose").unwrap();

        let mut heap = RegionHeap::new();
        let root = heap.new_region();
        let result = run_fn_in(f, &[MemValue::Bool(true)], &mut heap, root).unwrap();
        let result = match result {
            MemValue::Loc(loc) => loc,
            other => panic!("expected a location, got {other:?}"),
        };

        assert_eq!(heap.block(result).unwrap(), &ints(&[1, 2]));
        assert_eq!(heap.stats().allocations, 1);
    }

    #[test]
    fn direct_if_constructs_no_thunks() {
        let src = "fn choose(c: Bool) -> Int { if c then 1 else 2 }";
        let prog = check_source(src).unwrap();
        let f = prog.get("choose").unwrap();

        let mut heap = RegionHeap::new();
        let root = heap.new_region();
        let result = run_fn_in(f, &[MemValue::Bool(true)], &mut heap, root).unwrap();

        assert_eq!(result, MemValue::Int(1));
        assert_eq!(heap.stats().thunks, 0);
    }

    #[test]
    fn fallback_if_constructs_both_thunks() {
        let plan = MemExpr::If {
            cond: Box::new(MemExpr::Bool(true)),
            then_branch: Box::new(MemExpr::Int(1)),
            else_branch: Box::new(MemExpr::Int(2)),
            optimized: false,
        };

        let result = run(&plan).unwrap();
        assert_eq!(result.value, MemValue::Int(1));
        assert_eq!(result.stats().thunks, 2);
        assert_eq!(result.stats().allocations, 0);
    }

    #[test]
    fn fallback_if_executes_only_the_selected_branch() {
        let plan = MemExpr::If {
            cond: Box::new(MemExpr::Bool(true)),
            then_branch: Box::new(MemExpr::Alloc(vec![MemExpr::Int(1)])),
            else_branch: Box::new(MemExpr::Alloc(vec![MemExpr::Int(2)])),
            optimized: false,
        };

        let result = run(&plan).unwrap();
        assert!(matches!(result.value, MemValue::Loc(_)));
        assert_eq!(result.stats().thunks, 2);
        assert_eq!(result.stats().allocations, 1);
    }

    #[test]
    fn fallback_if_captures_the_environment() {
        let plan = MemExpr::Let {
            name: "x".into(),
            value: Box::new(MemExpr::Int(7)),
            body: Box::new(MemExpr::If {
                cond: Box::new(MemExpr::Bool(false)),
                then_branch: Box::new(MemExpr::Var("x".into())),
                else_branch: Box::new(MemExpr::Int(0)),
                optimized: false,
            }),
        };

        let result = run(&plan).unwrap();
        assert_eq!(result.value, MemValue::Int(0));
        assert_eq!(result.stats().thunks, 2);
    }

    #[test]
    fn lambda_identity_apply() {
        let plan = MemExpr::Apply {
            callee: Box::new(MemExpr::Lambda {
                params: vec!["x".into()],
                body: Box::new(MemExpr::Var("x".into())),
            }),
            args: vec![MemExpr::Int(7)],
        };

        let result = run(&plan).unwrap();
        assert_eq!(result.value, MemValue::Int(7));
        assert_eq!(result.stats().closures, 1);
    }

    #[test]
    fn lambda_captures_the_environment() {
        let plan = MemExpr::Let {
            name: "n".into(),
            value: Box::new(MemExpr::Int(7)),
            body: Box::new(MemExpr::Apply {
                callee: Box::new(MemExpr::Lambda {
                    params: vec![],
                    body: Box::new(MemExpr::Var("n".into())),
                }),
                args: vec![],
            }),
        };

        let result = run(&plan).unwrap();
        assert_eq!(result.value, MemValue::Int(7));
        assert_eq!(result.stats().closures, 1);
    }

    #[test]
    fn thunk_force() {
        let plan = MemExpr::Force(Box::new(MemExpr::Thunk(Box::new(MemExpr::Int(7)))));

        let result = run(&plan).unwrap();
        assert_eq!(result.value, MemValue::Int(7));
        assert_eq!(result.stats().thunks, 1);
    }

    #[test]
    fn higher_order_lambda_apply() {
        // let id = \x -> x in (\f -> f 7) id
        let plan = MemExpr::Let {
            name: "id".into(),
            value: Box::new(MemExpr::Lambda {
                params: vec!["x".into()],
                body: Box::new(MemExpr::Var("x".into())),
            }),
            body: Box::new(MemExpr::Apply {
                callee: Box::new(MemExpr::Lambda {
                    params: vec!["f".into()],
                    body: Box::new(MemExpr::Apply {
                        callee: Box::new(MemExpr::Var("f".into())),
                        args: vec![MemExpr::Int(7)],
                    }),
                }),
                args: vec![MemExpr::Var("id".into())],
            }),
        };

        let result = run(&plan).unwrap();
        assert_eq!(result.value, MemValue::Int(7));
        assert_eq!(result.stats().closures, 2);
    }
}
