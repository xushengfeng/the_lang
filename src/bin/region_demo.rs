use alethe::region::{MemValue, RegionHeap};

fn iv(n: i64) -> MemValue {
    MemValue::Int(n)
}

fn main() {
    let mut heap = RegionHeap::new();
    let r = heap.new_region();

    let a = heap.alloc(r, vec![iv(1), iv(2), iv(3)]).unwrap();
    println!("alloc a = {a:?}");

    let b = heap.set_in_place(a, 1, iv(9)).unwrap();
    println!(
        "in-place b = {:?}",
        heap.observe(&[MemValue::Loc(b)]).unwrap()
    );

    let c = heap.set_copy(a, 2, iv(7)).unwrap();
    println!(
        "copy c     = {:?}",
        heap.observe(&[MemValue::Loc(c)]).unwrap()
    );
    println!(
        "original a = {:?}",
        heap.observe(&[MemValue::Loc(a)]).unwrap()
    );

    println!("stats before free: {:?}", heap.stats());
    heap.free_region(r).unwrap();
    println!("stats after free:  {:?}", heap.stats());
    println!("access after free: {:?}", heap.load(a, 0));
}
