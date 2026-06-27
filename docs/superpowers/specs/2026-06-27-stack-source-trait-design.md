# StackSource Trait 抽象 — 设计文档

Date: 2026-06-27

## 概要

定义 `StackSource` trait，统一三种输入源（eu-stack、gdb、file/stdin）的接口，让采集与处理解耦。新增输入后端时只需实现 trait，无需修改调度逻辑。

## 动机

当前代码中三种输入源各走各的路，`main.rs` 的调度是 if-else 链：

```rust
if !cli.files.is_empty() {
    uniquify_stack_files(cli).await;
} else if cli.gdb_mode {
    run_gdb(&cli).await;
} else {
    run_eustack(&cli).await;
}
```

而且三者的公共模式（采样循环、格式化分流、错误处理）散布在各自实现中。定义 trait 后可以：

- 统一调度逻辑
- 新增后端（如 future 的 eBPF 输入）只需加一个实现
- 测试时可以注入 mock 数据源

## Trait 定义

```rust
/// 堆栈采集源：从某种来源获取原始堆栈数据并解析为结构化数据
#[async_trait]
pub trait StackSource {
    /// 采集并返回结构化的线程堆栈列表
    async fn collect(&self, cli: &Cli) -> Result<Vec<ThreadStack>, String>;

    /// 来源名称，用于输出中的 `tool` 字段（如 "eu-stack" / "gdb"）
    fn name(&self) -> &'static str;
}
```

## 三个实现

### `EustackSource`

```rust
pub struct EustackSource;

#[async_trait]
impl StackSource for EustackSource {
    async fn collect(&self, cli: &Cli) -> Result<Vec<ThreadStack>, String> {
        // 现有 do_run_eustack + parse_eustack 逻辑
    }

    fn name(&self) -> &'static str { "eu-stack" }
}
```

处理 corefile 路径（`-c`、`-e`）和实时 PID 路径（`-p`），内部调用 `collect_samples`。

### `GdbSource`

```rust
pub struct GdbSource;

#[async_trait]
impl StackSource for GdbSource {
    async fn collect(&self, cli: &Cli) -> Result<Vec<ThreadStack>, String> {
        // 现有 do_run_gdb + parse_gdb 逻辑
    }

    fn name(&self) -> &'static str { "gdb" }
}
```

### `FileSource`

```rust
pub struct FileSource;

#[async_trait]
impl StackSource for FileSource {
    async fn collect(&self, cli: &Cli) -> Result<Vec<ThreadStack>, String> {
        // 从文件/stdin 读取 + 双 parser 尝试 + 回退逻辑
    }

    fn name(&self) -> &'static str { "unknown" }
}
```

## 调度器重构

```rust
async fn run_source(cli: &Cli, source: impl StackSource) {
    let stacks = source.collect(cli).await.unwrap_or_else(|e| { ... });
    let groups = if cli.unique_mode {
        stack_data::dedup_stacks(stacks)
    } else {
        stack_data::to_groups(stacks)
    };
    let output = format_any(groups, cli, source.name());
    display_final(cli, &output, &errors);
}
```

`main.rs` 简化为：

```rust
let source: Box<dyn StackSource> = if !cli.files.is_empty() {
    Box::new(FileSource)
} else if cli.gdb_mode {
    Box::new(GdbSource)
} else {
    Box::new(EustackSource)
};
run_source(&cli, &*source).await;
```

## 依赖

需要引入 `async-trait` crate，或使用 Rust nightly 的 `async_fn_in_trait`。

## 不做

- 不改变 `parse_eustack` / `parse_gdb` 的可见性（仍可以从外部使用）
- 不改变 `input_file.rs` 的双 parser 回退策略
- 不改变数据模型层（`stack_data.rs`）
- 不引入动态分发的运行时开销（用 `Box<dyn>` 或泛型均可，选择泛型在编译期单态化）

## 影响范围

| 文件                                 | 改动                                                 |
| ------------------------------------ | ---------------------------------------------------- |
| `src/source.rs` 或 `src/stack_source.rs` | 新建模块，定义 trait                                 |
| `src/input_eustack.rs`                 | 添加 `EustackSource` impl，可从 `run_eustack` 重构调用   |
| `src/input_gdb.rs`                     | 添加 `GdbSource` impl                                  |
| `src/input_file.rs`                    | 添加 `FileSource` impl，更名为 `source_file.rs`          |
| `src/main.rs`                          | 调度逻辑简化                                         |
| `Cargo.toml`                           | 新增 `async-trait` 依赖（或启用 Rust nightly feature） |
