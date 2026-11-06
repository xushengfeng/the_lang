//! A small stack VM for the ownership-checked surface language.
//!
//! Pipeline:
//!   source -> parser -> ownership checker/lowering -> bytecode -> VM
//!
//! Proof parameters and proof arguments are already removed by the ownership
//! checker, so the VM never sees a proof value.  `set`/`push` arrive as
//! `__set_in_place` / `__set_copy` and are compiled to bytecode variants.

use std::collections::{HashMap, VecDeque};

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BType {
    Int,
    Bool,
    Unit,
    Array(Box<BType>),
}

impl BType {
    pub fn from_ty(ty: &crate::syntax::Ty) -> Result<Self, String> {
        match ty {
            crate::syntax::Ty::Int => Ok(BType::Int),
            crate::syntax::Ty::Bool => Ok(BType::Bool),
            crate::syntax::Ty::Unit => Ok(BType::Unit),
            crate::syntax::Ty::Array(t) => Ok(BType::Array(Box::new(Self::from_ty(t)?))),
            crate::syntax::Ty::Prop => Err("Prop must be erased before bytecode".to_string()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct BcFunction {
    pub name: String,
    pub arity: usize,
    pub param_tys: Vec<BType>,
    pub ret: BType,
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
    let param_tys = f
        .param_tys
        .iter()
        .map(BType::from_ty)
        .collect::<Result<Vec<_>, _>>()?;
    let ret = BType::from_ty(&f.ret)?;
    Ok(BcFunction {
        name: f.name.clone(),
        arity: f.params.len(),
        param_tys,
        ret,
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

    fn one_func(code: Vec<Instr>, ret: BType) -> BcProgram {
        BcProgram {
            funcs: vec![BcFunction {
                name: "bad".into(),
                arity: 0,
                param_tys: vec![],
                ret,
                nlocals: 1,
                code,
            }],
            main: 0,
        }
    }

    #[test]
    fn vm_runs_ownership_demo() {
        let src = std::fs::read_to_string("examples/ownership.alt").unwrap();
        let checked = check_source(&src).unwrap();
        let bc = compile(&checked).unwrap();
        verify(&bc).unwrap();
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
    fn verifier_rejects_stack_underflow() {
        let bc = one_func(vec![Instr::Add, Instr::Ret], BType::Int);
        let err = verify(&bc).unwrap_err();
        assert!(err.contains("underflow"), "{err}");
    }

    #[test]
    fn verifier_rejects_wrong_return_type() {
        let bc = one_func(vec![Instr::PushBool(true), Instr::Ret], BType::Int);
        let err = verify(&bc).unwrap_err();
        assert!(err.contains("ret returns"), "{err}");
    }

    #[test]
    fn verifier_rejects_bad_jump() {
        let bc = one_func(vec![Instr::Jump(99)], BType::Unit);
        let err = verify(&bc).unwrap_err();
        assert!(err.contains("out of range"), "{err}");
    }

    #[test]
    fn verifier_rejects_uninitialized_local() {
        let bc = one_func(vec![Instr::Load(0), Instr::Ret], BType::Int);
        let err = verify(&bc).unwrap_err();
        assert!(err.contains("uninitialized"), "{err}");
    }

    #[test]
    fn verifier_rejects_wrong_call_argument() {
        let callee = BcFunction {
            name: "callee".into(),
            arity: 1,
            param_tys: vec![BType::Int],
            ret: BType::Int,
            nlocals: 1,
            code: vec![Instr::Load(0), Instr::Ret],
        };
        let caller = BcFunction {
            name: "caller".into(),
            arity: 0,
            param_tys: vec![],
            ret: BType::Int,
            nlocals: 0,
            code: vec![
                Instr::PushBool(true),
                Instr::Call { func: 0, argc: 1 },
                Instr::Ret,
            ],
        };
        let bc = BcProgram {
            funcs: vec![callee, caller],
            main: 1,
        };
        let err = verify(&bc).unwrap_err();
        assert!(err.contains("argument"), "{err}");
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
