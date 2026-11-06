# 小内核设计

当前实现：`src/dependent.rs`。目标是一个**类 Lean 的依赖类型内核**，足够小、可审计，能检查真实归纳证明，而不是一个完整证明助手。

## 1. 设计目标

- **小**：一个文件、少量构造子、规则清晰。
- **可检查**：所有证明都是项，内核 `check` 通过才算数。
- **可擦除**：`Prop` 证明和类型索引最终不进入运行时（擦除是下一阶段）。
- **可扩展**：后续加表面语法、元变量、tactic、用户定义 inductive、宇宙多态。
- **不信任提供者**：SMT/LLM/搜索器只产生项或战术脚本；内核是唯一裁判。

## 2. 项与上下文

```text
Term ::= Var(i)                 -- de Bruijn index
       | Sort(u)                -- 0 = Prop, 1 = Type, 2 = Type 1, ...
       | Const(name)
       | App(f, a)
       | Lam(x, A, b)
       | Pi(x, A, B)
       | Let(x, A, v, b)
       | Nat(n)                 -- 字面量糖，whnf 展开为 zero/succ
```

上下文：

```text
Γ ::= [] | Γ, (x : A)
```

- 变量用 de Bruijn index；`Var(0)` 是最近绑定的变量。
- 类型存储在上下文中，查找时按 `i + 1` 移位（因为类型是在绑定前的上下文中写的）。
- 绑定名只用于 pretty print 和表面解析；内核不依赖名字。

## 3. 类型规则（非形式）

```text
Γ ⊢ Sort(u) : Sort(u+1)

Γ ⊢ A : Sort(u)    Γ, x:A ⊢ B : Sort(v)
----------------------------------------
Γ ⊢ Pi(x, A, B) : Sort(pi_level(u, v))

Γ ⊢ A : Sort(u)    Γ, x:A ⊢ b : B
----------------------------------
Γ ⊢ Lam(x, A, b) : Pi(x, A, B)

Γ ⊢ f : Pi(x, A, B)    Γ ⊢ a : A
--------------------------------
Γ ⊢ App(f, a) : B[x := a]

Γ ⊢ A : Sort(u)    Γ ⊢ v : A    Γ, x:A ⊢ b : B
----------------------------------------------
Γ ⊢ Let(x, A, v, b) : B[x := v]
```

`pi_level`：

- 如果 codomain 是 `Prop`（`Sort(0)`），整个 Pi 在 `Prop`（impredicative）；
- 否则 `max(u, v)`。

`Nat(n)` 的类型是 `Nat`。

### 定义等价

`def_eq(a, b)`：

1. 两边 `whnf`；
2. 结构比较 `Var`/`Sort`/`Const`/`Nat`/`App`/`Lam`/`Pi`/`Let`；
3. `Lam`/`Pi` 比较 domain 和 body（de Bruijn，天然 alpha 等价）。

`whnf` 做：

- **beta**：`App(Lam(x,A,b), a) → b[x:=a]`；
- **delta**：可归约定义展开（`Nat.add`、`sum_ref`、`sum_fast`、`Eq.symm`、`Eq.trans`、`congr_succ`）；
- **zeta**：`Let` 展开；
- **iota**：`Nat.rec`/`Nat.recProp`、`List.rec`/`List.recProp` 在构造子上归约；
- **Eq.subst**：`h` 是 `Eq.refl`（或 `a`/`b` 定义等价）时归约为 `pa`；
- **Nat 字面量**：`Nat(0) → Nat.zero`，`Nat(n) → succ (Nat(n-1))`。

不透明定义（`reducible = false`）只暴露类型，不展开；定理/引理默认不透明，避免证明项膨胀。

## 4. 内置类型

当前内置：

```text
Nat : Type
Nat.zero : Nat
Nat.succ : Nat -> Nat

List : Type -> Type
List.nil  : (α : Type) -> List α
List.cons : (α : Type) -> α -> List α -> List α

Eq : (α : Type) -> α -> α -> Prop
Eq.refl : (α : Type) -> (a : α) -> Eq α a a
Eq.subst : (α : Type) -> (motive : α -> Prop) ->
           (a b : α) -> Eq α a b -> motive a -> motive b
```

Recursor：

```text
Nat.rec :
  (motive : Nat -> Type) ->
  motive 0 ->
  ((n : Nat) -> motive n -> motive (succ n)) ->
  (n : Nat) -> motive n

Nat.recProp :
  (motive : Nat -> Prop) -> ...  -- 同样的形状，目标是 Prop

List.rec :
  (α : Type) ->
  (motive : List α -> Type) ->
  motive nil ->
  ((x : α) -> (xs : List α) -> motive xs -> motive (cons x xs)) ->
  (xs : List α) -> motive xs

List.recProp :
  (α : Type) -> (motive : List α -> Prop) -> ...
```

`recProp` 是为了在**没有宇宙多态**的情况下，既能用 recursor 写计算函数（motive 返回 `Type`），又能写归纳证明（motive 返回 `Prop`）。这是原型阶段的工程折中；正式版应换成宇宙多态 recursor。

