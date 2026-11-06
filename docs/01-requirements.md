# 需求与验收

## 1. 用户与使用方式

- **主要作者**：AI agent。它生成业务代码、规格、证明、优化版本。
- **审阅者**：人类工程师。重点审 spec、reference impl、provenance，而不是逐行读优化代码。
- **CI**：执行内核检查、`refines` 检查、benchmark、provenance 策略。

因此编译器首先是一个 **API**，其次才是 CLI/TUI。语法可以“不好写”，但必须：

- 无歧义、可稳定格式化、易 diff；
- 结构化报错（JSON），包含 goal、上下文、反例、hole；
- 支持增量检查和证明缓存；
- 输出可复现的构建清单。

## 2. 语言需求

### 2.1 表面语言

- 纯函数式、严格求值。
- ADT + 模式匹配 + 结构递归；终止性检查。
- 精化类型：`{x: T | P}`、`requires`、`ensures`、`invariant`。
- 证明参数/证明块；`by` 战术；`?` hole。
- 显式效果：`Pure`、`IO`、`State`、`Unsafe`（后续扩展并发/异步）。
- 所有权模式：`own`、`borrow`、`shared`；默认推断，边界显式。
- 规格可写在同文件（小函数）或独立 spec 文件（性能关键/重写目标）。
- 一个 spec 多个 impl；`refines` 机械检查。

### 2.2 类型与证明

- 小内核：依赖类型、`Prop`/`Type`、Pi、lambda、inductive、recursor、定义等价。
- 证明项可擦除；`Prop` 证明不可用于运行时分支（除非显式重化）。
- 精化/生命周期/成本注解可擦除。
- 支持运行时可重化的类型描述：`TypeRep T`、`Layout`、`Dynamic`、checked cast。
- 证明提供者：tactic、证明搜索、SMT 证书、LLM 候选；最终内核检查。
- 渐进验证：每个函数可标注 `verify = kernel | smt | runtime | trusted`。

### 2.3 内存与执行

- 无 tracing GC。
- 区域推断：分配属于区域，区域在最后一次使用后释放；逃逸值提升到 RC。
- 唯一性/线性：唯一值可原地更新；共享值复制或 RC。
- 纯函数式表面；优化器 lowering 成带所有权的运行时 IR。
- 目标：热路径零边界检查、零不必要分配、尾调用、单态化、unboxed ADT。
- 后端：先栈式 VM；后续 Cranelift/LLVM；预留 native。

### 2.4 规格与优化

- spec 包含：行为、效果、异常/发散、确定性、成本/资源、必要的安全性质。
- 成本注解先做声明 + benchmark 校验，后续做证明。
- 多实现：reference（可读）、optimized（AI 生成/重写）、可选 unsafe/representation 层。
- 构建按 profile/benchmark 选择实现；锁定版本并记录 provenance。
- 优化重写必须产出可检查的精化/等价证明，或经过翻译验证。

### 2.5 硬件无关

- 默认语义类型不暴露字长、字节序、布局、进制。
- 表示层显式：`Word<n>`、`Float`、`PackedArray`、`SoA`、`Device`、`Unsafe`。
- 表示精化：`Repr T R` 证明表示 `R` 实现语义 `T`。
- 只有可观察表示（序列化、`toString`、FFI、MMIO）进入 spec。
- 布局/进制优化是后续 TODO；先保证语义层与 `refines` 可工作。

## 3. 非功能需求

| 维度 | 要求 |
|---|---|
| 信任基 | 内核小、可审计；SMT/LLM 不在信任基；编译器尽量产出可检查证明 |
| 性能 | 适合代码 1.5–2× C/Rust；热路径无检查；可预测分配 |
| 编译时间 | 增量验证；证明缓存；并行检查 |
| 可重现 | 证明工件、依赖、benchmark 环境、AI 生成记录 |
| 安全 | 能力/效果类型、unsafe 边界、供应链 provenance |
| 可审阅 | reference impl 可读；optimized impl 有 source map 和差异说明 |
| AI 友好 | JSON goal/hole/反例；稳定错误码；MCP/工具接口 |

## 4. 当前里程碑验收（内核）

已完成：

- [x] `Nat`、`List`、`Eq` 的类型与 recursor；
- [x] de Bruijn 项、`Prop`/`Type`、Pi/lambda/application；
- [x] beta/delta/zeta/iota（`Nat.rec`、`List.rec`）与 `Eq.subst` 归约；
- [x] 类型检查与定义等价；
- [x] `sum_ref`（结构递归）与 `sum_fast`（尾递归/累加器）两个实现；
- [x] `add_assoc`、`add_zero`、`go_eq`、`sum_fast_eq` 内核检查；
- [x] 两个实现对具体列表计算结果一致；
- [x] 错误证明被拒绝；
- [x] 单元测试；
- [x] 栈式 VM 原型（`src/vm.rs`）：
  - source → ownership check/lowering → bytecode → VM；
  - 证明参数擦除（runtime arity 不含 proof）；
  - 分配/复制/原地/边界检查统计；
  - 与解释器结果/统计一致。
- [x] `own` / `borrow` / unique 检查器原型（`src/ownership.rs`）：
  - use-after-move、borrow escape、borrow→own move 被拒绝；
  - 唯一值 `set` 原地、共享值复制；
  - 分配/复制/原地统计。

未完成（下一里程碑）：

- [ ] 表面项/证明语法 + 元变量 + hole；
- [ ] 证明擦除与运行时 IR；
- [ ] `Vec`/`Fin` 与边界证明；
- [ ] `spec` / `impl` / `refines`；
- [ ] 唯一性/区域推断；
- [ ] 表示层与布局/进制优化；
- [ ] SMT 证书、证明搜索、LLM 接口；
- [ ] JSON goal 与增量检查。

## 5. 开放问题

1. **默认验证严格度**：默认 `runtime` 兜底还是默认 `kernel`？建议热路径 `kernel`、边缘 `runtime`，按模块配置。
2. **成本类型**：`alloc`/`O(...)` 第一版就进类型系统，还是先做 spec 声明 + benchmark？建议后者。
3. **运行时类型描述**：`TypeRep` 只用于序列化/反射/FFI，还是允许普通业务代码使用？建议限制在显式 `dynamic` 模块。
4. **多实现选择**：release 用 benchmark 自动选，还是 profile 固定？建议 benchmark 选 + 锁定版本 + provenance。
5. **实现语言**：Rust 内核 + TS 工具链，还是全 Rust？见 `09-decisions.md`。
6. **spec 质量**：如何防止 AI 写弱 spec？需要变异测试、差分测试、反例生成；第一版做到什么程度？
7. **表示层**：布局/进制优化先做哪些？`Array` 的 packed/SoA 与 `Int` 的定宽表示是最小闭环。
