# AI 驱动的可验证优化

你的设想：

> 类型作为约束，AI 或任何工具做优化；强类型保证代码正确，从而保证优化正确；业务代码可读，重写出的高性能代码可读性不保证。

这个方向是对的，但需要精确化：**强类型保证的是“优化后的代码满足同一个 spec”**，不是“优化过程一定正确”，也不是“优化一定更快”。这反而更好——你不需要信任优化器。

## 1. 核心架构：spec 优先 + 多实现

```text
Spec
 ├── Reference impl（可读、可执行、慢）
 ├── Fast impl A（AI 生成/重写，验证通过）
 └── Fast impl B（AI 生成/重写，验证通过）
```

```alethe
// sort.spec.alt
spec fn sort(xs: List<Int>) -> List<Int>
  ensures sorted(result) && permutation(result, xs)
  cost O(n log n)
  alloc 0
  deterministic
```

```alethe
// sort.ref.alt
impl fn sort_ref(xs: List<Int>) -> List<Int>
  refines sort
{
  // 归并排序，写得清楚
}
```

```alethe
// sort.fast.alt
impl fn sort_fast(xs: List<Int>) -> List<Int>
  refines sort
  cost O(n log n)
  alloc 1
{
  // 原地快排/插入混合，可读性不保证
}
```

- `refines` 生成精化义务；内核检查。
- reference impl 同时是**可执行规格**和差分测试的 oracle。
- 小函数内联 spec；性能关键/需要重写的模块拆文件。
- 构建按 profile/benchmark 选实现；锁定版本并记录 provenance。

## 2. 为什么不需要信任优化器

这是 **proof-carrying optimization / translation validation at the source level**：

- AI 可以乱来、启发式、生成一百个版本；
- 每个版本必须证明满足同一个 spec；
- 内核检查通过才进入构建；
- 优化器本身的 bug 不会破坏正确性，只会产生被拒绝的候选。

对比：

| 路线 | 信任什么 | 代价 |
|---|---|---|
| 验证优化器（CompCert 风格） | 优化器正确性证明 | 一次性巨大，pass 难写 |
| 翻译验证 | 每次运行的等价性证明 | 证明负担，但可用 AI/SMT |
| 规格检查（本方案） | spec + 内核 | spec 必须足够强 |

## 3. 关键限定：spec 必须覆盖“优化可能改变的东西”

如果 spec 只写功能正确性，一个“正确但更慢”的实现也能通过。要让优化有意义，spec 至少包含：

### 行为

- 纯函数/效果；
- 异常/`Result`/`panic`；
- 发散/终止；
- 浮点精度/舍入；
- 并发/原子性；
- 未定义行为/unsafe 边界。

### 资源

```alethe
cost O(n log n)       -- 时间复杂度
alloc 0               -- 堆分配次数
stack 64              -- 栈上界（可选）
latency 10ms          -- 延迟（可选）
energy ...            -- 能耗（可选）
```

### 确定性

```alethe
deterministic         -- 同输入同输出；可缓存/并行
pure                  -- 无副作用；可重排/融合
```

### 安全

- 能力/权限；
- 信息流；
- unsafe 封装；
- 供应链 provenance。

没有资源 spec，AI 可以把 `O(n log n)` 重写成 `O(n²)` 但“功能正确”，验证器会放行。  
所以成本类型至少要进 spec；第一版可以先做“声明 + benchmark 校验”，后续再做证明。

## 4. 三个重写层次

| 层次 | 例子 | 证明 | AI 适合度 |
|---|---|---|---|
| 表达式/peephole | `x*2 → x<<1` | 等价证明，容易 | 高 |
| 循环/数据布局 | 循环展开、向量化、原地更新 | 精化/不变式，中等 | 高 |
| 算法替换 | 归并排序 → 快排/基数排序 | 同一 spec 的精化，难 | 中高 |

第三层最有价值：AI 可以提出完全不同的算法，只要证明满足同一个 spec。这也是“业务代码可读、优化代码不可读”最自然的落点。

## 5. Benchmark 选择

AI 可以生成大量候选；验证器过滤，benchmark 排序：

```text
candidates
  → kernel/refines check
  → correctness tests / differential tests
  → benchmark on held-out inputs
  → Pareto frontier (time, memory, binary size)
  → select per profile
  → record provenance
```

