use std::collections::{HashMap, HashSet};

use super::ir::{BType, BcFunction, BcProgram, Instr};
use crate::ownership::{CheckedFn, CheckedProgram};
use crate::syntax::{BinOp, CallArg, Expr, LambdaParam, UnOp};

pub fn compile(prog: &CheckedProgram) -> Result<BcProgram, String> {
    let mut index = HashMap::new();
    for (i, f) in prog.funcs.iter().enumerate() {
        index.insert(f.name.clone(), i);
    }
    let main = *index
        .get("main")
        .ok_or_else(|| "program has no `main` function".to_string())?;

    let mut funcs = Vec::new();
    let mut lifted: Vec<Option<BcFunction>> = Vec::new();
    let mut next_func = prog.funcs.len();
    for f in &prog.funcs {
        funcs.push(compile_fn(prog, &index, f, &mut next_func, &mut lifted)?);
    }
    for (i, f) in lifted.into_iter().enumerate() {
        funcs.push(f.ok_or_else(|| format!("internal: missing lifted function {i}"))?);
    }
    Ok(BcProgram { funcs, main })
}

fn compile_fn(
    prog: &CheckedProgram,
    index: &HashMap<String, usize>,
    f: &CheckedFn,
    next_func: &mut usize,
    lifted: &mut Vec<Option<BcFunction>>,
) -> Result<BcFunction, String> {
    let mut c = Compiler::new(
        prog,
        index,
        f.name.clone(),
        prog.funcs.len(),
        next_func,
        lifted,
    );
    for (i, p) in f.params.iter().enumerate() {
        let ty = BType::from_ty(&f.param_tys[i])?;
        c.locals.insert(p.clone(), i);
        c.local_tys.insert(p.clone(), ty);
        c.next_slot = i + 1;
    }
    let body_ty = c
        .emit(&f.body)
        .map_err(|e| format!("while compiling `{}`:\n{e}", f.name))?;
    let ret = BType::from_ty(&f.ret)?;
    if body_ty != ret {
        return Err(format!(
            "while compiling `{}`:\nbody has type {body_ty:?}, expected {ret:?}",
            f.name
        ));
    }
    c.code.push(Instr::Ret);
    let param_tys = f
        .param_tys
        .iter()
        .map(BType::from_ty)
        .collect::<Result<Vec<_>, _>>()?;
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
    prog: &'a CheckedProgram,
    index: &'a HashMap<String, usize>,
    owner: String,
    base: usize,
    next_func: &'a mut usize,
    lifted: &'a mut Vec<Option<BcFunction>>,
    code: Vec<Instr>,
    locals: HashMap<String, usize>,
    local_tys: HashMap<String, BType>,
    next_slot: usize,
}

impl<'a> Compiler<'a> {
    fn new(
        prog: &'a CheckedProgram,
        index: &'a HashMap<String, usize>,
        owner: String,
        base: usize,
        next_func: &'a mut usize,
        lifted: &'a mut Vec<Option<BcFunction>>,
    ) -> Self {
        Self {
            prog,
            index,
            owner,
            base,
            next_func,
            lifted,
            code: Vec::new(),
            locals: HashMap::new(),
            local_tys: HashMap::new(),
            next_slot: 0,
        }
    }

    fn push(&mut self, i: Instr) {
        self.code.push(i);
    }

    fn alloc_local(&mut self, name: &str, ty: BType) -> Result<usize, String> {
        if self.locals.contains_key(name) {
            return Err(format!("local `{name}` already has a slot"));
        }
        let slot = self.next_slot;
        self.next_slot += 1;
        self.locals.insert(name.to_string(), slot);
        self.local_tys.insert(name.to_string(), ty);
        Ok(slot)
    }

