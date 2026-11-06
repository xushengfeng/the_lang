//! A small stack VM for the ownership-checked surface language.
//!
//! Pipeline:
//!   source -> parser -> ownership checker/lowering -> bytecode -> VM
//!
//! Proof parameters and proof arguments are already removed by the ownership
//! checker, so the VM never sees a proof value.  `set`/`push` arrive as
//! `__set_in_place` / `__set_copy` and are compiled to bytecode variants.

use std::collections::HashMap;

use crate::ownership::{CheckedFn, CheckedProgram, Stats, Value};
use crate::syntax::{BinOp, CallArg, Expr, UnOp};

#[derive(Clone, Debug)]
pub enum Instr {
    PushInt(i64),
    PushBool(bool),
    PushUnit,
    Load(usize),
    Store(usize),
    Add,
    Sub,
    Mul,
    Div { checked: bool },
    Mod { checked: bool },
    Neg,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Not,
    Jump(usize),
    JumpIfFalse(usize),
    Call { func: usize, argc: usize },
    Ret,
    MakeArray(usize),
    ArrayGet { checked: bool },
    ArraySet { in_place: bool },
    ArrayPush { in_place: bool },
    Len,
    Print,
    Pop,
    Halt,
}

#[derive(Clone, Debug)]
pub struct BcFunction {
    pub name: String,
    pub arity: usize,
    pub nlocals: usize,
    pub code: Vec<Instr>,
}

#[derive(Clone, Debug)]
pub struct BcProgram {
    pub funcs: Vec<BcFunction>,
    pub main: usize,
}

impl BcProgram {
    pub fn get(&self, name: &str) -> Option<&BcFunction> {
        self.funcs.iter().find(|f| f.name == name)
    }
}

pub struct VmResult {
    pub value: Value,
    pub stats: Stats,
}

pub fn compile(prog: &CheckedProgram) -> Result<BcProgram, String> {
    let mut index = HashMap::new();
    for (i, f) in prog.funcs.iter().enumerate() {
        index.insert(f.name.clone(), i);
    }
    let main = *index
        .get("main")
        .ok_or_else(|| "program has no `main` function".to_string())?;

    let mut funcs = Vec::new();
    for f in &prog.funcs {
        funcs.push(compile_fn(f, &index)?);
    }
    Ok(BcProgram { funcs, main })
}

fn compile_fn(f: &CheckedFn, index: &HashMap<String, usize>) -> Result<BcFunction, String> {
    let mut c = Compiler {
        index,
        code: Vec::new(),
        locals: HashMap::new(),
        next_slot: 0,
    };
    for (i, p) in f.params.iter().enumerate() {
        c.locals.insert(p.clone(), i);
        c.next_slot = i + 1;
    }
    c.emit(&f.body)
        .map_err(|e| format!("while compiling `{}`:\n{e}", f.name))?;
    c.code.push(Instr::Ret);
    Ok(BcFunction {
        name: f.name.clone(),
        arity: f.params.len(),
        nlocals: c.next_slot,
        code: c.code,
    })
}

struct Compiler<'a> {
    index: &'a HashMap<String, usize>,
    code: Vec<Instr>,
    locals: HashMap<String, usize>,
    next_slot: usize,
}

impl<'a> Compiler<'a> {
    fn push(&mut self, i: Instr) {
        self.code.push(i);
    }

    fn alloc_local(&mut self, name: &str) -> Result<usize, String> {
        if self.locals.contains_key(name) {
            return Err(format!("local `{name}` already has a slot"));
        }
        let slot = self.next_slot;
        self.next_slot += 1;
        self.locals.insert(name.to_string(), slot);
        Ok(slot)
    }

    fn local(&self, name: &str) -> Result<usize, String> {
        self.locals
            .get(name)
            .copied()
            .ok_or_else(|| format!("unknown local `{name}`"))
    }

    fn emit_jump_if_false(&mut self) -> usize {
        let i = self.code.len();
        self.push(Instr::JumpIfFalse(0));
        i
    }

    fn emit_jump(&mut self) -> usize {
        let i = self.code.len();
        self.push(Instr::Jump(0));
        i
    }

    fn patch_if(&mut self, at: usize, target: usize) {
        self.code[at] = Instr::JumpIfFalse(target);
    }

