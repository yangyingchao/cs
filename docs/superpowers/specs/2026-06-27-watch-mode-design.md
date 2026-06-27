# Watch 模式 — 设计文档

Date: 2026-06-27

## 概要

新增 `--watch` 模式，在终端中持续采样并实时刷新堆栈分布视图。类似 `top` 但看的是调用栈。

## 动机

- 观察正在运行的进程，实时看调用栈分布变化
- 压测时立即看到热点迁移
- 无需反复 `cs -t 1 -n 3` 手动比较

## 原型参考

stax 和 Profile Bee 都做了类似功能。我们的 `-t`/`-n` 采样+去重已经是基础，`--watch` 只是加一个周期性刷新显示层。

## CLI 接口

```
--watch                持续采样，终端实时刷新视图
```

与现有 flag 的组合：

| 组合                    | 行为                              |
| ----------------------- | --------------------------------- |
| `-p 12345 --watch -t 0.5` | 每 0.5 秒采样一次，刷新显示       |
| `--watch -t 1 -U`         | 每秒采样，去重显示                |
| `--watch --folded`        | 输出 folded 格式（非交互式）      |
| `--watch --json`          | 每次采样输出一行 JSON（非交互式） |

`--watch` 与 `--json`/`--folded` 组合时不启动 TUI，而是每次采样输出一行结构化数据（类似 tail -f 模式）。

## TUI 显示

```


  Sampling PID 12345 - every 1.0s (3 samples taken)
  ───────────────────────────────────────────────────────────────
  COUNT  TREND  STACK
  12     ▄▄▆█   clock_nanosleep;g_main_context_iteration
   8     ▄█▄    __poll;g_main_context_iteration
   3     ███    func_a;func_b
   1     ▄▄▄    start;main
  ───────────────────────────────────────────────────────────────
  Suspicious threads: (none)
  ↑/↓ scroll  q quit
```

布局：
- 头部：PID、采样间隔、已采样次数
- 分隔线
- 表格：计数 + 迷你趋势图（4 个字符宽） + 堆栈签名
- 按 COUNT 降序排列
- 高亮行：当前选中的行
- 页脚：操作提示

### 趋势图

维护最近 N 个采样周期的计数环形缓冲区（默认 N=4）。每列对应一次采样的相对计数（缩放到 0-4 字符高度）：

```
█ = 4/4
▆ = 3/4
▄ = 2/4
▁ = 1/4
  = 0/4
```

用 `termion` 或 `crossterm` 实现。项目已有 `termion` 和 `crossterm`（通过 inquire 传递依赖），可以选择不引入新依赖。

## 数据流

```
loop:
    sample (execute command + parse)
    update ring buffer
    render TUI
    handle key input (with timeout = interval)
    if 'q' pressed, exit
```

一次采样时间如果超过 interval，跳过补时（即尽力而为，不累积）。

## 实现策略

1. **`src/watch.rs`** 新模块，包含 TUI 渲染和事件循环
2. **复用现有采集管道**：`collect_samples()` 或直接调 `do_run_eustack`/`do_run_gdb`
3. **RingBuffer** 结构：记录每次采样的 `Vec<UniqueStackGroup>`，保留 N 帧
4. **渲染**：clear + draw，最小化闪烁
5. **键盘输入**：`termion::async_stdin` 或 `crossterm::event::poll` 非阻塞读

## 意外退出处理

- 按 `q` 退出，退出前恢复终端
- `SIGINT` (Ctrl+C) 同样恢复终端后退出
- 使用 `Drop` 或 `finally` 保证终端恢复

## 非 TUI 模式

当 `--watch` 与 `--json` 或 `--folded` 组合时：
- 不进入 TUI
- 每次采样完成，输出一行 JSON 或 folded 到 stdout
- 类似 `tail -f`，可 pipe：
  ```sh
  cs -p 12345 --watch --json | jq '.stacks | length'
  ```

## 不做

- 不做多 PID 同时监控（可简化）
- 不做鼠标支持
- 不做颜色主题配置
- 不做历史回溯

## 影响范围

| 文件         | 改动                                 |
| ------------ | ------------------------------------ |
| `src/watch.rs` | 新建模块                             |
| `src/args.rs`  | 新增 `--watch` flag                    |
| `src/main.rs`  | 新增 `mod watch` + watch 调度          |
| `Cargo.toml`   | 无新增依赖（复用 termion/crossterm） |
