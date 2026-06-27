# 工程清扫 — 设计文档

Date: 2026-06-27

## 概要

修复 `cs` 代码库中已发现的几处工程瑕疵。纯重构，不改行为、不加功能。

## 变更清单

### 1. 移除死依赖 `glob`

`Cargo.toml` 中声明了 `glob = "0.3.1"`，但代码中未引用。直接移除。

### 2. 正则表达式缓存

`stack_data.rs` 中 `is_suspicious()` 和 `format_text()` 每次调用都 `Regex::new`。改用 `std::sync::LazyLock`（Rust 1.80+ stable）缓存为正则静态变量，避免重复编译。

涉及函数：
- `is_suspicious(function: &str) -> bool`
- `format_text(groups, sampling_prefix)`

### 3. 提取公共采样循环

`do_run_eustack()` 和 `do_run_gdb()` 几乎一样：

```
loop N:
    execute_command
    collect raw_output
    sleep interval
parse all raw_outputs into Vec<ThreadStack>
```

提取为 `utils.rs` 中的公共函数：

```rust
pub async fn collect_samples(
    command: &str,
    args: &[String],
    interval: Option<f32>,
    count: i32,
) -> Result<Vec<String>, String>
```

返回原始输出列表，由调用方负责解析。

### 4. 修复 async 阻塞

`input_file.rs` 中使用 `std::io::stdin().lock()` 同步读取标准输入，在 async 上下文中会阻塞 tokio 线程。改为 `tokio::io::BufReader` + `tokio::io::stdin()`。

### 5. 补充测试

- `input_file.rs`：文件读取 + stdin 读取 + 解析回退逻辑 + `--json` + `--raw` 组合
- `main.rs`：自动检测回退（eu-stack 不可用时切 gdb）、调度逻辑
- `utils.rs`：`display_final()`、`ensure_file_exists()`

## 不做

- 不动错误处理策略（不引入 anyhow/thiserror）
- 不改动现有 API 签名
- 不重构模块边界
- `display_final()` 中的同步 stdin 阻塞不在本次修复范围内（出错后即 exit，实际影响有限）

## 实施后补充

### 正则缓存范围

实际实现覆盖了以下所有热路径：
- `stack_data.rs`: `is_suspicious()` + `format_text()`（如 spec）
- `input_eustack.rs`: `parse_eustack()` — 3 个 Regex
- `input_gdb.rs`: `parse_gdb()` — 6 个 Regex
- `utils.rs`: `parse_pid()` — 1 个 Regex

动态拼接的 Regex（如 `choose_process` 中的用户输入、PID 过滤）未纳入缓存。

### SUSPICIOUS_KEYWORDS 转义

关键字 `fatal.*signals` 有意使用 `.*` 作为正则片段，不应 `regex::escape`。使用方需约定该数组为合法正则片段。当前已通过 `LazyLock` 缓存，行为不变。

## 影响范围

| 文件             | 改动                                                    |
| ---------------- | ------------------------------------------------------- |
| `Cargo.toml`       | 移除 glob                                               |
| `stack_data.rs`    | LazyLock 缓存正则                                       |
| `utils.rs`         | 新增 `collect_samples()` + 测试 + `ensure_file_exists` 测试 |
| `input_eustack.rs` | `do_run_eustack` 改调用 `collect_samples`，正则缓存         |
| `input_gdb.rs`     | `do_run_gdb` 改调用 `collect_samples`，正则缓存             |
| `input_file.rs`    | 改用 `tokio::io::stdin`                                   |
| `args.rs`          | 新增 match mode diff/diff-live 测试                     |

## 验证

- `cargo build` 无警告
- `cargo clippy --all-targets --all-features -- -Dwarnings` 无警告
- `cargo test` 全部通过
