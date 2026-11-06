# 组件命名与职责

“checker” 现在被用来指太多东西：类型检查、所有权、借用、区域、效果、精化、证明检查。继续这样会混淆。本文给每个职责一个固定名字，并规定命名规则。

## 0. “模型”这个词也要禁用

“模型”现在至少指四样东西，必须分开叫：

| 名字 | 指什么 | 谁检查 |
|---|---|---|
| **ProofModel** | `src/ownership_proof.rs` 里的形式化定义/定理（堆、store、Unique、set_pure/set_in_place） | `proofcheck`（内核） |
| **OwnershipChecker** | `src/ownership.rs` 的 Rust 静态分析（owncheck/borrowck 原型） | 无形式证明；测试 |
| **Lowering** | `ownlower` 的原地/复制决策 | 无形式证明；测试/待翻译验证 |
| **BytecodeVM** | `src/vm.rs` 的字节码执行 | 无形式证明；测试 |
| **DesignSpec** | `docs/11`–`docs/15` 的规则/义务 | 不是机器检查对象 |

以后不要单独说“模型”。说 `ProofModel` / `OwnershipChecker` / `Lowering` / `BytecodeVM` / `DesignSpec`。

## 1. 总流水线

```text
Frontend
  parse        -- 词法/语法
  resolve      -- 名字/模块解析
  elaborate    -- 隐式参数、surface → core

Admission（静态检查，接受/拒绝）
  typecheck    -- 类型一致性
  owncheck     -- 所有权、move、线性、唯一性、drop
  borrowck     -- 借用、别名、逃逸、独占
  regioninfer  -- 区域/生命周期约束求解
  effectcheck  -- 纯度、read/write/alloc/io/unsafe
  termcheck    -- 终止性/递归度量
  refinecheck  -- 精化义务生成

Verification（证明检查）
  proofcheck   -- 小内核检查证明项

Lowering（IR 变换，可能产生验证义务）
  erase        -- 删除 proof/region/lifetime/type index
  ownlower     -- 原地 vs 复制、drop 插入、borrow 消除
  checkelide   -- 边界/除零检查消除
  anf          -- A-normal form
  codegen      -- 字节码生成

Execution
  verifybc     -- 字节码验证
  vm           -- 执行 / 解释
```

## 2. 分组定义

| 组 | 职责 | 是否在 TCB |
|---|---|---|
| **Frontend** | 语法、名字、elaboration | 否（输出可检查的 core） |
| **Admission** | 静态规则，接受/拒绝程序 | 否（但必须 sound） |
| **Verification** | 内核检查证明 | **是**（小内核） |
| **Lowering** | 语义保持的 IR 变换 | 可信或需证明/翻译验证 |
| **Execution** | 字节码验证 + VM | 验证器/VM 可信或需证明 |

## 3. 名字的含义

### 3.1 `typecheck`

只回答“类型是否一致”：

- `Int` vs `Bool`；
- 函数参数/返回类型；
- 泛型实例化；
- 结构的字段类型。

**不做** move/borrow/region/effect。

### 3.2 `owncheck`

回答“所有权/使用次数”：

- 变量是否被 move；
- move 后是否再用；
- `own` 是否恰好消费一次（或 affine drop）；
- 唯一性 `Unique(x)`；
- drop 插入点；
- 线性/仿射资源。

**不做**借用别名/生命周期。

### 3.3 `borrowck`

回答“借用/别名/逃逸”：

- `&x` / `&mut x` 是否合法；
- 借用期间是否 move/写冲突；
- 借用是否逃逸（返回值/结构体）；
- `Borrow(x,r)` / `BorrowMut(x,r)` 的生命周期；
- 共享读 vs 独占写。

`borrowck` 消费 `owncheck` 产出的 `Unique` facts，并产出 `Borrow`/`Live` 约束。

### 3.4 `regioninfer`

回答“区域/生命周期约束”：

- `Live(r)`；
- `r1 ⊑ r2`；
- 分配属于哪个区域；
- 区域何时结束/释放；
- 返回值是否引用本地 `r`；
- 区域多态实例化。

它与 `borrowck` 的分工：

- `borrowck` 产生约束；
- `regioninfer` 求解约束；
- 求解失败 → 拒绝或插入 RC/复制。

### 3.5 `effectcheck`

回答“效果”：

- 函数是否 `pure`/`read`/`write`/`alloc`/`io`/`unsafe`；
- 调用是否允许重排/并行；
- `borrow` 读 vs `borrow_mut` 写；
- 效果 row 的子类型/并集。

### 3.6 `termcheck`

回答“是否终止/递归度量”：

- 结构递归；
- 尺寸变化；
- 度量函数；
- 互递归。

与 `owncheck` 独立：终止性不是所有权。

### 3.7 `refinecheck`

回答“精化义务”：

- 生成 `requires`/`ensures`/`invariant` 的证明义务；
- 生成 `Bounds`/`NonZero`/`Unique` 等 facts 的义务；
- 把义务交给 `proofcheck` 或 SMT/搜索/LLM 候选。

