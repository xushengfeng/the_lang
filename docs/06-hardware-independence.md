# 硬件无关与表示精化

目标：**源码非常脚本化、硬件无关；内存布局、字长、进制等表示由优化器选择，但正确性由 spec/精化保证。**

这要求把“语义”和“表示”分开，而不是在源码里到处写 `i64`、`packed`、`little-endian`。

## 1. 语义层

默认类型是数学/抽象对象：

```alethe
Int            -- 数学整数（不是 i64）
Nat            -- 自然数
Array<Int>     -- 序列，不承诺布局
List<Int>      -- 链表/抽象序列
Map<K, V>      -- 映射，不承诺哈希/树
String         -- 文本，不承诺编码
```

语义层的公理/规格：

- `Int` 满足佩亚诺算术/整数环；
- `Array` 支持 `len`、`get`、`set`，有序列公理；
- `Map` 满足查找/插入/删除的映射公理；
- 不暴露地址、对齐、字节序、缓存行。

脚本式代码只写语义层：

```alethe
fn sum(a: Array<Int>) -> Int
  ensures result == sum_math(a)
{
  ...
}
```

## 2. 表示层

优化器/底层代码选择表示：

```alethe
type I64
type U32
type F64
type PackedArray<T>
type SoA<T>
type AoS<T>
type Bits<n>
type Device
type Unsafe
```

表示层有自己的不变量：

- `I64` 表示 `[-2^63, 2^63-1]` 的整数；
- `U32` 表示 `[0, 2^32-1]`；
- `PackedArray<T>` 表示连续布局的序列；
- `SoA<T>` 表示字段分离的布局；
- `Bits<n>` 表示定宽位向量；
- `F64` 表示 IEEE-754 双精度（有舍入/NaN 语义）。

## 3. 表示精化

核心关系：

```alethe
Repr (T : Type) (R : Type) : Prop
```

含义：表示 `R` 在可观察行为上实现语义 `T`。

例子：

```alethe
-- Int 用 I64 表示，前提是所有值在范围内
axiom repr_i64 :
  forall (x : Int),
    -2^63 <= x < 2^63 ->
    Repr Int I64

-- Array<Int> 用 PackedArray<I64> 表示
axiom repr_packed :
  forall (a : Array<Int>),
    (forall i, 0 <= i < len(a) -> in_i64_range(a[i])) ->
    Repr (Array Int) (PackedArray I64)
```

实际系统中，这些不是“随便 axiomatize”，而是：

1. **证明**：表示层的不变量 + 抽象函数（`to_math`/`from_math`）+ 精化定理；
2. **翻译验证**：每次 lowering 产出“源程序 = 目标程序”的证明/证书；
3. **运行时检查**：无法静态证明时，插入范围/对齐检查，退化为 `checked`。

`Repr` 不是信任的借口；它是需要证明的义务。

## 4. 布局优化

源层的 `Array<T>` 可以被 lowering 成：

| 表示 | 适用 | 代价 |
|---|---|---|
| `PackedArray<T>` | 顺序遍历、缓存友好 | 插入/删除贵 |
| `SoA<T>` | 按字段批量处理 | 单对象访问贵 |
| `AoS<T>` | 对象整体访问 | 字段批量处理差 |
| `List<T>` | 频繁插入/共享 | 遍历/缓存差 |
| `HashMap`/`BTree` | 查找 | 内存/常数 |

选择由优化器/AI 提出，由 `Repr` 精化或 benchmark 约束。源码不需要改。

## 5. 字长与进制

- `Int` 是数学整数；`i64`/`u32`/`bigint` 是表示。
- 如果程序可能溢出：
  - 源语义要么定义为数学整数（编译器必须证明不溢出，或插入检查/提升到 bigint）；
  - 要么显式用 `Word<n>`（模运算）并接受其语义。
- **进制**（二进制/十进制/其他）：
  - 内部表示可自由选择；
  - 如果程序观察它（`toString`、序列化、协议、UI），把它写进 spec；
  - 否则优化器可以改，只要不改变可观察输出。
- 浮点：`F64` 的舍入/NaN 是语义的一部分；不能用数学实数随便替换，除非 spec 允许误差。

## 6. 硬件相关代码

默认脚本代码不接触硬件。需要时用显式层：

```alethe
unsafe fn mmio_read(addr: Address) -> U32
  requires aligned(addr, 4)
  requires in_device_range(addr)
  ensures result == device_value(addr)
```

- `Device`/`Unsafe` 有单独的逻辑（分离逻辑/权限/volatile 语义）；
- 调用边界要求证明/检查；
- 普通代码只能通过安全封装使用；
- 这些函数的 spec 可以提到地址、对齐、volatile，因为它们是**可观察硬件行为**。

## 7. Lowering 的正确性

三种做法，按成本递增：

1. **Trusted lowering**：优化器/后端可信；快，但 TCB 大。适合早期原型。
2. **Translation validation**：每次 lowering 产出源/目标等价证明，内核/检查器验证；灵活，适合 AI 优化。
3. **Verified compiler**：一次性证明整个 pass/后端（CompCert/CakeML 风格）；最强，但工程量大。

建议路线：

- 语义层/`refines`：内核证明（当前）。
- 关键 lowering（原地更新、布局变换）：translation validation。
- 后端：先用 trusted VM/Cranelift，再考虑验证后端。

## 8. 当前阶段不做

- 完整的布局/进制优化；
- SIMD/缓存/并发内存模型；
- FFI/ABI；
- 浮点形式化；
- 验证后端。

先把语义层、内核、`refines` 跑通；表示层是后续 TODO。

## 9. 设计约束

- 源码不得依赖未写进 spec 的表示细节。
- 优化器不得改变可观察行为/资源上界。
- 任何表示选择都要有 `Repr` 精化、翻译验证或显式 `trusted` 标记。
- 如果无法证明，保留运行时检查（`checked`），而不是静默优化。
- benchmark 只用于选择，不用于证明正确性。
