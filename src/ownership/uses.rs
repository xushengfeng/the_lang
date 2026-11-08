use std::collections::{HashMap, HashSet};

use super::rules::is_fresh;
use crate::syntax::{CallArg, Expr, Ty};

pub(super) fn count_uses(e: &Expr, out: &mut HashMap<String, usize>) {
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
        Expr::FnRef(_) => {}
        Expr::Lambda { body, .. } => count_uses(body, out),
        Expr::Apply { callee, args } => {
            count_uses(callee, out);
            for arg in args {
                count_uses(arg, out);
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
pub(super) fn result_borrow(e: &Expr, result_ty: &Ty, borrows: &HashSet<String>) -> Option<String> {
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
        Expr::FnRef(_) | Expr::Lambda { .. } => None,
        Expr::Apply { callee, args } => first_borrow(callee, borrows)
            .or_else(|| args.iter().find_map(|a| first_borrow(a, borrows))),
        Expr::Index(a, _) => first_borrow(a, borrows),
        Expr::Un(_, e) => first_borrow(e, borrows),
        Expr::Bin(_, a, b) => first_borrow(a, borrows).or_else(|| first_borrow(b, borrows)),
        _ => None,
    }
}

pub(super) fn first_borrow(e: &Expr, borrows: &HashSet<String>) -> Option<String> {
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
        Expr::FnRef(_) | Expr::Lambda { .. } => None,
        Expr::Apply { callee, args } => first_borrow(callee, borrows)
            .or_else(|| args.iter().find_map(|a| first_borrow(a, borrows))),
        Expr::Index(a, b) => first_borrow(a, borrows).or_else(|| first_borrow(b, borrows)),
        Expr::Seq(_, b) => first_borrow(b, borrows),
    }
}
