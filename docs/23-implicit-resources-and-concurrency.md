# 隐式资源、Arena 与并发管理选项（草案）

本文记录当前讨论中的几种方案，不是已承诺语义。目标是先固定问题、权衡和证明义务，
再决定哪些进入 surface，哪些只属于 lowering/optimizer/runtime。

## 0. 问题

三个相关但不同的问题：

1. **Arena 能否隐式**：surface 不写 `region`/`arena`，由区域推断和 lowering 决定。
2. **资源关闭能否隐式**：surface 不写 `with`，由分析/证明器决定何时关闭。
3. **并发管理放在哪**：`scope`、`TaskGroup`、显式 `CancelToken`，还是管理器内部仲裁。

关键判据：**表示与分配可以隐式；可观察语义不能只靠优化器“觉得可以”。**

## 1. 判据：什么可以隐式

| 性质 | 例子 | 能否隐式 | 原因 |
|---|---|---|---|
| 分配位置 | 栈/区域/arena/堆 | 是 | 不改变纯语义，只改变成本 |
| 复制/原地 | `set` 的 lowering | 是 | 已有 `Unique` + `refines` 义务 |
| 流式/全量 | 文件读成 `Array` 或逐项处理 | 有条件 | 需要抽象接口、效果/成本契约 |
| 资源关闭时机 | 文件、锁、订阅 | 有条件 | 可能被外部观察；需要默认规则或 spec |
| 取消/超时 | 任务/IO 的取消 | 机制可隐式，策略不能 | 取消原因和完成时机是语义 |
| 调度/线程数 | 单线程/多线程 | 是（性能） | 不改变可观察结果，除非有公平性要求 |

一句话：

> 优化器可以决定“怎么做”，不能决定“做什么”。  
> 凡是外部可观察的（关闭、锁释放、错误时机、内存上界），必须先有语义规则或 spec。

## 2. 并发管理方案

### 2.1 A. `scope` / nursery

```text
scope s {
  let t = spawn(s) { work() }
  join(t)
}
```

- 结构化：作用域退出时 join 所有子任务；取消向下传播。
- 优点：任务树和生命周期都词法可见，证明容易。
- 缺点：surface 多一个控制结构；有人认为侵入性强。

### 2.2 B. `TaskGroup` / supervisor 对象

```text
let g = TaskGroup::new()
g.spawn { work() }
g.join_all()
```

- `TaskGroup` 是线性句柄；drop 等价于 cancel + join。
- 优点：不引入新的块语法；库类型即可。
- 缺点：句柄可能被传递/逃逸，需要线性类型约束；结构化程度取决于 API。

### 2.3 C. 显式 `CancelToken` + `select`

```text
let token = CancelToken::new()
let t = spawn { work(token) }
cancel(token)
join(t)
```

- 取消是普通信号，任务自己 `select` 它。
- 优点：机制简单，和事件系统统一。
- 缺点：每个任务都要正确参与；容易漏检查、漏 join；策略分散。

### 2.4 D. 管理器内部 race（当前提案）

> 取消 = 新事件与旧操作的竞争；取消行为在管理器内部发生。

形式化：

```text
cancel(work) = race(work, cancel_event)
```

管理器仲裁谁先完成：

- work 先完成 → 返回结果，撤销 cancel 注册；
- cancel 先完成 → 管理器取消 work，等 cleanup 完成，返回 `Cancelled(cause)`。

优点：

- 触发机制统一：用户取消、timeout、race 输家、父取消、shutdown 都是事件。
- 策略集中在管理器，用户代码不直接 race cancel。
- 不需要 surface 的 `scope`，任务树由管理器持有。

必须补全的部分：

1. **两层 race**：策略层先仲裁多个触发事件，得到 first cause；机制层再让每个任务 race 一个 sticky token。
2. **sticky token**：cancel 不是 one-shot event，而是“已取消”状态 + waker list；否则注册前发生的 cancel 会丢。
3. **单一终态**：`Completed` 与 `Cancelled` 互斥，仲裁原子。
4. **取消完成 ≠ 取消请求**：cancel 赢后必须 cancel loser、cleanup、join，才能返回。
5. **commit point**：已提交的操作不能被取消回滚；取消在下一个检查点生效。
6. **cause**：第一个原因获胜，后续不覆盖。
7. **清理**：`defer`/release 恰好一次；清理有界；必要时 shielded cleanup。

