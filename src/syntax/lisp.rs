//! A small S-expression frontend.
//!
//! This is a convenience syntax, not a second semantics: it lowers directly to
//! the same `Program` AST as the `.alt` parser, so every later stage is shared.

use super::ast::*;
use super::lexer::ParseError;

#[derive(Clone, Debug)]
enum Sexp {
    Atom(String, usize, usize),
    List(Vec<Sexp>, usize, usize),
}

impl Sexp {
    fn line(&self) -> usize {
        match self {
            Sexp::Atom(_, line, _) | Sexp::List(_, line, _) => *line,
        }
    }

    fn col(&self) -> usize {
        match self {
            Sexp::Atom(_, _, col) | Sexp::List(_, _, col) => *col,
        }
    }
}

fn err<T>(s: &Sexp, msg: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError::new(msg, s.line(), s.col()))
}

fn atom(s: &Sexp) -> Result<&str, ParseError> {
    match s {
        Sexp::Atom(a, _, _) => Ok(a),
        Sexp::List(_, line, col) => Err(ParseError::new("expected atom", *line, *col)),
    }
}

fn list(s: &Sexp) -> Result<&[Sexp], ParseError> {
    match s {
        Sexp::List(items, _, _) => Ok(items),
        Sexp::Atom(_, line, col) => Err(ParseError::new("expected list", *line, *col)),
    }
}

fn head(s: &Sexp) -> Option<&str> {
    match s {
        Sexp::List(items, _, _) => items.first().and_then(|x| match x {
            Sexp::Atom(a, _, _) => Some(a.as_str()),
            _ => None,
        }),
        _ => None,
    }
}

fn tail<'a>(s: &'a Sexp, expected: &str) -> Result<&'a [Sexp], ParseError> {
    let items = list(s)?;
    if items.is_empty() {
        return err(s, format!("expected `({expected} ...)`"));
    }
    let name = atom(&items[0])?;
    if name != expected {
        return err(s, format!("expected `{expected}`, found `{name}`"));
    }
    Ok(&items[1..])
}

// ---------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------

pub fn parse_lisp(src: &str) -> Result<Program, ParseError> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut col = 1usize;
    let mut forms = Vec::new();
    loop {
        skip_space(&chars, &mut i, &mut line, &mut col);
        if i >= chars.len() {
            break;
        }
        forms.push(read_sexp(&chars, &mut i, &mut line, &mut col)?);
    }
    let mut decls = Vec::with_capacity(forms.len());
    for form in &forms {
        decls.push(lower_decl(form)?);
    }
    Ok(Program { decls })
}

fn skip_space(chars: &[char], i: &mut usize, line: &mut usize, col: &mut usize) {
    while *i < chars.len() {
        let c = chars[*i];
        if c == '\n' {
            *i += 1;
            *line += 1;
            *col = 1;
        } else if c.is_whitespace() {
            *i += 1;
            *col += 1;
        } else if c == ';' {
            while *i < chars.len() && chars[*i] != '\n' {
                *i += 1;
                *col += 1;
            }
        } else {
            break;
        }
    }
}

fn read_sexp(
    chars: &[char],
    i: &mut usize,
    line: &mut usize,
    col: &mut usize,
) -> Result<Sexp, ParseError> {
    skip_space(chars, i, line, col);
    if *i >= chars.len() {
        return Err(ParseError::new("unexpected end of input", *line, *col));
    }
    let l = *line;
    let c = *col;
    match chars[*i] {
        '(' => {
            *i += 1;
            *col += 1;
            let mut items = Vec::new();
            loop {
                skip_space(chars, i, line, col);
                if *i >= chars.len() {
                    return Err(ParseError::new("unterminated list", l, c));
                }
                if chars[*i] == ')' {
                    *i += 1;
                    *col += 1;
                    break;
                }
                items.push(read_sexp(chars, i, line, col)?);
            }
            Ok(Sexp::List(items, l, c))
        }
        ')' => Err(ParseError::new("unexpected `)`", l, c)),
        _ => {
            let start = *i;
            while *i < chars.len() {
                let ch = chars[*i];
                if ch.is_whitespace() || ch == '(' || ch == ')' || ch == ';' {
                    break;
                }
                *i += 1;
                *col += 1;
            }
            Ok(Sexp::Atom(chars[start..*i].iter().collect(), l, c))
        }
    }
}

