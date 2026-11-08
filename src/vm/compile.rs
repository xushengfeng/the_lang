use std::collections::HashMap;

use super::ir::{BType, BcFunction, BcProgram, Instr};
use crate::ownership::{CheckedFn, CheckedProgram};
use crate::syntax::{BinOp, CallArg, Expr, UnOp};

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
