# Lowering 与精化模型

本文定义从表面语言到字节码之间的各层 IR、lowering 规则、精化关系，以及需要证明的义务。Rust 实现暂不展开。

## 1. IR 分层

```text
Surface
  纯函数式 + 精化 + own/borrow/region/effect + 证明
        ↓ elaboration
Core
  显式类型/证明项 + 所有权 facts + 区域约束
        ↓ erasure
Erased
  无 Prop / proof / 类型索引 / 生命周期
        ↓ ownership lowering
OwnIR
  moves / drops / borrows as aliases / in-place vs copy / checked vs unchecked
        ↓ ANF
ANF
  let-bound atoms; 显式求值顺序
        ↓ bytecode
Bytecode
  栈式指令
        ↓
VM / native
```

每层有自己的类型/不变量；层间用精化/翻译验证连接。

## 2. 各层不变量

### Surface

- 纯函数式语义（默认不可变）。
- `own`/`borrow`/`region`/effect 是静态约束。
- `set`/`push` 是纯操作；是否原地由 lowering 决定。

### Core

- 显式证明项；`Prop` 与 `Type` 分离。
- 所有权 facts：`Unique`、`Borrow`、`Live`、`Bounds`。
- 区域约束。
- 证明义务已生成/检查（或待检查）。

### Erased

- 删除：`Prop` 证明、proof 参数、精化谓词、类型索引、生命周期/区域、成本注解。
- 保留：运行时数据、`TypeRep`/`Layout`（若显式使用）、能力/句柄。
- 不变量：擦除不改变可观察行为。

### OwnIR

- 显式 `move`/`drop`。
- `borrow` 变成别名/指针（零运行时表示）。
- `set`/`push` 标记 `in_place`/`copy`。
- 数组访问标记 `checked`/`unchecked`。
- 不变量：每个 `own` 值恰好一个所有者；借用不逃逸；原地只在唯一时。

### ANF

- 每个子表达式绑定到原子变量；求值顺序显式。
- 便于常量折叠、DCE、CSE、字节码生成。
- 不变量：无嵌套副作用；每个 let 有类型。

### Bytecode

- 栈类型一致；局部变量类型一致；跳转目标合法。
- `checked=false` 有证明支持。
- `in_place` 有唯一性支持。

## 3. Lowering 规则

### 3.1 证明擦除

```text
erase(λ (x : Prop). e) = erase(e)
erase(f(e, p)) = erase(f)(erase(e))       -- p : Prop
erase({x : T | P}) = T
erase(Ref<r,T>) = Ptr T                   -- 或直接 T*
erase(Live(r)) = ∅
```

- `Prop` 证明、精化、类型索引、区域不进入运行时。
- `TypeRep`/`Layout` 只在运行时显式使用时保留。

### 3.2 所有权 lowering

```text
own x
  → move when consumed
  → drop at last use / region end

borrow(r) x
  → alias / pointer
  → no runtime representation
  → region end: invalidate

share x
  → 区域内只读别名，或复制（当前无 RC/GC）
```

### 3.3 `set` / `push`

```text
Unique(a) ∧ a 在续文中只出现一次
--------------------------------
set(a,i,v) → set_in_place(a,i,v)

q = share/copy
--------------------------------
set(a,i,v) → let a' = copy(a) in set_in_place(a',i,v)

q = borrow(r)
--------------------------------
set(a,i,v) → error
```

`push` 同理。

### 3.4 检查消除

```text
Bounds(i, len(a)) ∈ Δ
--------------------------------
a[i] → get { checked = false }

b ≠ 0 ∈ Δ
--------------------------------
a / b → div { checked = false }
```

否则 `checked = true`。

### 3.5 求值顺序 / ANF

```text
let x = e1 in e2
  → ANF: x = atom(e1); e2
```

- 纯表达式可重排；有 `io`/`write` 的保持顺序。
- `borrow` 不引入运行时操作，但限制重排。

## 4. 精化关系

### 4.1 源/擦除

