use std::collections::HashMap;

use super::ir::{BindMode, MemExpr, SrcExpr};

// ---------------------------------------------------------------------------
// Planner
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValType {
    Int,
    Array,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ownership {
    Unique,
    Shared,
    Consumed,
}

#[derive(Clone, Copy, Debug)]
struct VarState {
    ty: ValType,
    ownership: Ownership,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanError {
    UnknownVar(String),
    UseAfterConsume(String),
    NotAnArray,
    NotAnInt,
    BadArrayElement,
    RegionEscape,
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::UnknownVar(name) => write!(f, "unknown variable `{name}`"),
            PlanError::UseAfterConsume(name) => write!(f, "use after consume `{name}`"),
            PlanError::NotAnArray => write!(f, "expected an array"),
            PlanError::NotAnInt => write!(f, "expected an Int"),
            PlanError::BadArrayElement => write!(f, "array elements must be Int"),
            PlanError::RegionEscape => {
                write!(
                    f,
                    "a region body may not return an array (escape analysis not implemented)"
                )
            }
        }
    }
}

impl std::error::Error for PlanError {}

#[derive(Default)]
struct PlanEnv {
    vars: HashMap<String, VarState>,
}

impl PlanEnv {
    fn get(&self, name: &str) -> Result<VarState, PlanError> {
        let state = self
            .vars
            .get(name)
            .ok_or_else(|| PlanError::UnknownVar(name.to_string()))?;
        if state.ownership == Ownership::Consumed {
            return Err(PlanError::UseAfterConsume(name.to_string()));
        }
        Ok(*state)
    }

    fn consume(&mut self, name: &str) -> Result<(), PlanError> {
        let state = self
            .vars
            .get_mut(name)
            .ok_or_else(|| PlanError::UnknownVar(name.to_string()))?;
        if state.ownership == Ownership::Consumed {
            return Err(PlanError::UseAfterConsume(name.to_string()));
        }
        state.ownership = Ownership::Consumed;
        Ok(())
    }

    fn share(&mut self, name: &str) -> Result<(), PlanError> {
        let state = self
            .vars
            .get_mut(name)
            .ok_or_else(|| PlanError::UnknownVar(name.to_string()))?;
        if state.ownership == Ownership::Consumed {
            return Err(PlanError::UseAfterConsume(name.to_string()));
        }
        state.ownership = Ownership::Shared;
        Ok(())
    }
}

struct Planned {
    expr: MemExpr,
    ty: ValType,
    ownership: Ownership,
}

pub fn plan(src: &SrcExpr) -> Result<MemExpr, PlanError> {
    let mut env = PlanEnv::default();
    Ok(plan_expr(src, &mut env)?.expr)
}

