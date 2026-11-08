# 区域内存模型（无 RC/GC）

本文定义内存层的第一版模型：纯函数式源语言经过算法优化后，由内存规划器
（`regioninfer` / `borrowck` / `ownlower`）选择分配位置、释放时机和原地/复制，
再由固定规则 lowering 到字节码。这里没有引用计数，也没有 tracing GC。

## 1. 基本原则

无 RC/GC 时，可回收内存只有三种来源：

1. 线性/唯一所有权：一个对象恰好一个所有者，可以在最后使用后释放；
2. 区域：对象属于区域，区域结束时整体释放；
3. 复制：共享时复制，用空间换简单。

模型采用混合策略：

- 非逃逸值放栈或当前区域；
- 共享不可变值放区域，区域整体释放；
- `own`/unique 值可原地更新，最后使用后释放；
- 无法证明唯一或生命周期时，回退到复制。

不引入 RC/GC，因此共享数据的释放粒度是区域，而不是单个对象。

## 2. 堆模型

```text
RegionId, BlockId, Loc = (RegionId, BlockId)
Value  = Int | Bool | Unit | Loc
Region = { live : Bool, blocks : BlockId ⇀ [Value] }
Heap   = RegionId ⇀ Region
```

- `Loc` 是抽象位置，不是机器地址；
- 分配必须发生在活区域；
- 访问已释放区域返回显式错误，不产生未定义行为；
- `free_region` 一次性释放该区域的所有 block；
- 同一区域内的多个 `Loc` 可以共享同一 block。

观察函数 `obs(roots)` 从根出发遍历可达 block，并把 block 身份规范化为
遍历序号。它保留共享和环，不把分配地址当成可观察值。

## 3. 规则

| 源模式 | 决策 | 条件 | 回退 |
|---|---|---|---|
| 非逃逸局部值 | 栈/当前区域 | 不逃逸 | 外层区域 |
| 堆对象 | 区域 `r` | `r` 覆盖所有使用 | 提升/复制 |
| `set` / `push` | 原地 | `Unique(loc)` 且无 live borrow | 复制 |
| `borrow` | 只读别名（可擦除） | 生命周期 ≤ 所有者 | 拒绝 |
| `borrow_mut` | 独占别名 | 无其他借用 | 拒绝 |
| `share` | 区域内只读共享 | 区域覆盖所有使用 | 复制 |
| 返回/逃逸 | 提升到调用者区域 | 目标区域 outlives | 复制 |
| 区域结束 | 整体 free | 无 live 引用 | 类型错误 |

`Unique(loc)` 的含义是“恰好一个所有者，且没有 live borrow”。它和区域约束
一起，构成原地更新和提前释放的静态前提。

## 4. 证明义务

1. **无悬垂**：well-typed 程序不会访问已释放区域或越界 block。
2. **原地精化**：`Unique(loc) ∧ 无 live borrow ⇒ obs(set_in_place) = obs(set_pure)`。
3. **lowering 模拟**：每个内存 pass 保持观察等价（返回值、事件、显式错误、终止）。
4. **复制回退安全**：无法满足原地条件时，复制版本与纯语义等价。

这些定理针对模型，不针对 Rust 实现。Rust 实现要么作为可信的小运行时，
要么通过差分测试/翻译验证与模型对齐。

## 5. 原型

`src/region.rs` 实现第一版可执行模型：

- `RegionHeap`：`new_region`、`alloc`、`load`、`store`、`set_in_place`、
  `set_copy`、`copy`、`free_region`；
- `observe`：规范化的可达堆观察，保留共享和环；
- `MemStats`：分配、复制、原地、释放、thunk、closure 计数；
- 测试覆盖：越界、释放后访问、原地/复制观察等价、复制不改原值、
  区域嵌套、共享观察、悬垂嵌套区域拒绝。

`src/mem_plan.rs` 在其上加了最小内存规划器和 MemIR：

