# 路线图

目标不是一次性造完整语言，而是每一步都有**可运行的验证闭环**。

## M0：内核（已完成）

- 依赖类型内核：de Bruijn、`Prop`/`Type`、Pi/lambda、`Nat`/`List`/`Eq`；
- recursor 归约、`Eq.subst`、定义等价；
- `sum_ref` / `sum_fast` / `sum_fast_eq`；
- 错误证明被拒绝；
- 单元测试。

验收：`cargo test` 通过；`cargo run --bin sum_demo` 输出两个 15 和四个 ✓。

## M0.5：栈式 VM（已完成原型）

- [x] `source → ownership check/lowering → bytecode → VM`；
- [x] 栈式字节码、函数调用/返回、局部变量、跳转；
- [x] 证明参数擦除（runtime arity 不含 proof 参数）；
- [x] `set`/`push` 原地/复制字节码变体；
- [x] 分配/复制/原地/边界检查统计；
- [x] 与树遍历解释器结果/统计一致；
- [ ] IR/ANF；
- [x] 字节码验证器（栈/局部/跳转/调用/数组类型）；
- [ ] 闭包/高阶函数；
- [ ] 证明 facts 驱动的 unchecked 访问。

## M1：表面证明语言 + 擦除

- 表面项语法（`def`/`theorem`/`fun`/`match`）；
- 元变量、统一、hole（`?`）；
- tactic：`intro`、`exact`、`apply`、`induction`、`rw`、`simp`；
- 结构化 goal（JSON）；
- 证明擦除：`Prop` 参数、证明字段、类型索引；
- 运行时 IR（ANF，无 proof）。

验收：

- 用表面语法重写 `sum` 证明；
- `--dump-goals` 输出 JSON；
- 擦除前后 `sum_fast` 运行时参数/大小对比；
- 内核仍拒绝错误证明。

## M2：spec / impl / refines + 多实现

**部分完成**（`src/refines.rs`）：

- [x] `spec fn` / `impl fn ... refines ...` 语法；
- [x] 签名兼容（参数名/类型/模式/返回类型）；
- [x] 前置事实检查（`requires Unique(a)` 需要 `own`）；
- [x] 规格表达式等价（body 重化为 spec 表达式）；
- [x] demo：`fill` / `fill_shared` 通过，`fill_bad` 被拒绝；
- [x] kernel certificate：`fill_refines_spec` / `fill_shared_refines_spec`（手动映射，`proofcheck` 检查）；
- [ ] 从 surface spec 自动生成 kernel obligation；
- [ ] reference + optimized 多实现 + benchmark 选择；
- [ ] provenance 记录。

验收：

- `sum_spec` + `sum_ref` + `sum_fast`，`refines` 检查通过；
- 错误实现被拒绝；
- benchmark 选版本；
- 构建清单可重现。

## M3：唯一性/区域 + 原地更新

**部分完成**（`src/ownership.rs`）：

- [x] `own`/`borrow`/`shared` 模式；
- [x] 唯一性/仿射检查（简化版）；
- [x] 原地更新 lowering（`set`/`push`）；
- [x] 分配/复制/原地统计；
- [x] 区域堆模型（`src/region.rs`）：分配/释放、原地/复制、观察等价；
- [x] 内存规划器 + MemIR（`src/mem_plan.rs`）：move/downgrade/copy、原地/复制选择；
- [x] 普通函数调用 + 每调用区域 + 返回值复制；
- [x] 直接 `if` primitive branch + thunk 回退（`optimized=false`）；
- [x] MemIR 一等闭包/thunk（`Lambda`/`Apply`/`Thunk`/`Force`）；
- [ ] 表面函数类型 / lambda；
- [ ] 逃逸分析 / 区域提升 / 闭包逃逸；
- [ ] 区域推断（至少栈式区域）；
- [ ] 显式 `&`/`&mut` 与生命周期；
- [ ] 证明 facts 流入优化器（边界/除零检查消除）；
- [x] 最小原地精化证明（两位置堆模型，`src/ownership_proof.rs`）：
  - `set_result_same`：原地/纯返回同一数组；
  - `set_frame`：`Unique h` 下更新第一个位置对第二个观察者不可见；
- [x] 推广到任意位置/数组 id（`Nat` 索引，归纳证明 `update_at_other`）；
- [x] `Borrow`/`BorrowMut` 权限模型（互斥、只读/可写、proof-carrying read/write）；
- [ ] `acquire`/`release`、共享借用计数；
- [ ] 区域/drop 与生命周期；
- [ ] 真实 lowering 的等价证明/翻译验证。

验收：

- `sum` 的原地版本与 `sum_ref` 等价；
- 分配次数下降；
- 越界/除零检查按证明消除；
- 生命周期/所有权错误被拒绝。

## M4：AI 优化 demo

- AI 生成多个 `sum`/`sort` 变体；
- 每个变体必须 `refines`；
- benchmark 选择；
- 展示“可读 reference + 不可读 fast”；
- 统计 AI 通过率、证明时间、性能提升。

验收：

- 至少一个变体比 reference 快 2×；
- 所有入选变体有内核检查的 `refines`；
- provenance 完整。

## M5：表示层 + 翻译验证

- `Repr T R`；
- `Int` → `I64`、`Array` → `PackedArray`/`SoA`；
- 布局/进制优化；
- 关键 lowering 的翻译验证；
- trusted 标记与审计。

验收：

- `Array<Int>` 的 packed 表示通过精化；
- 溢出/边界条件被证明或检查；
- 布局改变不改变可观察行为；
- 至少一个 lowering 有可检查的等价证书。

## M6：SMT 证书 / 搜索 / LLM

- 线性算术 Farkas 证书 + 内核检查；
- 等式/congruence 证书；
- 证明搜索接口；
- LLM 候选接口（只产生项/战术）；
- 失败反例。

验收：

- 一个线性算术义务由 SMT 找证明 + 内核检查证书；
- LLM 生成的错误证明被拒绝；
- 反例可复现。

## M7：后端与性能

- 栈式 VM → Cranelift/LLVM；
- 单态化、unboxed ADT、尾调用；
- 区域运行时；RC/GC 作为备选；
- benchmark 套件；
- 与 C/Rust 对比。

验收：

- 适合的 benchmark 在 1.5–2× C/Rust 内；
- 热路径零检查；
- 内存/分配可预测；
- 回归基准。

## 评估指标

| 维度 | 指标 |
|---|---|
| 正确性 | 内核检查覆盖率；错误证明拒绝率；`refines` 覆盖率 |
| 性能 | 相对 C/Rust；分配次数；边界检查数；尾调用 |
| 证明 | 证明项大小；检查时间；AI 生成通过率 |
| 规格 | 变异测试杀死率；差分测试覆盖；反例数 |
| 工程 | 增量检查时间；构建可重现性；provenance 完整性 |
| 安全 | `trusted` 面积；unsafe 边界；依赖 provenance |

## 优先级

1. 内核/证明（已完成）→ 表面语法/擦除（M1）；
2. `refines`/多实现（M2）；
3. 唯一性/原地（M3）；
4. AI 优化 demo（M4）；
5. 表示层（M5）；
6. SMT/LLM（M6）；
7. 后端（M7）。

原则：**每一步都要有可运行的闭环，不做“只写文档不跑”的里程碑。**
