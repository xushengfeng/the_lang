# Alethe（工作名）

一门面向 AI 开发、以形式化证明为约束的编程语言。目标不是“让人类更容易写”，而是：

> AI 写代码，规格和类型做契约，小内核做裁判，SMT/LLM 只提供证明候选；
> 业务代码保持可读，性能代码由 AI 重写并被证明约束；
> 源码硬件无关，内存布局/进制等表示由优化器选择，并用精化关系保证正确。

当前仓库处于**内核里程碑**：一个类 Lean 的依赖类型小内核已经可以检查 `sum` 两个实现的等价证明。

## 快速开始

```bash
cargo test
cargo run --bin sum_demo
cargo run --bin ownership_demo
cargo run --bin vm_demo
cargo run --bin ownership_proof_demo
```

`sum_demo` 会输出：

- `sum_ref` 与 `sum_fast` 对 `[1,2,3,4,5]` 的计算结果；
- `sum_fast_eq : ∀ xs, sum_fast xs = sum_ref xs` 的内核检查；
- 一个错误证明被内核拒绝；
- 证明项规模。

`ownership_demo` / `vm_demo` 会检查 `own` / `borrow` / 推断 unique，把 `set` lowering 成原地或复制，并在解释器或栈式 VM 上执行，输出分配/复制/原地操作统计：

```text
ownership check passed
main = 30
stats:
  allocations:   5
  copies:        2
  in-place ops:  2
  bounds checks: 9
```

早期还有一个精化类型表面语法原型：

```bash
cargo run --bin alethe -- examples/checks.alt
```

它目前只做解析/AST 展示，尚未接到依赖类型内核。内核是当前主线。

## 仓库地图

| 路径 | 内容 |
|---|---|
| `src/dependent.rs` | 类 Lean 小内核：de Bruijn 项、`Prop`/`Type`、`Nat`/`List`/`Eq`、recursor、类型检查、定义等价 |
| `src/bin/sum_demo.rs` | 内核 demo：`sum_ref` / `sum_fast` / `sum_fast_eq` |
| `src/ownership.rs` | `own` / `borrow` / unique 检查器 + 原地 lowering + 解释器 |
| `src/bin/ownership_demo.rs` | 所有权 demo CLI |
| `src/vm.rs` | 字节码定义、编译器、栈式 VM |
| `src/bin/vm_demo.rs` | VM demo CLI |
| `examples/ownership.alt` | 所有权/VM 示例 |
| `src/syntax.rs` | 早期精化类型表面语法原型（独立实验） |
| `src/main.rs` | 表面语法原型 CLI |
| `docs/` | 设计、验证风格、内核、类型/运行时、AI 优化、硬件无关、可行性、路线图 |

## 文档索引

1. [愿景与原则](docs/00-vision.md)
2. [需求与验收](docs/01-requirements.md)
3. [验证风格对照：A/B/C/D/E](docs/02-verification-styles.md)
4. [小内核设计](docs/03-kernel.md)
5. [类型与运行时的交错](docs/04-types-runtime.md)
6. [AI 驱动的可验证优化](docs/05-ai-optimization.md)
7. [硬件无关与表示精化](docs/06-hardware-independence.md)
8. [可行性、先行工作与风险](docs/07-feasibility.md)
9. [路线图](docs/08-roadmap.md)
10. [简单栈式 VM](docs/10-vm.md)
11. [检查器模型：所有权/借用/区域/效果](docs/11-checker-model.md)
12. [VM 模型：字节码/堆/语义/验证](docs/12-vm-model.md)
13. [Lowering 与精化模型](docs/13-lowering-model.md)
14. [组件命名与职责](docs/14-roles.md)
15. [函数式默认复制、性能与原地修改](docs/15-functional-performance.md)
16. [编译器、检查器、VM 到底是什么](docs/16-compiler-architecture.md)
17. [业务代码、类型证明、优化：文件与数据流](docs/17-dataflow.md)
18. [决策记录与开放问题](docs/09-decisions.md)

## 模型优先

当前 Rust 代码是**参考原型**，不是规格。`docs/11`–`docs/15` 是目标模型：