// ---------------------------------------------------------------------------
// Declarations
// ---------------------------------------------------------------------------

fn lower_decl(form: &Sexp) -> Result<Decl, ParseError> {
    match head(form) {
        Some("fn") => Ok(Decl::Fn(lower_fn(form)?)),
        Some("spec") => Ok(Decl::Spec(lower_spec(form)?)),
        Some("impl") => Ok(Decl::Impl(lower_impl(form)?)),
        Some("theorem") => Ok(Decl::Theorem(lower_theorem(form)?)),
        Some(other) => err(form, format!("unknown declaration `{other}`")),
        None => err(form, "expected a declaration"),
    }
}

fn lower_fn(form: &Sexp) -> Result<FnDecl, ParseError> {
    let rest = tail(form, "fn")?;
    let (name, params, ret, requires, ensures, body) = lower_fn_parts(form, rest)?;
    Ok(FnDecl {
        name,
        params,
        ret,
        requires,
        ensures,
        body,
    })
}

#[allow(clippy::type_complexity)]
fn lower_fn_parts(
    form: &Sexp,
    rest: &[Sexp],
) -> Result<(String, Vec<Param>, Ty, Vec<PExpr>, Vec<PExpr>, Expr), ParseError> {
    if rest.len() < 3 {
        return err(form, "expected name, parameters, and return type");
    }
    let name = atom(&rest[0])?.to_string();
    let params = lower_params(&rest[1])?;
    let ret = lower_ty(&rest[2])?;
    let mut requires = Vec::new();
    let mut ensures = Vec::new();
    let mut body_forms = Vec::new();
    for item in &rest[3..] {
        match head(item) {
            Some("requires") => {
                let args = tail(item, "requires")?;
                if args.len() != 1 {
                    return err(item, "requires expects one proposition");
                }
                requires.push(lower_pexpr(&args[0])?);
            }
            Some("ensures") => {
                let args = tail(item, "ensures")?;
                if args.len() != 1 {
                    return err(item, "ensures expects one proposition");
                }
                ensures.push(lower_pexpr(&args[0])?);
            }
            _ => body_forms.push(item.clone()),
        }
    }
    let body = lower_body(form, &body_forms)?;
    Ok((name, params, ret, requires, ensures, body))
}

fn lower_spec(form: &Sexp) -> Result<SpecDecl, ParseError> {
    let rest = tail(form, "spec")?;
    if rest.len() < 3 {
        return err(form, "expected name, parameters, and return type");
    }
    let name = atom(&rest[0])?.to_string();
    let params = lower_params(&rest[1])?;
    let ret = lower_ty(&rest[2])?;
    let mut requires = Vec::new();
    let mut ensures = Vec::new();
    for item in &rest[3..] {
        match head(item) {
            Some("requires") => {
                let args = tail(item, "requires")?;
                if args.len() != 1 {
                    return err(item, "requires expects one proposition");
                }
                requires.push(lower_pexpr(&args[0])?);
            }
            Some("ensures") => {
                let args = tail(item, "ensures")?;
                if args.len() != 1 {
                    return err(item, "ensures expects one proposition");
                }
                ensures.push(lower_pexpr(&args[0])?);
            }
            Some(other) => return err(item, format!("unexpected spec item `{other}`")),
            None => return err(item, "expected requires/ensures"),
        }
    }
    Ok(SpecDecl {
        name,
        params,
        ret,
        requires,
        ensures,
    })
}

fn lower_impl(form: &Sexp) -> Result<ImplDecl, ParseError> {
    let rest = tail(form, "impl")?;
    if rest.len() < 3 {
        return err(form, "expected name, parameters, and return type");
    }
    let name = atom(&rest[0])?.to_string();
    let params = lower_params(&rest[1])?;
    let ret = lower_ty(&rest[2])?;
    let mut requires = Vec::new();
    let mut ensures = Vec::new();
    let mut refines = None;
    let mut body_forms = Vec::new();
    for item in &rest[3..] {
        match head(item) {
            Some("refines") => {
                let args = tail(item, "refines")?;
                if args.len() != 1 {
                    return err(item, "refines expects one spec name");
                }
                refines = Some(atom(&args[0])?.to_string());
            }
            Some("requires") => {
                let args = tail(item, "requires")?;
                if args.len() != 1 {
                    return err(item, "requires expects one proposition");
                }
                requires.push(lower_pexpr(&args[0])?);
            }
            Some("ensures") => {
                let args = tail(item, "ensures")?;
                if args.len() != 1 {
                    return err(item, "ensures expects one proposition");
                }
                ensures.push(lower_pexpr(&args[0])?);
            }
            _ => body_forms.push(item.clone()),
        }
    }
    let Some(refines) = refines else {
        return err(form, "impl needs `(refines spec)`");
    };
    let body = lower_body(form, &body_forms)?;
    Ok(ImplDecl {
        f: FnDecl {
            name,
            params,
            ret,
            requires,
            ensures,
            body,
        },
        refines,
    })
}