### 派生

```text
Eq.symm  : ∀ α a b, Eq α a b -> Eq α b a
Eq.trans : ∀ α a b c, Eq α a b -> Eq α b c -> Eq α a c
congr_succ : ∀ a b, Eq Nat a b -> Eq Nat (succ a) (succ b)
```

它们都是可归约定义，最终展开到 `Eq.subst`/`Eq.refl`。

## 5. Demo：两个 sum 实现

```text
sum_ref : List Nat -> Nat
sum_ref []       = 0
sum_ref (x :: xs) = sum_ref xs + x

sum_fast_go : List Nat -> Nat -> Nat
sum_fast_go []       acc = acc
sum_fast_go (x :: xs) acc = sum_fast_go xs (x + acc)

sum_fast : List Nat -> Nat
sum_fast xs = sum_fast_go xs 0
```

`sum_ref` 是可读的结构递归；`sum_fast` 是尾递归/累加器版本，计算顺序不同。

证明链：

```text
add_assoc : ∀ a b c, (a + b) + c = a + (b + c)
add_zero  : ∀ n, n + 0 = n

go_eq : ∀ xs acc,
  sum_fast_go xs acc = sum_ref xs + acc

sum_fast_eq : ∀ xs,
  sum_fast xs = sum_ref xs
```

`go_eq` 对 `xs` 归纳：

- `[]`：`go [] acc = acc`，`sum_ref [] + acc = 0 + acc = acc`；
- `x :: xs`：
  `go (x::xs) acc = go xs (x + acc)`
  `= sum_ref xs + (x + acc)`（归纳假设）
  `= (sum_ref xs + x) + acc`（`add_assoc` 反向）
  `= sum_ref (x::xs) + acc`。

`sum_fast_eq`：

```text
sum_fast xs
= sum_fast_go xs 0
= sum_ref xs + 0       (go_eq)
= sum_ref xs           (add_zero)
```

内核输出：

```text
sum_ref  [1,2,3,4,5] = 15
sum_fast [1,2,3,4,5] = 15

✓ add_assoc
✓ add_zero
✓ go_eq
✓ sum_fast_eq
```

错误证明（用 `Eq.refl` 证明 `sum_ref xs = sum_fast xs`）被拒绝，因为两边不是定义等价。

## 6. 信任边界

当前 TCB（trusted computing base）：

1. `src/dependent.rs` 的类型检查器、归约器、`def_eq`；
2. 内置声明（`Nat`/`List`/`Eq` 的类型、recursor 类型、`Eq.subst` 类型）；
3. 归约规则（beta/delta/zeta/iota、`Eq.subst`、Nat 字面量）；
4. Rust 编译器/运行时/硬件。

不在此列：

- demo 中构造证明的 Rust 代码（它只是生成项；项被内核检查）；
- 未来的 tactic、SMT、LLM（它们生成项/脚本，内核检查）；
- 未来的优化器（它产出代码 + 精化证明，或者经过翻译验证）。

### 已知限制

- 没有元变量/统一，证明项必须显式构造；
- 没有 tactic 语言，demo 用 Rust 构造项；
- 没有证明擦除/运行时 IR；
- 没有终止性检查；当前“递归”都用 recursor 表达，定义本身是非递归的；
- 没有用户定义 inductive，只有 `Nat`/`List`/`Eq`；
- `recProp` 是临时宇宙特化；
- 没有证明无关性（proof irrelevance）规则；
- 没有 `Eq.rec`/J 的通用形式，只有 `Eq.subst`；
- 没有并发/浮点/unsafe 语义。

### 下一步

1. **表面项语法 + 元变量 + hole**：`_`/`?`，`infer` 产生约束，统一后填洞。
2. **Tactic 语言**：`intro`、`exact`、`apply`、`induction`、`rw`、`simp`；tactic 只构造项，内核检查。
3. **证明擦除**：`Prop` 参数、证明字段、类型索引不进入运行时。
4. **`Vec`/`Fin`**：长度索引数组、边界证明、`get : Vec a n -> Fin n -> a`。
5. **用户定义 inductive**：通用声明、构造子、recursor 生成。
6. **宇宙多态**：去掉 `recProp`。
7. **JSON goal**：给 AI 的 goal/context/hole 接口。
8. **`refines`**：两个实现共享同一个 spec，生成精化义务。
9. **翻译验证**：优化 pass 产出等价证明或可检查证书。

## 7. 代码地图与运行

```bash
cargo test
cargo run --bin sum_demo
```

- `Term`/`Env`/`infer`/`check`/`whnf`/`def_eq`：`src/dependent.rs`
- demo：`src/bin/sum_demo.rs`
- 单元测试：`src/dependent.rs` 底部 `tests`

测试覆盖：

- 环境构建；
- `sum_fast_eq` 检查；
- 两个 sum 计算结果一致；
- 错误证明被拒绝。
