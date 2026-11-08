use std::collections::{HashMap, HashSet};

use super::context::{Ctx, VarInfo};
use super::rules::*;
use super::types::*;
use super::uses::{count_uses, result_borrow};
use crate::syntax::{
    parse, BinOp, CallArg, Decl, Expr, FnDecl, Mode, ParamKind, Program, Ty, UnOp,
};

pub fn check_program(prog: &Program) -> Result<CheckedProgram, String> {
    let mut checker = Checker::new();
    for d in &prog.decls {
        match d {
            Decl::Fn(f) => checker.collect(f)?,
            Decl::Impl(i) => checker.collect(&i.f)?,
            Decl::Spec(_) | Decl::Theorem(_) => {}
        }
    }
    let mut funcs = Vec::new();
    for d in &prog.decls {
        match d {
            Decl::Fn(f) => funcs.push(checker.check_fn(f)?),
            Decl::Impl(i) => funcs.push(checker.check_fn(&i.f)?),
            Decl::Spec(_) | Decl::Theorem(_) => {}
        }
    }
    Ok(CheckedProgram { funcs })
}

pub fn check_source(src: &str) -> Result<CheckedProgram, String> {
    let prog = parse(src).map_err(|e| e.to_string())?;
    check_program(&prog)
}

// ---------------------------------------------------------------------------
// Checker
// ---------------------------------------------------------------------------

struct Checker {
    funcs: HashMap<String, FnSig>,
}

impl Checker {
    fn new() -> Self {
        Self {
            funcs: HashMap::new(),
        }
    }

    fn collect(&mut self, f: &FnDecl) -> Result<(), String> {
        if self.funcs.contains_key(&f.name) {
            return Err(format!("duplicate function `{}`", f.name));
        }
        let mut seen = HashSet::new();
        let mut params = Vec::new();
        for p in &f.params {
            if !seen.insert(p.name.clone()) {
                return Err(format!("duplicate parameter `{}`", p.name));
            }
            match &p.kind {
                ParamKind::Runtime(ty) => {
                    if p.mode != Mode::Val && !matches!(ty, Ty::Array(_)) {
                        return Err(format!(
                            "`{}` parameter `{}` must be an array",
                            mode_name(&p.mode),
                            p.name
                        ));
                    }
                    params.push(ParamSig {
                        name: p.name.clone(),
                        ty: ty.clone(),
                        mode: p.mode.clone(),
                        is_proof: false,
                    });
                }
                ParamKind::Proof(_) => params.push(ParamSig {
                    name: p.name.clone(),
                    ty: Ty::Unit,
                    mode: Mode::Val,
                    is_proof: true,
                }),
            }
        }
        self.funcs.insert(
            f.name.clone(),
            FnSig {
                name: f.name.clone(),
                params,
                ret: f.ret.clone(),
            },
        );
        Ok(())
    }

    fn check_fn(&self, f: &FnDecl) -> Result<CheckedFn, String> {
        let mut uses = HashMap::new();
        count_uses(&f.body, &mut uses);

        let mut ctx = Ctx {
            vars: HashMap::new(),
            uses,
            borrow_names: HashSet::new(),
        };

        for p in &f.params {
            if let ParamKind::Runtime(ty) = &p.kind {
                let unique = p.mode == Mode::Own;
                if p.mode == Mode::Borrow {
                    ctx.borrow_names.insert(p.name.clone());
                }
                ctx.vars.insert(
                    p.name.clone(),
                    VarInfo {
                        ty: ty.clone(),
                        mode: p.mode.clone(),
                        unique,
                        consumed: false,
                        uses: *ctx.uses.get(&p.name).unwrap_or(&0),
                    },
                );
            }
        }

        let (ty, body) = self.check_expr(&mut ctx, &f.body)?;
        if ty != f.ret {
            return Err(format!(
                "function `{}` returns `{}`, expected `{}`",
                f.name, ty, f.ret
            ));
        }

        // A borrow may not escape through the result.
        if let Some(name) = result_borrow(&body, &f.ret, &ctx.borrow_names) {
            return Err(format!(
                "function `{}`: borrow `{name}` escapes through the result",
                f.name
            ));
        }

        for (name, info) in &ctx.vars {
            if info.mode == Mode::Borrow && info.consumed {
                return Err(format!(
                    "function `{}`: borrow `{name}` was consumed",
                    f.name
                ));
            }
        }

        let mut params = Vec::new();
        let mut param_tys = Vec::new();
        for p in &f.params {
            if let ParamKind::Runtime(ty) = &p.kind {
                params.push(p.name.clone());
                param_tys.push(ty.clone());
            }
        }

        Ok(CheckedFn {
            name: f.name.clone(),
            params,
            param_tys,
            ret: f.ret.clone(),
            body,
        })
    }

