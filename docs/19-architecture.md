# 架构与信任边界

本文是当前工程的单一架构说明：pipeline、各层职责、信任基、代码模块边界。
目标是防止语义分叉，不是描述最终形态。

## 1. Canonical pipeline

```text
Surface
  严格、纯函数式为主的源码；spec / impl；own / borrow
    ↓ typecheck / proofcheck / refines
Core
  类型、证明项、所有权 facts
    ↓ regioninfer / borrowck / ownlower
MemIR
  区域、分配/释放、原地/复制、闭包/thunk、每调用区域
    ↓ ReprIR（设计中）
表示层
  布局、字长、浮点格式、ABI
    ↓
Backend
  VM（参考）/ WASM（生产）/ native（生产）
```

- **Surface** 是用户/AI 写的那一层：`fn`、`spec`、`impl`、`requires`、`ensures`、`own`/`borrow`。
- **MemIR** 只回答内存问题：值在哪个区域、何时整体释放、原地还是复制、闭包/thunk 捕获什么、能否逃逸。
- **ReprIR** 回答表示问题：`Int` 是 i64 还是 bigint、`Float` 是 f64 还是精确十进制、数组布局、ABI。它还不存在。
- **VM** 是参考后端和语义 oracle，不是生产后端；WASM/native 是正式后端。
- **`refines`** 是 Surface 层的实现检查：一个 `impl` 是否满足 `spec`；通过后还要求内核证书。

## 2. MemIR 的边界

MemIR 是内存 lowering 的 IR，不是数据表示层。

属于 MemIR：

- 区域创建/释放；
- 每次函数调用的区域；
- `set`/`push` 的原地/复制选择；
- 闭包/thunk 的环境捕获；
- 返回值的区域复制/提升；
- `Unique` / borrow facts 的消费。

不属于 MemIR：

- `Int` 的字宽、溢出语义；
- `Float` 的格式与舍入；
- 数组是 packed/boxed/SoA；
- 记录字段布局、对齐、字节序；
- WASM 线性内存/GC 对象的具体映射。

这些属于 `ReprIR`。把表示塞进 MemIR 会让内存层与后端耦合。

## 3. 执行路径

当前有三条可执行路径：

```text
ownership::run      旧的树遍历解释器
vm::run             栈式字节码 VM（参考后端）
mem_plan::run       MemIR 区域求值器
```

短期定位：

- `mem_plan::run` 是 MemIR 的**可执行模型**；
- `vm::run` 是**参考后端**，用于差分测试和快速实验；
- `ownership::run` 是早期解释器，后续逐步退出主线；
- 生产后端 WASM/native 接 `MemIR + ReprIR`。

在 MemIR 类型化、区域/逃逸规则稳定前，不试图把三者合并；但新功能只应在
canonical pipeline 的一层实现，不能三处同时长。

## 4. 信任基（当前）

当前可信/未证明的部分：

- 依赖类型内核实现（`src/dependent.rs`）；
- parser / ownership checker / refines 原型；
- MemIR planner / bridge / evaluator；
- `RegionHeap` 的 Rust 实现；
- 字节码验证器与 VM；
- Rust 编译器/运行时本身。

已经由测试覆盖、但还没有形式化证明的性质：

- 错误证明被内核拒绝；
- `set` 原地/复制与纯语义的观察等价（模型测试）；
- `RegionHeap` 释放后访问拒绝；
- MemIR 的 scope/type/arity 验证；
- VM 的栈/局部/跳转/调用/数组验证。

目标信任基：

```text
小内核 + 小验证器 + 参考 VM 语义
```

生产后端通过以下方式降低信任：

- 翻译验证证书；
- 差分测试 vs 参考 VM；
- 关键 lowering 的精化证明；
- 小范围 `trusted` 标记与审计。

## 5. 代码模块

| 模块 | 职责 |
|---|---|
| `src/dependent.rs` | 内核模块入口；`dependent/term.rs`、`debruijn.rs`、`env.rs`、`check.rs` |
| `src/syntax.rs` | 表面语法模块入口；`syntax/ast.rs`、`syntax/lexer.rs`、`syntax/parser.rs`、`syntax/pretty.rs` |
| `src/ownership.rs` | 所有权模块入口；`ownership/types.rs`、`ownership/checker.rs`、`ownership/interp.rs` |
| `src/refines.rs` | `spec`/`impl`/`refines` 原型 |
| `src/refines_proof.rs` | refines 的内核证书映射 |
| `src/ownership_proof.rs` | 所有权模型证明入口；`two_pos.rs`、`nat.rs`、`borrow.rs`、`common.rs` |
| `src/region.rs` | 区域堆模块入口；`region/heap.rs` 分配/释放/观察；`region/types.rs` 类型 |
| `src/mem_plan/ir.rs` | `SrcExpr`、`MemExpr`、`MemFn`、`MemProgram` |
| `src/mem_plan/planner.rs` | move/downgrade/copy、原地/复制选择 |
| `src/mem_plan/eval.rs` | MemIR 求值、区域、闭包/thunk |
| `src/mem_plan/bridge.rs` | checked surface → MemIR、运行入口 |
| `src/mem_plan/validate.rs` | MemIR scope/type/arity 验证 |
| `src/mem_plan/tests.rs` | MemIR 单元测试 |
| `src/vm.rs` | 字节码模块入口；`vm/ir.rs`、`vm/compile.rs`、`vm/verify.rs`、`vm/run.rs` |
| `tests/` | 公开 API 集成测试 |

## 6. 当前状态

- `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`
  已纳入 CI；
- MemIR 有静态验证器；`run_program_in` 执行前验证；
- 核心模块已拆分：`mem_plan`（ir/planner/eval/bridge/validate）、`region`
  （heap/types）、`vm`（ir/compile/verify/run）、`syntax`（ast/lexer/parser/pretty）、
  `ownership`（types/checker/interp）、`dependent`（term/debruijn/env/check）、
  `ownership_proof`（common/two_pos/nat/borrow）；`src/*.rs` 只保留模块入口和 re-export；
- `tests/` 覆盖区域堆和 surface → MemIR → 区域执行；
- 文档以 `docs/18-region-memory.md` 为内存路线，本文为 pipeline 路线。

## 7. 未决问题

1. **闭包捕获语义**：不可变值捕获 vs JS 式共享 cell；后者复制不保语义。
2. **区域/逃逸推断**：返回值提升、闭包/thunk 逃逸、跨调用生命周期。
3. **Surface 函数类型/lambda**：MemIR 已有一等闭包/thunk，但表面还没有。
4. **表示层**：`Repr T R`、布局、字长、精确数值 vs f64/误差界。
5. **终止/可达性**：`Never` + `Reach = No | Maybe | Yes`、`MayCall`。
6. **表面证明语言**：`theorem`/hole/tactic/goal JSON；`refines` 自动义务生成。
7. **VM 冻结**：参考语义版本化、conformance/differential suite。
