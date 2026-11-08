# 语言方向与设想（草案）

这是一份设计空间文档，不是已承诺的里程碑。每个方向最终仍要满足 `docs/08-roadmap.md`
的原则：**有可运行的闭环，不只写文档**。

当前基础：

- 小内核 + 表面显式证明（`docs/03`）；
- ownership / borrow / 非逃逸闭包（`docs/11`、`docs/18`）；
- MemIR 区域堆、闭包/thunk（`docs/18`）；
- spec / impl / refines 最小闭环（`docs/13`）；
- 缩进语法与 S-expression 前端（`docs/20`）。

下面这些方向会同时影响语法、检查器、证明义务和工具链，应该分层推进。

---

## 1. 错误处理：typed errors + effect row + `throw` 语法糖

### 1.1 立场

- **错误默认是数据**：库自己定义 `Error` ADT，返回 `Result<E, T>` 或自己的和类型。
  不把异常当成不可见的控制流。
- **`throw` 不是黑盒异常**：它只是“带类型错误的提前返回”语法糖。
- **分析器知道错误行为**：函数类型携带错误集合；没有错误或已处理的函数是
  `throws ∅`。
- **证明可以消除错误**：除零、越界、解析不可能分支等由 proof 排除后，错误集合
  可以收缩；不能消除的才进入真实错误路径。
- **panic / abort / unsafe 与可恢复错误分开**：前者不做普通错误传播，单独建模。

### 1.2 函数类型

```text
f : (x : T) -> R ! E
E ::= ∅ | {throw(E1)} ∪ E2 | io | alloc(r) | ...
```

第一版可以只做：

```text
fn div(a: Int, b: Int) -> Int
  requires b != 0
  throws ∅
{ a / b }

fn div_checked(a: Int, b: Int) -> Int
  throws DivByZero
{ if b == 0 then throw DivByZero else a / b }
```

`throws ∅` 是类型的一部分，不是注释；调用者可以依赖它。

### 1.3 `throw` / `try` / `?`

```text
// 错误值进入结果 ADT；调用者决定怎么处理
fn parse(s: Str) -> Ast throws ParseError { ... }

// handler 消除 throw 效果
fn parse_or(s: Str, fallback: Ast) -> Ast throws ∅ {
  try parse(s) with
  | ParseError => fallback
}

// ? 是 try 的短语法，仍然产生 throws 效果
fn parse_pair(s: Str) -> Pair throws ParseError {
  let a = parse(s)?;
  let b = parse(s)?;
  Pair(a, b)
}
```

原则：

- `throw e` 的运行时表示就是错误 ADT 值 + 提前返回，不引入隐藏栈展开。
- `try` / handler 是普通函数/匹配，可作为库实现；语言只提供效果规则和便捷语法。
- 错误值遵循 ownership/region 规则；不能把局域 borrow 塞进错误值逃逸。
- `?` 只是语法糖；错误集合仍由类型系统计算。

### 1.4 分析器需要知道什么

- 每个函数/闭包的 `throws` 集合；
- 调用 union 错误集合；
- handler 删除对应错误；
- `throws ∅` 的函数可以放进“no-panic / no-error”上下文；
- 证明规则可以收缩错误集合，例如 `b != 0` 证明后 `div` 的 `DivByZero` 分支不可达。

实现顺序：

1. `Result`/错误 ADT + 解释器支持；
2. `throws` 效果集合 + 子集/并集检查；
3. `throw` / `try` / `?` 语法和 lowering（错误即数据）；
4. 错误 spec：`throws E`、每个错误的后置条件、`refines` 义务；
5. 证明驱动收缩错误集合（除零/越界/不可达分支）；
6. panic/abort、FFI 错误、异步取消的错误模型。

开放问题：

- 错误集合用结构化 row 还是 nominal set？是否需要 row 多态。
- `throw` 是否允许出现在 `pure` 函数里（建议：只能出现在带 `throw` 效果的函数）。
- 错误值是复制、移动还是借用；错误路径上的 region 释放规则。
- `try` 是语言构造还是纯库函数 + `?` 语法糖。

