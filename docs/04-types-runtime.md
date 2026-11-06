# 类型与运行时的交错

你提的关键点是：**有的类型直接可擦除，有的类型可以在运行时修改。**  
这不是一个“类型系统 vs 运行时”的二选一，而是两层之间的**精化/重化**关系。

## 1. 两层模型

### 静态层（可擦除）

- `Prop` 证明；
- 精化谓词；
- 生命周期/区域索引；
- 只用于类型的索引（如 `Vec a n` 的 `n`）；
- 成本/效果注解；
- 线性/唯一性约束。

这些在运行时**没有表示**。它们的存在是为了让编译器/内核知道事实。

### 运行时层（可重化）

- 数据：整数、数组、闭包、ADT；
- 类型描述：`TypeRep T`、`Layout`、`Dynamic`；
- 能力/权限：`Cap`、`Region`；
- 动态标签：`Tag`、`Schema`；
- 资源句柄：文件、锁、设备。

这些是普通值，有运行时表示，可以被检查、传递、修改——但有类型/不变量约束。

### 桥

```alethe
type TypeRep (T : Type) : Type

reify   : (T : Type) -> TypeRep T
reflect : (r : TypeRep T) -> Proof (r represents T)

cast : (r : TypeRep T) -> Dynamic -> Option T
```

- `reify T` 在运行时构造一个类型描述；如果代码不需要它，擦除分析会删掉。
- `TypeRep T` 的不变量：它描述的布局/构造子必须与 `T` 一致。
- `cast` 是**受检查**的动态转换：要么运行时检查标签/布局，要么要求一个 `r represents T` 的证明。
- 如果类型描述可以在运行时修改（例如反序列化 schema、热更新布局），修改操作必须保持不变量，或者退化成 `Dynamic` + checked cast。

## 2. 用量/擦除标注

用数量（quantity）表达“这个参数在运行时是否出现”：

```text
0   -- erased: 证明、索引、生命周期、成本注解
1   -- linear: 必须恰好使用一次的资源
ω   -- unrestricted: 普通运行时数据
```

示例：

```alethe
fn get(a: Array<Int>, i: Int, h: proof (0 <= i && i < len(a))) -> Int
```

- `h` 是 `proof`，quantity 0；擦除后 `get` 的运行时参数只有 `(a, i)`。
- `i` 是 `Int`，quantity ω；运行时保留。
- `a` 是 `Array<Int>`，quantity ω；运行时保留。

`Vec`：

```alethe
type Vec (a : Type) (n : Nat) : Type

-- 运行时只保留底层数组；n 和证明被擦除
fn vget (v : Vec a n) (i : Fin n) -> a
```

- `n` 只出现在类型中 → 擦除；
- `Fin n` 的证明部分 → 擦除，运行时就是一个整数索引；
- 数组本身保留。

**部分擦除**：`Sigma`/存在类型可能保留 witness、擦除 proof；`Vec` 保留数据、擦除长度。擦除分析按“是否在运行时被使用”逐参数决定，而不是整个类型一刀切。

## 3. 运行时类型描述

有些场景必须把类型带到运行时：

- 序列化/反序列化；
- 反射/动态派发；
- FFI/ABI；
- 热更新/插件；
- 泛型容器在运行时需要布局信息；
- 错误信息/调试。

设计：

```alethe
type Repr (T : Type) : Type     -- 布局/表示描述
type TypeRep (T : Type) : Type  -- 类型描述，携带 Repr

data Dynamic = Dynamic (r : TypeRep T) T   -- 存在类型
```

- `Dynamic` 的值必须携带自己的类型描述；使用前要 `cast`。
- `cast` 不会“凭空信任”：要么比较描述符（运行时检查），要么要求证明 `r` 与目标 `T` 相等。
- 如果描述符是可变的，类型系统要求修改操作维护不变量；否则只能得到 `Dynamic`，不能直接当 `T` 用。

这把“运行时可以修改类型”限制在**显式的动态边界**，不会污染普通代码的擦除和优化。

## 4. 渐进验证：三档

```text
verified  -- 有内核检查的证明；运行时零检查
checked   -- 没有证明，但有运行时检查；安全但慢
trusted   -- 显式 axiom/unsafe；CI 可禁止
```

- `verified`：`requires 0 <= i < len(a)` 被证明，`a[i]` 无检查。
- `checked`：没有证明，`a[i]` 保留边界检查。
- `trusted`：例如 `unsafe`/FFI；必须显式标记，审计。

可以逐函数/逐模块配置；release 构建可以要求热路径 `verified`，边缘允许 `checked`。

## 5. 内存：纯表面，唯一性 lowering

### 表面语义

```alethe
fn push(xs: Array<Int>, x: Int) -> Array<Int> {
  append(xs, [x])       -- 纯函数式：返回新数组
}
```

### 优化器 lowering

如果 `xs` 是**唯一**的（没有别名、旧值不再被观察），可以变成原地：

```text
push(xs, x):
  write(xs, len(xs), x)
  return xs
```

这不是“另一个语言”，而是同一个语言的**运行时 IR**。源码保持纯函数式，优化器在证明唯一性后引入 mutation。

### 唯一性/线性

- `own`：唯一所有权，可原地改；移动后不能再用。
- `borrow`：临时只读/只写借用；不能逃逸；不占运行时表示。
- `shared`：共享，复制或 RC。
- 默认在函数内部推断；API 边界显式。
- 唯一性判定：一个值只有一个所有者、没有 live borrow、旧值在修改后不再被观察。

### 无 GC 的组合

