# Changelog

## [0.2.1] - 2026-07-08

## 改进

- 诊断/状态信息从 stdout 移至 stderr，避免干扰管道输出

### Improvements

- Moved diagnostic/status messages from stdout to stderr to avoid polluting piped output

## [0.2.0] - 2026-06-27

## 新功能

- **JSON 输出** (`-U`/`--json`)：以 JSON 格式输出结构化堆栈数据
- **JSON 文件输入**：支持将之前的 JSON 输出作为输入 (`cs dump.json`)，自动检测并降级到文本解析
- **堆栈差异比较** (`--diff`, `--diff-live`)：比较两次堆栈采样，逐帧显示增减变化
- **匹配模式** (`--match`)：控制堆栈去重粒度（`fuzzy`/`precise`），根据上下文自动选择
- **父进程模式** (`--parent`)：收集指定 PID 的所有子进程的堆栈
- **帧数限制** (`-f`)：设置每个线程显示的调用帧数量
- **英文帮助** (`--en`)：强制英文帮助输出

## 改进

- 通过 `LazyLock` 在所有解析器模块中缓存正则表达式（性能优化）
- 提取 `collect_samples` 公共采样逻辑到 utils.rs
- 文件输入改用异步 stdin 读取
- 新增 JSON 测试夹具的集成测试
- 移除未使用的 `glob` 依赖
- 归档已完成的设计文档到 `docs/specs/completed/`

## 修复

- `collect_samples` 末次采样后不再 sleep（与重构前行为一致）
- `.json` 文件 JSON 解析失败时输出警告并自动降级为文本解析
- `--pid` 只接受整数参数

## CI 变更

- 移除 `x86_64-apple-darwin` CI 目标
- 更新 cargo format 检查的操作系统

### New Features

- **JSON Output** (`-U`/`--json`): Structured stack data output in JSON format
- **JSON File Input**: Read JSON output files as input (`cs dump.json`), auto-detected with text fallback
- **Stack Diff** (`--diff <before> <after>`): Compare two stack samples and display per-frame vertical diff
- **Live Diff** (`--diff-live`): Two-shot stack comparison with interval (`-t`) or ENTER-triggered sampling
- **Match Mode** (`--match fuzzy|precise`): Control stack dedup granularity, auto-selected based on context
- **Parent Mode** (`--parent <pid>`): Collect stacks for all child processes of given PID
- **Frame Limit** (`-f <n>`): Set number of frames per thread to display
- **English Help** (`--en`): Force English output for help messages

### Improvements

- Regex caching via `LazyLock` across all parser modules (performance)
- Shared `collect_samples` function extracted to utils.rs
- Async stdin reader for file input (`-` arg)
- Integration tests with JSON fixture files
- Removed `glob` dependency (unused)
- Reorganized specs: archived completed designs to `docs/specs/completed/`

### Fixes

- `collect_samples` no longer sleeps after the last sample (behavior match with pre-refactor)
- Warning when `.json` file fails JSON parse, falls back to text parsing
- `--pid` now only accepts integer arguments
- Various dependency bumps (tokio, inquire, colored, regex, actions/checkout)

### CI Changes

- Removed `x86_64-apple-darwin` CI target
- Updated cargo format check OS

## [0.1.13] - Earlier

### Improvements

- Better error reporting with raw output display
- Usage examples in help message
- Pause on error for interactive readability

### Fixes

- Various dependency bumps
- Typo fixes
- Clippy warnings