---

## 2. 自举与元编程：编译器作为可导入的库

### 2.1 目标

让 lowering、优化、代码生成不再是 Rust 里写死的 pass，而是：

- 有稳定输入/输出 IR 的 **pass 模块**；
- 可以被 import、组合、替换、单独测试；
- 可以由 Alethe 自己编写，最终走向自举；
- 新后端/新平台作为插件，而不是复制整个编译器。

### 2.2 编译管线作为数据

```text
type Pass = {
  name: String,
  input: Stage,
  output: Stage,
  run: Program -> Result<Program, Diagnostics>,
  obligations: [ProofObligation],
}

pipeline Release =
  parser
  |> typecheck
  |> owncheck
  |> regioninfer
  |> memlower
  |> reprelower
  |> bytecode
  |> verify
```

编译脚本可以选择 pass、插入自定义 pass、为某个 pass 提供证明或标记 trusted。

### 2.3 信任边界

- **生成的代码必须重新检查**：元程序可以生成 AST/IR/证明，但最终仍过 typecheck /
  owncheck / kernel。
- **编译器插件不可信**：pass 需要声明它做哪种变换；关键 pass 需要
  `refines`/translation validation 证明，否则只能标 `trusted` 并进入审计清单。
- **能力边界**：编译期代码有显式 capability（读文件、跑进程、网络、时间、随机），
  构建可复现时禁止不确定能力。
- **确定性**：同一输入 + 同一 pass 版本 + 同一随机种子 → 同一输出/证明工件。

### 2.4 元编程形式

候选：

- **quote / splice**：构造 AST/IR 的语法；
- **CTFE**：编译期求值，限制效果与终止性；
- **宏作为普通函数**：`Program -> Program`，类型检查后运行；
- **staged metaprogramming**：区分 object language 与 meta language，避免阶段混淆；
- **goal/hole 生成**：tactic/证明搜索本质上是元程序，产出项后由内核检查。

### 2.5 自举路径

1. Rust 编译器输出稳定 JSON IR + diagnostics；
2. pass 注册/导入机制（先 Rust 插件，再 Alethe pass）；
3. 用 Alethe 写前端/部分 lowering，和 Rust 版本做差分测试；
4. stage0（Rust）构建 stage1（Alethe），stage1 构建 stage2；
5. 对关键 pass 做翻译验证；未验证部分明确标 trusted；
6. 新后端通过同一 IR 接口接入，产出可验证的 lowering 或可审计的 trusted 边界。

开放问题：

- pass 的 ABI：类型化 IR 对象、序列化 IR，还是两者都有？
- 自举后的信任链：stage0 二进制 + 可复现构建 + 差分测试，还是追求 verified bootstrap？
- 宏/CTFE 的终止性与效果检查如何与内核统一？
- 编译期代码能否调用证明搜索/LLM？必须如何记录 provenance？

---

## 3. 多层级执行：脚本模式与验证模式共享语义

### 3.1 目标

同一份程序可以有不同执行档位：

| 档位 | 目标 | 分析 | 内存 |
|---|---|---|---|
| `script` | 秒级启动、交互/原型 | 最少；不要求所有权/证明 | 复制语义、解释器/字节码 |
| `checked` | 正常运行 | ownership/region/effects + 运行时检查 | 原地/复制 lowering |
| `verified` | 发布/热路径 | 内核证明 + 精化 | 区域提升、无检查、优化 IR |

关键不变量：**档位只改变性能和验证强度，不改变可观察语义。**

### 3.2 复制语义作为 reference

- `script` 模式采用复制语义，天然安全，适合做 oracle；
- `checked`/`verified` 的原地更新、区域释放、闭包捕获必须与复制语义观察等价；
- 差分测试：同一输入在 `script` 与 `verified` 下比较结果、错误、可观察副作用；
- 现有 `refines`/原地证明是这条路的种子。