| 机制 | 作用 | 代价 |
|---|---|---|
| 区域推断 | 分配在区域，最后一次使用后释放 | 需要区域约束求解 |
| 唯一性/线性 | 原地更新，避免复制 | 需要所有权分析 |
| RC/Perceus | 共享持久数据 | 引用计数开销，可复用优化 |
| 栈分配 | 不逃逸的值 | 逃逸分析 |

不是“纯栈分配”才能无 GC；现实路线是区域 + 唯一性 + RC 兜底。

## 6. 多 IR，不是多语言

```text
Surface（纯函数式 + 精化 + 证明）
   ↓ elaboration
Core（依赖类型内核；证明项）
   ↓ erasure
Erased IR（无 Prop/索引/生命周期）
   ↓ lowering + optimization
Runtime IR（所有权、原地更新、单态化、unboxed）
   ↓ representation
Repr IR（字长、布局、进制、SIMD）
   ↓
VM / native
```

- **一个表面语言**，多层语义。
- 每层有自己的类型/不变量；层间用精化/翻译验证连接。
- 手写底层代码是**可选的 representation 层**，不是第二门语言；它必须 `refines` 高层 spec。
- `sum_fast` 的原地版本属于 Runtime/Repr 层；它的正确性由源层 `sum_fast = sum_ref` + lowering 的精化保证。

## 7. 硬件相关怎么靠 spec 保证

核心思想：**spec 写在语义层，不写硬件细节；表示层证明自己精化语义。**

- `Int` 是数学整数；`i64` 是表示，需要范围证明或溢出语义。
- `Array` 是序列；packed/SoA/AoS 是表示，需要布局精化。
- 进制/格式：如果程序观察它（`toString`、序列化、协议），把它写进 spec；否则内部表示可自由选择。
- 硬件相关行为（MMIO、SIMD 舍入、缓存一致性）用显式 `Device`/`Unsafe` 类型和单独的逻辑层建模；默认脚本代码不接触。
- 优化器选择表示时，要么：
  1. 证明表示精化（`Repr T R`）；
  2. 对每次 lowering 做翻译验证；
  3. 或者把该 pass 标记为 trusted（不推荐用于安全关键代码）。

详见 `06-hardware-independence.md`。

## 8. 对当前内核的影响

内核只处理静态层：

- `Prop`/`Type`、证明项、定义等价；
- 还没有擦除、没有运行时类型描述、没有所有权。

下一阶段：

1. 擦除 pass：`Prop` 参数/证明字段/类型索引。
2. `TypeRep`/`Dynamic` 作为库原语，内核只提供类型和证明规则。
3. 唯一性/区域作为独立检查层，输出 facts 给优化器。
4. `refines` 连接 Surface/Core 与 Runtime/Repr。

---

## 9. 当前实现：own / borrow / unique 检查器

代码：`src/ownership.rs`；demo：`src/bin/ownership_demo.rs`；示例：`examples/ownership.alt`。

已实现：

- `own` 参数：唯一所有权；可以 move；可原地更新；move 后使用报错。
- `borrow` 参数：只读；不能 move 进 `own` 参数；不能通过返回值逃逸；不能存进数组。
- `unique`：局部变量绑定到 fresh array / `set` / `push` 结果时推断为 unique。
- `set` / `push` lowering：
  - 唯一 + 只出现一次 → `__set_in_place` / `__push_in_place`；
  - 共享/借用/多次出现 → `__set_copy` / `__push_copy`。
- 解释器统计：`allocations` / `copies` / `in_place` / `bounds_checks`。

示例输出：

```text
ownership check passed
main = 30
stats:
  allocations:   5
  copies:        2
  in-place ops:  2
  bounds checks: 9
```

演示的程序同时包含：

```alethe
fn fill(own a: Array<Int>, x: Int) -> Array<Int> {
  set(a, 0, x)          // in-place: own + 只使用一次
}

fn fill_shared(a: Array<Int>, x: Int) -> Array<Int> {
  set(a, 0, x)          // copy: 参数是共享 Val
}

fn read_borrow(borrow a: Array<Int>) -> Int {
  a[0] + a[1] + a[2]    // 只读，不消耗
}
```

错误用例（单元测试覆盖）：

```alethe
fn bad(own a: Array<Int>, x: Int) -> Int {
  let b = fill(a, x);
  len(a)                // error: use of moved value
}

fn leak(borrow a: Array<Int>) -> Array<Int> {
  a                     // error: borrow escapes
}

fn bad2(borrow a: Array<Int>, x: Int) -> Int {
  let b = fill(a, x);   // error: cannot move borrow into own parameter
  len(b)
}
```

### 当前限制

- 没有真正的区域/生命周期推断；`borrow` 只是函数参数级别的只读约束。
- 没有显式借用表达式（`&x`/`&mut x`），只有参数模式。
- 没有路径敏感分析；`if` 分支里 move 会保守地视为之后已 move。
- 没有 reborrow/生命周期子类型；`Borrow` 不能嵌套成结构体字段。
- 没有跨函数效果/别名协议；`Val` 参数返回时可能别名，当前保守地不标 unique。
- 没有和依赖类型内核连接；这是独立的表面层原型。
- 没有证明“降级成原地”等价；当前是可信 lowering，后续用翻译验证/精化证明。

下一步：

1. 把所有权 facts（unique/borrow/bounds）输出给优化器；
2. 加显式 `&`/`&mut` 与区域变量；
3. 把 `own`/`borrow` 接到内核的 `Prop`/证明，做“原地更新 = 纯语义”的等价证明；
4. 用 `refines` 把 `fill`/`fill_shared` 绑定到同一个 spec。


> 更完整的检查器模型（`borrow_mut`/`share`/区域/效果/结构体/唯一性规则/soundness 义务）见 `11-checker-model.md`。
