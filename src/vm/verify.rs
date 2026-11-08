use std::collections::VecDeque;

use super::ir::{BType, BcFunction, BcProgram, Instr};

// ---------------------------------------------------------------------------
// Bytecode verifier
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum LocalState {
    Uninit,
    Init(BType),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VState {
    stack: Vec<BType>,
    locals: Vec<LocalState>,
}

fn join_states(a: &VState, b: &VState) -> Result<VState, String> {
    if a.stack.len() != b.stack.len() {
        return Err(format!(
            "stack height mismatch at merge: {} vs {}",
            a.stack.len(),
            b.stack.len()
        ));
    }
    for (i, (x, y)) in a.stack.iter().zip(&b.stack).enumerate() {
        if x != y {
            return Err(format!(
                "stack type mismatch at merge depth {i}: {x:?} vs {y:?}"
            ));
        }
    }
    if a.locals.len() != b.locals.len() {
        return Err("local count mismatch at merge".to_string());
    }
    let mut locals = Vec::with_capacity(a.locals.len());
    for (i, (x, y)) in a.locals.iter().zip(&b.locals).enumerate() {
        let merged = match (x, y) {
            (LocalState::Init(t1), LocalState::Init(t2)) if t1 == t2 => {
                LocalState::Init(t1.clone())
            }
            (LocalState::Init(_), LocalState::Init(_)) => {
                return Err(format!("local {i} has different types on merge"))
            }
            _ => LocalState::Uninit,
        };
        locals.push(merged);
    }
    Ok(VState {
        stack: a.stack.clone(),
        locals,
    })
}

fn pop_any(stack: &mut Vec<BType>, pc: usize) -> Result<BType, String> {
    stack
        .pop()
        .ok_or_else(|| format!("pc {pc}: stack underflow"))
}

fn pop_expect(stack: &mut Vec<BType>, pc: usize, expected: BType) -> Result<(), String> {
    let actual = pop_any(stack, pc)?;
    if actual != expected {
        Err(format!(
            "pc {pc}: expected {expected:?} on stack, got {actual:?}"
        ))
    } else {
        Ok(())
    }
}

fn step(
    func: &BcFunction,
    pc: usize,
    instr: &Instr,
    state: &VState,
    prog: &BcProgram,
) -> Result<Vec<(usize, VState)>, String> {
    let mut st = state.clone();
    let mut succs = Vec::new();
    match instr {
        Instr::PushInt(_) => st.stack.push(BType::Int),
        Instr::PushBool(_) => st.stack.push(BType::Bool),
        Instr::PushUnit => st.stack.push(BType::Unit),
        Instr::Load(slot) => {
            if *slot >= st.locals.len() {
                return Err(format!("pc {pc}: load from invalid slot {slot}"));
            }
            match &st.locals[*slot] {
                LocalState::Init(t) => st.stack.push(t.clone()),
                LocalState::Uninit => {
                    return Err(format!("pc {pc}: load from uninitialized slot {slot}"))
                }
            }
        }
        Instr::Store(slot) => {
            if *slot >= st.locals.len() {
                return Err(format!("pc {pc}: store to invalid slot {slot}"));
            }
            let t = pop_any(&mut st.stack, pc)?;
            if let LocalState::Init(old) = &st.locals[*slot] {
                if *old != t {
                    return Err(format!(
                        "pc {pc}: slot {slot} already holds {old:?}, storing {t:?}"
                    ));
                }
            }
            st.locals[*slot] = LocalState::Init(t);
        }
        Instr::Add | Instr::Sub | Instr::Mul => {
            pop_expect(&mut st.stack, pc, BType::Int)?;
            pop_expect(&mut st.stack, pc, BType::Int)?;
            st.stack.push(BType::Int);
        }
        Instr::Div { .. } | Instr::Mod { .. } => {
            pop_expect(&mut st.stack, pc, BType::Int)?;
            pop_expect(&mut st.stack, pc, BType::Int)?;
            st.stack.push(BType::Int);
        }
        Instr::Neg => {
            pop_expect(&mut st.stack, pc, BType::Int)?;
            st.stack.push(BType::Int);
        }
        Instr::Eq | Instr::Ne | Instr::Lt | Instr::Le | Instr::Gt | Instr::Ge => {
            pop_expect(&mut st.stack, pc, BType::Int)?;
            pop_expect(&mut st.stack, pc, BType::Int)?;
            st.stack.push(BType::Bool);
        }
        Instr::Not => {
            pop_expect(&mut st.stack, pc, BType::Bool)?;
            st.stack.push(BType::Bool);
        }
        Instr::Jump(t) => succs.push((*t, st.clone())),
        Instr::JumpIfFalse(t) => {
            pop_expect(&mut st.stack, pc, BType::Bool)?;
            succs.push((*t, st.clone()));
            succs.push((pc + 1, st.clone()));
        }
        Instr::Call { func: f, argc } => {
            let target = prog
                .funcs
                .get(*f)
                .ok_or_else(|| format!("pc {pc}: call to invalid function {f}"))?;
            if *argc != target.arity || *argc != target.param_tys.len() {
                return Err(format!(
                    "pc {pc}: call argc {argc} does not match `{}` arity {}",
                    target.name, target.arity
                ));
            }
            let mut args = Vec::with_capacity(*argc);
            for _ in 0..*argc {
                args.push(pop_any(&mut st.stack, pc)?);
            }
            args.reverse();
            for (i, (actual, expected)) in args.iter().zip(&target.param_tys).enumerate() {
                if actual != expected {
                    return Err(format!(
                        "pc {pc}: argument {i} of `{}` has type {actual:?}, expected {expected:?}",
                        target.name
                    ));
                }
            }
            st.stack.push(target.ret.clone());
        }
        Instr::MakeClosure { func: f, captures } => {
            let target = prog
                .funcs
                .get(*f)
                .ok_or_else(|| format!("pc {pc}: closure of invalid function {f}"))?;
            if *captures > target.param_tys.len() {
                return Err(format!(
                    "pc {pc}: closure captures {captures} values, `{}` has {} parameters",
                    target.name,
                    target.param_tys.len()
                ));
            }
            let mut actual = Vec::with_capacity(*captures);
            for _ in 0..*captures {
                actual.push(pop_any(&mut st.stack, pc)?);
            }
            actual.reverse();
            for (i, (got, expected)) in actual.iter().zip(&target.param_tys).enumerate() {
                if got != expected {
                    return Err(format!(
                        "pc {pc}: capture {i} has type {got:?}, expected {expected:?}"
                    ));
                }
            }
            st.stack.push(BType::Fn(
                target.param_tys[*captures..].to_vec(),
                Box::new(target.ret.clone()),
            ));
        }
        Instr::CallClosure { argc } => {
            let mut args = Vec::with_capacity(*argc);
            for _ in 0..*argc {
                args.push(pop_any(&mut st.stack, pc)?);
            }
            args.reverse();
            let callee = pop_any(&mut st.stack, pc)?;
            let BType::Fn(params, ret) = callee else {
                return Err(format!("pc {pc}: call of non-function value {callee:?}"));
            };
            if *argc != params.len() {
                return Err(format!(
                    "pc {pc}: closure expects {} arguments, got {argc}",
                    params.len()
                ));
            }
            for (i, (actual, expected)) in args.iter().zip(&params).enumerate() {
                if actual != expected {
                    return Err(format!(
                        "pc {pc}: closure argument {i} has type {actual:?}, expected {expected:?}"
                    ));
                }
            }
            st.stack.push(*ret);
        }
        Instr::Ret => {
            if st.stack.len() != 1 {
                return Err(format!(
                    "pc {pc}: ret expects exactly one value, stack has {}",
                    st.stack.len()
                ));
            }
            let actual = st.stack[0].clone();
            if actual != func.ret {
                return Err(format!(
                    "pc {pc}: ret returns {actual:?}, expected {:?}",
                    func.ret
                ));
            }
        }
        Instr::MakeArray(n) => {
            if *n == 0 {
                return Err(format!(
                    "pc {pc}: empty array literal needs an explicit type"
                ));
            }
            let mut tys = Vec::with_capacity(*n);
            for _ in 0..*n {
                tys.push(pop_any(&mut st.stack, pc)?);
            }
            let first = tys[0].clone();
            if tys.iter().any(|t| *t != first) {
                return Err(format!("pc {pc}: array elements have different types"));
            }
            st.stack.push(BType::Array(Box::new(first)));
        }
        Instr::ArrayGet { .. } => {
            pop_expect(&mut st.stack, pc, BType::Int)?;
            let arr = pop_any(&mut st.stack, pc)?;
            match arr {
                BType::Array(t) => st.stack.push(*t),
                other => return Err(format!("pc {pc}: array get on non-array {other:?}")),
            }
        }
        Instr::ArraySet { .. } => {
            let value_ty = pop_any(&mut st.stack, pc)?;
            pop_expect(&mut st.stack, pc, BType::Int)?;
            let arr = pop_any(&mut st.stack, pc)?;
            match arr {
                BType::Array(t) => {
                    if *t != value_ty {
                        return Err(format!(
                            "pc {pc}: set value type {value_ty:?}, expected {t:?}"
                        ));
                    }
                    st.stack.push(BType::Array(t));
                }
                other => return Err(format!("pc {pc}: set on non-array {other:?}")),
            }
        }
        Instr::ArrayPush { .. } => {
            let value_ty = pop_any(&mut st.stack, pc)?;
            let arr = pop_any(&mut st.stack, pc)?;
            match arr {
                BType::Array(t) => {
                    if *t != value_ty {
                        return Err(format!(
                            "pc {pc}: push value type {value_ty:?}, expected {t:?}"
                        ));
                    }
                    st.stack.push(BType::Array(t));
                }
                other => return Err(format!("pc {pc}: push on non-array {other:?}")),
            }
        }
        Instr::Len => {
            let arr = pop_any(&mut st.stack, pc)?;
            match arr {
                BType::Array(_) => st.stack.push(BType::Int),
                other => return Err(format!("pc {pc}: len on non-array {other:?}")),
            }
        }
        Instr::Print => {
            pop_any(&mut st.stack, pc)?;
        }
        Instr::Pop => {
            pop_any(&mut st.stack, pc)?;
        }
        Instr::Halt => {
            if st.stack.is_empty() {
                return Err(format!("pc {pc}: halt with empty stack"));
            }
        }
    }

    // Non-terminal instructions fall through to the next instruction.
    if !matches!(
        instr,
        Instr::Ret | Instr::Halt | Instr::Jump(_) | Instr::JumpIfFalse(_)
    ) {
        succs.push((pc + 1, st));
    }
    Ok(succs)
}

fn verify_function(func: &BcFunction, prog: &BcProgram) -> Result<(), String> {
    if func.arity != func.param_tys.len() {
        return Err(format!(
            "{}: arity {} != param type count {}",
            func.name,
            func.arity,
            func.param_tys.len()
        ));
    }
    if func.nlocals < func.arity {
        return Err(format!(
            "{}: nlocals {} < arity {}",
            func.name, func.nlocals, func.arity
        ));
    }
    if func.code.is_empty() {
        return Err(format!("{}: empty code", func.name));
    }

    let mut locals = vec![LocalState::Uninit; func.nlocals];
    for (i, t) in func.param_tys.iter().enumerate() {
        locals[i] = LocalState::Init(t.clone());
    }
    let initial = VState {
        stack: Vec::new(),
        locals,
    };

    let mut states: Vec<Option<VState>> = vec![None; func.code.len()];
    states[0] = Some(initial);
    let mut work = VecDeque::new();
    work.push_back(0usize);

    while let Some(pc) = work.pop_front() {
        let state = states[pc].clone().unwrap();
        let instr = func.code[pc].clone();
        let succs =
            step(func, pc, &instr, &state, prog).map_err(|e| format!("{}: {e}", func.name))?;
        for (target, next) in succs {
            if target >= func.code.len() {
                return Err(format!(
                    "{}: pc {pc}: jump/call target {target} out of range (len {})",
                    func.name,
                    func.code.len()
                ));
            }
            match &states[target] {
                None => {
                    states[target] = Some(next);
                    work.push_back(target);
                }
                Some(old) => {
                    let merged = join_states(old, &next)
                        .map_err(|e| format!("{}: merge at pc {target}: {e}", func.name))?;
                    if merged != *old {
                        states[target] = Some(merged);
                        work.push_back(target);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Verify stack/local/jump/call/array types before running.
pub fn verify(prog: &BcProgram) -> Result<(), String> {
    for func in &prog.funcs {
        verify_function(func, prog)?;
    }
    Ok(())
}
