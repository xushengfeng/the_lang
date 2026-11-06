# 验证风格对照：A/B/C/D/E

同一个问题：**数组/列表求和，证明不越界、结果等于数学和。**  
下面用五种风格写，重点看“你写什么、机器替你做什么、运行时付什么代价”。

> 命名说明：这里的 A 是“精化类型 + SMT”，B 是“依赖类型 + 内核”，C 是“契约 + 分离逻辑”，D 是“spec/impl 双层”，E 是混合。  
> “拆分 vs 混合”是文件组织维度，与 A/B/C/D 正交；E 同时支持两者。

---

## A. 精化类型 + SMT（Dafny / Verus）

```dafny
function Sum(a: seq<int>, lo: int, hi: int): int
  requires 0 <= lo <= hi <= |a|
  decreases hi - lo
{
  if lo == hi then 0 else a[lo] + Sum(a, lo + 1, hi)
}

method SumArray(a: array<int>) returns (s: int)
  requires a != null
  ensures s == Sum(a[..], 0, a.Length)
{
  var i := 0;
  s := 0;
  while i < a.Length
    invariant 0 <= i <= a.Length
    invariant s == Sum(a[..], 0, i)
  {
    s := s + a[i];   // 验证器知道 i < a.Length
    i := i + 1;
  }
}
```

**你写**：循环不变式、前后条件、偶尔的引理。  
**机器做**：把 `i < a.Length` 等交给 SMT；证明通过后可以在优化阶段删检查。  
**代价**：SMT 常返回 `unknown`；量词/归纳/非线性需要人工引理；证明过程通常不能重建成小内核证明项（信任 Z3 + 编码器）。  
**性能**：Dafny 后端有 GC/运行时；Verus 用 Rust 所有权，性能接近 Rust，但是否删掉 Rust 的 bounds check 取决于代码/优化器，不是自动的。

Verus 片段：

```rust
fn div(a: u64, b: u64) -> u64
    requires b != 0,
    ensures result == a / b,
{
    a / b
}
```

**适合**：业务不变量、范围、空值、状态机；日常代码最像“普通程序 + 注解”。

---

## B. 依赖类型 + 内核（Lean / Idris）

```lean
def sumVec : {n : Nat} → Vector Nat n → Nat
  | _, .nil => 0
  | _, .cons x xs => x + sumVec xs

def getVec : (v : Vector α n) → Fin n → α
  | .cons x _, 0 => x
  | .cons _ xs, i + 1 => getVec xs i
```

```idris
div : (a, b : Nat) -> {auto h : NonZero b} -> Nat
div a b = a `div` b

get : (v : Vect n a) -> (i : Fin n) -> a
get (x :: xs) FZ = x
get (x :: xs) (FS i) = get xs i
```

**你写**：类型、递归函数、证明（项或 tactic）。  
**机器做**：内核检查每个证明项；`Prop` 证明和类型索引被擦除；`Fin n` 运行时就是一个整数。  
**代价**：学习曲线陡；`Fin`/`Vector` 会渗透签名；复杂归纳仍需手写；Lean 的 `Nat` 除法在 0 上有定义（返回 0），非零是规格而非防崩溃。  
**性能**：证明擦除；Lean 有 RC/编译器优化；高层代码仍可能有运行时开销。

**适合**：算法正确性、数据结构不变量、需要最强表达力的库。

---

## C. 契约 + 分离逻辑（Dafny / Why3 / Iris / Steel）

```coq
Lemma sum_spec (a : loc) (xs : list Z) :
  {{{ a ↦∗ xs }}}
    sum #a #(length xs)
  {{{ (s : Z), RET #s; ⌜s = sum_list xs⌝ }}}.
```

`a ↦∗ xs`：我拥有 `a` 指向的整块堆，内容是 `xs`。函数返回后堆所有权归还，且值等于数学和。

**你写**：前置/后置条件、堆断言、帧规则、并发协议。  
**机器做**：证明堆安全、别名、数据竞争；Steel/F* 可提取到 C。  
**代价**：自动化低，学习成本最高；对纯函数式代码是杀鸡用牛刀。  
**适合**：并发、mutable 数据结构、OS/运行时/加密底层。

---

## D. 双层：spec + impl（F* / Low* / KaRaMeL）

```fstar
// Sum.Spec.fst
val sum : list int -> Tot int
let rec sum xs =
  match xs with
  | [] -> 0
  | x :: rest -> x + sum rest

// Sum.Impl.fst
val sum_array : a:array int -> len:u32 -> Stack u32
  (requires fun h -> live h a /\ len == length h a)
  (ensures fun h0 r h1 ->
     h0 == h1 /\ r == sum (to_list h0 a))
let sum_array a len =
  let mut s = 0ul in
  ...
```

**你写**：spec 文件写数学定义和定理；impl 文件写循环和内存操作。  
**机器做**：KaRaMeL 提取成 C，规格和证明全部擦除；数组访问不带检查。HACL* 的加密库就是这样做到“经过验证且和 OpenSSL 竞争性能”。  
**代价**：两套语言/两套思维；spec 与 impl 绑定要严格，否则漂移；工具链重。  
**适合**：性能关键库、加密、解析器、需要零成本验证的组件。

