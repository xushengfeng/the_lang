# 编译器、检查器、VM 到底是什么

这份文档只做一件事：把最近混在一起的名词分开。

## 0. 一句话版

- **OwnershipChecker（所有权检查器）**：编译期检查器。它看代码，不运行代码。
- **VM（虚拟机）**：运行期执行器。它运行编译出来的字节码，目前只用于测试。
- **Compiler（编译器）**：把源码变成可执行形式的一整套工具，包括前端检查、证明检查、降级/优化、后端。
- **Runtime（运行时）**：程序真正跑起来时的环境/库（内存、数组、OS、GC/RC）。它不是检查器。

## 1. 两个时间点：编译期 vs 运行期

```text
编译期（写代码/构建时）
  parse          解析
  typecheck      类型检查
  owncheck       所有权检查
  borrowck       借用检查
  refinecheck    生成证明义务
  proofcheck     内核检查证明
  lowering       降级到低层 IR
  codegen        生成字节码/LLVM/WASM/机器码

运行期（程序跑起来时）
  VM / CPU       执行
  memory/arrays  内存和数组
  OS/runtime     系统调用、运行时库
```

**OwnershipChecker 在编译期，不在运行期。**  
它不会出现在程序运行时的调用栈里；它只是构建时的一个分析步骤。

## 2. 当前仓库里有什么

| 组件 | 文件 | 时间点 | 作用 |
|---|---|---|---|
| Parser | `src/syntax.rs` | 编译期 | 源码 → AST |
| OwnershipChecker（原型） | `src/ownership.rs` | 编译期 | `own`/`borrow`/unique 检查、`set` 原地/复制决策 |
| Lowering（原型） | `src/ownership.rs` | 编译期 | `set` → `__set_in_place`/`__set_copy` |
| Bytecode compiler | `src/vm.rs` | 编译期 | AST → 字节码 |
| BytecodeVM | `src/vm.rs` | 运行期 | 执行字节码，输出统计 |
| Proof kernel | `src/dependent.rs` | 编译期 | 检查证明项 |
| ProofModel | `src/ownership_proof.rs` | 编译期 | 数学定义 + 定理（原地/borrow） |

注意：`src/ownership.rs` 里同时有 **检查器** 和 **树遍历解释器**。  
解释器是运行期测试工具；检查器是编译期分析。它们放在一个文件里只是为了原型方便，不代表它们是同一个东西。

## 3. OwnershipChecker 是什么

输入：

```text
AST + 类型信息 + 函数签名
```

工作：

- `own` 值是否被 move；
- move 后是否再用；
- 唯一性 `Unique(x)`；
- `borrow` 是否逃逸/被 move/被写；
- `set`/`push` 能不能原地；
- 输出 facts 和 lowering 决策。

输出：

```text
降级后的 AST/IR + facts + 诊断
```

它**不运行程序**，也**不做运行时检查**。  
运行时只有 VM/CPU；VM 不检查 ownership。

## 4. VM 是测试后端，那 LLVM/WASM/native 呢

VM 只是后端之一。换后端时：

### 保持不变

- Parser；
- 类型检查；
- 所有权/借用检查；
- 证明检查；
- 高层 IR/优化决策（大部分）；
- `own`/`borrow`/unique 的 facts。

### 需要重做/适配

| 部分 | VM | LLVM | WASM | native |
|---|---|---|---|---|
| 代码生成 | 自定义字节码 | LLVM IR | WASM 字节码 | 机器码 |
| 调用约定/ABI | 自定义帧 | LLVM/C ABI | WASM ABI | 平台 ABI |
| 内存布局 | `Rc<RefCell>` 原型 | LLVM 类型/布局 | linear memory | 自己管理 |
| 验证器 | 还没做 | LLVM verifier | WASM validator | 无标准验证器 |
| 运行时库 | Rust VM | C/C++/Rust runtime | JS/WASM runtime | OS/自定义 |
| 信任基 | 小，但未验证 | 大（LLVM） | 中（规范+验证器） | 最大 |

所以：**前端检查和证明不用重做；后端代码生成、ABI、运行时、验证要分别做。**

“是不是还要做相同的操作？”  
**精神上相同：每个后端都必须保证语义保持。**  
具体手段不同：

- VM：自己写字节码验证器；
- WASM：用标准 validator；
- LLVM：用 LLVM verifier + 自己的 lowering 翻译验证；
- native：没有标准语义验证器，TCB 最大，需要自己验证或信任。

这正是先用 VM 的理由：它小、可控、好调试，可以先把语义、擦除、原地更新跑通，再换后端。

## 5. 编译器到底包括什么

```text
Compiler
├── Frontend
│   ├── parse
│   ├── resolve
│   ├── typecheck
│   ├── owncheck / borrowck / regioninfer / effectcheck
│   └── refinecheck
├── Verification
│   └── proofcheck（小内核）
├── Middle-end
│   ├── erase（证明/类型索引/生命周期擦除）
│   ├── ownlower（原地/复制）
│   ├── checkelide（边界/除零检查消除）
│   ├── ANF/IR
│   └── optimization
└── Backend
    ├── codegen（bytecode / LLVM / WASM / native）
    └── verify/validate（后端验证）
```

**Runtime 不在 Compiler 里。**  
Runtime 是程序跑起来时的环境：VM、OS、内存分配器、RC/GC、数组库等。

ProofModel / proofcheck 属于工具链的验证部分，不属于生成出来的程序。  
证明不会进入运行时。

## 6. 术语表（中文优先）

| 英文 | 中文 | 一句话 |
|---|---|---|
| Compiler | 编译器 | 源码 → 可执行形式的一整套工具 |
| Frontend | 前端 | 解析、类型、所有权、借用检查 |
| Middle-end | 中端 | IR、擦除、降级、优化 |
| Backend | 后端 | 生成字节码/LLVM/WASM/机器码 |
| Runtime | 运行时 | 程序跑起来时的环境和库 |
| AST | 抽象语法树 | 源码的结构化表示 |
| IR | 中间表示 | 编译器内部的程序表示 |
| Bytecode | 字节码 | VM 执行的指令 |
| VM | 虚拟机 | 执行字节码的测试后端 |
| OwnershipChecker | 所有权检查器 | 编译期检查 move/unique/原地 |
| BorrowChecker | 借用检查器 | 编译期检查别名/逃逸/独占 |
| Lowering | 降级/转换 | 高层 IR → 低层 IR |
| Proof kernel | 证明内核 | 检查证明项的小程序 |
| ProofModel | 证明模型 | 数学定义 + 定理，不是运行代码 |
| Refinement / refines | 精化 | 实现满足规格 |
| Translation validation | 翻译验证 | 每次转换都证明语义保持 |
| TCB | 信任基 | 必须相信、没有被证明的部分 |

## 7. 当前最准确的状态描述

- **能编译**：`examples/ownership.alt` → 所有权检查/lowering → 字节码 → VM。
- **能证明**：`ProofModel` 里的原地/borrow 定理由 `proofcheck` 检查。
- **没连起来**：ProofModel 的定理还没有和 `OwnershipChecker`/`Lowering`/VM 绑定。
- **没有**：字节码验证器、`refines`、LLVM/WASM/native 后端。

下一步最合理的是：**先做字节码验证器（verifybc）**，让 `compile → verify → run` 成立；然后再做 `refines`，把证明模型和实现连起来。
