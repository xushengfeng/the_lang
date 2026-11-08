# 表面语法：缩进语法与 S-expression

项目以 AST 和检查器为主，表面语法只负责把程序读成同一个 `Program`。目前有两个
前端，共享词法之后的全部阶段（ownership、refines、MemIR、VM）：

- `src/syntax/parser.rs`：缩进/表达式语法（`.alt`）；
- `src/syntax/lisp.rs`：S-expression 前端（`.lisp`/`.lsp`/`.sexp`）。

`src/main.rs` 按扩展名选择前端；`ownership_demo`、`vm_demo`、`refines_demo`
也做同样选择。

## 1. 缩进语法

```alethe
fn apply_twice(f: Fn(Int) -> Int, x: Int) -> Int {
  f(f(x))
}

fn make_adder(n: Int) -> Fn(Int) -> Int {
  fn(y: Int) -> Int { y + n }
}

fn main() -> Int {
  let double = fn(y: Int) -> Int { y * 2 };
  let a = apply_twice(double, 5);
  let add10 = make_adder(10);
  let b = add10(7);
  a + b
}
```

- 函数类型：`Fn(T1, T2) -> R`；零参为 `Fn() -> R`。
- lambda：`fn(x: T, y: U) -> R { body }`，也接受 `= body`。
- 顶层函数名在表达式位置是函数值；`f(x)` 在 `f` 是局部函数值时是函数值调用，
  否则是普通顶层函数调用。
- 任何表达式都可以被调用：`make_adder(10)(7)`。

## 2. S-expression 语法

同样的程序：

```lisp
(fn apply_twice ((f (fn (Int) Int)) (x Int)) Int
  (f (f x)))

(fn make_adder ((n Int)) (fn (Int) Int)
  (lambda ((y Int)) Int (+ y n)))

(fn main () Int
  (let double (lambda ((y Int)) Int (* y 2))
    (let a (apply_twice double 5)
      (let add10 (make_adder 10)
        (+ a (add10 7))))))
```

约定：

| 形式 | 含义 |
|---|---|
| `(fn name (params) ret body...)` | 函数声明 |
| `(spec name (params) ret (requires p)... (ensures p)...)` | 规格 |
| `(impl name (params) ret (refines spec) body...)` | 实现 |
| `(theorem name prop tactic...)` | 定理 |
| `(lambda ((x Int)) ret body...)` | lambda |
| `(let name value body...)` | let |
| `(if c t e)` / `(seq e...)` | 条件 / 顺序 |
| `(array e...)` / `(index a i)` | 数组 / 下标 |
| `(set a i v)` / `(push a v)` / `(len a)` / `(print e)` | 内置操作 |
| `(f a b)` / `((lambda ...) a)` / `(apply f a)` | 调用 / 函数值调用 |
| `(own x T)` / `(borrow x T)` / `(proof h P)` | 参数模式 |
| `(fn (T...) R)` / `(array T)` | 函数类型 / 数组类型 |

参数是 `(name type)`。中缀操作在 S-expression 里是前缀形式：
`(+ a b)`、`(<= 0 i)`、`(&& p q)`、`(! p)`。

## 3. 数学证明

`proof` 声明直接携带内核类型和证明项：

```alethe
proof nat_refl:
  Pi(n, Nat, Eq(Nat, n, n))
{
  Lam(n, Nat, Eq.refl(Nat, n))
}
```

```lisp
(proof nat_refl
  (forall ((n Nat)) (Eq Nat n n))
  (lambda ((n Nat)) (Eq.refl Nat n)))
```

- 内核项构造子：`Pi`/`forall`、`Lam`/`lambda`、`Let`、`App`、`Sort`、
  `Prop`、`Type`、`Nat` 字面量、点号常量（`Nat.add`、`Eq.refl`）。
- 名字解析：被 `Pi`/`Lam`/`Let` 绑定的名字是变量，其余是全局常量。
- `.alt` 用函数式语法 `Eq.refl(Nat, n)`；Lisp 用前缀应用
  `(Eq.refl Nat n)`。
- 显式项没有元变量/统一，也没有 tactic 自动构造；`theorem { intro ... }`
  仍只是解析层，尚未连内核。
- `proof_demo` 会把文件中的每条证明交给内核；成功打印 `✓`，失败退出并
  打印类型错误。`examples/proofs.*` 包含 `∀p, p→p`、`∀n, n=n`、`2+2=4`、
  复用已有定理，以及用 `Nat.recProp` 归纳证明的 `∀n, n+0=n`。

## 4. 回调语义与边界

- `Ty::Fn(params, ret)` 是运行时函数值；proof 参数在函数值里被擦除。
- lambda 捕获环境是**只读**的：可以读取外层值，但不能 move、原地修改捕获值；
  捕获的数组在 `set` 时走复制路径。
- lambda 不能捕获 `borrow` 参数；需要时把它作为回调参数显式传入。
- 嵌套 lambda 可以捕获外层 lambda 的参数（形成闭包链）。
- MemIR 桥支持 `Lambda`/`FnRef`/`Apply`；MemIR 仍不支持算术、`print`、
  动态下标，所以带这些操作的 callback 目前只在树解释器和 VM 中执行。
- 区域逃逸分析还没实现：闭包不能跨区域返回；lambda 由 VM 用 lambda lifting
  执行，捕获值作为合成函数的前导参数。