### 2.5 E. Actor / supervisor tree

- 每个任务/actor 有 mailbox；管理器监督重启/取消。
- 适合长期服务；与“函数式 + 区域”结合成本较高。
- 暂不作为第一版主模型。

### 2.6 F. 隐式任务组（每个任务/函数一个 nursery）

```text
fn f() {
  spawn { work() }     -- 默认挂到当前任务
  ...                  -- 函数退出时隐式 join 子任务
}
```

- 没有 `scope` 语法；`spawn` 默认归属于当前任务/调用帧。
- 函数/任务退出时隐式 join；取消沿任务树向下传播。
- 优点：surface 最简，仍然是结构化的。
- 缺点：隐藏的 join 会改变返回延迟；需要 `spawn` 效果；高阶函数/回调里
  `spawn` 的归属要定义清楚；递归/长期函数可能积压子任务。
- 适合作为 D 的 surface 形态之一，但要把“归属”和“退出即 join”写成明确规则。

### 2.7 对比

| 方案 | surface 负担 | 结构化 | 取消策略 | 证明成本 | 备注 |
|---|---|---|---|---|---|
| A `scope` | 中 | 强 | 作用域传播 | 低 | 词法清晰 |
| B `TaskGroup` | 低 | 中 | 句柄/drop | 中 | 库类型 |
| C `CancelToken` | 高 | 弱 | 任务自己检查 | 高 | 易漏 |
| D 管理器 race | 低 | 取决于任务树 | 集中仲裁 | 中 | 需补完成/清理 |
| E actor | 中 | 弱/监督树 | 监督策略 | 高 | 后置 |
| F 隐式任务组 | 最低 | 强（隐式） | 任务树传播 | 中 | 隐藏 join |

**待定**：A 还是 B 作为主 surface；D 作为内部机制可以独立存在。

## 3. Arena 隐式化

### 3.1 方案

- **A0 显式**：surface 写 `region r { ... }` / `Arena<T>`。
- **A1 隐式区域推断 + arena lowering**（当前倾向）：surface 只写 `Array`/`List`/`Tree`；
  `regioninfer` 给分配点分组，`ownlower` 选择 bump/arena；失败回退复制/堆。
- **A2 全局 arena**：`script` 模式整个程序一个区域，退出释放。
- **A3 RC/GC**：当前路线不采用。

### 3.2 隐式 arena 的规则

```text
Region = 生命周期纪律（静态）
Arena  = 区域内的分配策略（bump/分块/批量释放）
```

- 每个分配点推断所属区域 `r`；
- 同一生命周期组（含环的 SCC）放同一 arena；
- `Handle<r,T>` 不能逃出 `r`；
- 返回整个结构时，移动整个 arena 或提升到调用者区域；
- 分析失败/成本不合适 → 复制或普通堆分配；
- surface 不出现 `Handle`，由抽象类型隐藏；需要图算法时用 `Graph` API。

### 3.3 证明义务

1. 区域安全：无悬垂，`Handle` 不逃出区域。
2. 原地/复制精化：`obs(arena_lowering) = obs(pure)`。
3. 分配器：`used ≤ cap`、对齐/溢出、句柄有效。
4. 释放：区域结束无外部可达句柄；drop 恰好一次。
5. 环：整个 SCC 同区域；批量释放即可，不需要逐对象 liveness。

### 3.4 边界

- 隐式不等于不检查：**边界逃逸分析必须严格**，只是内部逐对象分析被粗粒度生命周期替代。
- 隐式推断可能不最优：需要 stats/成本模型；允许可选 `#[arena]`/`region` 提示，但不是必需。
- 长生命周期、频繁增删的图：隐式 arena 可能滞留内存；需要 generational arena / 多区域 / 显式控制。

## 4. 资源关闭

### 4.1 方案

