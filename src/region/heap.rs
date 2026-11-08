use std::collections::{BTreeMap, HashMap};

use super::types::{BlockId, Loc, MemError, MemStats, MemValue, ObsValue, Observation, RegionId};

#[derive(Clone, Debug)]
struct Region {
    live: bool,
    blocks: BTreeMap<BlockId, Vec<MemValue>>,
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
        self.regions.get(&region).is_some_and(|r| r.live)
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