    fn local(&self, name: &str) -> Result<(usize, BType), String> {
        let slot = self
            .locals
            .get(name)
            .copied()
            .ok_or_else(|| format!("unknown local `{name}`"))?;
        let ty = self
            .local_tys
            .get(name)
            .cloned()
            .ok_or_else(|| format!("missing type for local `{name}`"))?;
        Ok((slot, ty))
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

    fn emit(&mut self, e: &Expr) -> Result<BType, String> {
        match e {
            Expr::Var(n) => {
                let (slot, ty) = self.local(n)?;
                self.push(Instr::Load(slot));
                Ok(ty)
            }
            Expr::FnRef(name) => {
                let func = *self
                    .index
                    .get(name)
                    .ok_or_else(|| format!("unknown function `{name}`"))?;
                let target = &self.prog.funcs[func];
                self.push(Instr::MakeClosure { func, captures: 0 });
                Ok(BType::Fn(
                    target
                        .param_tys
                        .iter()
                        .map(BType::from_ty)
                        .collect::<Result<Vec<_>, _>>()?,
                    Box::new(BType::from_ty(&target.ret)?),
                ))
            }
            Expr::Lambda {
                params, ret, body, ..
            } => self.emit_lambda(params, ret, body),
            Expr::Apply { callee, args } => {
                let callee_ty = self.emit(callee)?;
                let (param_tys, ret) = match callee_ty {
                    BType::Fn(params, ret) => (params, *ret),
                    other => {
                        return Err(format!("cannot call value of type {other:?}"));
                    }
                };
                if args.len() != param_tys.len() {
                    return Err(format!(
                        "function value expects {} arguments, got {}",
                        param_tys.len(),
                        args.len()
                    ));
                }
                for (arg, expected) in args.iter().zip(&param_tys) {
                    let actual = self.emit(arg)?;
                    if actual != *expected {
                        return Err(format!(
                            "function argument has type {actual:?}, expected {expected:?}"
                        ));
                    }
                }
                self.push(Instr::CallClosure { argc: args.len() });
                Ok(ret)
            }
            Expr::Int(n) => {
                self.push(Instr::PushInt(*n));
                Ok(BType::Int)
            }
            Expr::Bool(b) => {
                self.push(Instr::PushBool(*b));
                Ok(BType::Bool)
            }
            Expr::Unit => {
                self.push(Instr::PushUnit);
                Ok(BType::Unit)
            }
            Expr::Array(items) => {
                if items.is_empty() {
                    return Err("empty array literal needs an explicit type".into());
                }
                let mut elem_ty = None;
                for i in items {
                    let ty = self.emit(i)?;
                    if let Some(expected) = &elem_ty {
                        if *expected != ty {
                            return Err(format!(
                                "array elements have different types: {expected:?} and {ty:?}"
                            ));
                        }
                    } else {
                        elem_ty = Some(ty);
                    }
                }
                self.push(Instr::MakeArray(items.len()));
                Ok(BType::Array(Box::new(elem_ty.unwrap())))
            }
            Expr::Un(op, inner) => {
                let ty = self.emit(inner)?;
                match op {
                    UnOp::Neg if ty == BType::Int => self.push(Instr::Neg),
                    UnOp::Not if ty == BType::Bool => self.push(Instr::Not),
                    UnOp::Neg => {
                        return Err(format!("unary `-` expects Int, got {ty:?}"));
                    }
                    UnOp::Not => {
                        return Err(format!("unary `!` expects Bool, got {ty:?}"));
                    }
                }
                Ok(match op {
                    UnOp::Neg => BType::Int,
                    UnOp::Not => BType::Bool,
                })
            }
            Expr::Bin(op, l, r) => self.emit_bin(*op, l, r),
            Expr::If(c, t, e) => {
                let cond = self.emit(c)?;
                if cond != BType::Bool {
                    return Err(format!("if condition has type {cond:?}, expected Bool"));
                }
                let j_false = self.emit_jump_if_false();
                let then_ty = self.emit(t)?;
                let j_end = self.emit_jump();
                let else_pc = self.code.len();
                self.patch_if(j_false, else_pc);
                let else_ty = self.emit(e)?;
                let end = self.code.len();
                self.patch_jump(j_end, end);
                if then_ty != else_ty {
                    return Err(format!(
                        "if branches have different types: {then_ty:?} and {else_ty:?}"
                    ));
                }
                Ok(then_ty)
            }
            Expr::Let(n, v, b) => {
                let vt = self.emit(v)?;
                let slot = self.alloc_local(n, vt)?;
                self.push(Instr::Store(slot));
                self.emit(b)
            }
            Expr::Seq(a, b) => {
                self.emit(a)?;
                self.push(Instr::Pop);
                self.emit(b)
            }
            Expr::Index(base, index) => {
                let base_ty = self.emit(base)?;
                let index_ty = self.emit(index)?;
                if index_ty != BType::Int {
                    return Err(format!("index has type {index_ty:?}, expected Int"));
                }
                match base_ty {
                    BType::Array(t) => {
                        self.push(Instr::ArrayGet { checked: true });
                        Ok(*t)
                    }
                    other => Err(format!("cannot index value of type {other:?}")),
                }
            }
            Expr::Call { name, args } => self.emit_call(name, args),
        }
    }

    fn emit_lambda(
        &mut self,
        params: &[LambdaParam],
        ret: &crate::syntax::Ty,
        body: &Expr,
    ) -> Result<BType, String> {
        let mut bound: HashSet<String> = params.iter().map(|p| p.name.clone()).collect();
        let mut free = Vec::new();
        free_vars(body, &mut bound, &mut free);

        let mut capture_names = Vec::new();
        let mut capture_tys = Vec::new();
        for name in free {
            if self.locals.contains_key(&name) {
                let ty = self
                    .local_tys
                    .get(&name)
                    .cloned()
                    .ok_or_else(|| format!("missing type for captured local `{name}`"))?;
                capture_names.push(name);
                capture_tys.push(ty);
            }
        }

        let lambda_param_tys = params
            .iter()
            .map(|p| BType::from_ty(&p.ty))
            .collect::<Result<Vec<_>, _>>()?;
        let ret_ty = BType::from_ty(ret)?;

        let func = *self.next_func;
        *self.next_func += 1;

        let mut inner = Compiler::new(
            self.prog,
            self.index,
            format!("{}::lambda{}", self.owner, func - self.base),
            self.base,
            &mut *self.next_func,
            &mut *self.lifted,
        );
        for (name, ty) in capture_names.iter().zip(&capture_tys) {
            let slot = inner.next_slot;
            inner.next_slot += 1;
            inner.locals.insert(name.clone(), slot);
            inner.local_tys.insert(name.clone(), ty.clone());
        }
        for (p, ty) in params.iter().zip(&lambda_param_tys) {
            let slot = inner.next_slot;
            inner.next_slot += 1;
            inner.locals.insert(p.name.clone(), slot);
            inner.local_tys.insert(p.name.clone(), ty.clone());
        }
        let body_ty = inner.emit(body)?;
        if body_ty != ret_ty {
            return Err(format!(
                "lambda body has type {body_ty:?}, expected {ret_ty:?}"
            ));
        }
        inner.push(Instr::Ret);
        let mut all_param_tys = capture_tys.clone();
        all_param_tys.extend(lambda_param_tys.iter().cloned());
        let lifted = BcFunction {
            name: format!("{}::lambda{}", self.owner, func - self.base),
            arity: all_param_tys.len(),
            param_tys: all_param_tys,
            ret: ret_ty.clone(),
            nlocals: inner.next_slot,
            code: inner.code,
        };
        while self.lifted.len() <= func - self.base {
            self.lifted.push(None);
        }
        self.lifted[func - self.base] = Some(lifted);

        for name in &capture_names {
            let (slot, _) = self.local(name)?;
            self.push(Instr::Load(slot));
        }
        self.push(Instr::MakeClosure {
            func,
            captures: capture_names.len(),
        });
        Ok(BType::Fn(lambda_param_tys, Box::new(ret_ty)))
    }

    fn emit_bin(&mut self, op: BinOp, l: &Expr, r: &Expr) -> Result<BType, String> {
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
                Ok(BType::Bool)
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
                Ok(BType::Bool)
            }
            _ => {
                self.emit(l)?;
                self.emit(r)?;
                let ty = match op {
                    BinOp::Add => {
                        self.push(Instr::Add);
                        BType::Int
                    }
                    BinOp::Sub => {
                        self.push(Instr::Sub);
                        BType::Int
                    }
                    BinOp::Mul => {
                        self.push(Instr::Mul);
                        BType::Int
                    }
                    BinOp::Div => {
                        self.push(Instr::Div { checked: true });
                        BType::Int
                    }
                    BinOp::Mod => {
                        self.push(Instr::Mod { checked: true });
                        BType::Int
                    }
                    BinOp::Eq => {
                        self.push(Instr::Eq);
                        BType::Bool
                    }
                    BinOp::Ne => {
                        self.push(Instr::Ne);
                        BType::Bool
                    }
                    BinOp::Lt => {
                        self.push(Instr::Lt);
                        BType::Bool
                    }
                    BinOp::Le => {
                        self.push(Instr::Le);
                        BType::Bool
                    }
                    BinOp::Gt => {
                        self.push(Instr::Gt);
                        BType::Bool
                    }
                    BinOp::Ge => {
                        self.push(Instr::Ge);
                        BType::Bool
                    }
                    BinOp::And | BinOp::Or => unreachable!(),
                };
                Ok(ty)
            }
        }
    }

    fn emit_call(&mut self, name: &str, args: &[CallArg]) -> Result<BType, String> {
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
                let ty = self.emit(exprs[0])?;
                match ty {
                    BType::Array(_) => {
                        self.push(Instr::Len);
                        Ok(BType::Int)
                    }
                    other => Err(format!("len expects an array, got {other:?}")),
                }
            }
            "print" => {
                self.emit(exprs[0])?;
                self.push(Instr::Print);
                self.push(Instr::PushUnit);
                Ok(BType::Unit)
            }
            "__set_in_place" | "__set_copy" => {
                let mut target_ty = None;
                for (i, e) in exprs.iter().enumerate() {
                    let ty = self.emit(e)?;
                    if i == 0 {
                        target_ty = Some(ty);
                    }
                }
                let Some(BType::Array(elem)) = target_ty else {
                    return Err("set target must be an array".into());
                };
                self.push(Instr::ArraySet {
                    in_place: name == "__set_in_place",
                });
                Ok(BType::Array(elem))
            }
            "__push_in_place" | "__push_copy" => {
                let mut target_ty = None;
                for (i, e) in exprs.iter().enumerate() {
                    let ty = self.emit(e)?;
                    if i == 0 {
                        target_ty = Some(ty);
                    }
                }
                let Some(BType::Array(elem)) = target_ty else {
                    return Err("push target must be an array".into());
                };
                self.push(Instr::ArrayPush {
                    in_place: name == "__push_in_place",
                });
                Ok(BType::Array(elem))
            }
            _ => {
                let func = *self
                    .index
                    .get(name)
                    .ok_or_else(|| format!("unknown function `{name}`"))?;
                let target = &self.prog.funcs[func];
                if exprs.len() != target.params.len() {
                    return Err(format!(
                        "function `{name}` expects {} arguments, got {}",
                        target.params.len(),
                        exprs.len()
                    ));
                }
                for e in &exprs {
                    self.emit(e)?;
                }
                self.push(Instr::Call {
                    func,
                    argc: exprs.len(),
                });
                BType::from_ty(&target.ret)
            }
        }
    }
}