注意：

- 避免过拟合 benchmark：用 held-out 输入、统计显著性、多个 workload；
- 记录环境：CPU、编译器 flags、库版本；
- 构建可重现：锁定选中的版本和证明工件；
- 允许“慢但省内存”/“快但费内存”的 Pareto 选择。

## 6. 编译器是 AI 的 API

AI 不需要“好看的报错”，它需要**结构化状态**：

```json
{
  "goal_id": "g42",
  "context": [
    {"name": "a", "type": "Array Int"},
    {"name": "i", "type": "Int"},
    {"name": "h", "type": "0 <= i && i < len(a)"}
  ],
  "target": "i < len(a)",
  "holes": ["?p"],
  "available_lemmas": ["..."],
  "counterexample": null
}
```

需要的接口：

- `check`：提交项/战术，返回成功/失败 + 结构化错误；
- `goal`：查询当前 goal/context/hole；
- `search`：证明搜索（本地/远程）；
- `llm`：把 goal 发给模型，返回候选项；
- `bench`：跑 benchmark，返回统计；
- `explain`：把证明项翻译成人类可读的步骤；
- `provenance`：记录 AI/人类/工具、axiom、trust level。

### Holes

```alethe
fn get(a: Array<Int>, i: Int) -> Int
  requires 0 <= i && i < len(a)
{
  a[i]
}
```

如果某个证明写不出来：

```alethe
theorem foo: P {
  ?
}
```

编译器返回 goal；AI 填：

```alethe
theorem foo: P {
  by { intro h; exact h }
}
```

内核检查；失败就返回反例/错误位置。

### 反例

证明失败时，尽量给具体输入：

```text
counterexample: a = [1,2,3], i = 5
obligation: 0 <= i && i < len(a)
```

这比“proof failed”有用得多，能让 AI 修代码或修 spec。

## 7. Spec 质量是最大风险

强类型/证明只保证“相对于 spec 正确”。如果 AI 写了一个弱 spec，验证会通过但程序仍然错。

需要的工具：

- **差分测试**：reference impl vs optimized impl，随机/边界输入；
- **属性测试**：从 spec 生成性质；
- **变异测试**：故意改坏实现，看 spec 是否能抓住；
- **反例生成**：SMT/搜索找违反 spec 的输入；
- **spec 审阅**：人类重点看 spec，而不是优化代码；
- **成本验证**：benchmark + 资源上界，防止“正确但更慢”。

第一版至少做差分测试 + 反例；变异测试可以后置。

## 8. Sum 的例子

当前内核已经有：

```text
sum_ref : List Nat -> Nat
sum_fast : List Nat -> Nat
sum_fast_eq : ∀ xs, sum_fast xs = sum_ref xs
```

这对应第一层/第二层：

- `sum_ref` 是可读 reference；
- `sum_fast` 是不同实现（尾递归/累加器）；
- `sum_fast_eq` 是精化证明；
- 两个实现对 `[1,2,3,4,5]` 都算出 15。

下一步把它提升为多实现：

```alethe
spec fn sum(xs: List<Int>) -> Int
  ensures result == sum_math(xs)
  cost O(n)
  alloc 0

impl fn sum_ref(xs: List<Int>) -> Int refines sum { ... }
impl fn sum_fast(xs: List<Int>) -> Int refines sum { ... }
```

然后加 benchmark，选最快且通过验证的版本。

## 9. 风险

| 风险 | 说明 | 缓解 |
|---|---|---|
| spec 过弱 | 验证通过但行为不对 | 差分/变异/反例/审阅 |
| 成本未约束 | 正确但更慢 | cost types + benchmark |
| 优化器/表示层 bug | 证明在源层，lowering 错 | 翻译验证/验证后端 |
| TCB 过大 | 内核/编译器/运行时 | 小内核 + 可检查工件 + 验证后端 |
| AI 幻觉 | 证明/规格看似合理 | 内核检查 + 反例 + 审阅 |
| benchmark 过拟合 | 选到“考试型”实现 | held-out + 统计 + 多 workload |
| 编译时间 | 依赖类型 + SMT + 搜索 | 增量/缓存/并行 |
| 安全 | AI 生成恶意代码 | 能力类型/沙箱/provenance |
