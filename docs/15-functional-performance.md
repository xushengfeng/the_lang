# 函数式默认复制、性能与原地修改

## 1. 先明确：函数式的默认语义确实是复制

纯函数式的参考语义是**持久化（persistent）**：

```text
let a = [1,2,3]
let b = set(a, 0, 9)
-- a 仍然是 [1,2,3]，b 是 [9,2,3]
```

这带来：

- **安全**：没有别名突变、没有数据竞争、没有 use-after-free；
- **可组合**：函数只依赖输入，局部推理；
- **可验证**：数学语义简单，`set` 就是序列更新；
- **可优化**：编译器可以在满足条件时把复制改成原地。

性能问题是真实的：大数组/记录的每次更新都复制，分配和 GC 压力大。

> 当前路线见 `docs/18-region-memory.md`：**不引入 RC/GC**，共享不可变值放区域或复制。本文的 RC/COW/Perceus 作为可选方案保留，不是当前实现计划。

目标不是放弃纯语义，而是：

> **保持纯语义作为参考，把“复制”在可证明安全的地方 lowering 成借用/原地。**

## 2. Rust 的生命周期对函数式有帮助吗？

有，但不是核心。

### 有帮助的部分

- **借用有效性**：`Ref<r,T>` 不能超过 `r`；防止悬垂/逃逸。
- **别名控制**：`Borrow(x,r)` 与 `BorrowMut(x,r)` 互斥；防止读写冲突。
- **独占可变借用**：`borrow_mut` 期间不能读/移动/再借用。
- **区域约束**：`r1 ⊑ r2` 让返回值/结构体字段的生命周期可检查。

这些在“把复制改成引用”时非常有用：  
你要把 `f(a)` 从“复制整个数组”改成“传一个只读视图”，就必须保证这个视图不会活过 `a`，也不会在 `a` 被 move/修改后还被用。Rust 的 borrow/lifetime 正是这套纪律。

### 不够的部分

- **函数式的性能核心是唯一性/复用，不是生命周期**。  
  生命周期告诉你“借用能活多久”；唯一性告诉你“能不能原地改”。后者才是把复制变原地更新的关键。
- **Rust 的 lifetime 语法/borrow checker 对函数式代码可能过重**。  
  纯函数式代码本来没有那么多可变别名；如果每个函数都写 `'a`，会把脚本式代码变成 Rust。
- **无 GC 函数式需要区域推断（或 RC）**。
  生命周期本身不解决“什么时候释放”。当前路线用区域 + 复制；MLKit 的区域推断、Koka 的 Perceus、Lean 的 RC 是参考。
- **共享持久数据结构需要区域/复制（RC/COW 为备选）**。
  Rust 的 `Rc`/`Arc` 是库，不是语言核心；函数式语言需要把共享/复制策略做成类型的一部分。

结论：

> **借 Rust 的“借用/别名/逃逸”思想，不照搬 Rust 的 lifetime 语法。**  
> 表面尽量推断；只在 API 边界写 `own`/`borrow`；核心是唯一性 + 区域 + 证明驱动的 lowering。

## 3. 我们想把复制改成什么

### 3.1 只读借用（零成本）

```alethe
fn sum(borrow a: Array<Int>) -> Int {
  ...
}
```

- 不复制数组；
- 只读；
- 不能逃逸；
- 运行时就是一个指针/句柄（或直接内联）。

这是最安全、收益最直接的优化。

### 3.2 唯一所有权 + 原地更新

```alethe
fn fill(own a: Array<Int>, x: Int) -> Array<Int> {
  set(a, 0, x)        -- 唯一 + 只使用一次 → 原地
}
```

- `own` 表示唯一所有权；
- `set` 消费 `a`，返回同一块数组；
- 旧值不再被观察，所以与纯 `set` 等价；
- 没有复制/分配。

### 3.3 共享 + 区域内只读 / 复制（RC/COW 为备选）

```alethe
fn fill_shared(share a: Array<Int>, x: Int) -> Array<Int> {
  set(a, 0, x)        -- 共享 → 复制后写
}
```

- 共享句柄；
- 当前路线：区域内只读共享，或写时复制；
- 安全，但可能分配；
- 优化器可以尝试证明“其实唯一”，失败就复制；
- RC/COW 是可选方案，不属于当前无 RC/GC 路线。

### 3.4 区域分配

```alethe
fn parse(input: borrow Slice<Char>) -> own Ast {
  region r {
    let tmp = new_r [...];
    ...
  }
}
```

- 临时值分配在区域 `r`；
- 区域结束统一释放；
- 不逃逸的值可以栈分配；
- 逃逸值提升到 `own`/`share`。

### 3.5 Perceus/RC 复用（备选）

若未来选择 RC 路线：

- 共享持久数据用 RC；
- 当 RC=1 时，原地更新（reuse analysis）；
- 避免 tracing GC；
- Koka/Roc/Lean 4 的经验。

## 4. 原地修改为什么危险

危险不在“写内存”，而在**别名**：

```text
let a = [1,2,3]
let b = a          -- b 和 a 是同一个数组？
set(a, 0, 9)
print(b)           -- 应该是 [1,2,3] 还是 [9,2,3]？
```

