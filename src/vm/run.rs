use std::rc::Rc;

use super::ir::{BcProgram, Instr, VmResult};
use crate::ownership::{Closure, Stats, Value};

struct Frame {
    func: usize,
    pc: usize,
    locals: Vec<Value>,
    stack: Vec<Value>,
}

pub fn run(prog: &BcProgram, entry: usize) -> Result<VmResult, String> {
    let mut stats = Stats::default();
    let mut frames = vec![Frame {
        func: entry,
        pc: 0,
        locals: Vec::new(),
        stack: Vec::new(),
    }];

    loop {
        let instr = {
            let frame = frames
                .last_mut()
                .ok_or_else(|| "empty call stack".to_string())?;
            let f = prog
                .funcs
                .get(frame.func)
                .ok_or_else(|| "bad function index".to_string())?;
            let i = f
                .code
                .get(frame.pc)
                .cloned()
                .ok_or_else(|| format!("pc {} out of range in `{}`", frame.pc, f.name))?;
            frame.pc += 1;
            i
        };

        match instr {
            Instr::PushInt(n) => frames.last_mut().unwrap().stack.push(Value::Int(n)),
            Instr::PushBool(b) => frames.last_mut().unwrap().stack.push(Value::Bool(b)),
            Instr::PushUnit => frames.last_mut().unwrap().stack.push(Value::Unit),
            Instr::Load(slot) => {
                let v = frames
                    .last()
                    .unwrap()
                    .locals
                    .get(slot)
                    .cloned()
                    .ok_or_else(|| format!("load from empty slot {slot}"))?;
                frames.last_mut().unwrap().stack.push(v);
            }
            Instr::Store(slot) => {
                let v = frames
                    .last_mut()
                    .unwrap()
                    .stack
                    .pop()
                    .ok_or_else(|| "store with empty stack".to_string())?;
                let frame = frames.last_mut().unwrap();
                if slot >= frame.locals.len() {
                    frame.locals.resize(slot + 1, Value::Unit);
                }
                frame.locals[slot] = v;
            }
            Instr::Add => bin_i64(&mut frames, |a, b| a.checked_add(b))?,
            Instr::Sub => bin_i64(&mut frames, |a, b| a.checked_sub(b))?,
            Instr::Mul => bin_i64(&mut frames, |a, b| a.checked_mul(b))?,
            Instr::Div { checked } => {
                let (a, b) = pop2_int(&mut frames)?;
                if b == 0 {
                    if checked {
                        return Err("division by zero".into());
                    }
                    return Err("division by zero (unchecked)".into());
                }
                frames.last_mut().unwrap().stack.push(Value::Int(a / b));
            }
            Instr::Mod { checked } => {
                let (a, b) = pop2_int(&mut frames)?;
                if b == 0 {
                    if checked {
                        return Err("division by zero".into());
                    }
                    return Err("division by zero (unchecked)".into());
                }
                frames.last_mut().unwrap().stack.push(Value::Int(a % b));
            }
            Instr::Neg => {
                let a = pop_int(&mut frames)?;
                let n = a
                    .checked_neg()
                    .ok_or_else(|| "integer overflow".to_string())?;
                frames.last_mut().unwrap().stack.push(Value::Int(n));
            }
            Instr::Eq => bin_bool(&mut frames, |a, b| a == b)?,
            Instr::Ne => bin_bool(&mut frames, |a, b| a != b)?,
            Instr::Lt => bin_bool(&mut frames, |a, b| a < b)?,
            Instr::Le => bin_bool(&mut frames, |a, b| a <= b)?,
            Instr::Gt => bin_bool(&mut frames, |a, b| a > b)?,
            Instr::Ge => bin_bool(&mut frames, |a, b| a >= b)?,
            Instr::Not => {
                let b = pop_bool(&mut frames)?;
                frames.last_mut().unwrap().stack.push(Value::Bool(!b));
            }
            Instr::Jump(t) => frames.last_mut().unwrap().pc = t,
            Instr::JumpIfFalse(t) => {
                let b = pop_bool(&mut frames)?;
                if !b {
                    frames.last_mut().unwrap().pc = t;
                }
            }
            Instr::Call { func, argc } => {
                let mut args = Vec::with_capacity(argc);
                for _ in 0..argc {
                    args.push(
                        frames
                            .last_mut()
                            .unwrap()
                            .stack
                            .pop()
                            .ok_or_else(|| "call with too few arguments".to_string())?,
                    );
                }
                args.reverse();
                frames.push(Frame {
                    func,
                    pc: 0,
                    locals: args,
                    stack: Vec::new(),
                });
            }
            Instr::MakeClosure { func, captures } => {
                let mut values = Vec::with_capacity(captures);
                for _ in 0..captures {
                    values.push(
                        frames
                            .last_mut()
                            .unwrap()
                            .stack
                            .pop()
                            .ok_or_else(|| "make-closure with too few captures".to_string())?,
                    );
                }
                values.reverse();
                frames
                    .last_mut()
                    .unwrap()
                    .stack
                    .push(Value::Closure(Rc::new(Closure::Bytecode {
                        func,
                        captures: values,
                    })));
            }
            Instr::CallClosure { argc } => {
                let mut args = Vec::with_capacity(argc);
                for _ in 0..argc {
                    args.push(
                        frames
                            .last_mut()
                            .unwrap()
                            .stack
                            .pop()
                            .ok_or_else(|| "closure call with too few arguments".to_string())?,
                    );
                }
                args.reverse();
                let callee = frames
                    .last_mut()
                    .unwrap()
                    .stack
                    .pop()
                    .ok_or_else(|| "closure call with empty stack".to_string())?;
                let Value::Closure(closure) = callee else {
                    return Err("attempted to call a non-function value".into());
                };
                let Closure::Bytecode { func, mut captures } = (*closure).clone() else {
                    return Err("surface closure cannot run in the bytecode VM".into());
                };
                captures.extend(args);
                frames.push(Frame {
                    func,
                    pc: 0,
                    locals: captures,
                    stack: Vec::new(),
                });
            }
            Instr::Ret => {
                let v = frames
                    .last_mut()
                    .unwrap()
                    .stack
                    .pop()
                    .unwrap_or(Value::Unit);
                frames.pop();
                if let Some(caller) = frames.last_mut() {
                    caller.stack.push(v);
                } else {
                    return Ok(VmResult { value: v, stats });
                }
            }
            Instr::MakeArray(n) => {
                let mut vals = Vec::with_capacity(n);
                for _ in 0..n {
                    vals.push(
                        frames
                            .last_mut()
                            .unwrap()
                            .stack
                            .pop()
                            .ok_or_else(|| "make-array with too few values".to_string())?,
                    );
                }
                vals.reverse();
                stats.allocations += 1;
                frames
                    .last_mut()
                    .unwrap()
                    .stack
                    .push(Value::Array(std::rc::Rc::new(std::cell::RefCell::new(
                        vals,
                    ))));
            }
            Instr::ArrayGet { checked } => {
                let i = pop_int(&mut frames)?;
                let arr = pop_array(&mut frames)?;
                let a = arr.borrow();
                if i < 0 || i as usize >= a.len() {
                    if checked {
                        return Err(format!("index {i} out of bounds (len {})", a.len()));
                    }
                    return Err(format!(
                        "unchecked index {i} out of bounds (len {})",
                        a.len()
                    ));
                }
                if checked {
                    stats.bounds_checks += 1;
                }
                let v = a[i as usize].clone();
                drop(a);
                frames.last_mut().unwrap().stack.push(v);
            }
            Instr::ArraySet { in_place } => {
                let value = frames
                    .last_mut()
                    .unwrap()
                    .stack
                    .pop()
                    .ok_or_else(|| "set with empty stack".to_string())?;
                let i = pop_int(&mut frames)?;
                let arr = pop_array(&mut frames)?;
                {
                    let a = arr.borrow();
                    if i < 0 || i as usize >= a.len() {
                        return Err(format!("set index {i} out of bounds (len {})", a.len()));
                    }
                }
                stats.bounds_checks += 1;
                if in_place {
                    arr.borrow_mut()[i as usize] = value;
                    stats.in_place += 1;
                    frames.last_mut().unwrap().stack.push(Value::Array(arr));
                } else {
                    let mut new = arr.borrow().clone();
                    new[i as usize] = value;
                    stats.copies += 1;
                    stats.allocations += 1;
                    frames
                        .last_mut()
                        .unwrap()
                        .stack
                        .push(Value::Array(std::rc::Rc::new(std::cell::RefCell::new(new))));
                }
            }
            Instr::ArrayPush { in_place } => {
                let value = frames
                    .last_mut()
                    .unwrap()
                    .stack
                    .pop()
                    .ok_or_else(|| "push with empty stack".to_string())?;
                let arr = pop_array(&mut frames)?;
                if in_place {
                    arr.borrow_mut().push(value);
                    stats.in_place += 1;
                    frames.last_mut().unwrap().stack.push(Value::Array(arr));
                } else {
                    let mut new = arr.borrow().clone();
                    new.push(value);
                    stats.copies += 1;
                    stats.allocations += 1;
                    frames
                        .last_mut()
                        .unwrap()
                        .stack
                        .push(Value::Array(std::rc::Rc::new(std::cell::RefCell::new(new))));
                }
            }
            Instr::Len => {
                let arr = pop_array(&mut frames)?;
                let n = arr.borrow().len() as i64;
                frames.last_mut().unwrap().stack.push(Value::Int(n));
            }
            Instr::Print => {
                let v = frames
                    .last_mut()
                    .unwrap()
                    .stack
                    .pop()
                    .ok_or_else(|| "print with empty stack".to_string())?;
                println!("{v}");
            }
            Instr::Pop => {
                frames
                    .last_mut()
                    .unwrap()
                    .stack
                    .pop()
                    .ok_or_else(|| "pop from empty stack".to_string())?;
            }
            Instr::Halt => {
                let v = frames
                    .last_mut()
                    .unwrap()
                    .stack
                    .pop()
                    .unwrap_or(Value::Unit);
                return Ok(VmResult { value: v, stats });
            }
        }
    }
}

