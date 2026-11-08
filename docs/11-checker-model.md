# 检查器模型：所有权、借用、区域、效果

本文是**模型规格**，不是 Rust 实现说明。目标是把检查器的表达力定清楚，之后才能证明 soundness。

当前 Rust 原型只覆盖 `own` / `borrow` / 推断 unique 的一小部分；本文是目标模型。

> 当前内存路线见 `docs/18-region-memory.md`：**不引入 RC/GC**。本文里的 RC/引用计数描述是备选方案；当前 `share` 表示区域内只读共享或复制。

## 1. 设计目标

检查器要同时回答四类问题：

1. **类型**：这个表达式是什么类型？
2. **所有权**：值是否被 move、是否唯一、能否原地修改？
3. **借用/区域**：借用是否活着、是否逃逸、是否与 move/写冲突？
4. **效果**：纯不纯、读/写/分配/IO、能否重排/并行？

输出不只是“通过/拒绝”，还要给优化器/AI 提供 facts：

- `Unique(x)`：可原地更新；
- `Borrow(x, r)` / `BorrowMut(x, r)`：别名/写权限；
- `Live(r)` / `r1 ⊑ r2`：生命周期；
- `Bounds(i, len(a))`：可删边界检查；
- `Pure(f)` / `Deterministic(f)`：可重排/并行；
- `Terminates(f)`：可内联/展开；
- `Cost(f)`：资源上界（后续）。

## 2. 类型与限定符

```text
T ::= Int | Bool | Unit
    | Array<T>
    | Slice<T>
    | Ref<r, T>              -- 共享借用，区域 r
    | RefMut<r, T>           -- 唯一可变借用，区域 r
    | Record { l_i : T_i }
    | Variant { l_i : T_i }
    | T -o T                 -- 线性函数
    | T -> T                 -- 无限制函数
    | forall r. T            -- 区域多态（后续）
```

限定符（qualifier）：

```text
q ::= copy          -- 可复制，无所有权
    | own           -- 唯一所有权；可 move；可原地改
    | share         -- 共享：区域内只读共享或复制；可复制句柄
    | borrow(r)     -- 共享只读借用
    | borrow_mut(r) -- 唯一可变借用
```

绑定：

```text
x : T @ q
```

`own` 不是“更严格的 share”，而是不同的能力：

| q | move | 读 | 写 | 可复制 | 运行时 |
|---|---|---|---|---|---|
| `copy` | 复制 | 是 | 是（值语义） | 是 | 值/立即数 |
| `own` | 是（消费） | 是 | 是（原地） | 否 | 唯一句柄 |
| `share` | 否 | 是 | 否（复制后改） | 是 | 区域只读/复制 |
| `borrow(r)` | 否 | 是 | 否 | 否 | 指针/别名，零成本 |
| `borrow_mut(r)` | 否 | 是 | 是（独占） | 否 | 唯一指针 |

## 3. 区域与效果

### 区域

```text
r ::= region variable
r1 ⊑ r2        -- r1 outlives r2
Live(r)
```

- `Ref<r,T>` / `Borrow(x,r)` 只在 `r` 活着时有效。
- 返回值不能引用本地区域（除非 `forall r` 多态）。
- 区域结束时，借用失效；`own` 值 drop；`share` 引用失效或复制值独立。
- 区域推断作为约束求解；表面可省略。

### 效果

```text
E ::= ∅ | pure | read | write | alloc(r) | io | unsafe
    | E ∪ E
```

- `pure`：无副作用、确定性；可 CSE/重排/并行。
- `read`：只读堆；可并行读。
- `write`：写堆；需要独占。
- `alloc(r)`：在区域 r 分配。
- `io`：不可重排。
- `unsafe`：显式边界。

函数类型携带效果：

```text
f : (x1 : T1 @ q1, ..., xn : Tn @ qn) -> T ! E
```

## 4. 上下文与判断

```text
Γ ::= ∅ | Γ, x : T @ q          -- 运行时绑定
Δ ::= ∅ | Δ, Unique(x)
         | Δ, Borrow(x, r)
         | Δ, BorrowMut(x, r)
         | Δ, Live(r)
         | Δ, r1 ⊑ r2
         | Δ, P                   -- 其他事实：Bounds、Pure、...
```

