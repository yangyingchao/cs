# Diff 模式 — 设计文档

Date: 2026-06-27

## 概要

新增 `--diff` 模式，对比两个堆栈快照（JSON 文件），输出堆栈分布的变化。用于回归分析、改动前后的对比。

## 动机

- 发布新版本后，对比改动前后的调用栈分布变化
- 压测前后，哪些路径变热了
- 定位"多了/少了哪些堆栈"
- 判断进程是否卡死（两次快照完全一致）

## 前置条件：匹配模式（`--match`）

`--diff` 和去重逻辑（`-U`）共享相同的**匹配机制**。为此新增通用 flag：

```
--match <precise|fuzzy>    堆栈匹配模式（默认 auto）
```

### 两种模式

| 模式    | 匹配 key                | 适用场景                  |
| ------- | ----------------------- | ------------------------- |
| `precise` | `Vec<Frame>`（全字段）    | 单进程内分析，ASLR 一致   |
| `fuzzy`   | `Vec<String>`（仅函数名） | 跨进程、跨文件，ASLR 不同 |

### 自动选择规则（auto）

auto 逻辑在**执行阶段**（而非 parse 阶段）计算。因为交互模式下 `cli.pids` 在 parse 之后才设置，parse 阶段无法确定最终模式。

```rust
impl Cli {
    pub fn effective_match_mode(&self) -> MatchMode {
        // 用户显式指定 → 直接使用
        if let Some(mode) = self.match_mode {
            return mode;
        }
        // auto 自动选择
        if self.diff.is_some()
            || !self.files.is_empty()
            || self.parent.is_some()
            || self.pattern.is_some()
            || self.pids.as_ref().is_some_and(|p| p.len() > 1)
        {
            MatchMode::Fuzzy
        } else {
            MatchMode::Precise
        }
    }
}
```

用户指定 `--match` 时覆盖自动选择。

### 冲突警告

当用户显式指定 `--match precise` 但检测到多源输入时（多文件/多 pid/diff 模式），打印警告：

```
warning: --match precise with multiple input sources may cause
         identical stacks to appear different due to ASLR.
         Consider using --match fuzzy (or omit --match for auto).
```

> **注意：** `warn_if_match_conflict()` 中的 `!self.unique_mode` 检查在加入 `--diff` 后需要同步更新为
> `!self.unique_mode && self.diff.is_none()`。

### 对现有去重的影响

`stack_data::dedup_stacks` 接受 `MatchMode` 参数：

- `Precise`：当前行为，`HashMap<Vec<Frame>, …>` key
- `Fuzzy`：`HashMap<Vec<String>, …>` key，仅函数名

`to_groups` 不需要改。

## CLI 接口

```
--diff <file1> <file2>    对比两个 JSON 输出文件
--match <precise|fuzzy>   堆栈匹配模式（默认 auto）
```

使用方式：

```sh
cs -p 12345 --json > before.json
# 改代码 / 重启 / 压测 ...
cs -p 12345 --json > after.json
cs --diff before.json after.json           # fuzzy auto
cs --diff --match precise b.json a.json   # 强制精确
```

## 匹配 key

```
// precise: 全字段匹配
key = (depth, address, function, library)

// fuzzy: 仅函数名
key = (function, ...)
```

diff 和 dedup 使用同一匹配机制，因此 `--match` 同时影响两者。

## 三分类

| 类别        | 条件                           | 含义                    |
| ----------- | ------------------------------ | ----------------------- |
| `[+]` added   | key 只在 after 中存在          | 新出现的调用路径        |
| `[-]` removed | key 只在 before 中存在         | 消失的调用路径          |
| `[~]` changed | key 两边都存在，但线程数量不同 | 热点迁移、瓶颈加剧/缓解 |
| (不输出)    | key 两边都存在，线程数量相同   | 无变化                  |

## 输出格式

### 文本格式

```
=== Stack Diff ===

[+] added: 2
    clock_nanosleep;g_main_context_iteration      1 thread
    __poll;g_main_context_iteration               1 thread

[-] removed: 1
    func_a;func_b                                 1 thread

[~] changed: 1
    func_c;func_d                                 3 → 5 threads  (+67%)
```