fn lower_theorem(form: &Sexp) -> Result<ThmDecl, ParseError> {
    let rest = tail(form, "theorem")?;
    if rest.len() < 2 {
        return err(form, "expected name and proposition");
    }
    let name = atom(&rest[0])?.to_string();
    let prop = lower_pexpr(&rest[1])?;
    let mut tactics = Vec::new();
    for item in &rest[2..] {
        tactics.push(lower_tactic(item)?);
    }
    Ok(ThmDecl {
        name,
        prop,
        tactics,
    })
}

fn lower_body(form: &Sexp, forms: &[Sexp]) -> Result<Expr, ParseError> {
    let mut exprs = Vec::with_capacity(forms.len());
    for item in forms {
        exprs.push(lower_expr(item)?);
    }
    match exprs.len() {
        0 => err(form, "missing body"),
        1 => Ok(exprs.pop().unwrap()),
        _ => {
            let mut out = exprs.pop().unwrap();
            while let Some(first) = exprs.pop() {
                out = Expr::Seq(Box::new(first), Box::new(out));
            }
            Ok(out)
        }
    }
}

// ---------------------------------------------------------------------------
// Parameters and types
// ---------------------------------------------------------------------------

fn lower_params(form: &Sexp) -> Result<Vec<Param>, ParseError> {
    let items = list(form)?;
    let mut params = Vec::new();
    for item in items {
        let parts = list(item)?;
        let (mode, name, ty, proof) = match parts {
            [n, t] => (Mode::Val, atom(n)?, lower_ty(t)?, None),
            [m, n, t] if atom(m)? == "own" => (Mode::Own, atom(n)?, lower_ty(t)?, None),
            [m, n, t] if atom(m)? == "borrow" => (Mode::Borrow, atom(n)?, lower_ty(t)?, None),
            [m, n, p] if atom(m)? == "proof" => {
                (Mode::Val, atom(n)?, Ty::Unit, Some(lower_pexpr(p)?))
            }
            _ => return err(
                item,
                "expected (name type), (own name type), (borrow name type), or (proof name prop)",
            ),
        };
        params.push(Param {
            name: name.to_string(),
            mode,
            kind: match proof {
                Some(p) => ParamKind::Proof(p),
                None => ParamKind::Runtime(ty),
            },
            refinement: None,
        });
    }
    Ok(params)
}