主判断：

```text
Γ; Δ ⊢ e : T ! E ⇝ Γ'; Δ'
```

读法：

- 在 `Γ; Δ` 下，`e` 类型为 `T`，效果为 `E`；
- 求值后上下文变成 `Γ'`（被消费的变量移除/标记）；
- 事实变成 `Δ'`。

辅助判断：

```text
Γ; Δ ⊢ e ⇝ Γ'; Δ'          -- 只做所有权/借用/消费
Δ ⊢ Unique(e)              -- 唯一性
Δ ⊢ BorrowOK(e)            -- 借用合法性
Δ ⊢ NoEscape(T)            -- 返回类型不逃逸本地区域
```

## 5. 核心规则

### 变量

```text
Γ(x) = T @ q      q ≠ consumed
--------------------------------
Γ; Δ ⊢ x : T ! pure ⇝ Γ; Δ
```

读变量不消费。`own` 变量可以被读（临时借用），但在借用期间不能 move。

### Move

```text
Γ(x) = T @ own      x ∉ consumed
Γ' = Γ \ {x}
--------------------------------
Γ; Δ ⊢ move x : T @ own ! pure ⇝ Γ'; Δ
```

`move` 后 `x` 不可再用。`copy` 不需要 move；`share` 复制句柄；`borrow` 不能 move。

### Let

```text
Γ; Δ ⊢ e : T ! E ⇝ Γ1; Δ1
q = mode(e)                       -- own / borrow(r) / share / copy
Γ1, x : T @ q; Δ1 ⊢ body : U ! E' ⇝ Γ2; Δ2
------------------------------------------------
Γ; Δ ⊢ let x = e in body : U ! E ∪ E' ⇝ Γ2 \ {x}; Δ2
```

`mode(e)`：

- `move x` / fresh allocation / `set` 结果 → `own`；
- `&r e` → `borrow(r)`；
- `&mut r e` → `borrow_mut(r)`；
- 字面量/`copy` 值 → `copy`；
- `share` 值 → `share`。

### 分配

```text
Δ ⊢ Live(r)
-----------------------------------------
Γ; Δ ⊢ new_r [e1..en] : Own Array<T> ! alloc(r) ⇝ Γ; Δ
```

fresh allocation 是唯一值；区域 `r` 决定释放点。

### 读

```text
Γ; Δ ⊢ a : Array<T> @ q ! E ⇝ Γ1; Δ1
Γ1; Δ1 ⊢ i : Int ! E' ⇝ Γ2; Δ2
q ∈ {own, share, borrow(r), borrow_mut(r)}
----------------------------------------------------
Γ; Δ ⊢ a[i] : T ! E ∪ E' ∪ read ⇝ Γ2; Δ2
```

读不消费 `a`；`borrow`/`borrow_mut` 的读受区域约束。

### 写 / 原地

```text
Γ; Δ ⊢ a : Array<T> @ q ! E ⇝ Γ1; Δ1
Γ1; Δ1 ⊢ i : Int ! E' ⇝ Γ2; Δ2
Γ2; Δ2 ⊢ v : T ! E'' ⇝ Γ3; Δ3
Δ3 ⊢ Unique(a)      a ∉ consumed
a 在续文中只出现一次
--------------------------------------------------------
Γ; Δ ⊢ set(a, i, v) : Own Array<T> ! E∪E'∪E''∪write
      ⇝ Γ3 \ {a}; Δ3 ∪ {Fresh(result)}
```

这是**原地**规则：`a` 被消费，结果拥有同一块数组。

非唯一时：

```text
q = share/copy     -- 复制后写
Δ ⊬ Unique(a)
------------------------------------------------
Γ; Δ ⊢ set(a,i,v) : Own Array<T> ! ... ∪ alloc(r')
      ⇝ Γ; Δ ∪ {Fresh(result)}
```

`borrow` 上写：

```text
q = borrow(r)
-----------------------------
Γ; Δ ⊢ set(a,i,v) : error
```

