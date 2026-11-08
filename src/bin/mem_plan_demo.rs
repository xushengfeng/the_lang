use alethe::mem_plan::{plan, run, BindMode, MemExpr, SrcExpr};

fn arr(xs: &[i64]) -> SrcExpr {
    SrcExpr::Array(xs.iter().map(|n| SrcExpr::Int(*n)).collect())
}

fn var(name: &str) -> SrcExpr {
    SrcExpr::Var(name.to_string())
}

fn own(name: &str, value: SrcExpr, body: SrcExpr) -> SrcExpr {
    SrcExpr::Let {
        name: name.to_string(),
        mode: BindMode::Own,
        value: Box::new(value),
        body: Box::new(body),
    }
}

fn share(name: &str, value: SrcExpr, body: SrcExpr) -> SrcExpr {
    SrcExpr::Let {
        name: name.to_string(),
        mode: BindMode::Share,
        value: Box::new(value),
        body: Box::new(body),
    }
}

fn set(name: &str, index: usize, value: i64) -> SrcExpr {
    SrcExpr::Set {
        target: Box::new(var(name)),
        index,
        value: Box::new(SrcExpr::Int(value)),
    }
}

fn get(name: &str, index: usize) -> SrcExpr {
    SrcExpr::Get {
        target: Box::new(var(name)),
        index,
    }
}

fn show(label: &str, src: &SrcExpr) {
    match plan(src) {
        Ok(planned) => {
            let result = run(&planned).unwrap();
            println!(
                "{label}\n  value: {:?}\n  stats: {:?}\n  obs:   {:?}",
                result.value,
                result.stats(),
                result.observe().unwrap()
            );
        }
        Err(e) => println!("{label}\n  rejected: {e}"),
    }
}

fn main() {
    show(
        "unique set",
        &own("a", arr(&[1, 2, 3]), own("b", set("a", 1, 9), get("b", 1))),
    );

    show(
        "shared set (copy)",
        &own(
            "a",
            arr(&[1, 2, 3]),
            share("s", var("a"), own("b", set("s", 1, 9), get("b", 1))),
        ),
    );

    show(
        "shared original after copy",
        &own(
            "a",
            arr(&[1, 2, 3]),
            share("s", var("a"), own("b", set("s", 1, 9), get("a", 1))),
        ),
    );

    show(
        "region",
        &SrcExpr::Region(Box::new(own("a", arr(&[1, 2, 3]), get("a", 1)))),
    );

    show("region escape", &SrcExpr::Region(Box::new(arr(&[1, 2, 3]))));

    show(
        "use after consume",
        &own("a", arr(&[1, 2, 3]), own("b", set("a", 1, 9), get("a", 1))),
    );

    let identity = MemExpr::Lambda {
        params: vec!["x".into()],
        body: Box::new(MemExpr::Var("x".into())),
    };
    let apply = MemExpr::Apply {
        callee: Box::new(identity),
        args: vec![MemExpr::Int(7)],
    };
    let result = run(&apply).unwrap();
    println!(
        "lambda + apply\n  value: {:?}\n  stats: {:?}",
        result.value,
        result.stats()
    );

    let thunk = MemExpr::Force(Box::new(MemExpr::Thunk(Box::new(MemExpr::Int(9)))));
    let result = run(&thunk).unwrap();
    println!(
        "thunk + force\n  value: {:?}\n  stats: {:?}",
        result.value,
        result.stats()
    );

    let fallback = MemExpr::If {
        cond: Box::new(MemExpr::Bool(true)),
        then_branch: Box::new(MemExpr::Int(1)),
        else_branch: Box::new(MemExpr::Int(2)),
        optimized: false,
    };
    let result = run(&fallback).unwrap();
    println!(
        "if fallback (two thunks)\n  value: {:?}\n  stats: {:?}",
        result.value,
        result.stats()
    );
}
