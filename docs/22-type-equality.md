# 类型相等：语义同一、表示共享、值相等

核心结论：

> **二进制表示相同，不代表类型相同；类型相同，也不要求证明里展开到二进制。**

现实数据最终都是 bits，但 bits 只是**表示**。类型还承载解释、不变量和证明边界。
把“底层一样”直接当成“类型相等”，会让每个函数都重新证明解释合法，反而让证明
更复杂。正确做法是把相等分成几层。

## 1. 四层相等

### 1.1 定义等价（definitional equality）

内核判断两个项是否可互换：

```text
a ≡ b  :=  alpha/beta/delta/zeta/iota 归约后结构相同
```

- 透明定义/`type alias` 可以展开；
- `newtype`、不透明定义、归纳类型声明**不展开**；
- 两个字段完全相同的独立 `struct`/`inductive` 默认**不相等**。

这是内核的类型检查基础，必须可判定、尽量小。

### 1.2 语义类型同一（semantic identity）

回答“这两个类型在规格/证明里是不是同一个类型”。

建议默认：**nominal**。

```text
type UserId = newtype UInt64
type OrderId = newtype UInt64
type Char = newtype UInt8 invariant is_scalar
```

`UserId`、`OrderId`、`UInt64` 即使表示相同，也不是同一个语义类型。这样：

- `UserId` 不会误传给 `OrderId`；
- `Char` 的函数可以假设 `is_scalar`；
- invariant 只需要在构造/解析边界证明一次。

### 1.3 表示等价（representation equality）

回答“这两个类型能否共享同一套布局/编码/后端操作”。

```text
Repr A R
```

含义：`A` 可以用表示 `R` 实现，并带有：

- `to : A -> R`；
- `from : {r : R | Invariant r} -> A`；
- round-trip 证明；
- 必要的不变量证明。

`Repr Char UInt8` 可以让代码生成复用字节操作，但 `Char` 和 `UInt8` 在语义层
仍然不同。表示层是结构化的，语义层是 nominal 的。

### 1.4 值相等与类型等价

- `Eq A x y`：同一类型内两个值的命题相等；
- `A ≃ B`：类型同构，有双向转换和 round-trip 证明；
- `A <: B` / `Refines A B`：子类型/精化，不是相等；
- `Quotient A R`：显式按等价关系商掉，不是“bits 相同就相等”。

不要把 `Eq A x y`、`Repr A R`、`A ≃ B`、`A <: B` 混成一个“相等”。

## 2. 建议的默认策略

| 构造 | 语义身份 | 表示 | 是否隐式转换 |
|---|---|---|---|
| `type alias T = U` | 透明，可展开 | 同 `U` | 是（定义等价） |
| `newtype T = U` | nominal | 同 `U`，零成本 | 否；需显式构造/投影 |
| `opaque T` | nominal，隐藏内部 | 由实现决定 | 否 |
| `struct`/`record` | nominal 声明 | 结构化字段布局 | 否 |
| `inductive` | nominal 声明 | 结构化标签/字段 | 否 |
| 精化 `{x:T | P}` | 语义上细化 `T` | 同 `T` | 只能带证明转换 |
| `Repr A R` | 不改变语义身份 | 结构化共享 | 仅表示层 |
| `A ≃ B` | 同构但不等同 | 可不同 | 显式 `cast` |

原则：

1. **语义层默认 nominal**，只有显式 `type alias` 才透明。
2. **表示层结构化**，用 `Repr`/layout 共享，不用它替代类型相等。
3. **跨 nominal 类型必须显式转换**；转换要么是构造/投影，要么带证明。
4. **不变量放在类型边界**，不要在每个函数里重复证明。
5. **需要“同构共享”时用 `≃` 或 `Repr`，不要改类型相等**。

## 3. 解析、字符与数值

“同样 8 个 bit，有时当数值，有时当字符”不是类型相等的证据，而是**解释不同**：

```text
parse_char : Bytes -> Result<Char, DecodeError>
char_code  : Char -> UInt8
Repr Char UInt8
```

- `UInt8 -> Char` 不是恒等函数，需要证明 `is_scalar`；
- `Char -> UInt8` 可以零成本投影；
- `parse_char` 是边界，负责建立不变量；
- 内部函数拿到 `Char` 后不必再检查字节合法性。

如果反过来把 `Char` 等同于 `UInt8`，那么所有使用 `Char` 的地方都要重新检查
合法性，证明负担反而爆炸。

## 4. 分布、统计与模型

“数据分布不同”也不应进类型相等。建议作为**模型/精化**：

```text
type Probability = {p : Float | 0 <= p <= 1}
type Sorted<T>  = {xs : Array<T> | is_sorted xs}
type Normalized = {xs : Array<Float> | sum xs == 1}
type Model<T>   = ...
```

或单独的 `Distribution<T>`、`Sample<T>`、`Belief<T>` 类型。

- 分布是值的属性/模型，不是表示；
- 进入优化或统计算法时，显式假设/证明；
- 不要为了“分布相同”把两个领域类型判等；
- 如果确实要在等价关系下识别类型/值，用 `Quotient`，并明确该等价关系。

## 5. 证明复杂度怎么控制

1. **不变量局部化**：`newtype` 智能构造器 + 私有字段；内部函数直接假设不变量。
2. **表示证明复用**：`Repr A R` 只证一次，所有底层操作共享；不要每个函数展开 bits。
3. **参数化表示代码**：底层算法可以写成 `Repr` 多态，语义层仍用 nominal 类型。
4. **显式 `cast`**：跨语义类型/表示边界时写清转换和证明，编译器不猜。
5. **不可变 `alias` 与 `newtype` 分开**：别名用于缩写，newtype 用于身份。
6. **需要同构时用 `≃`**：不要改定义等价，避免内核膨胀。
7. **分布/模型用谓词**：用精化或独立逻辑层，不塞进类型相等。

这样证明主要围绕**语义类型**写；只有表示 lowering 需要处理 bits。

## 6. 候选语法

```text
type alias Index = UInt32          -- 透明，定义等价
type UserId = newtype UInt64       -- nominal，零成本
type Char = newtype UInt8 invariant is_scalar
type Sorted<T> = {xs : Array<T> | is_sorted xs}

Repr Char UInt8
Repr UserId UInt64
Repr Sorted<T> (Array<T>)          -- 表示相同，但语义不同

fn parse_char(b : UInt8) -> Result<Char, DecodeError>
fn char_code(c : Char) -> UInt8
```

转换只有三种：

- `alias`：直接展开；
- `newtype`：构造/投影；
- `Repr`/`≃`/精化：显式 `cast`，带证明或运行时检查。

## 7. 开放问题

1. `newtype` 是否允许定义等价透明？建议默认不透明，提供 `transparent newtype` 选项。
2. `Repr A R` 的 `from` 是偏函数还是带 `Invariant` 的精化输入？
3. `struct`/`record` 是否提供结构化兼容（row subtyping），还是完全 nominal？
4. `≃` 的证明义务：只需要 round-trip，还是还要保持 `Repr`/不变量？
5. 类型类/trait/接口是 nominal 还是结构化？建议 nominal + 显式字典。
6. 分布/统计模型放在精化、独立逻辑层，还是内核的 `Model` 类型？
7. `Quotient` 的最小设计；是否会污染定义等价？
8. 表示层能否对用户可见 `cast` 到另一个 nominal 类型，还是只允许在 `Repr` 模块内？