fn lower_ty(form: &Sexp) -> Result<Ty, ParseError> {
    match form {
        Sexp::Atom(name, _, _) => match name.as_str() {
            "Int" | "int" => Ok(Ty::Int),
            "Bool" | "bool" => Ok(Ty::Bool),
            "Unit" | "unit" => Ok(Ty::Unit),
            "Prop" | "prop" => Ok(Ty::Prop),
            other => err(form, format!("unknown type `{other}`")),
        },
        Sexp::List(items, _, _) => {
            let Some(Sexp::Atom(name, _, _)) = items.first() else {
                return err(form, "expected a type constructor");
            };
            match name.as_str() {
                "array" | "Array" => {
                    if items.len() != 2 {
                        return err(form, "array expects one element type");
                    }
                    Ok(Ty::Array(Box::new(lower_ty(&items[1])?)))
                }
                "fn" | "Fn" | "->" => {
                    if items.len() != 3 {
                        return err(form, "function type expects (fn (types...) ret)");
                    }
                    let params = list(&items[1])?
                        .iter()
                        .map(lower_ty)
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(Ty::Fn(params, Box::new(lower_ty(&items[2])?)))
                }
                other => err(form, format!("unknown type constructor `{other}`")),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

fn lower_expr(form: &Sexp) -> Result<Expr, ParseError> {
    match form {
        Sexp::Atom(name, _, _) => {
            if let Ok(n) = name.parse::<i64>() {
                Ok(Expr::Int(n))
            } else if name == "true" {
                Ok(Expr::Bool(true))
            } else if name == "false" {
                Ok(Expr::Bool(false))
            } else if name == "()" {
                Ok(Expr::Unit)
            } else {
                Ok(Expr::Var(name.clone()))
            }
        }
        Sexp::List(items, _, _) => {
            if items.is_empty() {
                return Ok(Expr::Unit);
            }
            let Some(Sexp::Atom(name, _, _)) = items.first() else {
                let callee = lower_expr(&items[0])?;
                let args = items[1..]
                    .iter()
                    .map(lower_expr)
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(Expr::Apply {
                    callee: Box::new(callee),
                    args,
                });
            };
            let args = &items[1..];
            match name.as_str() {
                "array" => Ok(Expr::Array(
                    args.iter().map(lower_expr).collect::<Result<Vec<_>, _>>()?,
                )),
                "if" => {
                    if args.len() != 3 {
                        return err(form, "if expects condition, then, else");
                    }
                    Ok(Expr::If(
                        Box::new(lower_expr(&args[0])?),
                        Box::new(lower_expr(&args[1])?),
                        Box::new(lower_expr(&args[2])?),
                    ))
                }
                "let" => {
                    if args.len() < 3 {
                        return err(form, "let expects name, value, body");
                    }
                    let name = atom(&args[0])?.to_string();
                    let value = lower_expr(&args[1])?;
                    let body = lower_body(form, &args[2..])?;
                    Ok(Expr::Let(name, Box::new(value), Box::new(body)))
                }
                "seq" => {
                    let mut exprs = args.iter().map(lower_expr).collect::<Result<Vec<_>, _>>()?;
                    if exprs.is_empty() {
                        return Ok(Expr::Unit);
                    }
                    let mut out = exprs.pop().unwrap();
                    while let Some(first) = exprs.pop() {
                        out = Expr::Seq(Box::new(first), Box::new(out));
                    }
                    Ok(out)
                }
                "lambda" | "fn*" => {
                    if args.len() < 2 {
                        return err(form, "lambda expects (parameters) return-type body");
                    }
                    let params = list(&args[0])?
                        .iter()
                        .map(|p| {
                            let parts = list(p)?;
                            match parts {
                                [n, t] => Ok(LambdaParam {
                                    name: atom(n)?.to_string(),
                                    ty: lower_ty(t)?,
                                }),
                                _ => err(p, "lambda parameter expects (name type)"),
                            }
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let ret = lower_ty(&args[1])?;
                    let body = lower_body(form, &args[2..])?;
                    Ok(Expr::Lambda {
                        params,
                        ret,
                        body: Box::new(body),
                    })
                }
                "apply" => {
                    if args.is_empty() {
                        return err(form, "apply expects a callee");
                    }
                    Ok(Expr::Apply {
                        callee: Box::new(lower_expr(&args[0])?),
                        args: args[1..]
                            .iter()
                            .map(lower_expr)
                            .collect::<Result<Vec<_>, _>>()?,
                    })
                }
                "index" | "get" => {
                    if args.len() != 2 {
                        return err(form, "index expects array and index");
                    }
                    Ok(Expr::Index(
                        Box::new(lower_expr(&args[0])?),
                        Box::new(lower_expr(&args[1])?),
                    ))
                }
                "set" | "push" | "len" | "print" => {
                    let mut out = Vec::with_capacity(args.len());
                    for arg in args {
                        out.push(CallArg::Expr(lower_expr(arg)?));
                    }
                    Ok(Expr::Call {
                        name: name.clone(),
                        args: out,
                    })
                }
                _ => {
                    if let Some(op) = lisp_binop(name) {
                        return lower_binop(form, op, args);
                    }
                    if let Some(op) = lisp_unop(name) {
                        if args.len() != 1 {
                            return err(form, "unary operator expects one argument");
                        }
                        return Ok(Expr::Un(op, Box::new(lower_expr(&args[0])?)));
                    }
                    let mut out = Vec::with_capacity(args.len());
                    for arg in args {
                        out.push(CallArg::Expr(lower_expr(arg)?));
                    }
                    Ok(Expr::Call {
                        name: name.clone(),
                        args: out,
                    })
                }
            }
        }
    }
}

fn lower_binop(form: &Sexp, op: BinOp, args: &[Sexp]) -> Result<Expr, ParseError> {
    if args.is_empty() {
        return err(form, "operator expects arguments");
    }
    if args.len() == 1 {
        if op == BinOp::Sub {
            return Ok(Expr::Un(UnOp::Neg, Box::new(lower_expr(&args[0])?)));
        }
        return lower_expr(&args[0]);
    }
    let mut out = lower_expr(&args[0])?;
    for arg in &args[1..] {
        out = Expr::Bin(op, Box::new(out), Box::new(lower_expr(arg)?));
    }
    Ok(out)
}

fn lisp_binop(name: &str) -> Option<BinOp> {
    Some(match name {
        "+" => BinOp::Add,
        "-" => BinOp::Sub,
        "*" => BinOp::Mul,
        "/" => BinOp::Div,
        "%" => BinOp::Mod,
        "==" | "=" => BinOp::Eq,
        "!=" => BinOp::Ne,
        "<" => BinOp::Lt,
        "<=" => BinOp::Le,
        ">" => BinOp::Gt,
        ">=" => BinOp::Ge,
        "&&" => BinOp::And,
        "||" => BinOp::Or,
        _ => return None,
    })
}

fn lisp_unop(name: &str) -> Option<UnOp> {
    Some(match name {
        "neg" => UnOp::Neg,
        "not" | "!" => UnOp::Not,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Propositions and tactics
// ---------------------------------------------------------------------------

fn lower_pexpr(form: &Sexp) -> Result<PExpr, ParseError> {
    match form {
        Sexp::Atom(name, _, _) => {
            if let Ok(n) = name.parse::<i64>() {
                Ok(PExpr::Int(n))
            } else if name == "true" {
                Ok(PExpr::Bool(true))
            } else if name == "false" {
                Ok(PExpr::Bool(false))
            } else {
                Ok(PExpr::Var(name.clone()))
            }
        }
        Sexp::List(items, _, _) => {
            if items.is_empty() {
                return err(form, "empty proposition");
            }
            let Some(Sexp::Atom(name, _, _)) = items.first() else {
                return err(form, "expected proposition operator");
            };
            let args = &items[1..];
            match name.as_str() {
                "forall" | "exists" => {
                    if args.len() != 2 {
                        return err(form, "quantifier expects (name sort) body");
                    }
                    let parts = list(&args[0])?;
                    if parts.len() != 2 {
                        return err(form, "quantifier binder expects (name sort)");
                    }
                    let bind = atom(&parts[0])?.to_string();
                    let sort = lower_sort(&parts[1])?;
                    let body = lower_pexpr(&args[1])?;
                    if name == "forall" {
                        Ok(PExpr::Forall(bind, sort, Box::new(body)))
                    } else {
                        Ok(PExpr::Exists(bind, sort, Box::new(body)))
                    }
                }
                "=>" | "->" => {
                    if args.len() != 2 {
                        return err(form, "implication expects two propositions");
                    }
                    Ok(PExpr::Imp(
                        Box::new(lower_pexpr(&args[0])?),
                        Box::new(lower_pexpr(&args[1])?),
                    ))
                }
                "&&" => {
                    let mut out = lower_pexpr(args.first().ok_or_else(|| {
                        ParseError::new("&& expects arguments", form.line(), form.col())
                    })?)?;
                    for arg in &args[1..] {
                        out = PExpr::And(Box::new(out), Box::new(lower_pexpr(arg)?));
                    }
                    Ok(out)
                }
                "||" => {
                    let mut out = lower_pexpr(args.first().ok_or_else(|| {
                        ParseError::new("|| expects arguments", form.line(), form.col())
                    })?)?;
                    for arg in &args[1..] {
                        out = PExpr::Or(Box::new(out), Box::new(lower_pexpr(arg)?));
                    }
                    Ok(out)
                }
                "!" | "not" => {
                    if args.len() != 1 {
                        return err(form, "not expects one proposition");
                    }
                    Ok(PExpr::Not(Box::new(lower_pexpr(&args[0])?)))
                }
                "array" => Ok(PExpr::Array(
                    args.iter()
                        .map(lower_pexpr)
                        .collect::<Result<Vec<_>, _>>()?,
                )),
                "len" => {
                    if args.len() != 1 {
                        return err(form, "len expects one array");
                    }
                    Ok(PExpr::Len(Box::new(lower_pexpr(&args[0])?)))
                }
                _ => {
                    if let Some(op) = lisp_binop(name) {
                        let lower = |x: &Sexp| lower_pexpr(x);
                        return match (args.len(), op) {
                            (1, BinOp::Sub) => Ok(PExpr::Un(UnOp::Neg, Box::new(lower(&args[0])?))),
                            (1, _) => lower(&args[0]),
                            (
                                2,
                                BinOp::Eq
                                | BinOp::Ne
                                | BinOp::Lt
                                | BinOp::Le
                                | BinOp::Gt
                                | BinOp::Ge,
                            ) => Ok(PExpr::Cmp(
                                op,
                                Box::new(lower(&args[0])?),
                                Box::new(lower(&args[1])?),
                            )),
                            (2, BinOp::And) => Ok(PExpr::And(
                                Box::new(lower(&args[0])?),
                                Box::new(lower(&args[1])?),
                            )),
                            (2, BinOp::Or) => Ok(PExpr::Or(
                                Box::new(lower(&args[0])?),
                                Box::new(lower(&args[1])?),
                            )),
                            (2, _) => Ok(PExpr::Bin(
                                op,
                                Box::new(lower(&args[0])?),
                                Box::new(lower(&args[1])?),
                            )),
                            _ => err(form, "operator expects one or two arguments"),
                        };
                    }
                    Ok(PExpr::Call(
                        name.clone(),
                        args.iter()
                            .map(lower_pexpr)
                            .collect::<Result<Vec<_>, _>>()?,
                    ))
                }
            }
        }
    }
}

fn lower_sort(form: &Sexp) -> Result<SortAst, ParseError> {
    match atom(form)? {
        "Int" | "int" => Ok(SortAst::Int),
        "Bool" | "bool" => Ok(SortAst::Bool),
        "Array" | "array" => Ok(SortAst::Array),
        other => err(form, format!("unknown sort `{other}`")),
    }
}

fn lower_tactic(form: &Sexp) -> Result<Tactic, ParseError> {
    match form {
        Sexp::Atom(name, _, _) => match name.as_str() {
            "assumption" => Ok(Tactic::Assumption),
            "split" => Ok(Tactic::Split),
            "left" => Ok(Tactic::Left),
            "right" => Ok(Tactic::Right),
            "rfl" => Ok(Tactic::Rfl),
            "trivial" => Ok(Tactic::Trivial),
            "decide" => Ok(Tactic::Decide),
            "contradiction" => Ok(Tactic::Contradiction),
            other => err(form, format!("unknown tactic `{other}`")),
        },
        Sexp::List(items, _, _) => {
            let Some(Sexp::Atom(name, _, _)) = items.first() else {
                return err(form, "expected tactic");
            };
            let args = &items[1..];
            match name.as_str() {
                "intro" => Ok(Tactic::Intro(match args.first() {
                    Some(arg) => Some(atom(arg)?.to_string()),
                    None => None,
                })),
                "exact" => Ok(Tactic::Exact(lower_proof_expr(args.first().ok_or_else(
                    || ParseError::new("exact expects a term", form.line(), form.col()),
                )?)?)),
                "apply" => Ok(Tactic::Apply(lower_proof_expr(args.first().ok_or_else(
                    || ParseError::new("apply expects a term", form.line(), form.col()),
                )?)?)),
                other => err(form, format!("unknown tactic `{other}`")),
            }
        }
    }
}

fn lower_proof_expr(form: &Sexp) -> Result<ProofExpr, ParseError> {
    match form {
        Sexp::Atom(name, _, _) => Ok(ProofExpr::Var(name.clone())),
        Sexp::List(items, _, _) => {
            if items.is_empty() {
                return err(form, "empty proof term");
            }
            let mut out = lower_proof_expr(&items[0])?;
            for arg in &items[1..] {
                out = ProofExpr::App(Box::new(out), Box::new(lower_proof_expr(arg)?));
            }
            Ok(out)
        }
    }
}
