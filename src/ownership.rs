//! Ownership / uniqueness prototype.
//!
//! This is a separate layer from the dependent kernel: it checks the surface
//! language's `own` / `borrow` / inferred-unique modes, lowers `set`/`push` to
//! in-place or copy operations, and runs the result in a tiny interpreter.
//!
//! It is deliberately not a full type system.  The goal is to make the
//! ownership rules executable and testable before they are connected to the
//! proof kernel.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::syntax::{
    parse, BinOp, CallArg, Decl, Expr, FnDecl, Mode, ParamKind, Program, Ty, UnOp,
};

// ---------------------------------------------------------------------------
// Signatures and checked program
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ParamSig {
    pub name: String,
    pub ty: Ty,
    pub mode: Mode,
    pub is_proof: bool,
}

#[derive(Clone, Debug)]
pub struct FnSig {
    pub name: String,
    pub params: Vec<ParamSig>,
    pub ret: Ty,
}

#[derive(Clone, Debug)]
pub struct CheckedFn {
    pub name: String,
    pub params: Vec<String>,
    pub ret: Ty,
    pub body: Expr,
}

#[derive(Clone, Debug)]
pub struct CheckedProgram {
    pub funcs: Vec<CheckedFn>,
}

impl CheckedProgram {
    pub fn get(&self, name: &str) -> Option<&CheckedFn> {
        self.funcs.iter().find(|f| f.name == name)
    }
}

