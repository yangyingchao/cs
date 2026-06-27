# Folded 输出格式 — 设计文档

Date: 2026-06-27

## 概要

新增 `--folded` 输出格式开关，输出 Brendan Gregg FlameGraph 生态兼容的 folded 格式，使 `cs` 可以充当火焰图的采集器。

## 动机

当前 JSON 和文本输出都是面向人或通用程序处理。Folded format 是火焰图工具链的事实标准输入格式。加上此功能后：

```sh
cs -p 12345 --folded | inferno-flamegraph > flame.svg
cs *.stack --folded | inferno-flamegraph --percent > flame.svg
```

无需任何中间转换脚本，`cs` 直接对接 FlameGraph 生态。

## 什么是 Folded Format

每行一个唯一的堆栈轨迹，分号分隔帧，空格后跟计数：

```
func_a;func_b;func_c 3
func_a;func_b;func_d 1
start;main;__libc_start_main;func_x 5
```

- 帧按调用深度从外层到内层排列（或反之，需与下游约定一致）
- 计数=该轨迹出现的总次数
- 无 header，无 metadata，纯文本

## CLI 接口

```
--folded    以 folded 格式输出堆栈信息
```

与现有 flag 的交互：

| 组合            | 行为                           |
| --------------- | ------------------------------ |
| `--folded`        | 所有线程去重后输出 folded 行   |
| `--folded -U`     | 同 `--folded`（folded 天然去重） |
| `--folded -R`     | folded + raw 帧内容（不简化）  |
| `--folded -G`     | gdb 后端的 folded 输出         |
| `--folded --json` | 报错退出，两者互斥             |

`--folded` 与 `--json` 互斥，同时指定时报错。

## 数据模型

```rust
pub struct FoldedStack {
    pub frames: Vec<String>,    // 每帧的函数名，连接成一行
    pub count: usize,
}

pub fn format_folded(groups: &[UniqueStackGroup]) -> String
```

将 `UniqueStackGroup` 转化为 folded 行：

```rust
fn format_folded(groups: &[UniqueStackGroup]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for group in groups {
        let signature: Vec<&str> = group.frames.iter().map(|f| f.function.as_str()).collect();
        let signature = signature.join(";");
        lines.push(format!("{} {}", signature, group.threads.len()));
    }
    lines.sort();
    lines.join("\n")
}
```

## 输出示例（对应 JSON spec 中的场景）

```
__poll;g_main_context_iteration 2
g_type_check_instance_is_a;gdk_frame_clock_request_phase 1
```

帧顺序从栈底（depth=0）到栈顶，即 main 在最右、最近调用在最左——兼容 FlameGraph 的习惯（根部在底部）。

## 实现策略

1. **`args.rs`** 新增 `--folded` flag：`folded_mode: bool`
2. **`stack_data.rs`** 新增 `format_folded()` 函数
3. **格式化分流**：`format_result` 中新增 `--folded` 分支，调用 `format_folded()`，互斥检查
4. **后端无关**：与 `--json` 一样在格式化层处理，三个后端无需改动

## 不做

- 不支持折叠帧（简化逻辑复用现有 `function` 字段）
- 不输出 count=0 的行
- 不支持自定义分隔符（固定 `;`）
- 不支持 metadata header

## 相关项目

- [inferno](https://github.com/jonhoo/inferno) — Rust 实现的 FlameGraph 工具集
- [FlameGraph](https://github.com/brendangregg/FlameGraph) — Brendan Gregg 原版 Perl 脚本
