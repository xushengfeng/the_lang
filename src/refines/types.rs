#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpecExpr {
    Var(String),
    Int(i64),
    Bool(bool),
    Array(Vec<SpecExpr>),
    Call(String, Vec<SpecExpr>),
}

impl SpecExpr {
    pub(super) fn pretty(&self) -> String {
        match self {
            SpecExpr::Var(n) => n.clone(),
            SpecExpr::Int(n) => n.to_string(),
            SpecExpr::Bool(b) => b.to_string(),
            SpecExpr::Array(xs) => format!(
                "[{}]",
                xs.iter().map(|x| x.pretty()).collect::<Vec<_>>().join(", ")
            ),
            SpecExpr::Call(n, args) => format!(
                "{}({})",
                n,
                args.iter()
                    .map(|x| x.pretty())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RefinesReport {
    pub impl_name: String,
    pub spec_name: String,
    pub ok: bool,
    pub message: String,
    pub kernel_proof: Option<String>,
}