- **R0 `with_*` / bracket**：显式作用域，release 保证执行。
- **R1 词法 RAII**：`let f = open(...)`；作用域结束自动 close。
- **R2 最后使用关闭**：分析算 last use，提前 close。
- **R3 区域所有**：资源属于区域，区域结束 close。
- **R4 显式 `close` + 运行时 finalizer**：不推荐（时机不确定）。
- **R5 优化器/证明器驱动**：分析资源使用后决定何时 close（当前提案）。

### 4.2 对比

| 方案 | surface | 关闭时机 | 可观察性 | 证明成本 | 备注 |
|---|---|---|---|---|---|
| R0 `with` | 中 | 词法块结束 | 明确 | 低 | 可处理失败/异步 |
| R1 RAII | 低 | 词法作用域结束 | 明确 | 低 | 需 drop 语义 |
| R2 last-use | 最低 | 最后使用后 | 依赖分析 | 中 | 只适合私有线性资源 |
| R3 region | 低 | 区域结束 | 明确但可能偏晚 | 中 | 适合批量 |
| R4 finalizer | 无 | 不确定 | 弱 | 高 | 不推荐 |
| R5 优化器 | 无 | 分析决定 | 需 spec | 高 | 不能改变外部可观察行为 |

### 4.3 关键限制

资源关闭通常**不是纯内部实现细节**：

- 文件关闭对其它进程可观察；
- 锁释放影响并发；
- 订阅关闭影响事件是否送达；
- 关闭失败可能产生错误。

所以 R5 要成立，必须先有默认语义或 spec：

```text
默认规则：资源是线性的，在词法作用域结束关闭；close 失败按 best-effort 记录。
优化规则：只有证明资源私有、无别名、close 幂等/不可观察时，才允许提前到最后使用。
```

证明器在这里的角色是**产出事实和证书**：

```text
Unique(f), Linear(f), LastUse(f, p), NoAlias(f), NoEscape(f)
```

优化器根据 facts 决定是否提前 close；证明器检查“提前 close 后无 use-after-close，
且可观察行为不变”。证明器不负责“决定策略”。

### 4.4 `with` 还需要吗

- 作为**库语法糖**保留：`with_file` 处理失败/异步 close，适合需要确定性关闭的代码。
- 不作为唯一手段：默认 RAII/区域关闭已覆盖大部分情况。
- 如果 surface 完全不想出现 `with`，可以把它降为库函数，内部用 bracket + defer 实现。

## 5. 流式 vs 全量

优化器可以介入，但有条件：

```text
surface 类型：Seq<T> / Iterable<T>（不承诺 materialized）
优化选择：流式 或 读入 Array
```

允许切换需要：

1. 抽象接口不要求随机访问/`len`/重复遍历；
2. 错误时机和部分副作用等价，或 spec 允许；
3. 内存/成本上界满足 spec；
4. 关闭/取消时机兼容；
5. 有 `refines` 或翻译验证证书。

如果 spec 写了 `mem ≤ M` 或“流式处理”，全量载入就是违约，不是优化。
如果程序依赖“读完后才返回”，流式可能改变错误时机，需要显式建模。

## 6. 证明器 / 优化器 / 运行时分工

```text
surface/spec   定义可观察行为：值、效果、错误、关闭保证、成本上界
analysis       产出 facts：Unique/NoEscape/LastUse/Linear/Bounds/Cost
optimizer      选择机制：arena/stream/copy/close 插入/调度
proofcheck     检查 refines/翻译验证证书
runtime        执行 decisions：分配、关闭、调度、取消
```

原则：

- **facts 不是命令**；优化器消费 facts，但每个关键变换仍需证书。
- **证明器不决定产品策略**；它证明某个选择满足 spec。
- **运行时不知道 facts/proofs**；只执行 decisions。

## 7. 推荐默认（待定）

1. **Arena 隐式**：默认区域推断 + arena lowering；surface 不变；失败回退复制；可选提示。
2. **资源默认 RAII**：词法作用域结束 close；`with_*` 作为库糖；last-use/region close 需证明。
3. **流式/全量**：优化器在抽象接口 + 效果/成本契约下选择；不能无条件切换。
4. **并发**：记录 A–E；若不喜欢 `scope`，可先做 `TaskGroup` + 管理器内部取消；
   但取消必须包含 `cancel loser + cleanup + join`。