- 纯语义要求 `b` 仍然是 `[1,2,3]`；
- 原地修改会让 `b` 变成 `[9,2,3]`；
- 如果 `b` 仍可观察，原地就是错的。

所以原地修改的充分条件不是“类型是 Array”，而是：

```text
Unique(a) ∧
a 没有被其他句柄观察 ∧
a 没有被 live borrow ∧
a 在续文中不再被使用 ∧
没有并发读者/FFI 别名
```

## 5. 我们的设计：复制默认，原地靠证明

### 5.1 源语义保持纯

- `set(a,i,v)` 在源语义中返回新数组；
- 旧值永远有效；
- 编译器可以选择复制或原地，但**不能改变可观察行为**。

### 5.2 类型/检查器提供事实

```text
own     -- 唯一所有权，可 move
share   -- 共享：区域内只读或复制
borrow  -- 只读借用，不能逃逸
borrow_mut -- 独占可变借用
copy    -- 小值/值语义
```

检查器产出：

```text
Unique(a)
Borrow(a,r), BorrowMut(a,r)
Live(r), r1 ⊑ r2
Bounds(i, len(a))
```

### 5.3 lowering 只消费事实

```text
Unique(a) ∧ a 只出现一次
  → set_in_place(a,i,v)

否则
  → set_copy(a,i,v)
```

- 证明失败 → 复制，安全但慢；
- 不静默 UB；
- 可以按模块要求 `verified`，否则 `checked`。

### 5.4 原地 lowering 可验证

每个 `set_in_place` 产生一个精化义务：

```text
Unique(a) ∧ 0 <= i < len(a)
--------------------------------
set_in_place(a,i,v) ≈ set_pure(a,i,v)
```

- 内核/翻译验证器检查；
- 检查失败 → 回退 copy；
- 优化器不需要被信任。

### 5.5 参考解释器 + 优化 VM 差分

- 树遍历解释器执行纯语义；
- VM 执行 lowering 后的原地/复制；
- 同一程序两边结果/观察轨迹一致；
- 统计分配/复制/原地次数。

这不能替代证明，但能抓 lowering/VM 的 bug。

## 6. 这个设计能做好吗？

能，但必须坚持几个原则：

1. **源语义永远纯**。  
   原地是优化，不是源语言的可观察行为。
2. **唯一性是证明义务，不是猜测**。  
   `Unique(a)` 必须由 `owncheck`/`borrowck` 建立；lowering 不能“觉得应该唯一”。
3. **没有可观察别名才原地**。  
   `share`/`borrow`/FFI/并发都要纳入模型；否则一律复制。
4. **失败就复制**。  
   证明不了就保留检查/复制，正确性不依赖优化成功。
5. **lowering 可验证**。  
   关键变换产出证书/精化证明；否则标 trusted。
6. **区域兜底**。
   不是所有值都能栈分配；共享持久数据放区域或复制。RC/GC 只是备选路线。
7. **效果系统防重排**。  
   `write`/`io` 不能随便重排；`pure`/`read` 才能并行。
8. **渐进验证**。  
   热路径要求 `verified`；边缘允许 `checked`。

## 7. 先例

| 系统 | 经验 |
|---|---|
| **Koka / Perceus** | 函数式 + RC + reuse analysis；唯一时原地；无 tracing GC |
| **Roc** | morphic RC；函数式，无 GC |
| **Lean 4** | RC、证明擦除、编译器优化 |
| **MLKit** | 区域推断；无 GC |
| **Rust** | 所有权/借用/生命周期；别名与独占写 |
| **ATS / Cogent** | 线性/唯一性类型；系统编程 |
| **F\* / Low\*** | 验证 + 提取 C；数组前置条件；零成本 |
| **Haskell ST / runST** | 纯 API 内部封装局部 mutation |
| **Linear Haskell** | 线性类型做原地/资源 |

## 8. 一句话

> **函数式默认复制是安全的参考语义；我们把复制 lowering 成借用/原地，但只在 `Unique` 事实成立、且 lowering 可验证时。**  
> Rust 的 lifetime 给了借用/别名纪律，但函数式性能的核心是**唯一性 + 区域 + 证明驱动的 lowering**，不是照搬 Rust 的 `'a` 语法。
> 危险是真实的；控制危险的方式是：源语义纯、默认复制、失败回退、关键 lowering 出证明。

---

## 9. 最小机器检查模型（已实现）

代码：`src/ownership_proof.rs`；demo：`cargo run --bin ownership_proof_demo`。

**这是 `ProofModel`，不是 `OwnershipChecker`。**  
它由 `proofcheck` 在构建/开发阶段检查；`OwnershipChecker`、`Lowering`、`BytecodeVM` 都没有被这个证明覆盖。运行时不做验证；VM 的统计只是测试观察。

为了不把内核扩成完整堆逻辑，先用一个**两位置模型**把核心条件证出来：

```text
Loc   = Bool                 -- 两个位置
Aid   = Bool                 -- 两个数组 id
Heap  = Loc -> Aid
Store = Aid -> Array
Array = List Nat

Unique h := h true != h false
```