```text
e ≈ erase(e)
```

观察等价：

```text
obs(e) = obs(erase(e))
```

`obs` 包括：

- 返回值；
- `print` 事件序列；
- 显式错误；
- 运行时资源统计（可选，用于 benchmark，不属于语义）。

证明：simulation；proof/region 擦除不改变求值。

### 4.2 原地精化

```text
Unique(H,l) ∧ 0 ≤ i < |H(l)|
----------------------------------------
set_in_place(H,l,i,v) ≈ set_pure(H,l,i,v)
```

`≈`：对所有仍可观察的句柄，结果相同。

这是“原地更新正确”的核心定理。

### 4.3 Lowering 精化

```text
OwnIR(e) ≈ Erased(e)
Bytecode(bc) ≈ OwnIR(e)
```

每个 pass 保持 `obs`。

### 4.4 编译正确性

```text
compile(e) = bc
e ⇓ v, H'
----------------
bc ⇓ v, H'
```

或 refinement：

```text
obs(bc) ⊑ obs(e)
```

## 5. 翻译验证

不建议一开始证明整个编译器。建议：

1. 每个 lowering pass 产出**证书**：
   - 源/目标程序的等价关系；
   - 或关键步骤的精化条件。
2. 一个小检查器验证证书。
3. 内核/验证器检查。

例子：

```text
pass: set_pure → set_in_place
certificate:
  Unique(a)
  a 不被其他句柄观察
  -------------------
  obs(set_in_place) = obs(set_pure)
```

如果无法证明，保留 `copy`/`checked`，退化为安全但慢的版本。

## 6. 证明义务清单

| 层 | 义务 |
|---|---|
| 证明擦除 | `obs(e) = obs(erase(e))` |
| 所有权 | `Unique` 保持；无 use-after-move；无借用逃逸 |
| 原地 | `Unique` 下 `set_in_place ≈ set_pure` |
| 检查消除 | `Bounds`/`NonZero` 下 unchecked 访问安全 |
| 区域 | 借用不越界；drop 在区域结束 |
| ANF | 求值顺序/效果保持 |
| 字节码 | 栈/局部/跳转/类型验证 |
| 编译 | simulation/refinement |
| VM | 语义正确；验证器 sound |

## 7. 例子：`fill`

```alethe
fn fill(own a: Array<Int>, x: Int) -> Array<Int> {
  set(a, 0, x)
}
```

Lowering：

```text
fill(a, x):
  require Unique(a)
  set_in_place(a, 0, x)
  return a
```

证明义务：

```text
Unique(a)
∧ a 未被其他句柄观察
--------------------------------
fill(a,x) ≈ pure_set(a,0,x)
```

如果 `a` 是 `share`：

```alethe
fn fill_shared(a: Array<Int>, x: Int) -> Array<Int> {
  set(a, 0, x)
}
```

Lowering：

```text
fill_shared(a, x):
  let a' = copy(a)
  set_in_place(a', 0, x)
  return a'
```

证明义务：

```text
fill_shared(a,x) ≈ pure_set(a,0,x)
```

两者都满足同一个 `spec`，但分配/复制不同。

## 8. 与 VM 模型的关系

- OwnIR 的 `set_in_place`/`set_copy` 对应字节码 `ArraySet { in_place }`。
- OwnIR 的 `checked`/`unchecked` 对应字节码 `ArrayGet { checked }`。
- ANF 的 let/atom 对应局部变量槽。
- 字节码验证器保证栈/局部/跳转安全。
- 编译正确性 = OwnIR/ANF → Bytecode 的 simulation。

## 9. 下一步

1. 把 Surface/Core/Erased/OwnIR 的语法和语义写成正式规则。
2. 先证明最小闭环：
   - proof erasure；
   - `Unique` 下 `set_in_place ≈ set_pure`；
   - `fill`/`fill_shared` 共享同一个 spec。
3. 再做 ANF/字节码验证。
4. 最后做完整编译正确性或 translation validation。
