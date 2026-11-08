# VM 模型：字节码、堆、语义、验证

本文是**模型规格**，不是 Rust VM 说明。目标是把字节码、堆、调用栈、检查/擦除/原地更新写成可证明的对象。

## 1. 设计目标

VM 模型要支持：

- 证明参数/证明值擦除后，运行时行为不变；
- `own`/`unique` 的原地更新 vs 共享复制；
- 边界/除零检查的有无；
- 字节码验证（栈/局部变量/跳转/类型）；
- 源语言到字节码的编译正确性（或翻译验证）。

## 2. 字节码

```text
Instr ::= push_i n
        | push_b b
        | push_unit
        | load x
        | store x

        | add | sub | mul | neg
        | div { checked }
        | mod { checked }
        | eq | ne | lt | le | gt | ge | not

        | jump l
        | jz l

        | call f n
        | ret

        | newarr n
        | get { checked }
        | set { checked, in_place }
        | pusharr { in_place }
        | len

        | print
        | pop
        | halt
```

- `checked` 是布尔标记；有证明时可为 `false`。
- `in_place` 由所有权检查/lowering 决定。
- `call f n`：`f` 是函数索引，`n` 是参数个数。

## 3. 值、堆、帧

```text
Value ::= Int(i) | Bool(b) | Unit | Loc(l)

Obj   ::= Array [Value]
        | Closure { code : f, env : [Value] }   -- 后续
        | ...

Heap  ::= Loc ⇀ Obj

Frame ::= (f : FunIdx, pc : Nat, L : [Value], S : [Value])

State ::= (H : Heap, Fs : [Frame])
```

- 数组是堆对象；`Loc` 是句柄。
- `own`/`share`/`borrow` 不在运行时值里；它们只影响 lowering。
- `Frame` 的 `L` 是局部变量槽，`S` 是操作数栈。

## 4. 栈类型

字节码验证使用栈类型：

```text
τ ::= Int | Bool | Unit | Loc
S ::= ε | S, τ
```

指令的栈效果：

| 指令 | 栈效果 |
|---|---|
| `push_i n` | `S → S, Int` |
| `push_b b` | `S → S, Bool` |
| `push_unit` | `S → S, Unit` |
| `load x` | `S → S, Γ(x)` |
| `store x` | `S, Γ(x) → S` |
| `add/sub/mul/div/mod` | `S, Int, Int → S, Int` |
| `neg` | `S, Int → S, Int` |
| `eq/ne/lt/le/gt/ge` | `S, Int, Int → S, Bool` |
| `not` | `S, Bool → S, Bool` |
| `jump l` | `S →`（目标处栈类型必须也是 `S`） |
| `jz l` | `S, Bool → S`（目标处栈类型为 `S`） |
| `call f n` | `S, τ1..τn → S, τret` |
| `ret` | `S, τret →` |
| `newarr n` | `S, τ..τ → S, Loc` |
| `get` | `S, Loc, Int → S, τ` |
| `set` | `S, Loc, Int, τ → S, Loc` |
| `pusharr` | `S, Loc, τ → S, Loc` |
| `len` | `S, Loc → S, Int` |
| `print` | `S, τ → S` |
| `pop` | `S, τ → S` |
| `halt` | `S, τ →` |

## 5. 小步语义

状态：

```text
(H, Fs) → (H', Fs')
```

当前帧：

```text
F = (f, pc, L, S)
```

### 基本指令

```text
code(f)[pc] = push_i n
----------------------------------------
(H, (f,pc,L,S) :: Fs) → (H, (f,pc+1,L,S,n) :: Fs)
```

```text
code(f)[pc] = load x      L[x] = v
----------------------------------------
(H, (f,pc,L,S) :: Fs) → (H, (f,pc+1,L,S,v) :: Fs)
```

```text
code(f)[pc] = store x     S = S', v
----------------------------------------
(H, (f,pc,L,S',v) :: Fs) → (H, (f,pc+1,L[x:=v],S') :: Fs)
```

### 算术/比较

```text
code(f)[pc] = add     S = S', a, b
----------------------------------------------------
(H, (f,pc,L,S) :: Fs) → (H, (f,pc+1,L,S',a+b) :: Fs)
```

溢出/除零：

```text
div { checked = true }     b = 0
----------------------------------------
(H, ...) → Error(DivZero)
```

`checked = false` 时，语义上仍不允许除以 0；编译器必须证明 `b ≠ 0`。

### 跳转

```text
code(f)[pc] = jump l
----------------------------------------
(H, (f,pc,L,S) :: Fs) → (H, (f,l,L,S) :: Fs)
```

```text
code(f)[pc] = jz l     S = S', false
----------------------------------------
(H, (f,pc,L,S',false) :: Fs) → (H, (f,l,L,S') :: Fs)
```

### 调用/返回

```text
code(f)[pc] = call g n     S = S', v1..vn
S'' = S'
--------------------------------------------------------------
(H, (f,pc,L,S) :: Fs)
  → (H, (g,0,[v1..vn],[]) :: (f,pc+1,L,S'') :: Fs)
```