pub fn check_program(prog: &Program) -> Result<CheckedProgram, String> {
    let mut checker = Checker::new();
    for d in &prog.decls {
        if let Decl::Fn(f) = d {
            checker.collect(f)?;
        }
    }
    let mut funcs = Vec::new();
    for d in &prog.decls {
        if let Decl::Fn(f) = d {
            funcs.push(checker.check_fn(f)?);
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

#[derive(Clone, Debug)]
struct VarInfo {
    ty: Ty,
    mode: Mode,
    unique: bool,
    consumed: bool,
    uses: usize,
}

#[derive(Clone, Debug)]
struct Ctx {
    vars: HashMap<String, VarInfo>,
    uses: HashMap<String, usize>,
    /// Names that are borrows, including locals, for escape analysis.
    borrow_names: HashSet<String>,
}

impl Ctx {
    fn info(&self, name: &str) -> Result<&VarInfo, String> {
        self.vars
            .get(name)
            .ok_or_else(|| format!("unknown variable `{name}`"))
    }

    fn info_mut(&mut self, name: &str) -> Result<&mut VarInfo, String> {
        self.vars
            .get_mut(name)
            .ok_or_else(|| format!("unknown variable `{name}`"))
    }
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

        let params = f
            .params
            .iter()
            .filter_map(|p| match p.kind {
                ParamKind::Runtime(_) => Some(p.name.clone()),
                ParamKind::Proof(_) => None,
            })
            .collect();

        Ok(CheckedFn {
            name: f.name.clone(),
            params,
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

// ---------------------------------------------------------------------------
// Ownership rules
// ---------------------------------------------------------------------------

fn expr_arg(a: &CallArg) -> Option<&Expr> {
    match a {
        CallArg::Expr(e) => Some(e),
        CallArg::Proof(_) => None,
    }
}

fn mode_name(m: &Mode) -> &'static str {
    match m {
        Mode::Val => "value",
        Mode::Own => "own",
        Mode::Borrow => "borrow",
    }
}

fn expect_ty(actual: &Ty, expected: &Ty, what: &str) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!("{what} expects `{expected}`, got `{actual}`"))
    }
}

/// Is this expression a fresh array (safe to treat as uniquely owned)?
fn is_fresh(e: &Expr) -> bool {
    match e {
        Expr::Array(_) => true,
        Expr::Call { name, .. } => matches!(
            name.as_str(),
            "set" | "push" | "__set_in_place" | "__set_copy" | "__push_in_place" | "__push_copy"
        ),
        _ => false,
    }
}

/// Can a `set`/`push` on this base be done in place?
///
/// Requires:
/// - the base is a variable;
/// - it is uniquely owned;
/// - it has not been consumed;
/// - it has exactly one occurrence in the function (this operation);
/// - it is not a borrow.
fn can_in_place(ctx: &Ctx, base: Option<&Expr>) -> bool {
    let Some(Expr::Var(v)) = base else {
        return false;
    };
    let Some(info) = ctx.vars.get(v) else {
        return false;
    };
    info.unique && !info.consumed && info.uses == 1 && info.mode != Mode::Borrow
}

fn consume_expr(ctx: &mut Ctx, base: Option<&Expr>) -> Result<(), String> {
    let Some(Expr::Var(v)) = base else {
        return Ok(());
    };
    let info = ctx.info_mut(v)?;
    if info.consumed {
        return Err(format!("use of moved value `{v}`"));
    }
    if info.mode == Mode::Borrow {
        return Err(format!("cannot move borrow `{v}`"));
    }
    info.consumed = true;
    Ok(())
}

/// Passing to an `own` parameter: move a unique variable, or a fresh array.
fn transfer_arg(ctx: &mut Ctx, arg: &Expr, param: &str, fname: &str) -> Result<(), String> {
    match arg {
        Expr::Var(v) => {
            let info = ctx.info(v)?.clone();
            if info.consumed {
                return Err(format!("use of moved value `{v}`"));
            }
            if info.mode == Mode::Borrow {
                return Err(format!(
                    "cannot pass borrow `{v}` to own parameter `{param}` of `{fname}`"
                ));
            }
            if !info.unique {
                return Err(format!(
                    "cannot move shared value `{v}` into own parameter `{param}` of `{fname}`"
                ));
            }
            ctx.info_mut(v)?.consumed = true;
            Ok(())
        }
        e if is_fresh(e) => Ok(()),
        _ => Err(format!(
            "argument `{param}` of `{fname}` must be a unique variable or a fresh array"
        )),
    }
}

/// Passing to a `borrow` parameter: no move, but the value must be alive.
fn borrow_arg(ctx: &Ctx, arg: &Expr, param: &str, fname: &str) -> Result<(), String> {
    match arg {
        Expr::Var(v) => {
            let info = ctx.info(v)?;
            if info.consumed {
                return Err(format!("use of moved value `{v}`"));
            }
            if !matches!(info.ty, Ty::Array(_)) {
                return Err(format!(
                    "borrow parameter `{param}` of `{fname}` must be an array"
                ));
            }
            Ok(())
        }
        Expr::Array(_) => Ok(()),
        _ => Err(format!(
            "argument `{param}` of `{fname}` must be a variable or array literal"
        )),
    }
}

/// Passing to a `Val` parameter: read-only, no move.
fn read_arg(ctx: &Ctx, arg: &Expr, param: &str, fname: &str) -> Result<(), String> {
    if let Expr::Var(v) = arg {
        let info = ctx.info(v)?;
        if info.consumed {
            return Err(format!("use of moved value `{v}`"));
        }
    }
    let _ = (param, fname);
    Ok(())
}

// ---------------------------------------------------------------------------
// Use counting and escape analysis
// ---------------------------------------------------------------------------

fn count_uses(e: &Expr, out: &mut HashMap<String, usize>) {
    match e {
        Expr::Var(n) => {
            *out.entry(n.clone()).or_insert(0) += 1;
        }
        Expr::Int(_) | Expr::Bool(_) | Expr::Unit => {}
        Expr::Array(items) => {
            for i in items {
                count_uses(i, out);
            }
        }
        Expr::Un(_, e) => count_uses(e, out),
        Expr::Bin(_, a, b) => {
            count_uses(a, out);
            count_uses(b, out);
        }
        Expr::If(c, t, e) => {
            count_uses(c, out);
            count_uses(t, out);
            count_uses(e, out);
        }
        Expr::Let(n, v, b) => {
            count_uses(v, out);
            count_uses(b, out);
            let _ = n; // binding occurrence is not a use
        }
        Expr::Call { args, .. } => {
            for a in args {
                if let CallArg::Expr(e) = a {
                    count_uses(e, out);
                }
            }
        }
        Expr::Index(a, b) => {
            count_uses(a, out);
            count_uses(b, out);
        }
        Expr::Seq(a, b) => {
            count_uses(a, out);
            count_uses(b, out);
        }
    }
}

/// Conservative escape check: a borrow may not be returned as array data.
fn result_borrow(e: &Expr, result_ty: &Ty, borrows: &HashSet<String>) -> Option<String> {
    if !matches!(result_ty, Ty::Array(_)) {
        return None;
    }
    // An array literal is only safe if its elements contain no borrows.
    if let Expr::Array(items) = e {
        return items.iter().find_map(|i| first_borrow(i, borrows));
    }
    // A fresh copy is safe even if its input was a borrow.
    if is_fresh(e) {
        return None;
    }
    match e {
        Expr::Var(n) if borrows.contains(n) => Some(n.clone()),
        Expr::Let(_, v, b) => first_borrow(v, borrows).or_else(|| first_borrow(b, borrows)),
        Expr::If(_, t, e) => first_borrow(t, borrows).or_else(|| first_borrow(e, borrows)),
        Expr::Seq(_, b) => first_borrow(b, borrows),
        Expr::Call { name, args } if name == "print" || name == "len" => None,
        Expr::Call { args, .. } => args.iter().find_map(|a| match a {
            CallArg::Expr(e) => first_borrow(e, borrows),
            CallArg::Proof(_) => None,
        }),
        Expr::Index(a, _) => first_borrow(a, borrows),
        Expr::Un(_, e) => first_borrow(e, borrows),
        Expr::Bin(_, a, b) => first_borrow(a, borrows).or_else(|| first_borrow(b, borrows)),
        _ => None,
    }
}

fn first_borrow(e: &Expr, borrows: &HashSet<String>) -> Option<String> {
    match e {
        Expr::Var(n) if borrows.contains(n) => Some(n.clone()),
        Expr::Var(_) | Expr::Int(_) | Expr::Bool(_) | Expr::Unit => None,
        Expr::Array(items) => items.iter().find_map(|i| first_borrow(i, borrows)),
        Expr::Un(_, e) => first_borrow(e, borrows),
        Expr::Bin(_, a, b) => first_borrow(a, borrows).or_else(|| first_borrow(b, borrows)),
        Expr::If(c, t, e) => first_borrow(c, borrows)
            .or_else(|| first_borrow(t, borrows))
            .or_else(|| first_borrow(e, borrows)),
        Expr::Let(_, v, b) => first_borrow(v, borrows).or_else(|| first_borrow(b, borrows)),
        Expr::Call { name, args } if name == "print" || name == "len" => None,
        Expr::Call { args, .. } => args.iter().find_map(|a| match a {
            CallArg::Expr(e) => first_borrow(e, borrows),
            CallArg::Proof(_) => None,
        }),
        Expr::Index(a, b) => first_borrow(a, borrows).or_else(|| first_borrow(b, borrows)),
        Expr::Seq(_, b) => first_borrow(b, borrows),
    }
}

// ---------------------------------------------------------------------------
// Interpreter
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Unit,
    Array(Rc<RefCell<Vec<Value>>>),
}

impl Value {
    pub fn as_int(&self) -> Result<i64, String> {
        match self {
            Value::Int(n) => Ok(*n),
            other => Err(format!("expected Int, got {other:?}")),
        }
    }

    pub fn as_bool(&self) -> Result<bool, String> {
        match self {
            Value::Bool(b) => Ok(*b),
            other => Err(format!("expected Bool, got {other:?}")),
        }
    }

    pub fn as_array(&self) -> Result<Rc<RefCell<Vec<Value>>>, String> {
        match self {
            Value::Array(a) => Ok(a.clone()),
            other => Err(format!("expected Array, got {other:?}")),
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Unit => write!(f, "()"),
            Value::Array(a) => {
                write!(f, "[")?;
                for (i, v) in a.borrow().iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
        }
    }
}

#[derive(Default, Debug)]
pub struct Stats {
    pub allocations: usize,
    pub copies: usize,
    pub in_place: usize,
    pub bounds_checks: usize,
}

pub struct RunResult {
    pub value: Value,
    pub stats: Stats,
}

pub fn run(prog: &CheckedProgram, entry: &str) -> Result<RunResult, String> {
    let mut interp = Interpreter {
        prog,
        stats: Stats::default(),
    };
    let value = interp.call(entry, Vec::new())?;
    Ok(RunResult {
        value,
        stats: interp.stats,
    })
}

struct Interpreter<'a> {
    prog: &'a CheckedProgram,
    stats: Stats,
}

impl<'a> Interpreter<'a> {
    fn call(&mut self, name: &str, args: Vec<Value>) -> Result<Value, String> {
        let f = self
            .prog
            .get(name)
            .ok_or_else(|| format!("unknown function `{name}`"))?;
        if f.params.len() != args.len() {
            return Err(format!(
                "function `{name}` expects {} arguments, got {}",
                f.params.len(),
                args.len()
            ));
        }
        let mut env = HashMap::new();
        for (p, v) in f.params.iter().zip(args) {
            env.insert(p.clone(), v);
        }
        self.eval(&f.body, &mut env)
    }

    fn eval(&mut self, e: &Expr, env: &mut HashMap<String, Value>) -> Result<Value, String> {
        match e {
            Expr::Var(n) => env
                .get(n)
                .cloned()
                .ok_or_else(|| format!("unbound variable `{n}`")),
            Expr::Int(n) => Ok(Value::Int(*n)),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Unit => Ok(Value::Unit),
            Expr::Array(items) => {
                let mut vals = Vec::new();
                for i in items {
                    vals.push(self.eval(i, env)?);
                }
                self.stats.allocations += 1;
                Ok(Value::Array(Rc::new(RefCell::new(vals))))
            }
            Expr::Un(op, inner) => {
                let v = self.eval(inner, env)?;
                match op {
                    UnOp::Neg => Ok(Value::Int(-v.as_int()?)),
                    UnOp::Not => Ok(Value::Bool(!v.as_bool()?)),
                }
            }
            Expr::Bin(op, l, r) => {
                let a = self.eval(l, env)?;
                let b = self.eval(r, env)?;
                match op {
                    BinOp::Add => Ok(Value::Int(a.as_int()? + b.as_int()?)),
                    BinOp::Sub => Ok(Value::Int(a.as_int()? - b.as_int()?)),
                    BinOp::Mul => Ok(Value::Int(a.as_int()? * b.as_int()?)),
                    BinOp::Div => {
                        let d = b.as_int()?;
                        if d == 0 {
                            return Err("division by zero".into());
                        }
                        Ok(Value::Int(a.as_int()? / d))
                    }
                    BinOp::Mod => {
                        let d = b.as_int()?;
                        if d == 0 {
                            return Err("division by zero".into());
                        }
                        Ok(Value::Int(a.as_int()? % d))
                    }
                    BinOp::Eq => Ok(Value::Bool(a.as_int()? == b.as_int()?)),
                    BinOp::Ne => Ok(Value::Bool(a.as_int()? != b.as_int()?)),
                    BinOp::Lt => Ok(Value::Bool(a.as_int()? < b.as_int()?)),
                    BinOp::Le => Ok(Value::Bool(a.as_int()? <= b.as_int()?)),
                    BinOp::Gt => Ok(Value::Bool(a.as_int()? > b.as_int()?)),
                    BinOp::Ge => Ok(Value::Bool(a.as_int()? >= b.as_int()?)),
                    BinOp::And => Ok(Value::Bool(a.as_bool()? && b.as_bool()?)),
                    BinOp::Or => Ok(Value::Bool(a.as_bool()? || b.as_bool()?)),
                }
            }
            Expr::If(c, t, e) => {
                let cond = self.eval(c, env)?.as_bool()?;
                if cond {
                    self.eval(t, env)
                } else {
                    self.eval(e, env)
                }
            }
            Expr::Let(n, v, b) => {
                let val = self.eval(v, env)?;
                env.insert(n.clone(), val);
                let out = self.eval(b, env);
                env.remove(n);
                out
            }
            Expr::Seq(a, b) => {
                self.eval(a, env)?;
                self.eval(b, env)
            }
            Expr::Index(base, index) => {
                let arr = self.eval(base, env)?.as_array()?;
                let i = self.eval(index, env)?.as_int()?;
                self.stats.bounds_checks += 1;
                let arr = arr.borrow();
                if i < 0 || i as usize >= arr.len() {
                    return Err(format!("index {i} out of bounds (len {})", arr.len()));
                }
                Ok(arr[i as usize].clone())
            }
            Expr::Call { name, args } => {
                let mut vals = Vec::new();
                for a in args {
                    if let CallArg::Expr(e) = a {
                        vals.push(self.eval(e, env)?);
                    }
                }
                match name.as_str() {
                    "len" => {
                        let arr = vals[0].as_array()?;
                        let n = arr.borrow().len();
                        Ok(Value::Int(n as i64))
                    }
                    "print" => {
                        println!("{}", vals[0]);
                        Ok(Value::Unit)
                    }
                    "__set_in_place" => {
                        let arr = vals[0].as_array()?;
                        let i = vals[1].as_int()?;
                        self.stats.bounds_checks += 1;
                        let mut a = arr.borrow_mut();
                        if i < 0 || i as usize >= a.len() {
                            return Err(format!("set index {i} out of bounds (len {})", a.len()));
                        }
                        a[i as usize] = vals[2].clone();
                        self.stats.in_place += 1;
                        Ok(Value::Array(arr.clone()))
                    }
                    "__set_copy" => {
                        let arr = vals[0].as_array()?;
                        let i = vals[1].as_int()?;
                        let old = arr.borrow();
                        self.stats.bounds_checks += 1;
                        if i < 0 || i as usize >= old.len() {
                            return Err(format!("set index {i} out of bounds (len {})", old.len()));
                        }
                        let mut new = old.clone();
                        new[i as usize] = vals[2].clone();
                        drop(old);
                        self.stats.copies += 1;
                        self.stats.allocations += 1;
                        Ok(Value::Array(Rc::new(RefCell::new(new))))
                    }
                    "__push_in_place" => {
                        let arr = vals[0].as_array()?;
                        arr.borrow_mut().push(vals[1].clone());
                        self.stats.in_place += 1;
                        Ok(Value::Array(arr.clone()))
                    }
                    "__push_copy" => {
                        let arr = vals[0].as_array()?;
                        let mut new = arr.borrow().clone();
                        new.push(vals[1].clone());
                        self.stats.copies += 1;
                        self.stats.allocations += 1;
                        Ok(Value::Array(Rc::new(RefCell::new(new))))
                    }
                    _ => self.call(name, vals),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILL: &str = r#"
        fn fill(own a: Array<Int>, x: Int) -> Array<Int> { set(a, 0, x) }
    "#;

    #[test]
    fn own_is_in_place() {
        let src = format!(
            "{FILL}
             fn main() -> Int {{
               let a = [1, 2, 3];
               let b = fill(a, 9);
               print(b);
               b[0]
             }}"
        );
        let prog = check_source(&src).unwrap();
        let r = run(&prog, "main").unwrap();
        assert_eq!(r.stats.in_place, 1);
        assert_eq!(r.stats.copies, 0);
        assert_eq!(r.stats.allocations, 1);
    }

    #[test]
    fn shared_is_copy() {
        let src = r#"
            fn fill_shared(a: Array<Int>, x: Int) -> Array<Int> { set(a, 0, x) }
            fn main() -> Int {
              let a = [1, 2, 3];
              let b = fill_shared(a, 9);
              print(a);
              print(b);
              b[0]
            }
        "#;
        let prog = check_source(src).unwrap();
        let r = run(&prog, "main").unwrap();
        assert_eq!(r.stats.in_place, 0);
        assert_eq!(r.stats.copies, 1);
    }

    #[test]
    fn rejects_use_after_move() {
        let src = format!(
            "{FILL}
             fn bad(own a: Array<Int>, x: Int) -> Int {{
               let b = fill(a, x);
               len(a)
             }}"
        );
        let err = check_source(&src).unwrap_err();
        assert!(err.contains("use of moved value"), "{err}");
    }

    #[test]
    fn rejects_borrow_escape() {
        let src = r#"
            fn leak(borrow a: Array<Int>) -> Array<Int> { a }
        "#;
        let err = check_source(src).unwrap_err();
        assert!(err.contains("escapes"), "{err}");
    }

    #[test]
    fn rejects_move_borrow() {
        let src = format!(
            "{FILL}
             fn bad(borrow a: Array<Int>, x: Int) -> Int {{
               let b = fill(a, x);
               len(b)
             }}"
        );
        let err = check_source(&src).unwrap_err();
        assert!(err.contains("cannot pass borrow"), "{err}");
    }

    #[test]
    fn rejects_stored_borrow() {
        let src = r#"
            fn bad(borrow a: Array<Int>) -> Array<Array<Int>> { [a] }
        "#;
        let err = check_source(src).unwrap_err();
        assert!(err.contains("escapes"), "{err}");
    }
}
