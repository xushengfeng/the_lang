# 业务代码、类型证明、优化：文件与数据流

这份文档回答三件事：

1. 业务代码、类型/证明、优化分别放在哪；
2. 中端到底做什么；
3. facts / obligations / proofs / decisions 往哪流。

## 1. 三类用户文件

目标形态（不是当前语法，只是文件职责）：

```text
project/
  src/
    main.impl.alt        -- 业务实现，可读
    array.spec.alt       -- 类型、requires/ensures、effect、cost
    array.impl.alt       -- 可读参考实现
    array.opt.alt        -- AI/优化实现，可读性不保证
  proofs/
    array.proof.alt      -- 定理/证明脚本
    ownership.proof.alt  -- borrow/unique/原地定理
  lib/
    math/                -- 常用数学定义/定理
    ownership/           -- 所有权/借用模型
```

### 业务代码

- 写业务逻辑；
- 调用 spec 里的类型/接口；
- 不写 proof term；
- 尽量可读。

### 类型/规格/证明

- `spec`：类型、`requires`、`ensures`、effect、cost、确定性；
- `proof`：定理、证明脚本、库引理；
- 由 `proofcheck` 检查；
- 运行时擦除。

### 优化实现

- AI 生成或人类重写；
- 必须 `refines` 同一个 spec；
- 可读性不保证；
- 由 benchmark 选择；
- 记录 provenance。

### 编译器本身

- 不是用户文件；
- 包含前端检查、证明检查、middle-end、后端。

## 2. 当前仓库对应关系

| 角色 | 当前文件 | 说明 |
|---|---|---|
| 业务示例 | `examples/ownership.alt` | runtime 表面语言 |
| 类型/精化示例 | `examples/checks.alt` | 早期精化语法原型 |
| 业务检查 + lowering | `src/ownership.rs` | 原型把 checker/lowering/interpreter 放一起 |
| 字节码 + VM | `src/vm.rs` | codegen + VM |
| 证明内核 | `src/dependent.rs` | `proofcheck` |
| 证明模型/库 | `src/ownership_proof.rs` | ProofModel：原地/borrow 定理 |
| 设计文档 | `docs/` | 规则、模型、路线图 |

注意：当前 `src/ownership.rs` 同时做了三件事：

1. OwnershipChecker（编译期）；
2. Lowering（编译期）；
3. 树遍历解释器（运行期测试）。

目标是把它们拆开；现在只是原型。

## 3. 编译器分层

```text
Frontend
  parse
  resolve
  typecheck
  owncheck / borrowck / regioninfer / effectcheck
  refinecheck

Verification
  proofcheck

Middle-end
  erase          删除 proof / 类型索引 / 生命周期
  ownlower       原地 vs 复制、drop、borrow 消除
  checkelide     删边界/除零检查
  ANF/IR         显式求值顺序
  optimization   inline / DCE / CSE / loop / unboxing
  validation     refines / 翻译验证

Backend
  codegen        bytecode / LLVM / WASM / native
  verify        字节码验证 / LLVM verifier / WASM validator

Runtime
  VM / CPU / OS / 内存 / RC / 数组库
```

## 4. 中端是干什么的

中端**不只是优化**。它做四件事：

1. **擦除**：证明、类型索引、生命周期、精化谓词不进入运行时；
2. **降级**：高层 IR → 低层 IR，决定 `set` 原地/复制、检查有无；
3. **优化**：inline、DCE、CSE、循环变换、unboxing、单态化；
4. **验证**：每个关键转换产出 `refines`/翻译验证条件。

所以：

- “优化”主要发生在中端和后端；
- 但中端还负责“把证明/类型变成运行时决策”；
- 不是所有优化都在中端：算法选择在 spec 层，指令选择在 backend。

## 5. 数据流：四类数据

| 数据 | 产生 | 编译期？ | 运行期？ | 例子 |
|---|---|---|---|---|
| **Fact** | owncheck/borrowck/refinecheck | 是 | 否 | `Unique(a)`、`Bounds(i,len)` |
| **Obligation** | refinecheck | 是 | 否 | `refines(fast, spec)`、`Unique -> set_in_place = set_pure` |
| **Proof** | 人类/tactic/SMT/LLM | 是 | 否 | 内核证明项 |
| **Decision** | lowering | 是 | 是（编码在 IR） | `in_place=true`、`checked=false` |

