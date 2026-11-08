//! Region identifiers, values, errors, statistics, and observations.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegionId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Loc {
    pub region: RegionId,
    pub block: BlockId,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemValue {
    Int(i64),
    Bool(bool),
    Unit,
    Loc(Loc),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemError {
    UnknownRegion(RegionId),
    RegionNotLive(RegionId),
    UnknownBlock(Loc),
    OutOfBounds { loc: Loc, index: usize, len: usize },
}

impl std::fmt::Display for MemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemError::UnknownRegion(r) => write!(f, "unknown region {}", r.0),
            MemError::RegionNotLive(r) => write!(f, "region {} is not live", r.0),
            MemError::UnknownBlock(l) => write!(f, "unknown block {:?}", l),
            MemError::OutOfBounds { loc, index, len } => {
                write!(f, "out of bounds: {:?}[{}] (len {})", loc, index, len)
            }
        }
    }
}

impl std::error::Error for MemError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemStats {
    pub allocations: usize,
    pub copies: usize,
    pub in_place: usize,
    pub frees: usize,
    pub thunks: usize,
    pub closures: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub roots: Vec<ObsValue>,
    pub blocks: Vec<Vec<ObsValue>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObsValue {
    Int(i64),
    Bool(bool),
    Unit,
    LocRef(usize),
}
