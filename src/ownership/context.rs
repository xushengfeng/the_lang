use std::collections::{HashMap, HashSet};

use crate::syntax::{Mode, Ty};

#[derive(Clone, Debug)]
pub(super) struct VarInfo {
    pub(super) ty: Ty,
    pub(super) mode: Mode,
    pub(super) unique: bool,
    pub(super) consumed: bool,
    pub(super) uses: usize,
}

#[derive(Clone, Debug)]
pub(super) struct Ctx {
    pub(super) vars: HashMap<String, VarInfo>,
    pub(super) uses: HashMap<String, usize>,
    /// Names that are borrows, including locals, for escape analysis.
    pub(super) borrow_names: HashSet<String>,
}

impl Ctx {
    pub(super) fn info(&self, name: &str) -> Result<&VarInfo, String> {
        self.vars
            .get(name)
            .ok_or_else(|| format!("unknown variable `{name}`"))
    }

    pub(super) fn info_mut(&mut self, name: &str) -> Result<&mut VarInfo, String> {
        self.vars
            .get_mut(name)
            .ok_or_else(|| format!("unknown variable `{name}`"))
    }
}