`borrow_mut(r)` 上写：

```text
q = borrow_mut(r)      Δ ⊢ Unique(a)     无其他借用
----------------------------------------------------
Γ; Δ ⊢ set(a,i,v) : Own Array<T> ! write ⇝ Γ; Δ
```

### 函数调用

```text
f : (x1:T1@q1, ..., xn:Tn@qn) -> T ! Ef
Γ; Δ ⊢ ei : Ti ! Ei ⇝ Γi; Δi
own:  arg 是 unique，消费
share/copy: 不消费
borrow(r): 实参区域 r' 满足 r' ⊑ r
borrow_mut(r): 实参唯一，调用期间独占
------------------------------------------------------
Γ; Δ ⊢ f(e1..en) : T ! Ef ∪ ⋃Ei ⇝ Γn; Δn
```

返回：

- `own` 返回值消费；
- `borrow` 返回值不能逃逸本地区域；
- `share` 返回值是共享句柄。

### 借用

```text
Γ(x) = T @ q      q ≠ borrow_mut(r') 冲突
r fresh, Live(r), r ⊑ ...
------------------------------------------------
Γ; Δ ⊢ &r x : Ref<r,T> ! read ⇝ Γ; Δ ∪ {Borrow(x,r)}
```

```text
Γ(x) = T @ own      Δ ⊢ Unique(x)
无其他 Borrow/BorrowMut
r fresh
------------------------------------------------
Γ; Δ ⊢ &mut r x : RefMut<r,T> ! write
      ⇝ Γ[x 暂时不可读/不可 move]; Δ ∪ {BorrowMut(x,r)}
```

借用规则：

- `Borrow(x,r)` 活着时：`x` 可读；不可 move；不可 `&mut`。
- `BorrowMut(x,r)` 活着时：`x` 不可读、不可 move、不可再借用。
- `end(r)`：删除 `Borrow`/`BorrowMut`；恢复 `x` 的能力。
- 借用不能存进返回值/结构体，除非区域参数允许。

### If / Match

```text
Γ; Δ ⊢ c : Bool ! E ⇝ Γ1; Δ1
Γ1; Δ1 ∪ {c} ⊢ e1 : T ! E1 ⇝ Γ2; Δ2
Γ1; Δ1 ∪ {¬c} ⊢ e2 : T ! E2 ⇝ Γ3; Δ3
T1 = T2
------------------------------------------------
Γ; Δ ⊢ if c then e1 else e2 : T ! E∪E1∪E2
      ⇝ merge(Γ2,Γ3); merge(Δ2,Δ3)
```

`merge`：

- 任一支路消费的 `own` 变量，合并后视为已消费（保守）；
- 任一支路建立的借用，合并后保留（保守）；
- 支路本地变量不外泄。

### 返回

```text
Γ; Δ ⊢ e : T ! E ⇝ Γ'; Δ'
Δ' ⊢ NoEscape(T)          -- 不含本地区域/借用
own 返回值已消费
------------------------------------------------
Γ; Δ ⊢ return e : T ! E ⇝ Γ'; Δ'
```

`NoEscape`：

- `T` 不含本地 `Ref<r,_>`/`Borrow(_,r)`；
- 不返回 `borrow` 变量；
- 不返回引用本地区域的 `own` 结构。

### Drop

```text
Γ(x) = T @ own      x 未被消费
----------------------------
Γ; Δ ⊢ drop x : Unit ! pure ⇝ Γ \ {x}; Δ
```

`own` 可 drop（affine）；`borrow` 由区域结束隐式结束；`share` 由区域结束或复制值独立。

### 递归

函数签名先声明：

```text
f : (T1@q1, ..., Tn@qn) -> T ! E
```

递归调用按签名检查；终止性/资源上界是独立判断（后续）。

## 6. 唯一性与原地更新

`Unique(x)` 的来源：

- fresh allocation；
- `own` 绑定且未被借用/共享；
- `&mut` 期间临时唯一；
- `let y = move x` 后 `y` 继承唯一性；
- `set` 原地结果。

`Unique(x)` 的保持条件：

