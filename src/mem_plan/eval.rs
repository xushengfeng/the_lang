use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::bridge::{value_matches_ty, BridgeError};
use super::ir::{MemExpr, MemProgram};
use super::validate::{validate_expr, ValidateError};
use crate::region::{Loc, MemError, MemStats, MemValue, Observation, RegionHeap, RegionId};
use crate::syntax::Ty;

// ---------------------------------------------------------------------------
// Runtime values
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub(crate) enum Value {
    Data(MemValue),
    Closure(Rc<Closure>),
    Thunk(Rc<Thunk>),
}

#[derive(Clone, Debug)]
pub(crate) struct Closure {
    params: Vec<String>,
    body: MemExpr,
    env: HashMap<String, Value>,
}

#[derive(Clone, Debug)]
pub(crate) struct Thunk {
    body: MemExpr,
    env: HashMap<String, Value>,
}

impl Value {
    pub(crate) fn into_data(self) -> Result<MemValue, EvalError> {
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvalError {
    Mem(MemError),
    Bridge(BridgeError),
    Validation(ValidateError),
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

impl From<ValidateError> for EvalError {
    fn from(e: ValidateError) -> Self {
        EvalError::Validation(e)
    }
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalError::Mem(e) => write!(f, "{e}"),
            EvalError::Bridge(e) => write!(f, "{e}"),
            EvalError::Validation(e) => write!(f, "{e}"),
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
        self.heap.observe(std::slice::from_ref(&self.value))
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
    validate_expr(plan, &program)?;
    let value = eval(plan, &mut heap, &mut env, root, &program)?.into_data()?;
    Ok(MemRun { value, root, heap })
}

pub(crate) fn eval(
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
