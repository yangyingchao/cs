# JSON 输出格式 — 设计文档

Date: 2026-06-24

## 概要

给 `cs` CLI 工具增加 `--json` 输出格式开关，输出机器可消费的结构化堆栈数据。

## 动机

当前输出是纯文本，面向人阅读。增加 JSON 格式使得工具可以 pipe 到 `jq`、监控系统、日志收集器等进行程序化处理。

## CLI 接口

新增一个 flag：

```
--json    以 JSON 格式输出堆栈信息
```

与现有 flags 的组合行为：

| 组合                  | 行为                                           |
| --------------------- | ---------------------------------------------- |
| `--json`                | 标准 JSON 输出，每个线程一条记录               |
| `--json -U`             | JSON + unique 模式，重复栈合并，附加 `tids` 字段 |
| `--json -R`             | JSON + raw 模式，不简化帧内容                  |
| `--json -G`             | gdb 后端的 JSON 输出，字段统一                 |
| `-t 0.5 -n 3 --json -U` | 多次采样 + 去重后的 JSON                       |

JSON 输出到 stdout，不额外支持文件参数（重定向由 shell 处理）。

## 数据模型

```rust
/// 线程标识：标识一个线程的来源
pub struct ThreadIdent {
    pub pid: i32,
    pub tid: i32,
    pub thread_name: String,
}

/// 帧：堆栈中的一层
pub struct Frame {
    pub depth: u32,
    pub address: String,
    pub function: String,
    // 库文件路径/名称，无法解析时为 null
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
}

/// 一组共享相同堆栈帧的线程（非 unique 模式时长度为 1）
pub struct UniqueStackGroup {
    pub threads: Vec<ThreadIdent>,
    pub frames: Vec<Frame>,
    pub suspicious: bool,
}

/// 顶层输出
pub struct OutputData {
    pub tool: String,       // "eu-stack" | "gdb"
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<SamplingInfo>,
    pub stacks: Vec<UniqueStackGroup>,
}
```

`SamplingInfo` 在 `-t`/`-n` 多次采样时填充：

```rust
pub struct SamplingInfo {
    pub interval: f32,
    pub count: i32,
}
```
```

## 实现策略

采用「内部结构化 → 外部格式化」的架构：

1. **定义数据模型**（独立模块，如 `src/stack_data.rs`），加 `serde::Serialize` 派生
2. **`args.rs`** 新增 `--json` flag
3. **改造后端解析**：`do_run_eustack` / `do_run_gdb` 不再返回 `String`，而是解析为 `Vec<ThreadStack>`。现有的 regex 解析逻辑从 `uniquify` 模块提取到解析阶段，产出自描述的结构化数据
4. **去重基于结构体**：`uniquify` 模块不再对文本做 regex 二次解析，而是在 `ThreadStack` / `Frame` 结构体上直接比较（`Hash` / `Eq` 派生或手写比较）。`sort_and_print_stack` 改为接收 `Vec<ThreadStack>` 并输出
5. **格式化分流**：最后一步根据 `--json` 标识调用 JSON 序列化或文本格式化，两者共享同一份 `Vec<ThreadStack>`
6. **依赖新增**：`serde = { version = "1", features = ["derive"] }`、`serde_json = "1"`

这样改动范围虽广但结构清晰——解析从原来的「正则拼文本」变成「正则填结构体」，去重从「文本比较」变成「结构体比较」，整体代码复杂度反而是下降的。

## JSON 输出示例（-U 模式）

```json
{
  "tool": "eu-stack",
  "timestamp": "2026-06-24T10:30:00Z",
  "stacks": [
    {
      "threads": [{"pid": 14794, "tid": 14794, "thread_name": "main"}],
      "suspicious": false,
      "frames": [
        {"depth": 0, "address": "0x7f83df80a3ec", "function": "g_type_check_instance_is_a"},
        {"depth": 1, "address": "0x7f83df14f421", "function": "gdk_frame_clock_request_phase"}
      ]
    },
    {
      "threads": [
        {"pid": 14794, "tid": 14822, "thread_name": "worker-1"},
        {"pid": 14794, "tid": 14820, "thread_name": "worker-2"}
      ],
      "suspicious": true,
      "frames": [
        {"depth": 0, "address": "0x7f83ddc5363f", "function": "__poll"},
        {"depth": 1, "address": "0x7f83de32a8d7", "function": "g_main_context_iteration"}
      ]
    }
  ]
}
```

## 不做

- 不出 JSON 到文件（重定向由用户处理）
- 不修改堆栈文本输出的默认行为（`--json` 关闭时的输出格式保持不变）
- 不改变进程选择交互流程（inquire 模式）

## 相关变更

- 随本功能一起重构了帮助系统：clap 属性统一使用中文，`--en --help` 显示独立维护的英文帮助文本。该变更不属于 JSON 输出的核心 scope，但与 `--json` 同批次提交。
