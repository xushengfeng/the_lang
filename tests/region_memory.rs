use alethe::region::{MemValue, RegionHeap};

fn iv(n: i64) -> MemValue {
    MemValue::Int(n)
}

#[test]
fn free_region_rejects_later_access() {
    let mut heap = RegionHeap::new();
    let region = heap.new_region();
    let array = heap.alloc(region, vec![iv(1), iv(2)]).unwrap();

    heap.free_region(region).unwrap();

    assert!(heap.load(array, 0).is_err());
    assert!(!heap.is_live(region));
    assert_eq!(heap.stats().frees, 1);
}

#[test]
fn in_place_and_copy_observe_the_same_from_one_root() {
    let mut in_place_heap = RegionHeap::new();
    let r1 = in_place_heap.new_region();
    let a1 = in_place_heap.alloc(r1, vec![iv(1), iv(2)]).unwrap();
    let b1 = in_place_heap.set_in_place(a1, 0, iv(9)).unwrap();
    let in_place_obs = in_place_heap.observe(&[MemValue::Loc(b1)]).unwrap();

    let mut copy_heap = RegionHeap::new();
    let r2 = copy_heap.new_region();
    let a2 = copy_heap.alloc(r2, vec![iv(1), iv(2)]).unwrap();
    let b2 = copy_heap.set_copy(a2, 0, iv(9)).unwrap();
    let copy_obs = copy_heap.observe(&[MemValue::Loc(b2)]).unwrap();

    assert_eq!(in_place_obs, copy_obs);
    assert_eq!(copy_heap.load(a2, 0).unwrap(), iv(1));
}

#[test]
fn observation_preserves_sharing() {
    let mut heap = RegionHeap::new();
    let region = heap.new_region();
    let shared = heap.alloc(region, vec![iv(7)]).unwrap();
    let root = heap
        .alloc(region, vec![MemValue::Loc(shared), MemValue::Loc(shared)])
        .unwrap();

    let obs = heap.observe(&[MemValue::Loc(root)]).unwrap();
    assert_eq!(
        obs.blocks[0],
        vec![
            alethe::region::ObsValue::LocRef(1),
            alethe::region::ObsValue::LocRef(1),
        ]
    );
}