    fn check_expr(&self, ctx: &mut Ctx, e: &Expr) -> Result<(Ty, Expr), String> {
        match e {
            Expr::Var(name) => {
                let info = ctx.info(name)?;
                if info.consumed {
                    return Err(format!("use of moved value `{name}`"));
                }
                Ok((info.ty.clone(), Expr::Var(name.clone())))
            }
            Expr::Int(n) => Ok((Ty::Int, Expr::Int(*n))),
            Expr::Bool(b) => Ok((Ty::Bool, Expr::Bool(*b))),
            Expr::Unit => Ok((Ty::Unit, Expr::Unit)),
            Expr::Array(items) => {
                let mut checked = Vec::new();
                let mut elem_ty: Option<Ty> = None;
                for item in items {
                    let (t, c) = self.check_expr(ctx, item)?;
                    if let Some(expected) = &elem_ty {
                        if *expected != t {
                            return Err(format!(
                                "array elements have different types: `{expected}` and `{t}`"
                            ));
                        }
                    } else {
                        elem_ty = Some(t);
                    }
                    checked.push(c);
                }
                let t = elem_ty.ok_or_else(|| "cannot infer type of empty array".to_string())?;
                Ok((Ty::Array(Box::new(t)), Expr::Array(checked)))
            }
            Expr::Un(op, inner) => {
                let (t, c) = self.check_expr(ctx, inner)?;
                match op {
                    UnOp::Neg => {
                        expect_ty(&t, &Ty::Int, "unary `-`")?;
                        Ok((Ty::Int, Expr::Un(*op, Box::new(c))))
                    }
                    UnOp::Not => {
                        expect_ty(&t, &Ty::Bool, "unary `!`")?;
                        Ok((Ty::Bool, Expr::Un(*op, Box::new(c))))
                    }
                }
            }
            Expr::Bin(op, l, r) => {
                let (lt, lc) = self.check_expr(ctx, l)?;
                let (rt, rc) = self.check_expr(ctx, r)?;
                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                        expect_ty(&lt, &Ty::Int, "arithmetic")?;
                        expect_ty(&rt, &Ty::Int, "arithmetic")?;
                        Ok((Ty::Int, Expr::Bin(*op, Box::new(lc), Box::new(rc))))
                    }
                    BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        expect_ty(&lt, &Ty::Int, "comparison")?;
                        expect_ty(&rt, &Ty::Int, "comparison")?;
                        Ok((Ty::Bool, Expr::Bin(*op, Box::new(lc), Box::new(rc))))
                    }
                    BinOp::Eq | BinOp::Ne => {
                        if lt != rt {
                            return Err(format!("cannot compare `{lt}` with `{rt}`"));
                        }
                        if lt != Ty::Int {
                            return Err(format!(
                                "`==`/`!=` currently only supported for Int, got `{lt}`"
                            ));
                        }
                        Ok((Ty::Bool, Expr::Bin(*op, Box::new(lc), Box::new(rc))))
                    }
                    BinOp::And | BinOp::Or => {
                        expect_ty(&lt, &Ty::Bool, "boolean operator")?;
                        expect_ty(&rt, &Ty::Bool, "boolean operator")?;
                        Ok((Ty::Bool, Expr::Bin(*op, Box::new(lc), Box::new(rc))))
                    }
                }
            }
            Expr::If(cond, then, els) => {
                let (ct, cc) = self.check_expr(ctx, cond)?;
                expect_ty(&ct, &Ty::Bool, "if condition")?;

                let mut then_ctx = ctx.clone();
                let mut else_ctx = ctx.clone();
                let (tt, tc) = self.check_expr(&mut then_ctx, then)?;
                let (et, ec) = self.check_expr(&mut else_ctx, els)?;
                if tt != et {
                    return Err(format!(
                        "if branches have different types: `{tt}` and `{et}`"
                    ));
                }

                // Conservative: if either branch consumes an outer value, it is
                // considered consumed after the if.
                for (name, info) in ctx.vars.iter_mut() {
                    if then_ctx.vars.get(name).map(|i| i.consumed).unwrap_or(false)
                        || else_ctx.vars.get(name).map(|i| i.consumed).unwrap_or(false)
                    {
                        info.consumed = true;
                    }
                }
                ctx.borrow_names.extend(then_ctx.borrow_names);
                ctx.borrow_names.extend(else_ctx.borrow_names);

                Ok((tt, Expr::If(Box::new(cc), Box::new(tc), Box::new(ec))))
            }
            Expr::Let(name, value, body) => {
                if ctx.vars.contains_key(name) {
                    return Err(format!(
                        "shadowing `{name}` is not supported; use a fresh name"
                    ));
                }
                let (vt, cv) = self.check_expr(ctx, value)?;

                let (mode, unique) = match value.as_ref() {
                    Expr::Var(v) => {
                        let info = ctx.info(v)?.clone();
                        if info.consumed {
                            return Err(format!("use of moved value `{v}`"));
                        }
                        match info.mode {
                            Mode::Own => {
                                ctx.info_mut(v)?.consumed = true;
                                (Mode::Own, true)
                            }
                            Mode::Borrow => {
                                ctx.borrow_names.insert(name.clone());
                                (Mode::Borrow, false)
                            }
                            Mode::Val => (Mode::Val, false),
                        }
                    }
                    _ if matches!(vt, Ty::Array(_)) && is_fresh(value) => (Mode::Own, true),
                    _ => (Mode::Val, false),
                };

                let uses = *ctx.uses.get(name).unwrap_or(&0);
                ctx.vars.insert(
                    name.clone(),
                    VarInfo {
                        ty: vt.clone(),
                        mode,
                        unique,
                        consumed: false,
                        uses,
                    },
                );
                let (bt, cb) = self.check_expr(ctx, body)?;
                ctx.vars.remove(name);
                Ok((bt, Expr::Let(name.clone(), Box::new(cv), Box::new(cb))))
            }
            Expr::Call { name, args } => self.check_call(ctx, name, args),
            Expr::Index(base, index) => {
                let (bt, cb) = self.check_expr(ctx, base)?;
                let elem = match bt {
                    Ty::Array(t) => *t,
                    other => return Err(format!("cannot index `{other}`")),
                };
                let (it, ci) = self.check_expr(ctx, index)?;
                expect_ty(&it, &Ty::Int, "index")?;
                Ok((elem, Expr::Index(Box::new(cb), Box::new(ci))))
            }
            Expr::Seq(a, b) => {
                let (_, ca) = self.check_expr(ctx, a)?;
                let (bt, cb) = self.check_expr(ctx, b)?;
                Ok((bt, Expr::Seq(Box::new(ca), Box::new(cb))))
            }
        }
    }

    fn check_call(
        &self,
        ctx: &mut Ctx,
        name: &str,
        args: &[CallArg],
    ) -> Result<(Ty, Expr), String> {
        match name {
            "len" => {
                let (t, c) = self.one_expr_arg(ctx, args, "len")?;
                match t {
                    Ty::Array(_) => Ok((
                        Ty::Int,
                        Expr::Call {
                            name: "len".into(),
                            args: vec![CallArg::Expr(c)],
                        },
                    )),
                    other => Err(format!("len expects an array, got `{other}`")),
                }
            }
            "print" => {
                let (_, c) = self.one_expr_arg(ctx, args, "print")?;
                Ok((
                    Ty::Unit,
                    Expr::Call {
                        name: "print".into(),
                        args: vec![CallArg::Expr(c)],
                    },
                ))
            }
            "set" => {
                let (base_t, base_c) = self.nth_expr_arg(ctx, args, 0, "set")?;
                let elem = match base_t {
                    Ty::Array(t) => *t,
                    other => return Err(format!("set expects an array, got `{other}`")),
                };
                let (idx_t, idx_c) = self.nth_expr_arg(ctx, args, 1, "set")?;
                expect_ty(&idx_t, &Ty::Int, "set index")?;
                let (val_t, val_c) = self.nth_expr_arg(ctx, args, 2, "set")?;
                if val_t != elem {
                    return Err(format!("set value has type `{val_t}`, expected `{elem}`"));
                }
                if let Some(b) = result_borrow(&val_c, &elem, &ctx.borrow_names) {
                    return Err(format!("set stores borrow `{b}`"));
                }

                let in_place = can_in_place(ctx, args.first().and_then(expr_arg));
                if in_place {
                    consume_expr(ctx, args.first().and_then(expr_arg))?;
                }
                Ok((
                    Ty::Array(Box::new(elem)),
                    Expr::Call {
                        name: if in_place {
                            "__set_in_place".into()
                        } else {
                            "__set_copy".into()
                        },
                        args: vec![
                            CallArg::Expr(base_c),
                            CallArg::Expr(idx_c),
                            CallArg::Expr(val_c),
                        ],
                    },
                ))
            }
            "push" => {
                let (base_t, base_c) = self.nth_expr_arg(ctx, args, 0, "push")?;
                let elem = match base_t {
                    Ty::Array(t) => *t,
                    other => return Err(format!("push expects an array, got `{other}`")),
                };
                let (val_t, val_c) = self.nth_expr_arg(ctx, args, 1, "push")?;
                if val_t != elem {
                    return Err(format!("push value has type `{val_t}`, expected `{elem}`"));
                }
                if let Some(b) = result_borrow(&val_c, &elem, &ctx.borrow_names) {
                    return Err(format!("push stores borrow `{b}`"));
                }

                let in_place = can_in_place(ctx, args.first().and_then(expr_arg));
                if in_place {
                    consume_expr(ctx, args.first().and_then(expr_arg))?;
                }
                Ok((
                    Ty::Array(Box::new(elem)),
                    Expr::Call {
                        name: if in_place {
                            "__push_in_place".into()
                        } else {
                            "__push_copy".into()
                        },
                        args: vec![CallArg::Expr(base_c), CallArg::Expr(val_c)],
                    },
                ))
            }
            _ => {
                let sig = self
                    .funcs
                    .get(name)
                    .cloned()
                    .ok_or_else(|| format!("unknown function `{name}`"))?;

                let mut runtime_args = Vec::new();
                let mut arg_i = 0usize;
                for p in &sig.params {
                    if p.is_proof {
                        // Proof arguments are erased from the runtime call.  If
                        // one is present, skip it.
                        if let Some(CallArg::Proof(_)) = args.get(arg_i) {
                            arg_i += 1;
                        }
                        continue;
                    }
                    let arg = match args.get(arg_i) {
                        Some(CallArg::Expr(e)) => e,
                        Some(CallArg::Proof(_)) => {
                            return Err(format!(
                                "function `{name}` expects a runtime argument for `{}`",
                                p.name
                            ))
                        }
                        None => {
                            return Err(format!(
                                "function `{name}` is missing argument `{}`",
                                p.name
                            ))
                        }
                    };
                    arg_i += 1;
                    let (at, ac) = self.check_expr(ctx, arg)?;
                    if at != p.ty {
                        return Err(format!(
                            "argument `{}` of `{name}` has type `{at}`, expected `{}`",
                            p.name, p.ty
                        ));
                    }
                    match p.mode {
                        Mode::Own => transfer_arg(ctx, arg, &p.name, name)?,
                        Mode::Borrow => borrow_arg(ctx, arg, &p.name, name)?,
                        Mode::Val => read_arg(ctx, arg, &p.name, name)?,
                    }
                    runtime_args.push(CallArg::Expr(ac));
                }

                // Extra runtime arguments are rejected; extra proof arguments
                // are ignored.
                while arg_i < args.len() {
                    match &args[arg_i] {
                        CallArg::Proof(_) => arg_i += 1,
                        CallArg::Expr(_) => return Err(format!("too many arguments to `{name}`")),
                    }
                }

                Ok((
                    sig.ret.clone(),
                    Expr::Call {
                        name: name.to_string(),
                        args: runtime_args,
                    },
                ))
            }
        }
    }

    fn one_expr_arg(
        &self,
        ctx: &mut Ctx,
        args: &[CallArg],
        what: &str,
    ) -> Result<(Ty, Expr), String> {
        match args {
            [CallArg::Expr(e)] => self.check_expr(ctx, e),
            _ => Err(format!("`{what}` expects exactly one runtime argument")),
        }
    }

    fn nth_expr_arg(
        &self,
        ctx: &mut Ctx,
        args: &[CallArg],
        i: usize,
        what: &str,
    ) -> Result<(Ty, Expr), String> {
        match args.get(i) {
            Some(CallArg::Expr(e)) => self.check_expr(ctx, e),
            _ => Err(format!("`{what}` is missing runtime argument {}", i + 1)),
        }
    }
}