- 没有 `Borrow(x,r)` / `BorrowMut(x,r)` 活着（`&mut` 自己除外）；
- 没有 `share` 别名；
- 未被消费；
- 在续文中只出现一次（对原地 lowering 的保守充分条件）。

原地规则：

```text
Δ ⊢ Unique(a)
a ∉ consumed
a 在续文中只出现一次
----------------------------
set(a,i,v) ⇒ set_in_place(a,i,v)
```

否则：

- `share`/`copy` → `set_copy`；
- `borrow` → 错误（不能写共享借用）；
- `borrow_mut` → 允许写，但不能复制别名。

## 7. 检查器输出给优化器/AI 的 facts

```text
Unique(x)
Borrow(x,r) / BorrowMut(x,r)
Live(r), r1 ⊑ r2
Bounds(i, len(a))
Pure(f), Deterministic(f)
Terminates(f)
Cost(f)
NoAlias(a,b)
```

这些 facts 是**证明义务/事实**，不是优化器的命令：

- 优化器可以用 `Unique(a)` 做原地；
- 用 `Bounds(i,len(a))` 删边界检查；
- 用 `Pure(f)` 做 CSE/并行；
- 用 `NoAlias` 做向量化；
- 但每个变换仍需 `refines`/翻译验证。

## 8. Soundness 义务

要证明的定理（模型层）：

1. **类型安全**：`Γ; Δ ⊢ e : T ! E ⇝ Γ'; Δ'` 且初始堆满足 `Δ`，则求值不会 stuck（除显式错误）。
2. **所有权安全**：`Unique(x)` ⇒ 无别名；`Borrow` 只读；`BorrowMut` 独占。
3. **区域安全**：`Ref<r,T>` 不会在 `r` 结束后被使用；返回值不逃逸本地区域。
4. **原地精化**：`Unique(a)` 下，`set_in_place(a,i,v)` 与 `set_pure(a,i,v)` 对观察者等价。
5. **效果 soundness**：`E` 是实际效果的上界；`pure` 函数无副作用且确定性。
6. **擦除**：擦除 `own/borrow/region/proof` 后，可观察行为不变。

证明技术：

- 类型安全：progress/preservation；
- 所有权/借用：逻辑关系或分离逻辑；
- 区域：lifetime semantics / 语义模型；
- 原地：separation logic + 观察等价；
- 效果：effect semantics；
- 擦除：simulation。

## 9. 与当前原型的关系

当前 `src/ownership.rs` 只实现：

- `own` / `borrow` / 推断 unique；
- move 后使用、borrow 逃逸、borrow→own move 的拒绝；
- `set`/`push` 的原地/复制选择；
- 解释器/VM 统计。

它没有：

- `Ref`/`RefMut` 显式借用；
- 区域变量/约束/推断；
- 效果 row；
- `share` 的区域/复制语义（当前无 RC/GC）；
- 结构体/变体/切片；
- 函数所有权契约的完整检查；
- soundness 证明。

本文模型是后续目标；Rust 原型是参考实现，不是规格。

已知差异：

- 原型对 `set(borrow,...)` 走 copy；模型倾向于直接报错，要求显式 copy 或 `share`。
- 原型没有区域/效果/`borrow_mut`/`share`，因此不能表达真正的生命周期和并发读。
- 原型的 `Val` 参数返回时可能别名；模型要求 `NoEscape`/`share` 规则。

## 10. 下一步

0. **最小模型已机器检查**：`src/ownership_proof.rs` 用两位置堆证明了
   `set_result_same` 和 `set_frame`（`Unique h` 下原地更新对另一观察者不可见）。
1. 把本文的规则缩成一个**核心演算**（小步语义 + 类型/所有权规则）。
2. 选一个最小子集做 soundness 证明：
   - `own` + `borrow` + `set` 原地/复制；
   - 不含区域多态/效果 row。
3. 用内核深嵌入这个核心演算，证明：
   - progress/preservation；
   - `Unique` 下原地精化；
   - 检查器接受 ⇒ well-owned。
4. 再逐步加区域、效果、`&mut`、结构体。
