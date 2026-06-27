# Diff 模式 — 设计文档

Date: 2026-06-27

## 概要

新增 `--diff` 模式，对比两个堆栈快照（JSON 文件或即时采样），输出堆栈分布的变化。用于回归分析、改动前后的对比。

## 动机

- 发布新版本后，对比改动前后的调用栈分布变化
- 压测前后，哪些路径变热了
- 定位"多了/少了哪些堆栈"

## CLI 接口

```
--diff <file1> <file2>    对比两个 JSON 输出文件
```

两种使用方法：

### A. 文件对比

```sh
cs -p 12345 --json > before.json
# 改代码 / 重启
cs -p 12345 --json > after.json
cs --diff before.json after.json
```

### B. 即时对比（一步完成）

```sh
cs -p 12345 --diff -t 1 -n 5
# 内部: 采样前半段 (count/2) → before → 继续采样后半段 → after → diff
```

但 B 会增加 CLI 复杂度，第一期只做 A（文件对比）。

## 输出格式

文本格式，举例：

```
=== Stack Diff ===

[+] new threads: 2
    func_new;func_a (tid: 12346)

[-] gone threads: 1
    func_old;func_b (tid: 12347)

[~] changed: 1 stack signature
    func_a;func_b
      before: 3 threads (12348, 12349)
      after:  5 threads (12350, 12351, 12352, 12353, 12354)
```

### JSON 输出

`--diff --json` 也支持：

```json
{
  "tool": "cs diff",
  "timestamp": "2026-06-27T10:00:00Z",
  "before_label": "before.json",
  "after_label": "after.json",
  "added": [...],
  "removed": [...],
  "changed": [...]
}
```

## 数据模型

```rust
pub struct DiffResult {
    pub added: Vec<StackDiffEntry>,
    pub removed: Vec<StackDiffEntry>,
    pub changed: Vec<StackChangedEntry>,
}

pub struct StackDiffEntry {
    pub signature: Vec<Frame>,  // 帧列表（作为唯一签名）
    pub threads: Vec<ThreadIdent>,
}

pub struct StackChangedEntry {
    pub signature: Vec<Frame>,
    pub before: Vec<ThreadIdent>,
    pub after: Vec<ThreadIdent>,
}
```

## 实现策略

1. **`src/diff.rs`** 新模块，包含 `compute_diff()` 和格式化函数
2. 读取 JSON 文件 -> 反序列化为 `OutputData` -> 提取 `Vec<UniqueStackGroup>` -> 按 signature（帧序列）建 HashMap -> 对比两个 HashMap
3. 签名匹配算法：
   - 以 `Vec<Frame>` 为 key（复用 `Frame` 的 `Hash + Eq`）
   - keys 只在 before 中 → removed
   - keys 只在 after 中 → added
   - keys 在两边都存在但 thread 列表不一致 → changed
   - keys 在两边完全相同 → unchanged（不输出）
4. **文本格式化**，可选 JSON 输出
5. **`args.rs`** 新增 `--diff` flag，接受两个 `String` 参数

## 实现策略

```rust
pub fn compute_diff(
    before: Vec<UniqueStackGroup>,
    after: Vec<UniqueStackGroup>,
) -> DiffResult {
    let mut before_map: HashMap<&[Frame], &UniqueStackGroup> = ...;
    let mut after_map: HashMap<&[Frame], &UniqueStackGroup> = ...;
    // 遍历 keys 分类
}
```

## 不做

- 不做即时双段采样（B 方案），第一期只做文件对比
- 不做百分比/统计显著性计算
- 不做折叠格式的 diff（非一一对应）
- 不支持非 JSON 输入文件

## 影响范围

| 文件        | 改动                      |
| ----------- | ------------------------- |
| `src/diff.rs` | 新建模块                  |
| `src/args.rs` | 新增 `--diff` flag          |
| `src/main.rs` | 新增 `mod diff` + diff 调度 |
| `Cargo.toml`  | 无新增依赖                |