### 3.3 实现顺序

1. 明确 `script` 档位的语义边界（错误、IO、并发、unsafe 怎么处理）；
2. 用现有树解释器做 `script` 参考实现，默认复制；
3. CLI/profile：`--script` / `--checked` / `--release`，构建清单记录档位；
4. 区域/所有权 lowering 接入 `checked`；
5. 证明和优化接入 `verified`；
6. 跨档位差分测试 + 性能基准。

开放问题：

- `script` 模式是否仍做类型检查/效果检查？建议：做类型检查，跳过证明和区域推断；
- `script` 模式的错误处理是否仍用同一 `throws` 规则？建议：是；
- 未证明的 unsafe/FFI 在 `script` 下如何限制；
- 档位是否影响 ABI/序列化/持久化格式。

---

## 4. 并发与多线程：结构化并发 + 所有权发送

### 4.1 立场

函数式核心天然适合并行，但不能假设“自动并行”总是安全或高效。需要一个
**控制器/调度器**，以及显式的并发效果。

### 4.2 最小模型

```text
spawn : (() -> T ! E) -> Task<T> ! {spawn}
join  : Task<T> -> T ! {join}
parallel_map : (A -> B) -> Array<A> -> Array<B> ! {parallel}
```

规则：

- `spawn` 只能捕获/传递 `own` 或 `send` 的值；`borrow` 不能跨任务；
- 默认**结构化并发**：任务必须在作用域结束时 join，不支持 detached；
- 取消/超时是一等效果，取消后的清理走 region/drop 规则；
- 共享可变状态只能通过显式 `share`/锁/原子类型，数据竞争检查独立于普通 borrow；
- 默认调度器确定性（可复现），工作窃取/多核调度作为运行时策略；
- `parallel_map` 先要求纯函数或只读效果，作为最安全的并行入口。

### 4.3 证明路线

- 第一层：ownership 保证 `send` 的值没有别名；纯函数并行不共享可变状态；
- 第二层：结构化并发的取消/异常传播；
- 第三层：共享内存的线性化/数据竞争证明（分离逻辑/Iris 风格，后置）。

实现顺序：

1. 纯 `parallel_map`/`parallel_reduce`（确定性、无共享写）；
2. 结构化 `spawn`/`join` + `own` 发送；
3. channel/message passing；
4. 取消/超时/异常传播；
5. 共享内存与锁的验证模型。

开放问题：

- actor、task、还是 data-parallel 作为主模型？
- 调度器是运行时能力还是库？如何控制确定性/公平性？
- 并发效果如何与错误 effect row 组合？
- 取消是否等价于 `throw Cancelled`？清理如何证明？

---

## 5. 搜索工具：优化、证明、结构

### 5.1 共同基础

所有搜索器都是编译器 API 的客户端，输入/输出是结构化 IR、goal、diagnostics：

- 目标/上下文/洞（JSON）；
- 候选项/候选 IR；
- 验证结果（内核/refines）；
- benchmark/成本；
- provenance 与随机种子。

搜索器**不被信任**；它们只产生候选，最终由内核/验证器/基准筛选。

### 5.2 优化搜索器

- 枚举保持语义的 rewrite；
- 每个候选必须通过 `refines` 或 translation validation；
- 用 benchmark + 成本模型排序；
- 记录 source map、证明、AI 生成记录；
- 失败候选提供反例/差异。

### 5.3 证明搜索器

- 输入 goal/context/hole；
- 产生项或 tactic 脚本；
- 内核检查；
- 缓存证明、复用引理、学习搜索策略；
- LLM 只是候选生成器；错误证明被拒绝；
- 失败时返回结构化反例/剩余 goal，供 AI 迭代。

### 5.4 结构搜索器

- 在 AST/IR 上搜索重写/组合；
- 按特性/效果/表示组合搜索语义（例如不同 region/布局/并发策略）；
- 语义哈希/等价检查，检测重复/冗余代码；
- 项目级：找可合并函数、重复规格、可替换实现；
- 必须区分“语法相似”和“语义等价”；等价需要证明或测试 oracle。