**不**自己证明；它只生成/管理义务。

### 3.8 `proofcheck`

小内核：

- 检查证明项；
- 定义等价；
- 不生成证明，不搜索，不调用 SMT/LLM。

`proofcheck` 是唯一验证者；其他组件只提供候选。

### 3.9 `erase`

删除运行时不需要的东西：

- `Prop` 证明、proof 参数/字段；
- 精化谓词；
- 类型索引；
- 区域/生命周期；
- 成本注解。

### 3.10 `ownlower`

把 `own`/`borrow`/`unique` facts 变成运行时 IR：

- `set`/`push` → 原地或复制；
- `borrow` → 别名/指针（零运行时）；
- `own` → move；
- 插入 `drop`；
- 插入/删除 RC 操作。

`ownlower` 是**决策**，不是证明；决策需要 `Unique` facts，并且应可被验证/翻译验证。

### 3.11 `checkelide`

用 facts 删除运行时检查：

- `Bounds(i,len(a))` → `ArrayGet { checked: false }`；
- `NonZero(b)` → `Div { checked: false }`；
- 否则保留检查。

### 3.12 `anf` / `codegen` / `verifybc` / `vm`

- `anf`：显式求值顺序、let-bound atoms；
- `codegen`：生成字节码；
- `verifybc`：栈/局部/跳转/类型验证；
- `vm`：执行。

## 4. 事实、义务、证明

三个概念不要混：

| 概念 | 例子 | 谁产生 | 谁消费 |
|---|---|---|---|
| **Fact** | `Unique(a)`、`Bounds(i,len(a))`、`Pure(f)` | admission（owncheck/borrowck/refinecheck） | lowering/optimizer |
| **Obligation** | `refines(fast, spec)`、`i < len(a)` | refinecheck | proofcheck/SMT/LLM |
| **Proof** | 内核证明项 | 人类/搜索/SMT/LLM | proofcheck |

- Fact 是“已知为真”的命题，进入上下文。
- Obligation 是“必须证明”的命题。
- Proof 是内核接受的具体证明项。
- 优化器只能使用 Fact；不能把“未证明的 Obligation”当 Fact。

## 5. 命名规则

1. **Pass 用动词**：`parse`、`resolve`、`typecheck`、`ownlower`、`erase`。
2. **IR 用名词**：`Surface`、`Core`、`Erased`、`OwnIR`、`ANF`、`Bytecode`。
3. **`-check` = 接受/拒绝**：`typecheck`、`owncheck`、`borrowck`、`effectcheck`。
4. **`-infer` = 约束求解**：`regioninfer`、`typeinfer`（如果需要）。
5. **`-lower` = IR 变换**：`ownlower`、`checkelide`。
6. **不要单独说 “checker”**：说 `owncheck` / `borrowck` / `proofcheck`，避免歧义。
7. **模式用小写**：`own`、`share`、`borrow`、`borrow_mut`、`copy`。
8. **事实用大写谓词**：`Unique(x)`、`Borrow(x,r)`、`Live(r)`、`Bounds(...)`。
9. **区域用 `r`，生命周期用 “region/lifetime” 时明确**：  
   - `region` = 分配/借用作用域；
   - `lifetime` = 值的语义存活范围；
   - 不要用 “lifetime” 指代所有区域。
10. **复制相关**：
    - `copy` = 值语义复制；
    - `clone` = 深复制；
    - `cow` = copy-on-write；
    - `in_place` = 原地更新；
    - `share` = 共享句柄/RC。

## 6. 当前原型的职责混乱

`src/ownership.rs` 现在同时做：

- typecheck（隐式）；
- owncheck；
- borrowck（简化）；
- ownlower（`set`/`push` 原地/复制）；
- 解释器。

这是原型为了快速验证，不是目标结构。按本文拆分后应为：

```text
typecheck
  → owncheck
  → borrowck
  → regioninfer
  → effectcheck
  → refinecheck
  → erase
  → ownlower
  → anf
  → codegen
  → verifybc
  → vm
```

`src/dependent.rs` 是 `proofcheck`（内核）；`src/vm.rs` 是 `codegen` + `vm`，还缺 `verifybc`。

## 7. 对外接口

每个 pass 的接口应显式：

```text
Pass: InputIR + Facts -> OutputIR + NewFacts + Obligations + Diagnostics
```

- Facts 只能增加/推导，不能凭空产生。
- Obligations 由 `refinecheck`/`proofcheck` 管理。
- Diagnostics 结构化（JSON），给 AI 用。
- 每个 pass 可独立测试/替换。

## 8. 一句话

> **`typecheck` 管类型，`owncheck` 管所有权，`borrowck` 管别名/借用，`regioninfer` 管区域，`effectcheck` 管效果，`refinecheck` 管义务，`proofcheck` 管证明；`ownlower` 只是消费 facts 做决策。**