语义：

```text
get s h l      = s (h l)
set_pure      h s i v = arr_set (s (h true)) i v        -- 返回新数组，store 不变
set_in_place  h s i v = update_store s (h true)
                          (arr_set (s (h true)) i v)     -- 原地更新 store
```

检查通过的定理：

```text
set_result_same :
  ∀ h s i v,
    obs (set_in_place h s i v) h true = set_pure h s i v

set_frame :
  ∀ h s i v,
    Unique h ->
    obs (set_in_place h s i v) h false = obs s h false
```

含义：

- 原地和纯版本返回同一个更新后的数组；
- 如果两个位置不别名（`Unique h`），原地更新第一个位置对第二个观察者不可见。

实际输出：

```text
✓ set_result_same    proof nodes: 33
✓ set_frame          proof nodes: 43
```

这不是完整堆模型，但它把“唯一性 ⇒ 原地更新不改变其他观察者”变成了内核检查的定理。  
两位置的 `Bool.rec` 只是最小基例；多位置应使用**索引上的结构归纳**（`update_at_other`），不是枚举所有位置。

下一步可以把它推广到：

- 任意数量的位置/数组 id；
- `Borrow`/`BorrowMut`；
- 区域结束与 drop；
- 真实字节码的 `set { in_place }`。

---

## 10. Nat 索引模型（已机器检查）

`Bool2` 只是两位置的最小基例。任意位置用 `Nat` 索引，证明靠归纳，不是枚举。

```text
Loc = Nat
Aid = Nat
Heap  = Loc -> Aid
Store = Aid -> Array
Array = List Nat

UniqueNat h := ∀ i j, i ≠ j -> h i ≠ h j
```

核心是 `update_at`：

```text
update_at a v s b =
  if b = a then v else s b
```

实现用递归 + store 移位：

```text
update_at 0       v s 0       = v
update_at 0       v s (succ b) = s (succ b)
update_at (succ a) v s 0       = s 0
update_at (succ a) v s (succ b) = update_at a v (shift s) b
shift s = λ x. s (succ x)
```

证明的一般引理：

```text
update_at_same  : ∀ a v s, update_at a v s a = v
update_at_other : ∀ a b v s, a ≠ b -> update_at a v s b = s b
```

`update_at_other` 对 `a` 做 `Nat.recProp` 归纳；`succ/succ` 情形用
`neq_succ_succ` 和归纳假设，不是枚举所有位置。

Frame：

```text
set_frame_nat :
  ∀ h s l i v j,
    UniqueNat h ->
    j ≠ l ->
    get_store (set_in_place_at h s l i v) h j
      = get_store s h j
```

内核检查通过：

```text
✓ set_result_same_nat    proof nodes: 35
✓ set_frame_nat          proof nodes: 65
```

这仍然是 `ProofModel`，不是 `OwnershipChecker` / `Lowering` / `BytecodeVM` 的验证。  
但它已经把“任意位置 + 唯一性 ⇒ 原地更新对其它位置不可见”证成了内核定理。

---

## 11. Borrow 权限模型（已机器检查）

`ProofModel` 里现在也加了最小 borrow 权限模型：

```text
Perm p : Loc -> Nat
  0 = free
  1 = shared borrow
  2 = mutable borrow

Free p l      := p l = 0
Borrow p l    := p l = 1
BorrowMut p l := p l = 2
CanRead p l   := p l ≠ 0
CanWrite p l  := BorrowMut p l
```

proof-carrying 读写：

```text
read_with  : Perm -> Loc -> Heap -> Store -> CanRead p l -> Array
write_with : Perm -> Loc -> Heap -> Store -> Nat -> Nat
             -> BorrowMut p l -> Store
```

检查通过的定理：

```text
borrow_not_mut      : Borrow p l -> BorrowMut p l -> False
borrow_not_free     : Borrow p l -> Free p l -> False
borrow_mut_not_free : BorrowMut p l -> Free p l -> False
borrow_can_read     : Borrow p l -> CanRead p l
borrow_mut_can_read : BorrowMut p l -> CanRead p l
borrow_cannot_write : Borrow p l -> CanWrite p l -> False
borrow_mut_can_write: BorrowMut p l -> CanWrite p l
```

含义：

- 共享借用只读；写需要 `BorrowMut`；
- 共享借用和可变借用互斥；
- 可变借用存在时，位置不是 free，不能再获取可变借用；
- `read_with`/`write_with` 的权限证明参数在运行时擦除，只由类型检查约束。

当前限制：

- 还没有 `acquire`/`release` 状态转移；
- 共享借用没有计数（状态只表示“有共享借用”，不表示几个）；
- 没有区域/生命周期；
- 没有和 `set_frame_nat` 的堆唯一性模型合并。

下一步：

1. `acquire_borrow` / `acquire_borrow_mut` / `release`，用 `update_perm` 证明 frame；
2. 共享借用计数（`p l = 1 + n` 或单独的计数状态）；
3. 区域/生命周期，把 borrow 限制在 `r` 内；
4. 把 `write_with` 与 `set_frame_nat` 组合，得到“可变借用下原地写安全”。