```text
code(f)[pc] = ret     S = S', v
--------------------------------------------------------
(H, (f,pc,L,v) :: Fs) → (H, Fs)     -- 返回值 v 压回调用者
```

### 数组

```text
code(f)[pc] = newarr n     S = S', v1..vn
l ∉ dom(H)
H' = H, l ↦ Array [v1..vn]
----------------------------------------------------------
(H, (f,pc,L,S) :: Fs) → (H', (f,pc+1,L,S',l) :: Fs)
```

```text
code(f)[pc] = get { checked = true }
S = S', l, i      H(l) = Array xs      0 ≤ i < |xs|
----------------------------------------------------------
(H, ...) → (H, (f,pc+1,L,S',xs[i]) :: Fs)
```

越界：

```text
checked = true      i < 0 或 i ≥ |xs|
----------------------------------------
→ Error(Bounds)
```

`checked = false` 时，越界是未定义行为；编译器必须证明越界不可能（或者验证器拒绝）。

```text
code(f)[pc] = set { in_place = true }
S = S', l, i, v      H(l) = Array xs
H' = H, l ↦ Array xs[i:=v]
----------------------------------------------------------
(H, ...) → (H', (f,pc+1,L,S',l) :: Fs)
```

```text
code(f)[pc] = set { in_place = false }
S = S', l, i, v      H(l) = Array xs
l' fresh
H' = H, l' ↦ Array xs[i:=v]
----------------------------------------------------------
(H, ...) → (H', (f,pc+1,L,S',l') :: Fs)
```

`pusharr` 类似：`in_place` 修改/复制数组并追加。

### 观察

```text
trace : [Event]
Event ::= Print(Value) | Return(Value) | Error(...)
```

分配/复制/检查次数是**仪器统计**，不是语义；用于 benchmark/验证。

## 6. 字节码验证

验证判断：

```text
⊢ F ok
```

要求：

1. **局部变量**：所有 `load x` 有 `x ∈ dom(L)`；`store x` 类型匹配。
2. **栈类型**：每条指令前后栈类型一致；`add` 前两个是 `Int`；`get` 前是 `Loc, Int`。
3. **跳转**：`jump l`/`jz l` 的目标 `l` 在函数内，且目标处栈类型与跳转前一致。
4. **调用**：`call f n` 的 `n` 与 `f` 的参数个数/类型匹配；返回类型匹配。
5. **返回**：`ret` 时栈顶类型等于函数返回类型。
6. **数组**：`get/set` 的 `Loc` 类型正确；`newarr` 元素类型一致。

定理（目标）：

> 若 `⊢ P ok`，则 VM 执行不会出现栈下溢/上溢、类型混淆、非法跳转；只会出现显式错误（越界、除零、溢出）。

证明：每条指令保持 `⊢ F ok`；progress + preservation。

字节码验证把 VM 的信任面缩小到“验证器 + 语义”。

## 7. 编译关系

```text
compile : Core -> BcProgram
```

正确性目标：

```text
e ⇓ v, H'   ⇒   compile(e) ⇓ v, H'
```

或 refinement：

```text
obs(compile(e)) ⊑ obs(e)
```

关键子义务：

1. **证明擦除**：`Prop`/proof 参数/证明字段不进入字节码。
2. **所有权 lowering**：`set`/`push` 的 `in_place` 选择正确。
3. **检查消除**：`checked=false` 的访问有证明支持。
4. **调用约定**：参数顺序/局部槽/返回值一致。
5. **控制流**：`if`/短路/递归的字节码语义正确。

推荐做法：

- 每个 lowering pass 产出**翻译验证**条件；
- 或用 verified pass；
- 至少对 `in_place` 和 `checked=false` 做证明/证书。

## 8. 原地更新的精化

源语义（纯）：

```text
set_pure(H, l, i, v) = H, l' ↦ Array (H(l)[i:=v])
```

目标语义（原地）：

```text
set_in_place(H, l, i, v) = H, l ↦ Array (H(l)[i:=v])
```

精化定理（需要 `Unique`）：

```text
Unique(H, l)
∧ H(l) = Array xs
∧ 0 ≤ i < |xs|
------------------------------------------------
obs(set_in_place(H,l,i,v)) = obs(set_pure(H,l,i,v))
```

其中 `obs` 是“所有仍可观察的句柄/值”的观察等价。  
如果没有其他句柄指向 `l`，原地与复制对观察者不可区分。

## 9. 与当前 Rust VM 的关系

当前 `src/vm.rs`：

- 栈式字节码、帧、调用/返回、数组、原地/复制、统计；
- 有字节码验证器：栈高度/类型、局部变量、跳转、调用/返回、数组类型；
- 没有形式化语义，也没有验证器的 soundness 证明；
- 没有闭包/异常/GC/并发；
- 边界/除零检查始终打开（`checked=true`）。

本文模型是目标；Rust VM 是参考实现。

## 10. 下一步

1. 把字节码语法/栈类型/小步语义写成正式规则（本文已给骨架）。
2. 证明字节码验证器的 soundness，并给 progress/preservation。
3. 证明 `Unique` 下 `set_in_place` 精化 `set_pure`。
4. 再证明 `compile` 的 simulation，或先做 translation validation。
5. 闭包/异常/并发模型后置。