关键：

- **Facts 不进入运行时**；
- **Proofs 不进入运行时**；
- 只有 **Decisions** 进入 IR/字节码；
- 运行时只执行 decisions，不知道 facts/proofs。

## 6. 整体数据流

```text
业务源码
  │ parse
  ▼
AST
  │ typecheck
  ▼
Typed AST
  │ owncheck / borrowck / regioninfer / effectcheck
  ▼
Facts + Ownership AST
  │ refinecheck
  ├──► Obligations ──► proofcheck ◄── Proofs（人/SMT/LLM）
  │                         │
  │                         ▼
  │                    discharged / rejected
  ▼
Erased IR（无 proof/类型索引/生命周期）
  │ ownlower / checkelide
  ▼
OwnIR（in_place / checked 决策）
  │ ANF + optimization
  ▼
Optimized IR
  │ codegen
  ▼
Bytecode / LLVM IR / WASM / native
  │ verify
  ▼
Runtime
```

### Facts 的流向

```text
owncheck/borrowck
  → Unique(a), Borrow(a,r), Live(r), Bounds(i,len), Pure(f)
     ├─► refinecheck：变成 obligations
     ├─► ownlower：决定 in_place
     ├─► checkelide：决定 checked
     └─► optimizer：CSE/并行/向量化
```

Facts 是编译期知识，最后只留下决策。

### Proofs 的流向

```text
refinecheck → obligation
  → 人类/tactic/SMT/LLM 产生 proof
  → proofcheck
  → 成功：obligation discharged
  → 失败：报错，或回退 checked/copy（按策略）
```

Proof 不进入 IR/字节码。

### Optimization 的流向

```text
spec + 多实现（ref / fast）
  → refines 检查
  → benchmark
  → 选中的实现

中端 IR
  → ownlower / checkelide / inline / DCE / CSE
  → 每个关键 pass 产出验证条件
  → 翻译验证/内核检查
  → 后端
```

## 7. 例子：`fill`

```text
业务实现：
  fill(own a, x) { set(a, 0, x) }

typecheck:
  a : Array<Int>, x : Int
  set : Array<Int> -> Int -> Int -> Array<Int>

owncheck:
  a 是 own，且只使用一次
  → Fact: Unique(a)

borrowck:
  无 live borrow
  → 允许写

refinecheck:
  Obligation: Unique(a) -> fill 的行为 = set_pure(a,0,x)

proofcheck:
  用 ProofModel 的 set_frame_nat / update_at 定理 discharge

erase:
  删除 own/Unique/proof/类型索引

ownlower:
  a 唯一 → ArraySet { in_place: true }
  否则   → ArraySet { in_place: false }

codegen:
  bytecode / LLVM / WASM / native

runtime:
  执行 in_place 决策，不知道 Unique/proof
```

## 8. 优化分层

| 层 | 谁决定 | 例子 | 怎么保证正确 |
|---|---|---|---|
| spec 层 | 人/AI | 归并排序 vs 快排 | `refines` + 内核 |
| 中端 | 编译器/AI | 原地、删检查、inline、DCE | facts + 翻译验证 |
| 后端 | 编译器 | 寄存器分配、指令选择 | 验证后端/信任 LLVM |
| 运行时 | 系统 | RC/内存分配 | 运行时正确性 |

AI 可以在 spec 层和中端提候选；验证器和 benchmark 过滤。

## 9. 当前 vs 目标

**当前：**

- 业务示例：`examples/ownership.alt`
- 检查/lowering：`src/ownership.rs`
- 字节码/VM：`src/vm.rs`
- proofcheck：`src/dependent.rs`
- ProofModel：`src/ownership_proof.rs`
- 没有 `refines`、没有 IR、没有真正的中端优化。

**目标：**

- 用户文件分离：`impl` / `spec` / `proof` / `opt`；
- 编译器内部拆分：frontend / verification / middle / backend；
- facts/obligations/proofs/decisions 按本文方向流动；
- 每个关键 lowering 有 `refines`/翻译验证。

一句话：

> **业务代码产生 facts；facts 产生 obligations；obligations 由 proofs 消解；lowering 只消费 facts，产出 decisions；runtime 只看到 decisions。**