fn pop_int(frames: &mut [Frame]) -> Result<i64, String> {
    frames
        .last_mut()
        .unwrap()
        .stack
        .pop()
        .ok_or_else(|| "empty stack".to_string())?
        .as_int()
}

fn pop_bool(frames: &mut [Frame]) -> Result<bool, String> {
    frames
        .last_mut()
        .unwrap()
        .stack
        .pop()
        .ok_or_else(|| "empty stack".to_string())?
        .as_bool()
}

fn pop_array(frames: &mut [Frame]) -> Result<std::rc::Rc<std::cell::RefCell<Vec<Value>>>, String> {
    frames
        .last_mut()
        .unwrap()
        .stack
        .pop()
        .ok_or_else(|| "empty stack".to_string())?
        .as_array()
}

fn pop2_int(frames: &mut [Frame]) -> Result<(i64, i64), String> {
    let b = pop_int(frames)?;
    let a = pop_int(frames)?;
    Ok((a, b))
}

fn bin_i64<F>(frames: &mut [Frame], f: F) -> Result<(), String>
where
    F: FnOnce(i64, i64) -> Option<i64>,
{
    let (a, b) = pop2_int(frames)?;
    let n = f(a, b).ok_or_else(|| "integer overflow".to_string())?;
    frames.last_mut().unwrap().stack.push(Value::Int(n));
    Ok(())
}

fn bin_bool<F>(frames: &mut [Frame], f: F) -> Result<(), String>
where
    F: FnOnce(i64, i64) -> bool,
{
    let (a, b) = pop2_int(frames)?;
    frames.last_mut().unwrap().stack.push(Value::Bool(f(a, b)));
    Ok(())
}

pub fn disassemble(p: &BcProgram) -> String {
    let mut out = String::new();
    for (i, f) in p.funcs.iter().enumerate() {
        out.push_str(&format!(
            "fn {} @{} (arity {}, locals {})\n",
            f.name, i, f.arity, f.nlocals
        ));
        for (pc, instr) in f.code.iter().enumerate() {
            out.push_str(&format!("  {:04} {:?}\n", pc, instr));
        }
    }
    out
}