---

## E. 混合（Alethe）

### E1. 日常：精化类型 + 隐式证明参数

```alethe
fn get(a: Array<Int>, i: Int) -> Int
  requires 0 <= i && i < len(a)
{
  a[i]          // 有 requires 事实 → unchecked access
}

fn get_plain(a: Array<Int>, i: Int) -> Int {
  a[i]          // 没有事实 → checked access
}
```

### E2. 强性质：落到小内核

```alethe
theorem nonzero_refl:
  forall b: Int. b != 0 -> b != 0
{
  intro b;
  intro h;
  exact h
}
```

证明项就是 `λb. λh. h`；SMT/搜索/LLM 只是候选生成器，内核检查。

### E3. 双文件与多实现

```alethe
// sum.spec.alt
spec fn sum(xs: List<Int>) -> Int
  ensures result == sum_math(xs)
  cost O(n)
  alloc 0
```

```alethe
// sum.ref.alt
impl fn sum_ref(xs: List<Int>) -> Int
  refines sum
{
  match xs with
  | [] => 0
  | x :: rest => x + sum_ref(rest)
}
```

```alethe
// sum.fast.alt
impl fn sum_fast(xs: List<Int>) -> Int
  refines sum
  cost O(n)
  alloc 0
{
  // 尾递归/累加器，可读性不保证
}
```

### E4. 当前内核里程碑

内核已经检查了：

```text
sum_ref  [1,2,3,4,5] = 15
sum_fast [1,2,3,4,5] = 15
✓ add_assoc
✓ add_zero
✓ go_eq
✓ sum_fast_eq : ∀ xs, sum_fast xs = sum_ref xs
✓ bogus proof rejected
```

这不是 SMT 的结果，而是内核逐项检查的证明项。

---

## 对比表

| | A 精化+SMT | B 依赖类型 | C 分离逻辑 | D 双层 | E 混合 |
|---|---|---|---|---|---|
| 日常写法 | 普通代码 + 注解 | 类型即证明 | 堆断言 | 两套文件 | 精化为主，必要时依赖 |
| 自动化 | 高 | 中 | 低 | 中 | 高，可降级到内核 |
| 表达力 | 中（一阶/算术） | 极高 | 高（堆/并发） | 高 | 高（分层） |
| 证明可重建 | 难（Z3） | 是 | 是 | 部分 | 是（内核） |
| 性能潜力 | 中高 | 高 | 高 | 极高（C） | 高 |
| 学习成本 | 中 | 高 | 高 | 高 | 中高 |
| 工程成熟度 | 高（Dafny/Verus/F*） | 高（Lean/Idris） | 中 | 高（F*） | 低（新语言） |
| 适合 | 业务不变量 | 算法/数据结构 | 并发/底层 | 性能关键库 | 通用 + 性能 |

---

## 为什么 SMT 的证明“可重建”难

SMT 返回的是**反证**：一组约束不可满足，并给出一串理论推理。要变成小内核能检查的证明项，要跨过四座山：

### 1. 编码信任

程序逻辑 → SMT-LIB 的翻译器如果有 bug，可以证明出假命题。内核证明直接针对程序语义，没有这层编码信任。

### 2. 预处理

CNF 变换、重写、Skolem 化、量词实例化——每一步都要么证明等价，要么信任。

### 3. 理论组合

线性算术、数组、位向量、未解释函数组合在一起（Nelson-Oppen），每个理论引理都要有证书：

- 线性算术：Farkas 证书，可检查；
- 等式/congruence：proof forest，可检查；
- 数组：read-over-write 引理，可检查；
- 位向量：bit-blasting，可检查但证明巨大；
- **量词实例化**：启发式，通常没有证书；
- **非线性算术/归纳**：基本没有通用证书。

例子：

```text
x < y, y <= x ⊢ False
```

线性算术给出 Farkas 证书 `1*(x<y) + 1*(y<=x) ⇒ 0 <= -1`，内核只要一个小检查器。

但：

```text
∀x. P(x) ⊢ P(t)
```

SMT 通过启发式把 `x` 实例化成 `t`。这个实例化本身不在证明里，或者格式不可靠；内核需要知道“为什么是 `t`”。量词实例化是 SMT 自动化的核心，也是证明重建最难的部分。

### 4. 证明格式

Z3 的 proof 是 solver-specific、不稳定、可能巨大；Alethe/LFSC 这类可检查格式覆盖还不完整。Isabelle 能重建一部分 SMT 证明，但不是全部。

**务实路线**：

1. 内存安全/类型安全：内核证明，不用 SMT；
2. 线性算术/等式/数组等可出证书的片段：SMT 找证明 + 内核检查证书；
3. 复杂性质：SMT 只做 oracle，显式标记 `smt` 信任；
4. 性能等价性：可以先信任 SMT，但要有审计标记；
5. 长期：证明搜索/LLM 生成候选，内核检查。

## 结论

- 日常代码用 A 的写法（精化 + 自动），但证明项落到 B 的内核。
- 性能关键模块用 D 的组织（spec/impl 多实现）。
- 堆/并发/unsafe 用 C 的逻辑（后续）。
- SMT/LLM 只做提供者，不做验证者。