    fn patch_jump(&mut self, at: usize, target: usize) {
        self.code[at] = Instr::Jump(target);
    }

    fn emit(&mut self, e: &Expr) -> Result<(), String> {
        match e {
            Expr::Var(n) => {
                let slot = self.local(n)?;
                self.push(Instr::Load(slot));
            }
            Expr::Int(n) => self.push(Instr::PushInt(*n)),
            Expr::Bool(b) => self.push(Instr::PushBool(*b)),
            Expr::Unit => self.push(Instr::PushUnit),
            Expr::Array(items) => {
                for i in items {
                    self.emit(i)?;
                }
                self.push(Instr::MakeArray(items.len()));
            }
            Expr::Un(op, inner) => {
                self.emit(inner)?;
                self.push(match op {
                    UnOp::Neg => Instr::Neg,
                    UnOp::Not => Instr::Not,
                });
            }
            Expr::Bin(op, l, r) => self.emit_bin(*op, l, r)?,
            Expr::If(c, t, e) => {
                self.emit(c)?;
                let j_false = self.emit_jump_if_false();
                self.emit(t)?;
                let j_end = self.emit_jump();
                let else_pc = self.code.len();
                self.patch_if(j_false, else_pc);
                self.emit(e)?;
                let end = self.code.len();
                self.patch_jump(j_end, end);
            }
            Expr::Let(n, v, b) => {
                self.emit(v)?;
                let slot = self.alloc_local(n)?;
                self.push(Instr::Store(slot));
                self.emit(b)?;
            }
            Expr::Seq(a, b) => {
                self.emit(a)?;
                self.push(Instr::Pop);
                self.emit(b)?;
            }
            Expr::Index(base, index) => {
                self.emit(base)?;
                self.emit(index)?;
                self.push(Instr::ArrayGet { checked: true });
            }
            Expr::Call { name, args } => self.emit_call(name, args)?,
        }
        Ok(())
    }

    fn emit_bin(&mut self, op: BinOp, l: &Expr, r: &Expr) -> Result<(), String> {
        match op {
            BinOp::And => {
                self.emit(l)?;
                let j_false_a = self.emit_jump_if_false();
                self.emit(r)?;
                let j_false_b = self.emit_jump_if_false();
                self.push(Instr::PushBool(true));
                let j_end = self.emit_jump();
                let false_pc = self.code.len();
                self.patch_if(j_false_a, false_pc);
                self.patch_if(j_false_b, false_pc);
                self.push(Instr::PushBool(false));
                let end = self.code.len();
                self.patch_jump(j_end, end);
            }
            BinOp::Or => {
                self.emit(l)?;
                let j_rhs = self.emit_jump_if_false();
                self.push(Instr::PushBool(true));
                let j_end = self.emit_jump();
                let rhs_pc = self.code.len();
                self.patch_if(j_rhs, rhs_pc);
                self.emit(r)?;
                let end = self.code.len();
                self.patch_jump(j_end, end);
            }
            _ => {
                self.emit(l)?;
                self.emit(r)?;
                self.push(match op {
                    BinOp::Add => Instr::Add,
                    BinOp::Sub => Instr::Sub,
                    BinOp::Mul => Instr::Mul,
                    BinOp::Div => Instr::Div { checked: true },
                    BinOp::Mod => Instr::Mod { checked: true },
                    BinOp::Eq => Instr::Eq,
                    BinOp::Ne => Instr::Ne,
                    BinOp::Lt => Instr::Lt,
                    BinOp::Le => Instr::Le,
                    BinOp::Gt => Instr::Gt,
                    BinOp::Ge => Instr::Ge,
                    BinOp::And | BinOp::Or => unreachable!(),
                });
            }
        }
        Ok(())
    }

