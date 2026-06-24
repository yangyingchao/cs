# Code Review: Chinese help support

Review scope: `git diff --staged`  
Review date: 2026-06-23

---

<!-- yg:issue severity=严重 -->
### ISSUE #1: [严重] `cargo fmt -- --check` 失败 ✅
- **原因:** `chinese_help_text()` 中 `"#.to_string()` 的链式写法不符合 rustfmt 默认格式，`cargo fmt -- --check` 会报错。工程 `AGENTS.md` 将 `cargo fmt -- --check` 列为 CI gate 第一步，此问题会阻塞合并。
- **建议:** 运行 `cargo fmt` 并提交格式化后的代码。

<!-- yg:issue severity=严重 -->
### ISSUE #2: [严重] `parse_args()` 内直接调用 `exit(0)`，导致核心流程不可内联测试 ✅
- **原因:** `print_chinese_help()` 在参数解析器内部直接 `exit(0)`。虽然 spec 要求“Test `--zh --help` exits 0 with Chinese output”，但 `#[test]` 无法断言 `exit(0)`，否则会终止测试进程。这与工程“测试内联在源文件中”的标准相冲突。
- **建议:** 让 `parse_args()` 返回一个枚举或 `Option<()>`，把“是否打印中文 help 并退出”的决定权交还给 `main.rs`；这样既保持解析器可测试，也符合单一职责。

<!-- yg:issue severity=中等 -->
### ISSUE #3: [中等] 缺少 `--zh --help` 退出路径的测试 ✅
- **原因:** Spec 明确要求测试该路径会退出并输出中文，但当前仅测试了辅助函数和非退出的解析路径，没有覆盖“拦截 `--help` 并退出”这一核心行为。
- **建议:** 在测试中验证 `is_chinese_mode(&["cs", "--zh", "--help"]) && is_help_requested(...)` 为 true 后，再补充一个运行子进程的集成测试（或重构后通过返回枚举来断言）。

<!-- yg:issue severity=中等 -->
### ISSUE #4: [中等] 缺少 `--help` 保持英文的测试 ✅
- **原因:** Spec 要求测试“`--help` (without `--zh`) shows English”，但 diff 中没有对应断言。
- **建议:** 添加断言，确认普通 `--help` 不进入中文分支。

<!-- yg:issue severity=中等 -->
### ISSUE #5: [中等] `chinese_help_text()` 对静态字面量返回 `String` ✅
- **原因:** 函数对 `r#"..."#` 字面量调用 `.to_string()`，造成不必要的堆分配。帮助文本是只读的，返回 `&'static str` 更经济、更符合 Rust 习惯。
- **建议:** 返回类型改为 `&'static str`，调用处直接 `println!("{}", chinese_help_text())`。

<!-- yg:issue severity=中等 -->
### ISSUE #6: [中等] `--zh` 的文档注释与实际行为不符 ✅
- **原因:** 注释为 `/// Show Chinese help`，容易让人误以为单独传 `--zh` 就会显示中文 help。实际上 `--zh` 只有在与 `--help`/`-h` 同时出现时才会触发中文 help。
- **建议:** 改为 `/// Use Chinese for help output (with --help)` 或更精确的描述。

<!-- yg:issue severity=轻度 -->
### ISSUE #7: [轻度] `print_chinese_help` 名称掩盖了副作用 ✅
- **原因:** 函数名暗示只打印，实际还会 `exit(0)`，语义不透明。
- **建议:** 若保留当前实现，可改名为 `print_chinese_help_and_exit`；若按 ISSUE #2 重构，则副作用自然消失。

<!-- yg:issue severity=轻度 -->
### ISSUE #8: [轻度] 本次 staged 变更包含非代码产物 ✅（用户决定保持打包）
- **原因:** Spec 要求“All logic in `args.rs`”，但 staged diff 还包含 `AGENTS.md` 和 `docs/superpowers/specs/2026-06-23-chinese-help-design.md`。虽然文档有价值，但与功能代码混在一起，-review 焦点被分散。
- **建议:** 文档/spec 变更与功能代码分两次提交（除非本次有意打包）。

<!-- yg:issue severity=轻度 -->
### ISSUE #9: [轻度] 辅助函数测试超出 spec 范围 ✅（用户决定保留）
- **原因:** Spec 只要求行为级测试，但 diff 额外增加了 `is_chinese_mode()` 和 `is_help_requested()` 的独立单元测试。虽无坏处，但占用了本可用于覆盖核心退出路径的测试资源。
- **建议:** 保留辅助测试作为补充，优先补齐 ISSUE #3、#4 的行为测试。

<!-- yg:issue severity=建议 -->
### ISSUE #10: [建议] 将进程退出逻辑移出 `parse_args()` ✅
- **原因:** 让参数解析器同时负责“解析”和“决定是否退出”会降低可测试性，也与主代理 `main.rs` 的职责边界模糊。
- **建议:** 参考 `clap::Error` 模式，或返回自定义结果，让 `main()` 根据返回值决定打印/退出。

<!-- yg:good -->
### GOOD #11: [优点] 核心实现忠实匹配 spec
- **原因:** `--zh` 仅长标志、无短标志；在 `Cli::parse_from()` 前拦截；中文 help 静态字符串放在 `args.rs`；未做 locale 自动检测——这些均与 spec 一致。

<!-- yg:good -->
### GOOD #12: [优点] 变更范围集中，未侵入无关模块
- **原因:** 所有代码逻辑都收敛在 `args.rs`，没有改动 `main.rs`、`eu_stack.rs`、`gdb.rs` 等模块，符合“局部性优于表面整洁”原则。

---

## Summary

| 类型 | 数量 | 关键项                                                     |
| ---- | ---- | ---------------------------------------------------------- |
| 严重 | 2    | `cargo fmt` 失败；`parse_args()` 内 `exit(0)` 不可内联测试       |
| 中等 | 4    | 缺少 `--zh --help` / 英文 `--help` 测试；`String` 分配；注释误导 |
| 轻度 | 3    | 函数名掩盖副作用；staged 含非代码产物；辅助测试超出 spec   |
| 建议 | 1    | 将退出逻辑移出 `parse_args()`                                |
| 优点 | 2    | 核心实现匹配 spec；变更范围集中                            |
<!-- table not formatted: invalid structure -->

---

## Fix 处理总结

- **已处理**：ISSUE #1~#7、#10（代码修改或重构解决）
- **用户决定保持现状**：ISSUE #8（不拆分文档/spec 提交）、ISSUE #9（保留辅助函数测试）
- **已忽略（优点项）**：GOOD #11、GOOD #12
- **验证**：`cargo fmt -- --check`、`cargo clippy --all-targets --all-features -- -Dwarnings`、`cargo test args::` 均通过
- **说明**：所有代码修改目前保持未暂存状态，需用户自行 `git add` 或指示暂存