fn free_vars(e: &Expr, bound: &mut HashSet<String>, out: &mut Vec<String>) {
    match e {
        Expr::Var(name) => {
            if !bound.contains(name) && !out.contains(name) {
                out.push(name.clone());
            }
        }
        Expr::Int(_) | Expr::Bool(_) | Expr::Unit | Expr::FnRef(_) => {}
        Expr::Array(items) => {
            for item in items {
                free_vars(item, bound, out);
            }
        }
        Expr::Un(_, inner) => free_vars(inner, bound, out),
        Expr::Bin(_, l, r) => {
            free_vars(l, bound, out);
            free_vars(r, bound, out);
        }
        Expr::If(c, t, e) => {
            free_vars(c, bound, out);
            free_vars(t, bound, out);
            free_vars(e, bound, out);
        }
        Expr::Let(name, value, body) => {
            free_vars(value, bound, out);
            let inserted = bound.insert(name.clone());
            free_vars(body, bound, out);
            if inserted {
                bound.remove(name);
            }
        }
        Expr::Call { args, .. } => {
            for arg in args {
                if let CallArg::Expr(e) = arg {
                    free_vars(e, bound, out);
                }
            }
        }
        Expr::Lambda { params, body, .. } => {
            let mut added = Vec::new();
            for p in params {
                if bound.insert(p.name.clone()) {
                    added.push(p.name.clone());
                }
            }
            free_vars(body, bound, out);
            for name in added {
                bound.remove(&name);
            }
        }
        Expr::Apply { callee, args } => {
            free_vars(callee, bound, out);
            for arg in args {
                free_vars(arg, bound, out);
            }
        }
        Expr::Index(base, index) => {
            free_vars(base, bound, out);
            free_vars(index, bound, out);
        }
        Expr::Seq(a, b) => {
            free_vars(a, bound, out);
            free_vars(b, bound, out);
        }
    }
}
