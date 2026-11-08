use alethe::mem_plan::{plan_checked_fn, plan_checked_program, run_fn_in, run_program};
use alethe::ownership::check_source;
use alethe::region::{MemValue, RegionHeap};

fn array_value(xs: &[i64]) -> Vec<MemValue> {
    xs.iter().map(|n| MemValue::Int(*n)).collect()
}

fn main() {
    let src = r#"
        fn fill(own a: Array<Int>, x: Int) -> Array<Int> {
            set(a, 0, x)
        }

        fn fill_shared(a: Array<Int>, x: Int) -> Array<Int> {
            set(a, 0, x)
        }

        fn grow(own a: Array<Int>, x: Int) -> Array<Int> {
            push(a, x)
        }

        fn third() -> Int {
            let a = [1, 2, 3];
            a[2]
        }
    "#;

    let prog = check_source(src).unwrap();

    {
        let f = prog.get("fill").unwrap();
        let mut heap = RegionHeap::new();
        let root = heap.new_region();
        let a = heap.alloc(root, array_value(&[1, 2, 3])).unwrap();
        let result = run_fn_in(f, &[MemValue::Loc(a), MemValue::Int(9)], &mut heap, root).unwrap();
        println!("fill (own, in place)");
        println!("  result: {:?}", heap.observe(&[result]).unwrap());
        println!("  stats:  {:?}", heap.stats());
    }

    {
        let f = prog.get("fill_shared").unwrap();
        let mut heap = RegionHeap::new();
        let root = heap.new_region();
        let a = heap.alloc(root, array_value(&[1, 2, 3])).unwrap();
        let result = run_fn_in(f, &[MemValue::Loc(a), MemValue::Int(9)], &mut heap, root).unwrap();
        println!("fill_shared (copy)");
        println!(
            "  original: {:?}",
            heap.observe(&[MemValue::Loc(a)]).unwrap()
        );
        println!("  result:   {:?}", heap.observe(&[result]).unwrap());
        println!("  stats:    {:?}", heap.stats());
    }

    {
        let f = prog.get("grow").unwrap();
        let mut heap = RegionHeap::new();
        let root = heap.new_region();
        let a = heap.alloc(root, array_value(&[1, 2, 3])).unwrap();
        let result = run_fn_in(f, &[MemValue::Loc(a), MemValue::Int(9)], &mut heap, root).unwrap();
        println!("grow (own push)");
        println!("  result: {:?}", heap.observe(&[result]).unwrap());
        println!("  stats:  {:?}", heap.stats());
    }

    {
        let f = prog.get("third").unwrap();
        let mut heap = RegionHeap::new();
        let root = heap.new_region();
        let result = run_fn_in(f, &[], &mut heap, root).unwrap();
        println!("third (let + index)");
        println!("  result: {result:?}");
        println!("  stats:  {:?}", heap.stats());
    }

    {
        let src = "fn choose(c: Bool) -> Int { if c then 1 else 2 }";
        let prog = check_source(src).unwrap();
        let f = prog.get("choose").unwrap();
        let mut heap = RegionHeap::new();
        let root = heap.new_region();
        let yes = run_fn_in(f, &[MemValue::Bool(true)], &mut heap, root).unwrap();
        let no = run_fn_in(f, &[MemValue::Bool(false)], &mut heap, root).unwrap();
        println!("direct if");
        println!("  true:  {yes:?}");
        println!("  false: {no:?}");
        println!("  stats: {:?}", heap.stats());
    }

    let unsupported = check_source("fn add(a: Int) -> Int { a + 1 }").unwrap();
    let err = plan_checked_fn(unsupported.get("add").unwrap()).unwrap_err();
    println!("unsupported");
    println!("  rejected: {err}");

    {
        let src = r#"
            fn fill(own a: Array<Int>, x: Int) -> Array<Int> {
                set(a, 0, x)
            }
            fn main() -> Int {
                let a = [1, 2, 3];
                let b = fill(a, 9);
                b[0]
            }
        "#;
        let prog = check_source(src).unwrap();
        let mem = plan_checked_program(&prog).unwrap();
        let result = run_program(&mem, "main").unwrap();
        println!("call fill (owned argument, in place)");
        println!("  result: {:?}", result.value);
        println!("  stats:  {:?}", result.stats());
    }

    {
        let src = r#"
            fn make(x: Int) -> Array<Int> {
                [x, x]
            }
            fn main() -> Int {
                let a = make(7);
                a[0]
            }
        "#;
        let prog = check_source(src).unwrap();
        let mem = plan_checked_program(&prog).unwrap();
        let result = run_program(&mem, "main").unwrap();
        println!("call make (callee allocates, return copy)");
        println!("  result: {:?}", result.value);
        println!("  stats:  {:?}", result.stats());
    }
}
