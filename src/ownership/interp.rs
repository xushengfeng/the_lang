use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::types::*;
use crate::syntax::{BinOp, CallArg, Expr, UnOp};

// ---------------------------------------------------------------------------
// Interpreter
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Unit,
    Array(Rc<RefCell<Vec<Value>>>),
    Closure(Rc<Closure>),
}

#[derive(Clone, Debug)]
pub enum Closure {
    /// Anonymous function evaluated by the tree-walking interpreter.
    Expr {
        params: Vec<String>,
        body: Expr,
        env: HashMap<String, Value>,
    },
    /// Reference to a top-level function, resolved by name at call time.
    Named(String),
    /// Lifted anonymous function executed by the bytecode VM.
    Bytecode { func: usize, captures: Vec<Value> },
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
            Value::Closure(_) => write!(f, "<fn>"),
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
            Expr::FnRef(name) => Ok(Value::Closure(Rc::new(Closure::Named(name.clone())))),
            Expr::Lambda { params, body, .. } => Ok(Value::Closure(Rc::new(Closure::Expr {
                params: params.iter().map(|p| p.name.clone()).collect(),
                body: (**body).clone(),
                env: env.clone(),
            }))),
            Expr::Apply { callee, args } => {
                let callee = self.eval(callee, env)?;
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval(arg, env)?);
                }
                self.apply(callee, values)
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

    fn apply(&mut self, callee: Value, args: Vec<Value>) -> Result<Value, String> {
        let Value::Closure(closure) = callee else {
            return Err("attempted to call a non-function value".into());
        };
        match closure.as_ref() {
            Closure::Named(name) => self.call(name, args),
            Closure::Expr {
                params,
                body,
                env: captured,
            } => {
                if params.len() != args.len() {
                    return Err(format!(
                        "function value expects {} arguments, got {}",
                        params.len(),
                        args.len()
                    ));
                }
                let mut env = captured.clone();
                for (param, value) in params.iter().zip(args) {
                    env.insert(param.clone(), value);
                }
                self.eval(body, &mut env)
            }
            Closure::Bytecode { .. } => {
                Err("bytecode closure cannot run in the tree interpreter".into())
            }
        }
    }
}