5. **可观察性**：提供 stats/trace（分配、arena、关闭、取消），否则隐式优化不可调试。

## 8. 与仓库模块对应

| 主题 | 现有 | 后续 |
|---|---|---|
| 区域/arena | `src/region/heap.rs`、`src/mem_plan/planner.rs` | `AllocPolicy::Bump`、`Handle`、`freeze` |
| 逃逸 | `src/ownership/uses.rs` 的语法检查 | 区域约束、`Regions(e)`、边界证书 |
| 资源 | 无 | 线性 `File`/`Subscription`、RAII、`with_*` 库 |
| 并发 | 无 | `TaskGroup`/管理器、`CancelToken`、单线程 executor |
| 证明 | `src/ownership_proof`、`src/refines_proof.rs` | arena/区域/取消的模型与证书 |

## 9. 开放问题

1. 并发主 surface：`scope`、`TaskGroup`，还是完全管理器内部？
2. 取消的 cause 是否进入错误类型/日志 API？
3. 资源默认关闭点：词法作用域、最后使用，还是区域结束？
4. close 失败在 RAII 下如何处理：记录、abort、还是要求显式 close？
5. 隐式 arena 的推断范围：函数内、调用图、还是全程序？
6. 流式/全量是否需要 surface 的成本/内存注解，还是完全由 spec 推断？
7. 事件订阅的关闭：drop 即退订，还是显式 `Subscription.cancel`？
8. 多线程下取消仲裁的 tie-break 与确定性。

## 10. Surface 抽象 + 低级表示：两层/多层精化

目标可以概括为：

```text
Surface：只描述值、效果、可观察行为和约束
Lowering/Optimizer：产生另一个高性能低级表示（开发者/AI 可介入）
Prover：证明每层内部自洽，并证明层间精化/等价
```

这里的“等价”建议拆成三个义务，而不是一个笼统的“功能等价”：

### 10.1 层内自洽

```text
WF_L(state)
```

每层自己的不变量：

- Surface：类型安全、所有权/区域/效果规则；
- MemIR：区域/句柄有效、原地/复制条件、闭包捕获；
- ReprIR：布局、对齐、字长、溢出；
- Backend：栈/局部/跳转/字节码验证；
- 并发层：任务树、取消状态机、资源线性。

证明方式：小检查器 + 小语义模型；关键部分进内核。

### 10.2 层内符合 spec

```text
impl_L ⊑ Spec
```

低级表示不一定要和 surface 逐指令相同，只要满足同一个 spec。  
这是比“两层完全等价”更可扩展的支点：

```text
SurfaceImpl ⊑ Spec
LowImpl     ⊑ Spec
```

两者都 refinement 同一个 spec，就不需要证明 SurfaceImpl 和 LowImpl 彼此完全等价。

### 10.3 层间精化

```text
obs_S(e) = obs_L(L(e))          -- 确定性、纯程序
L(e) ⊑_obs e                    -- 允许低级实现更确定/更少行为
```

或带表示关系的 simulation：

```text
α : LowState -> HighState
I(low) ∧ α(low) = high
step_low(low) = low'  =>  step_high(α(low)) = α(low')
```

`obs` 必须明确包含什么：

- 返回值、显式错误；
- IO / 事件 / 资源关闭；
- 终止/发散；
- 并发下的允许结果集合（trace refinement / linearizability）；
- 若 spec 写了成本/内存上界，还包括 cost/mem。

### 10.4 开发者/AI 介入的信任边界

- 开发者/AI 可以写 lowering、优化、低级实现；
- 它们都是**不可信候选**；
- 通过 `refines`、翻译验证证书或按 pass 的元证明进入信任；
- 证不了就回退安全表示，或显式标 `trusted` 并审计；
- 不允许“优化器生成什么就信什么”。

### 10.5 现实路线