- `11-checker-model.md`：`own`/`borrow`/`borrow_mut`/`share`、区域、效果、唯一性、原地规则、soundness 义务；
- `12-vm-model.md`：字节码语法、栈类型、堆/帧、小步语义、字节码验证、编译正确性；
- `13-lowering-model.md`：IR 分层、证明擦除、原地/复制 lowering、精化/翻译验证；
- `14-roles.md`：组件命名（`typecheck`/`owncheck`/`borrowck`/`regioninfer`/`effectcheck`/`refinecheck`/`proofcheck`/`ownlower`）；
- `15-functional-performance.md`：函数式复制语义、Rust 生命周期能借什么、原地修改的安全条件。

模型稳定前，Rust 实现先冻结；后续只在模型确定后回填代码。

| 能力 | 模型（docs 11–13） | Rust 原型 |
|---|---|---|
| `own` / `borrow` / 推断 unique | ✅ 规则完整 | ✅ 简化版 |
| `borrow_mut` / `share` / RC | ✅ 规则 | ❌ |
| 区域/生命周期约束 | ✅ 规则 | ❌ |
| 效果 row（pure/read/write/alloc/io） | ✅ 规则 | ❌ |
| 结构体/变体/切片 | ✅ 类型 | ❌ |
| 字节码语法/栈类型 | ✅ 规格 | ✅ 实现（无验证器） |
| 堆/帧/小步语义 | ✅ 规格 | ❌ |
| 字节码验证 | ✅ 义务 | ❌ |
| 证明擦除 | ✅ 规格 | ⚠️ 仅 proof 参数 |
| 原地精化证明 | ✅ `Bool2` + `Nat` 模型已检查 | ❌ 真实 lowering |
| Borrow/BorrowMut 权限 | ✅ 互斥/只读/可写已检查 | ⚠️ 无 acquire/release/区域 |
| 编译正确性 | ✅ 义务 | ❌ |
| 闭包/异常/并发 | 后续 | ❌ |

## 当前已证明 / 尚未实现

**已跑通**

- 依赖类型内核的类型检查与定义等价；
- `Nat.rec` / `List.rec` / `Eq.subst` 的归约；
- `add_assoc`、`add_zero`、`go_eq`、`sum_fast_eq` 的证明检查；
- 错误证明被拒绝；
- 两个 `sum` 实现的计算结果一致；
- `own` / `borrow` / unique 检查：use-after-move、借用逃逸、把借用 move 进 `own` 参数都会被拒绝；
- `set` 在唯一值上 lowering 成原地，在共享值上复制；解释器输出分配/复制统计；
- 栈式 VM：`source → ownership check/lowering → bytecode → VM`，证明参数擦除（`get` 的 runtime arity 为 2），统计与解释器一致；
- 最小所有权模型（两位置堆）的内核证明：
  - `set_result_same`：原地/纯返回同一数组；
  - `set_frame`：`Unique h` 下原地更新第一个位置对第二个观察者不可见；
- 任意 `Nat` 位置模型的内核证明（归纳，不是枚举）：
  - `set_result_same_nat`；
  - `set_frame_nat`：`UniqueNat h` 下原地更新对任意其它位置不可见；
- Borrow 权限模型的内核证明：
  - `borrow_not_mut`：共享/可变借用互斥；
  - `borrow_cannot_write`：共享借用只读；
  - `borrow_can_read` / `borrow_mut_can_write`；
  - `read_with` / `write_with` 的 proof-carrying 接口。

**下一步**

- 表面证明语言 + 元变量/hole + JSON goal；
- 证明擦除与运行时 IR（VM 已经展示 surface 层的 proof 参数擦除）；
- `Vec`/`Fin` 与更真实的数组/边界证明；
- `spec` / `impl` / `refines` 多实现；
- 完整区域/借用生命周期（当前是简化版唯一性检查）；
- IR/ANF、字节码验证器、闭包；
- 表示层（布局、字长、进制）与翻译验证；
- SMT 证书、证明搜索、LLM 候选生成。