fn plan_expr(src: &SrcExpr, env: &mut PlanEnv) -> Result<Planned, PlanError> {
    match src {
        SrcExpr::Int(n) => Ok(Planned {
            expr: MemExpr::Int(*n),
            ty: ValType::Int,
            ownership: Ownership::Shared,
        }),
        SrcExpr::Var(name) => {
            let state = env.get(name)?;
            Ok(Planned {
                expr: MemExpr::Var(name.clone()),
                ty: state.ty,
                ownership: state.ownership,
            })
        }
        SrcExpr::Array(items) => {
            let mut planned = Vec::with_capacity(items.len());
            for item in items {
                let p = plan_expr(item, env)?;
                if p.ty != ValType::Int {
                    return Err(PlanError::BadArrayElement);
                }
                planned.push(p.expr);
            }
            Ok(Planned {
                expr: MemExpr::Alloc(planned),
                ty: ValType::Array,
                ownership: Ownership::Unique,
            })
        }
        SrcExpr::Let {
            name,
            mode,
            value,
            body,
        } => {
            let value_plan = plan_expr(value, env)?;
            let binding = match mode {
                BindMode::Own => match (value_plan.ty, value_plan.ownership) {
                    (ValType::Array, Ownership::Unique) => {
                        if let SrcExpr::Var(old) = value.as_ref() {
                            env.consume(old)?;
                        }
                        VarState {
                            ty: ValType::Array,
                            ownership: Ownership::Unique,
                        }
                    }
                    (ValType::Array, Ownership::Shared) => {
                        // `own` of a shared value: copy, leave the original shared.
                        VarState {
                            ty: ValType::Array,
                            ownership: Ownership::Unique,
                        }
                    }
                    (ValType::Array, Ownership::Consumed) => {
                        unreachable!("planned value cannot be consumed")
                    }
                    _ => VarState {
                        ty: value_plan.ty,
                        ownership: Ownership::Shared,
                    },
                },
                BindMode::Share => {
                    if value_plan.ty == ValType::Array {
                        if let SrcExpr::Var(old) = value.as_ref() {
                            if env.get(old)?.ownership == Ownership::Unique {
                                env.share(old)?;
                            }
                        }
                    }
                    VarState {
                        ty: value_plan.ty,
                        ownership: Ownership::Shared,
                    }
                }
            };

            let value_expr = match (mode, value_plan.ty, value_plan.ownership) {
                (BindMode::Own, ValType::Array, Ownership::Shared) => {
                    MemExpr::Copy(Box::new(value_plan.expr))
                }
                _ => value_plan.expr,
            };

            let old = env.vars.insert(name.clone(), binding);
            let body_result = plan_expr(body, env);
            match old {
                Some(old) => {
                    env.vars.insert(name.clone(), old);
                }
                None => {
                    env.vars.remove(name);
                }
            }
            let body_plan = body_result?;
            Ok(Planned {
                expr: MemExpr::Let {
                    name: name.clone(),
                    value: Box::new(value_expr),
                    body: Box::new(body_plan.expr),
                },
                ty: body_plan.ty,
                ownership: body_plan.ownership,
            })
        }
        SrcExpr::Set {
            target,
            index,
            value,
        } => {
            let target_plan = plan_expr(target, env)?;
            if target_plan.ty != ValType::Array {
                return Err(PlanError::NotAnArray);
            }
            let value_plan = plan_expr(value, env)?;
            if value_plan.ty != ValType::Int {
                return Err(PlanError::NotAnInt);
            }

            let in_place = target_plan.ownership == Ownership::Unique;
            if in_place {
                if let SrcExpr::Var(name) = target.as_ref() {
                    env.consume(name)?;
                }
            }

            Ok(Planned {
                expr: MemExpr::Set {
                    target: Box::new(target_plan.expr),
                    index: *index,
                    value: Box::new(value_plan.expr),
                    in_place,
                },
                ty: ValType::Array,
                ownership: Ownership::Unique,
            })
        }
        SrcExpr::Get { target, index } => {
            let target_plan = plan_expr(target, env)?;
            if target_plan.ty != ValType::Array {
                return Err(PlanError::NotAnArray);
            }
            Ok(Planned {
                expr: MemExpr::Get {
                    target: Box::new(target_plan.expr),
                    index: *index,
                },
                ty: ValType::Int,
                ownership: Ownership::Shared,
            })
        }
        SrcExpr::Region(body) => {
            let body_plan = plan_expr(body, env)?;
            if body_plan.ty == ValType::Array {
                return Err(PlanError::RegionEscape);
            }
            Ok(Planned {
                expr: MemExpr::Region(Box::new(body_plan.expr)),
                ty: body_plan.ty,
                ownership: body_plan.ownership,
            })
        }
        SrcExpr::Return(inner) => {
            let inner_plan = plan_expr(inner, env)?;
            Ok(Planned {
                expr: MemExpr::Return(Box::new(inner_plan.expr)),
                ty: inner_plan.ty,
                ownership: inner_plan.ownership,
            })
        }
    }
}
