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