函数名分号连接作为栈签名，对齐显示。changed 行显示 before → after 数量及百分比变化。

### JSON 格式

`--diff --json` 同样支持：

```json
{
  "tool": "cs diff",
  "timestamp": "2026-06-27T10:00:00Z",
  "before_label": "before.json",
  "after_label": "after.json",
  "added": [
    {"signature": "func_c;func_d", "count": 1}
  ],
  "removed": [
    {"signature": "func_a;func_b", "count": 2}
  ],
  "changed": [
    {
      "signature": "func_e;func_f",
      "before_count": 3,
      "after_count": 5,
      "percent_change": 66.7
    }
  ]
}
```

## 数据模型

```rust
pub struct DiffResult {
    pub added: Vec<StackDiffEntry>,
    pub removed: Vec<StackDiffEntry>,
    pub changed: Vec<ChangedEntry>,
}

pub struct StackDiffEntry {
    pub signature: String,   // 分号连接的函数名
    pub count: usize,
}

pub struct ChangedEntry {
    pub signature: String,
    pub before_count: usize,
    pub after_count: usize,
    pub percent_change: f64,
}
```

## 算法

1. 读取两个 JSON 文件，反序列化为 `OutputData`
2. 提取 `stacks: Vec<UniqueStackGroup>`
3. 根据 `MatchMode` 构建签名 key：

```rust
fn stack_key(group: &UniqueStackGroup, mode: MatchMode) -> Key {
    match mode {
        MatchMode::Precise => Key::Full(group.frames.clone()),   // Vec<Frame>
        MatchMode::Fuzzy => {
            let sig: Vec<&str> = group.frames.iter()
                .map(|f| f.function.as_str()).collect();
            Key::Signature(sig.join(";"))                         // String
        }
    }
}
```

4. 建两个 HashMap（key → group），key 类型取决于 MatchMode
5. 遍历 key 集合：
   - 仅在 after → added
   - 仅在 before → removed
   - 两边都有，但 `threads.len()` 不同 → changed
   - 两边都有，`threads.len()` 相同 → 跳过
6. 各分类按 count 降序排序

## 实现策略

1. **`src/match_mode.rs`** 新模块，定义 `MatchMode` 枚举（`Precise` / `Fuzzy`）
2. **`src/args.rs`** 新增 `--diff`、`--match` flag；`Cli` 新增 `effective_match_mode()` 方法和冲突警告
3. **`src/stack_data.rs`** `dedup_stacks` 增加 `MatchMode` 参数，构建对应 key 类型
4. **`src/diff.rs`** 新模块，包含 `compute_diff()` 和文本/JSON 格式化函数
5. **`src/main.rs`** 新增 `mod diff` + `mod match_mode` + diff 调度入口
6. **三个后端**（`input_eustack`/`input_gdb`/`input_file`）：传 MatchMode 给 `dedup_stacks`
7. **零新依赖**：JSON 反序列化复用已有的 `serde_json`

## 不做

- 不做即时双段采样（`cs -p 12345 --diff -t 1 -n 5`），仅做文件对比
- 不做百分比/统计显著性计算之外的量化分析
- 不做折叠格式 diff
- 不支持非 JSON 输入文件

## 影响范围

| 文件                 | 改动                                         |
| -------------------- | -------------------------------------------- |
| `src/diff.rs`          | 新建模块                                     |
| `src/match_mode.rs`    | 新建模块，定义 `MatchMode` 枚举 |
<!-- table not formatted: invalid structure -->
| `src/args.rs`          | 新增 `--diff`、`--match` flag                    |
| `src/stack_data.rs`    | `dedup_stacks` 接受 `MatchMode` 参数             |
| `src/input_eustack.rs` | 调用 `dedup_stacks` 处传参                     |
| `src/input_gdb.rs`     | 同上                                         |
| `src/input_file.rs`    | 同上                                         |
| `src/main.rs`          | 新增 `mod diff` + `mod match_mode`               |
| `Cargo.toml`           | 无新增依赖                                   |