实现顺序：

1. 稳定 JSON goal + 小片段证明搜索；
2. 带验证的 rewrite 引擎；
3. 语义哈希/克隆检测；
4. 特性组合搜索（布局/区域/并发策略）；
5. AI 闭环：生成 → 验证 → benchmark → provenance。

开放问题：

- rewrite 规则如何声明和证明？
- 语义哈希用什么规范形式（ANF/IR/证明无关）？
- 结构搜索的“特性方向”如何编码，避免组合爆炸？
- 搜索器之间的缓存/证明复用协议。

---

## 6. 性能与语言工程基准

### 6.1 与 C/Rust/Zig 比较

- 只在核心功能稳定后做正式对比；
- 比较维度：运行时、编译时间、内存、二进制大小、验证时间、代码/证明规模；
- 同一算法、同一输入、同一硬件；多次运行 + 统计；
- 区分 `script` / `checked` / `verified`；
- 公开 benchmark 与 provenance，避免只报最好结果；
- 记录“总成本”：证明/搜索/验证时间也是成本。

### 6.2 领域与语言工程难度

候选套件：

- parser/解释器/编译器；
- 数据处理（sort/group/join）；
- 数值/科学计算；
- 并发/并行；
- 分配/字符串/序列化；
- 小型完整语言工程（lexer→parser→typecheck→lower→VM）。

指标：

- 代码行数、spec/证明行数、AI 生成通过率；
- 开发时间、调试时间、验证时间；
- 相对 reference 的性能/内存；
- 重写一个领域算法或完整语言工程的难度评分。

### 6.3 阶段

1. 微基准（函数调用、数组、闭包、region）；
2. 领域基准（parser、VM、数据处理）；
3. 语言工程基准（自举前端/编译器）；
4. 与 C/Rust/Zig 的公开对比；
5. 回归基准进入 CI。

---

## 7. 推荐顺序与张力

### 推荐顺序

1. **错误/效果系统**：最小但影响所有调用边界，先做 `Result` + `throws ∅` 检查；
2. **执行档位**：用现有解释器做 `script` reference，建立跨档位差分测试；
3. **并发**：从纯 `parallel_map` 和结构化 `spawn/join` 开始；
4. **元编程/自举**：先稳定 IR/diagnostics JSON，再做 pass 导入和 Alethe pass；
5. **搜索工具**：依赖前面的 IR、goal、验证器；
6. **性能对比**：最后做，避免过早优化。

也可以把“证明搜索”提前，因为它直接提升 AI 写证明的可用性；但它依赖稳定的
goal/term 协议。

### 主要张力

- **错误即数据 vs `throw`**：用 effect row 统一；`throw` 只是带类型的提前返回。
- **元编程 vs 信任基**：元程序生成的一切都要重新检查；关键 pass 要证明或标 trusted。
- **脚本模式 vs 验证**：共享同一语义；复制语义做 reference，优化必须证明等价。
- **并发 vs 所有权**：先结构化、隔离、发送，后共享内存。
- **搜索 vs 规格质量**：搜索/基准不能替代 spec；证明和反例是过滤器。
- **性能 vs 工程成本**：证明、验证、搜索时间都要计入总成本。

### 需要先决定的问题

1. 错误集合是结构化 row 还是 nominal 集合？是否支持 row 多态？
2. `throw` 是否只允许在 effectful 函数中？`panic` 的语义边界？
3. `script` 档位是否仍做完整类型/效果检查？
4. 并发主模型：结构化 task、actor、还是 data-parallel？
5. 编译器 pass 的 ABI：类型化 IR 对象 vs 序列化 IR？
6. 自举信任链：stage0 + 可复现构建，还是追求 verified bootstrap？
7. 语义哈希/结构搜索的规范形式是什么？
8. 性能验收：相对 C/Rust 的倍率，还是总成本/能效？
