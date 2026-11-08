use super::context::Ctx;
use crate::syntax::{CallArg, Expr, Mode, Ty};

pub(super) fn expr_arg(a: &CallArg) -> Option<&Expr> {
    match a {
        CallArg::Expr(e) => Some(e),
        CallArg::Proof(_) => None,
    }
}

pub(super) fn mode_name(m: &Mode) -> &'static str {
    match m {
        Mode::Val => "value",
        Mode::Own => "own",
        Mode::Borrow => "borrow",
    }
}

pub(super) fn expect_ty(actual: &Ty, expected: &Ty, what: &str) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!("{what} expects `{expected}`, got `{actual}`"))
    }
}

/// Is this expression a fresh array (safe to treat as uniquely owned)?
pub(super) fn is_fresh(e: &Expr) -> bool {
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
pub(super) fn can_in_place(ctx: &Ctx, base: Option<&Expr>) -> bool {
    let Some(Expr::Var(v)) = base else {
        return false;
    };
    let Some(info) = ctx.vars.get(v) else {
        return false;
    };
    info.unique && !info.consumed && info.uses == 1 && info.mode != Mode::Borrow
}

pub(super) fn consume_expr(ctx: &mut Ctx, base: Option<&Expr>) -> Result<(), String> {
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
pub(super) fn transfer_arg(
    ctx: &mut Ctx,
    arg: &Expr,
    param: &str,
    fname: &str,
) -> Result<(), String> {
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
pub(super) fn borrow_arg(ctx: &Ctx, arg: &Expr, param: &str, fname: &str) -> Result<(), String> {
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
pub(super) fn read_arg(ctx: &Ctx, arg: &Expr, param: &str, fname: &str) -> Result<(), String> {
    if let Expr::Var(v) = arg {
        let info = ctx.info(v)?;
        if info.consumed {
            return Err(format!("use of moved value `{v}`"));
        }
    }
    let _ = (param, fname);
    Ok(())
}