- 源项带 `own` / `share` 注解；
- 规划器做 move/downgrade/copy，选择 `set` 的原地或复制；
- 拒绝 use-after-consume，以及区域体返回数组的逃逸；
- 求值器在 `RegionHeap` 上执行，区域结束时整体释放；
- 测试覆盖：唯一原地、共享复制、复制保留原值、`own` 共享值复制、
  use-after-consume、区域释放、区域逃逸拒绝、返回数组观察。

`src/mem_plan.rs` 还提供到现有 surface 的桥：

- `plan_checked_fn` 把 `ownership::CheckedFn` 的 body 降到 MemIR；
- `plan_checked_program` 把整个 `CheckedProgram` 降成多函数 `MemProgram`；
- `run_fn_in` 在已有 `RegionHeap`/区域上执行单个函数；
- `run_program` / `run_program_in` 执行多函数程序；
- 每次函数调用创建一个新区域；标量返回后释放；被调用者在自己的区域里
  新建的数组返回值会复制到调用者区域，再释放调用区域；
- 已支持 `Int`/`Bool`/`Unit`、`Var`、`Array`、`Let`、`Seq`、
  常量下标 `Index`、普通函数调用、直接 `if`、`__set_in_place`/`__set_copy`、
  `__push_in_place`/`__push_copy`、`len`；
- 直接 `if` 降成 primitive branch：只执行选中的分支，不构造分支闭包；
- `MemExpr::If { optimized: false }` 构造两个分支 thunk（捕获当前环境），
  只强制选中的一个；`MemStats.thunks` 会计数；
- MemIR 里 thunk/closure 已是一等值：`MemExpr::Lambda` / `Apply` /
  `Thunk` / `Force`，可以绑定、传递、调用；`MemStats.closures` 计数；
- thunk 目前是“零参闭包 + 环境 + body”，`Force` 直接求值，没有 memoization；
  `if` 的 thunk 最多 force 一次，所以不需要缓存；
- thunk/closure 还不能存进数组，也不能跨函数返回；区域逃逸会拒绝；
  表面语言还没有函数类型和 lambda；
- 算术、`print`、动态下标暂不支持。

Thunk 和闭包的区别：

```text
Closure = params + body + captured env     -- 可以带参数调用
Thunk   = body + captured env              -- 零参延迟计算，force 后得到值
```

在严格语言里，thunk 用来表示“现在不求值、以后可能求值”的表达式；它不是
惰性语言本身，而是一种显式的延迟值。`if` 的未选中分支就是这种值。

`if` 的语义基线是严格求值 + 显式 thunk 的函数形式：

```text
if : Bool -> (Unit -> A) -> (Unit -> A) -> A
```

直接使用的 `if c then t else e` 可以证明为饱和应用、thunk 不逃逸时，
优化成 primitive branch（零成本）；分析不了或把 `if` 当一等值时，保留
thunk/闭包构造。当前 surface 只走直接形式，MemIR 已有一等闭包和
`optimized = false` 的 thunk 回退。

`cargo run --bin region_demo` 展示区域堆；`cargo run --bin mem_plan_demo`
展示规划器、闭包/thunk 和统计；`cargo run --bin surface_mem_demo`
展示 checked surface 函数和普通函数调用在区域堆上的执行。

当前边界：只有 `Int` 数组；surface 还没有函数类型和 lambda；MemIR 的
闭包/thunk 还不能跨函数返回或跨区域逃逸；返回值按复制处理，还没有区域提升；
顶层返回值留在调用者区域，由调用者释放。

## 6. 下一步

1. 加表面函数类型/lambda，把闭包和 thunk 接到 surface；
2. 实现逃逸分析/区域提升，支持闭包/thunk 逃逸和返回值；
3. 加非逃逸值的栈分配和 `borrow`/`borrow_mut`；
4. 用一个真实函数证明“区域安全 + 原地精化”的端到端闭环。
