use std::collections::{HashMap, HashSet};

use super::context::{Ctx, VarInfo};
use super::rules::*;
use super::types::*;
use super::uses::{count_uses, result_borrow};
use crate::syntax::{
    parse, parse_lisp, BinOp, CallArg, Decl, Expr, FnDecl, Mode, ParamKind, Program, Ty, UnOp,
};

pub fn check_program(prog: &Program) -> Result<CheckedProgram, String> {
    let mut checker = Checker::new();
    for d in &prog.decls {
        match d {
            Decl::Fn(f) => checker.collect(f)?,
            Decl::Impl(i) => checker.collect(&i.f)?,
            Decl::Spec(_) | Decl::Theorem(_) | Decl::Proof(_) => {}
        }
    }
    let mut funcs = Vec::new();
    for d in &prog.decls {
        match d {
            Decl::Fn(f) => funcs.push(checker.check_fn(f)?),
            Decl::Impl(i) => funcs.push(checker.check_fn(&i.f)?),
            Decl::Spec(_) | Decl::Theorem(_) | Decl::Proof(_) => {}
        }
    }
    Ok(CheckedProgram { funcs })
}

pub fn check_source(src: &str) -> Result<CheckedProgram, String> {
    let prog = parse(src).map_err(|e| e.to_string())?;
    check_program(&prog)
}

pub fn check_source_lisp(src: &str) -> Result<CheckedProgram, String> {
    let prog = parse_lisp(src).map_err(|e| e.to_string())?;
    check_program(&prog)
}

fn function_ty(sig: &FnSig) -> Ty {
    Ty::Fn(
        sig.params
            .iter()
            .filter(|p| !p.is_proof)
            .map(|p| p.ty.clone())
            .collect(),
        Box::new(sig.ret.clone()),
        Vec::new(),
    )
}

fn call_args_to_exprs(args: &[CallArg]) -> Result<Vec<Expr>, String> {
    args.iter()
        .map(|arg| match arg {
            CallArg::Expr(e) => Ok(e.clone()),
            CallArg::Proof(_) => Err("proof arguments require a named function call".into()),
        })
        .collect()
}

