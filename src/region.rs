//! A small region-based memory model.
//!
//! There is no RC and no GC.  A region owns a set of blocks; freeing a region
//! frees all of its blocks at once.  `set` mutates in place when the caller has
//! proved uniqueness, and copies otherwise.
//!
//! The heap is deliberately abstract: `Loc` is `(region, block)`, not a machine
//! address.  This is the executable seed for the model in
//! `docs/18-region-memory.md`; it is not yet wired to the surface language or
//! the VM.

use std::collections::{BTreeMap, HashMap};

// ---------------------------------------------------------------------------
// Identifiers and values
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Heap
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Region {
    live: bool,
    blocks: BTreeMap<BlockId, Vec<MemValue>>,
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

pub struct RegionHeap {
    regions: BTreeMap<RegionId, Region>,
    next_region: u32,
    next_block: u32,
    stats: MemStats,
}

impl Default for RegionHeap {
    fn default() -> Self {
        Self::new()
    }
}

impl RegionHeap {
    pub fn new() -> Self {
        Self {
            regions: BTreeMap::new(),
            next_region: 0,
            next_block: 0,
            stats: MemStats::default(),
        }
    }

    pub fn stats(&self) -> MemStats {
        self.stats
    }

    /// Record thunk/closure construction that is not a heap block.
    pub fn record_thunks(&mut self, n: usize) {
        self.stats.thunks += n;
    }

    /// Record closure construction that is not a heap block.
    pub fn record_closures(&mut self, n: usize) {
        self.stats.closures += n;
    }

    pub fn new_region(&mut self) -> RegionId {
        let id = RegionId(self.next_region);
        self.next_region = self.next_region.checked_add(1).expect("region id overflow");
        self.regions.insert(
            id,
            Region {
                live: true,
                blocks: BTreeMap::new(),
            },
        );
        id
    }

    pub fn is_live(&self, region: RegionId) -> bool {
        self.regions.get(&region).map_or(false, |r| r.live)
    }

    fn live_region(&self, region: RegionId) -> Result<&Region, MemError> {
        let r = self
            .regions
            .get(&region)
            .ok_or(MemError::UnknownRegion(region))?;
        if !r.live {
            return Err(MemError::RegionNotLive(region));
        }
        Ok(r)
    }

    fn live_region_mut(&mut self, region: RegionId) -> Result<&mut Region, MemError> {
        let r = self
            .regions
            .get_mut(&region)
            .ok_or(MemError::UnknownRegion(region))?;
        if !r.live {
            return Err(MemError::RegionNotLive(region));
        }
        Ok(r)
    }

    pub fn alloc(&mut self, region: RegionId, values: Vec<MemValue>) -> Result<Loc, MemError> {
        let block = BlockId(self.next_block);
        self.next_block = self.next_block.checked_add(1).expect("block id overflow");
        let r = self.live_region_mut(region)?;
        r.blocks.insert(block, values);
        self.stats.allocations += 1;
        Ok(Loc { region, block })
    }

    pub fn block(&self, loc: Loc) -> Result<&[MemValue], MemError> {
        let r = self.live_region(loc.region)?;
        r.blocks
            .get(&loc.block)
            .map(|v| v.as_slice())
            .ok_or(MemError::UnknownBlock(loc))
    }

    fn block_mut(&mut self, loc: Loc) -> Result<&mut Vec<MemValue>, MemError> {
        let r = self.live_region_mut(loc.region)?;
        r.blocks
            .get_mut(&loc.block)
            .ok_or(MemError::UnknownBlock(loc))
    }

    pub fn len(&self, loc: Loc) -> Result<usize, MemError> {
        Ok(self.block(loc)?.len())
    }

    pub fn is_empty(&self, loc: Loc) -> Result<bool, MemError> {
        Ok(self.len(loc)? == 0)
    }

    pub fn load(&self, loc: Loc, index: usize) -> Result<MemValue, MemError> {
        let b = self.block(loc)?;
        b.get(index).cloned().ok_or(MemError::OutOfBounds {
            loc,
            index,
            len: b.len(),
        })
    }

    pub fn store(&mut self, loc: Loc, index: usize, value: MemValue) -> Result<(), MemError> {
        let b = self.block_mut(loc)?;
        let len = b.len();
        if index >= len {
            return Err(MemError::OutOfBounds { loc, index, len });
        }
        b[index] = value;
        Ok(())
    }

    /// In-place update.
    ///
    /// Precondition (checked by the memory planner, not by this method):
    /// `Unique(loc)` holds and there is no live borrow of `loc`.
    pub fn set_in_place(
        &mut self,
        loc: Loc,
        index: usize,
        value: MemValue,
    ) -> Result<Loc, MemError> {
        self.store(loc, index, value)?;
        self.stats.in_place += 1;
        Ok(loc)
    }

    /// Copying update: the original block is left unchanged.
    pub fn set_copy(&mut self, loc: Loc, index: usize, value: MemValue) -> Result<Loc, MemError> {
        let mut values = self.block(loc)?.to_vec();
        if index >= values.len() {
            return Err(MemError::OutOfBounds {
                loc,
                index,
                len: values.len(),
            });
        }
        values[index] = value;
        let new_loc = self.alloc(loc.region, values)?;
        self.stats.copies += 1;
        Ok(new_loc)
    }

    /// Copy a block into `target_region`, leaving the source unchanged.
    pub fn copy(&mut self, loc: Loc, target_region: RegionId) -> Result<Loc, MemError> {
        let values = self.block(loc)?.to_vec();
        let new_loc = self.alloc(target_region, values)?;
        self.stats.copies += 1;
        Ok(new_loc)
    }

    /// Append in place.
    ///
    /// Precondition (checked by the memory planner, not by this method):
    /// `Unique(loc)` holds and there is no live borrow of `loc`.
    pub fn push_in_place(&mut self, loc: Loc, value: MemValue) -> Result<Loc, MemError> {
        let block = self.block_mut(loc)?;
        block.push(value);
        self.stats.in_place += 1;
        Ok(loc)
    }

    /// Append by copying the block.
    pub fn push_copy(&mut self, loc: Loc, value: MemValue) -> Result<Loc, MemError> {
        let mut values = self.block(loc)?.to_vec();
        values.push(value);
        let new_loc = self.alloc(loc.region, values)?;
        self.stats.copies += 1;
        Ok(new_loc)
    }

    pub fn free_region(&mut self, region: RegionId) -> Result<(), MemError> {
        let r = self
            .regions
            .get_mut(&region)
            .ok_or(MemError::UnknownRegion(region))?;
        if !r.live {
            return Err(MemError::RegionNotLive(region));
        }
        r.live = false;
        r.blocks.clear();
        self.stats.frees += 1;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Observation
    // -----------------------------------------------------------------------

    /// Canonical observation of the reachable heap from `roots`.
    ///
    /// Block identity is abstracted to a small integer assigned in traversal
    /// order.  Sharing is preserved: two references to the same block get the
    /// same `LocRef`, and a cycle terminates instead of unfolding forever.
    pub fn observe(&self, roots: &[MemValue]) -> Result<Observation, MemError> {
        let mut out = Observation {
            roots: Vec::with_capacity(roots.len()),
            blocks: Vec::new(),
        };
        let mut seen: HashMap<Loc, usize> = HashMap::new();
        for root in roots {
            let v = self.observe_value(root, &mut seen, &mut out.blocks)?;
            out.roots.push(v);
        }
        Ok(out)
    }

    fn observe_value(
        &self,
        value: &MemValue,
        seen: &mut HashMap<Loc, usize>,
        blocks: &mut Vec<Vec<ObsValue>>,
    ) -> Result<ObsValue, MemError> {
        match value {
            MemValue::Int(n) => Ok(ObsValue::Int(*n)),
            MemValue::Bool(b) => Ok(ObsValue::Bool(*b)),
            MemValue::Unit => Ok(ObsValue::Unit),
            MemValue::Loc(loc) => {
                if let Some(id) = seen.get(loc) {
                    return Ok(ObsValue::LocRef(*id));
                }
                let id = blocks.len();
                seen.insert(*loc, id);
                let values = self.block(*loc)?.to_vec();
                blocks.push(Vec::new());
                let mut rendered = Vec::with_capacity(values.len());
                for v in &values {
                    rendered.push(self.observe_value(v, seen, blocks)?);
                }
                blocks[id] = rendered;
                Ok(ObsValue::LocRef(id))
            }
        }
    }
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn iv(n: i64) -> MemValue {
        MemValue::Int(n)
    }

    #[test]
    fn alloc_load_store() {
        let mut h = RegionHeap::new();
        let r = h.new_region();
        let a = h.alloc(r, vec![iv(1), iv(2)]).unwrap();

        assert_eq!(h.len(a).unwrap(), 2);
        assert_eq!(h.load(a, 1).unwrap(), iv(2));

        h.store(a, 1, iv(9)).unwrap();
        assert_eq!(h.load(a, 1).unwrap(), iv(9));
        assert_eq!(h.stats().allocations, 1);
    }

    #[test]
    fn out_of_bounds_is_rejected() {
        let mut h = RegionHeap::new();
        let r = h.new_region();
        let a = h.alloc(r, vec![iv(1)]).unwrap();

        assert!(matches!(
            h.load(a, 1),
            Err(MemError::OutOfBounds {
                index: 1,
                len: 1,
                ..
            })
        ));
        assert!(matches!(
            h.store(a, 1, iv(2)),
            Err(MemError::OutOfBounds {
                index: 1,
                len: 1,
                ..
            })
        ));
    }

    #[test]
    fn push_in_place_and_push_copy() {
        let mut h = RegionHeap::new();
        let r = h.new_region();
        let a = h.alloc(r, vec![iv(1)]).unwrap();

        h.push_in_place(a, iv(2)).unwrap();
        assert_eq!(h.block(a).unwrap(), &[iv(1), iv(2)]);
        assert_eq!(h.stats().in_place, 1);

        let b = h.push_copy(a, iv(3)).unwrap();
        assert_eq!(h.block(a).unwrap(), &[iv(1), iv(2)]);
        assert_eq!(h.block(b).unwrap(), &[iv(1), iv(2), iv(3)]);
        assert_eq!(h.stats().copies, 1);
    }

    #[test]
    fn free_region_rejects_later_access() {
        let mut h = RegionHeap::new();
        let r = h.new_region();
        let a = h.alloc(r, vec![iv(1)]).unwrap();

        h.free_region(r).unwrap();
        assert_eq!(h.load(a, 0), Err(MemError::RegionNotLive(r)));
        assert_eq!(h.free_region(r), Err(MemError::RegionNotLive(r)));
        assert_eq!(h.stats().frees, 1);
    }

    #[test]
    fn in_place_and_copy_observe_the_same_under_one_root() {
        let mut in_place_heap = RegionHeap::new();
        let r1 = in_place_heap.new_region();
        let a1 = in_place_heap.alloc(r1, vec![iv(1), iv(2), iv(3)]).unwrap();
        let b1 = in_place_heap.set_in_place(a1, 1, iv(9)).unwrap();
        let in_place_obs = in_place_heap.observe(&[MemValue::Loc(b1)]).unwrap();

        let mut copy_heap = RegionHeap::new();
        let r2 = copy_heap.new_region();
        let a2 = copy_heap.alloc(r2, vec![iv(1), iv(2), iv(3)]).unwrap();
        let b2 = copy_heap.set_copy(a2, 1, iv(9)).unwrap();
        let copy_obs = copy_heap.observe(&[MemValue::Loc(b2)]).unwrap();

        assert_eq!(in_place_obs, copy_obs);
    }

    #[test]
    fn copy_leaves_the_original_unchanged() {
        let mut h = RegionHeap::new();
        let r = h.new_region();
        let a = h.alloc(r, vec![iv(1), iv(2), iv(3)]).unwrap();
        let b = h.set_copy(a, 1, iv(9)).unwrap();

        assert_eq!(h.load(a, 1).unwrap(), iv(2));
        assert_eq!(h.load(b, 1).unwrap(), iv(9));
        assert_eq!(h.stats().allocations, 2);
        assert_eq!(h.stats().copies, 1);
    }

    #[test]
    fn inner_region_can_die_while_outer_stays_live() {
        let mut h = RegionHeap::new();
        let outer = h.new_region();
        let inner = h.new_region();
        let a = h.alloc(inner, vec![iv(1)]).unwrap();
        let b = h.alloc(outer, vec![iv(2)]).unwrap();

        h.free_region(inner).unwrap();
        assert_eq!(h.load(a, 0), Err(MemError::RegionNotLive(inner)));
        assert_eq!(h.load(b, 0).unwrap(), iv(2));
    }

    #[test]
    fn observe_preserves_sharing() {
        let mut h = RegionHeap::new();
        let r = h.new_region();
        let shared = h.alloc(r, vec![iv(7)]).unwrap();
        let root = h
            .alloc(r, vec![MemValue::Loc(shared), MemValue::Loc(shared)])
            .unwrap();

        let obs = h.observe(&[MemValue::Loc(root)]).unwrap();
        assert_eq!(obs.roots, vec![ObsValue::LocRef(0)]);
        assert_eq!(
            obs.blocks[0],
            vec![ObsValue::LocRef(1), ObsValue::LocRef(1)]
        );
        assert_eq!(obs.blocks[1], vec![ObsValue::Int(7)]);
    }

    #[test]
    fn observing_a_dead_nested_region_is_rejected() {
        let mut h = RegionHeap::new();
        let outer = h.new_region();
        let inner = h.new_region();
        let a = h.alloc(inner, vec![iv(1)]).unwrap();
        let root = h.alloc(outer, vec![MemValue::Loc(a)]).unwrap();

        h.free_region(inner).unwrap();
        assert_eq!(
            h.observe(&[MemValue::Loc(root)]),
            Err(MemError::RegionNotLive(inner))
        );
    }
}