    fn emit_call(&mut self, name: &str, args: &[CallArg]) -> Result<(), String> {
        // All call arguments are already runtime-only after ownership checking.
        let exprs: Vec<&Expr> = args
            .iter()
            .filter_map(|a| match a {
                CallArg::Expr(e) => Some(e),
                CallArg::Proof(_) => None,
            })
            .collect();

        match name {
            "len" => {
                self.emit(exprs[0])?;
                self.push(Instr::Len);
            }
            "print" => {
                self.emit(exprs[0])?;
                self.push(Instr::Print);
                self.push(Instr::PushUnit);
            }
            "__set_in_place" | "__set_copy" => {
                for e in &exprs {
                    self.emit(e)?;
                }
                self.push(Instr::ArraySet {
                    in_place: name == "__set_in_place",
                });
            }
            "__push_in_place" | "__push_copy" => {
                for e in &exprs {
                    self.emit(e)?;
                }
                self.push(Instr::ArrayPush {
                    in_place: name == "__push_in_place",
                });
            }
            _ => {
                let func = *self
                    .index
                    .get(name)
                    .ok_or_else(|| format!("unknown function `{name}`"))?;
                for e in &exprs {
                    self.emit(e)?;
                }
                self.push(Instr::Call {
                    func,
                    argc: exprs.len(),
                });
            }
        }
        Ok(())
    }
}

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

fn pop_int(frames: &mut Vec<Frame>) -> Result<i64, String> {
    frames
        .last_mut()
        .unwrap()
        .stack
        .pop()
        .ok_or_else(|| "empty stack".to_string())?
        .as_int()
}

fn pop_bool(frames: &mut Vec<Frame>) -> Result<bool, String> {
    frames
        .last_mut()
        .unwrap()
        .stack
        .pop()
        .ok_or_else(|| "empty stack".to_string())?
        .as_bool()
}

fn pop_array(
    frames: &mut Vec<Frame>,
) -> Result<std::rc::Rc<std::cell::RefCell<Vec<Value>>>, String> {
    frames
        .last_mut()
        .unwrap()
        .stack
        .pop()
        .ok_or_else(|| "empty stack".to_string())?
        .as_array()
}

fn pop2_int(frames: &mut Vec<Frame>) -> Result<(i64, i64), String> {
    let b = pop_int(frames)?;
    let a = pop_int(frames)?;
    Ok((a, b))
}

fn bin_i64<F>(frames: &mut Vec<Frame>, f: F) -> Result<(), String>
where
    F: FnOnce(i64, i64) -> Option<i64>,
{
    let (a, b) = pop2_int(frames)?;
    let n = f(a, b).ok_or_else(|| "integer overflow".to_string())?;
    frames.last_mut().unwrap().stack.push(Value::Int(n));
    Ok(())
}

fn bin_bool<F>(frames: &mut Vec<Frame>, f: F) -> Result<(), String>
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ownership::{check_source, run as interp_run};

    #[test]
    fn vm_runs_ownership_demo() {
        let src = std::fs::read_to_string("examples/ownership.alt").unwrap();
        let checked = check_source(&src).unwrap();
        let bc = compile(&checked).unwrap();
        let r = run(&bc, bc.main).unwrap();
        assert_eq!(r.value.to_string(), "30");
        assert_eq!(r.stats.in_place, 2);
        assert_eq!(r.stats.copies, 2);
        assert_eq!(r.stats.allocations, 5);
        assert_eq!(r.stats.bounds_checks, 9);
    }

    #[test]
    fn vm_matches_interpreter() {
        let src = std::fs::read_to_string("examples/ownership.alt").unwrap();
        let checked = check_source(&src).unwrap();
        let bc = compile(&checked).unwrap();
        let vm = run(&bc, bc.main).unwrap();
        let interp = interp_run(&checked, "main").unwrap();
        assert_eq!(vm.value.to_string(), interp.value.to_string());
        assert_eq!(vm.stats.allocations, interp.stats.allocations);
        assert_eq!(vm.stats.copies, interp.stats.copies);
        assert_eq!(vm.stats.in_place, interp.stats.in_place);
        assert_eq!(vm.stats.bounds_checks, interp.stats.bounds_checks);
    }

    #[test]
    fn vm_erases_proof_parameters() {
        let src = r#"
            fn get(a: Array<Int>, i: Int, h: proof 0 <= i) -> Int { a[i] }
            fn main() -> Int {
              let a = [10, 20, 30];
              get(a, 1, by trivial)
            }
        "#;
        let checked = check_source(src).unwrap();
        let bc = compile(&checked).unwrap();
        let get = bc.get("get").unwrap();
        assert_eq!(get.arity, 2, "proof parameter must be erased");
        let r = run(&bc, bc.main).unwrap();
        assert_eq!(r.value.to_string(), "20");
    }
}