/// Variables referenced by `body` that are not bound by `bound` or by a
/// nested lambda/let.
fn lambda_free_vars(body: &Expr, params: &HashSet<String>) -> Vec<String> {
    fn go(e: &Expr, bound: &mut HashSet<String>, out: &mut Vec<String>) {
        match e {
            Expr::Var(name) => {
                if !bound.contains(name) && !out.contains(name) {
                    out.push(name.clone());
                }
            }
            Expr::Int(_) | Expr::Bool(_) | Expr::Unit | Expr::FnRef(_) => {}
            Expr::Array(items) => {
                for item in items {
                    go(item, bound, out);
                }
            }
            Expr::Un(_, inner) => go(inner, bound, out),
            Expr::Bin(_, l, r) => {
                go(l, bound, out);
                go(r, bound, out);
            }
            Expr::If(c, t, e) => {
                go(c, bound, out);
                go(t, bound, out);
                go(e, bound, out);
            }
            Expr::Let(name, value, body) => {
                go(value, bound, out);
                let inserted = bound.insert(name.clone());
                go(body, bound, out);
                if inserted {
                    bound.remove(name);
                }
            }
            Expr::Call { args, .. } => {
                for arg in args {
                    if let CallArg::Expr(e) = arg {
                        go(e, bound, out);
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
                go(body, bound, out);
                for name in added {
                    bound.remove(&name);
                }
            }
            Expr::Apply { callee, args } => {
                go(callee, bound, out);
                for arg in args {
                    go(arg, bound, out);
                }
            }
            Expr::Index(base, index) => {
                go(base, bound, out);
                go(index, bound, out);
            }
            Expr::Seq(a, b) => {
                go(a, bound, out);
                go(b, bound, out);
            }
        }
    }

    let mut bound = params.clone();
    let mut out = Vec::new();
    go(body, &mut bound, &mut out);
    out
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
        if ty.captures_borrow() {
            return Err(format!(
                "function `{}`: a closure capturing a borrow escapes through the result",
                f.name
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
                if let Some(info) = ctx.vars.get(name) {
                    if info.consumed {
                        return Err(format!("use of moved value `{name}`"));
                    }
                    return Ok((info.ty.clone(), Expr::Var(name.clone())));
                }
                // A bare function name used in expression position is a
                // first-class function value (a callback).
                if let Some(sig) = self.funcs.get(name) {
                    return Ok((function_ty(sig), Expr::FnRef(name.clone())));
                }
                Err(format!("unknown variable `{name}`"))
            }
            Expr::FnRef(name) => {
                let sig = self
                    .funcs
                    .get(name)
                    .ok_or_else(|| format!("unknown function `{name}`"))?;
                Ok((function_ty(sig), Expr::FnRef(name.clone())))
            }
            Expr::Lambda {
                params, ret, body, ..
            } => self.check_lambda(ctx, params, ret, body),
            Expr::Apply { callee, args } => self.check_apply(ctx, callee, args),
            Expr::Int(n) => Ok((Ty::Int, Expr::Int(*n))),
            Expr::Bool(b) => Ok((Ty::Bool, Expr::Bool(*b))),
            Expr::Unit => Ok((Ty::Unit, Expr::Unit)),
            Expr::Array(items) => {
                let mut checked = Vec::new();
                let mut elem_ty: Option<Ty> = None;
                for item in items {
                    let (t, c) = self.check_expr(ctx, item)?;
                    if t.captures_borrow() {
                        return Err(
                            "a closure capturing a borrow cannot be stored in an array".into()
                        );
                    }
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

                let merged = tt.merge_captures(&et);
                Ok((merged, Expr::If(Box::new(cc), Box::new(tc), Box::new(ec))))
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
                if val_t.captures_borrow() {
                    return Err("set stores a closure capturing a borrow".into());
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
                if val_t.captures_borrow() {
                    return Err("push stores a closure capturing a borrow".into());
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
                if !self.funcs.contains_key(name) {
                    if let Ok(info) = ctx.info(name) {
                        if matches!(info.ty, Ty::Fn(..)) {
                            let exprs = call_args_to_exprs(args)?;
                            return self.check_apply(ctx, &Expr::Var(name.to_string()), &exprs);
                        }
                        return Err(format!("`{name}` is not a function"));
                    }
                    return Err(format!("unknown function `{name}`"));
                }
                let sig = self.funcs.get(name).cloned().unwrap();

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
                    if at.captures_borrow() {
                        return Err(format!(
                            "argument `{}` of `{name}` is a closure capturing a borrow; call it directly or keep it in this function",
                            p.name
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

    fn check_lambda(
        &self,
        ctx: &mut Ctx,
        params: &[crate::syntax::LambdaParam],
        ret: &Ty,
        body: &Expr,
    ) -> Result<(Ty, Expr), String> {
        let mut names = HashSet::new();
        for p in params {
            if !names.insert(p.name.clone()) {
                return Err(format!("duplicate lambda parameter `{}`", p.name));
            }
            if ctx.vars.contains_key(&p.name) {
                return Err(format!(
                    "lambda parameter `{}` shadows an outer variable; use a fresh name",
                    p.name
                ));
            }
        }

        let free = lambda_free_vars(body, &names);
        let mut inner = ctx.clone();
        let mut captures = Vec::new();
        let mut borrowed_captures = Vec::new();
        for name in &free {
            let Some(info) = ctx.vars.get(name).cloned() else {
                continue;
            };
            captures.push(name.clone());
            if info.mode == Mode::Borrow {
                borrowed_captures.push(name.clone());
            }
            // A captured function value may itself capture borrows; carrying
            // those through makes the new closure transitively non-escaping.
            for borrow in info.ty.borrow_captures() {
                if !borrowed_captures.contains(borrow) {
                    borrowed_captures.push(borrow.clone());
                }
            }

            // Captured values are read-only inside a closure: the closure may
            // be called more than once, so it may not move or mutate them.
            // Borrow captures keep borrow mode so the body cannot store or
            // return the borrowed array.
            let entry = inner
                .vars
                .get_mut(name)
                .expect("captured variable present in cloned context");
            entry.unique = false;
            entry.consumed = false;
            if info.mode != Mode::Borrow {
                entry.mode = Mode::Val;
                inner.borrow_names.remove(name);
            }
        }

        for p in params {
            inner.vars.insert(
                p.name.clone(),
                VarInfo {
                    ty: p.ty.clone(),
                    mode: Mode::Val,
                    unique: false,
                    consumed: false,
                    uses: *inner.uses.get(&p.name).unwrap_or(&0),
                },
            );
        }

        let (body_ty, body) = self.check_expr(&mut inner, body)?;
        if body_ty != *ret {
            return Err(format!("lambda returns `{body_ty}`, expected `{ret}`"));
        }
        if body_ty.captures_borrow() {
            return Err("lambda returns a closure capturing a borrow, which would escape".into());
        }
        if let Some(name) = result_borrow(&body, ret, &inner.borrow_names) {
            return Err(format!(
                "lambda returns borrow `{name}`, which would escape"
            ));
        }
        for name in &captures {
            if inner.vars.get(name).map(|i| i.consumed).unwrap_or(false) {
                return Err(format!("lambda may not consume captured value `{name}`"));
            }
        }
        for p in params {
            inner.vars.remove(&p.name);
        }

        let ty = Ty::Fn(
            params.iter().map(|p| p.ty.clone()).collect(),
            Box::new(ret.clone()),
            borrowed_captures,
        );
        Ok((
            ty,
            Expr::Lambda {
                params: params.to_vec(),
                ret: ret.clone(),
                body: Box::new(body),
                captures,
            },
        ))
    }

    fn check_apply(
        &self,
        ctx: &mut Ctx,
        callee: &Expr,
        args: &[Expr],
    ) -> Result<(Ty, Expr), String> {
        let (callee_ty, callee) = self.check_expr(ctx, callee)?;
        let (param_tys, ret) = match callee_ty {
            Ty::Fn(params, ret, _) => (params, *ret),
            other => return Err(format!("cannot call value of type `{other}`")),
        };
        if args.len() != param_tys.len() {
            return Err(format!(
                "function value expects {} arguments, got {}",
                param_tys.len(),
                args.len()
            ));
        }
        let mut checked = Vec::with_capacity(args.len());
        for (arg, expected) in args.iter().zip(&param_tys) {
            let (ty, expr) = self.check_expr(ctx, arg)?;
            if ty != *expected {
                return Err(format!(
                    "function argument has type `{ty}`, expected `{expected}`"
                ));
            }
            if ty.captures_borrow() {
                return Err(
                    "a closure capturing a borrow cannot be passed as an argument; call it directly"
                        .into(),
                );
            }
            checked.push(expr);
        }
        Ok((
            ret,
            Expr::Apply {
                callee: Box::new(callee),
                args: checked,
            },
        ))
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