```text
关键 pass：一次证明（翻译验证/模拟关系）
其余 pass：每程序证书 + 小 validator
用户实现：spec/impl/refines + 内核证书
兜底：参考 VM 差分测试
```

不追求一开始证明整个编译器；追求：

```text
小的信任基 + 可组合的精化义务 + 失败可回退
```

## 11. 极简 Surface：只保证“计算”？

提案：surface 假设无限资源；内存回收、性能、并行、取消 liveness 都交给
lowering/optimizer；开发者只约束自己关心的值。这个方向可以成立，但要把
“计算正确”和“安全”定义清楚。

### 11.1 三层类型

| 层 | 内容 | 谁决定 |
|---|---|---|
| Behavior | 值、错误、IO trace、取消允许结果集 | surface + spec |
| Resource | mem / time / alloc / parallelism / close 保证 | surface 可选约束，默认 `⊤` |
| Representation | 布局、arena、stream、thread、coroutine | lowering/optimizer |

Surface 看到 Behavior；Resource 默认宽松；Representation 完全不出现。

### 11.2 “计算正确”的定义

把表达式看成输入到可观察结果的 relation：

```text
⟦e⟧ ⊆ Input × Outcome

Correct(impl, spec) :=
  ∀ i ∈ Pre.
    Safe(impl, i)
    ∧ Obs(impl, i) ⊆ Obs(spec, i)
```

其中：

- `Obs` 默认只含值、显式错误、IO trace；
- `Safe` 是语言保证：类型安全、无 UB、无 use-after-free、无数据竞争；
- 内存/性能/取消 liveness 默认不在 `Obs`；
- 如果 `spec = ⊤`，程序只受类型安全和效果规则约束。

这是**部分正确性**：终止不是默认保证。要终止就单独写 `Terminates` 或要求
结构递归；无限资源不等于一定停机。

### 11.3 无限资源假设

```text
surface：假设无限内存、无 OOM、无 GC 暂停、无 finalizer 时机
```

于是 arena、GC、RC、复制、泄漏都可以是 lowering 选择，只要值语义一致。
但要注意：

- 真实机器仍然有限；OOM/abort 要么被声明为语义外（runtime abort），
  要么进入 Resource 契约；
- 算术/布局仍是表示义务：`Int` 是数学整数，low-level 用 i64 就要证不溢出，
  否则不是“等价”；
- 外部资源不是无限的：fd、锁、连接、网络都有现实上限，仍需要 effect/契约。

### 11.4 取消“不保证”

要区分 safety 和 liveness：

- **Safety**：取消不会导致错误值、double close、use-after-free、数据竞争；
  这个必须保证。
- **Liveness**：取消最终会完成、资源最终释放、任务最终停止；这个可以默认不保证，
  由开发者用 spec 收紧。
- **Observability**：取消原因、清理时机、允许的取消点；默认可以很宽松，
  但一旦可观察，就要写进 `Obs`。

如果完全不给取消语义，最安全的解释是：

```text
取消可以在任何挂起点发生；
程序必须对允许的取消结果保持安全；
但不保证取消一定发生或一定很快完成。
```

这样取消不是“无定义”，而是“允许行为集合很大”；开发者可以收窄。

### 11.5 开发者约束

开发者不需要知道 arena/GC/线程，但可以写：

```text
requires / ensures        值正确
effects                   允许的 IO/写/阻塞
cost / mem                资源上界
deterministic             确定性
close / cancel            资源与取消保证
```

这些约束把 Resource/Behavior 类型收窄；lowering 必须选择满足约束的表示。
没有约束的维度默认 `⊤`，优化器自由选择。

### 11.6 风险

- **弱 spec 只剩类型安全**：没有 `ensures` 时，“正确”几乎无内容。
- **无限资源隐藏真实 OOM**：生产档位必须选择 Resource 策略（GC/arena/预算）。
- **可观察性**：隐式内存/取消需要 stats/trace，否则不可调试。
- **效果边界**：纯核心之外仍需 `IO`/`World` 或 effect 接口；无限内存不解决外部世界。
- **证明成本**：spec 越弱越容易证，但也越不能说明程序符合预期。
