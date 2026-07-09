# cs — Call stack tool

Rust CLI tool for analyzing process call stacks. Uses `eu-stack` by default, falls back to `gdb`.

## Quick start

```sh
cargo build
cargo test
```

## CI gate (run before push)

```sh
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -Dwarnings
cargo build --locked --release
cargo test --locked
```

Run them in this order — `cargo fmt` first, then clippy, build, test.

## Testing

- `cargo test` — runs all tests (inline `#[test]` / `#[tokio::test]`, no external framework)
- Tests live inside `args.rs`, `uniquify.rs`, `utils.rs` alongside production code
- No separate test directory, no test fixtures
- Some tests shell out to real binaries (`eu-stack`, `gdb`, `ps`, `ls`) and will fail if those are missing

## Architecture

```
main.rs          — entrypoint, dispatches to eu_stack / gdb / uniquify
args.rs          — clap CLI definition + arg parsing + arg tests
eu_stack.rs      — runs eu-stack on pids / core files
gdb.rs           — runs gdb --batch -p on pids
uniquify.rs      — deduplicates stack traces (eu-stack format & gdb format)
utils.rs         — process listing (ps), terminal, pager, test helpers
```

- Single binary crate (`src/main.rs`), no library crate
- Tokio async runtime (`#[tokio::main]`)
- External deps needed at runtime: `eu-stack`, `gdb`, `ps`

## CLI quirks

- No args → interactive mode (inquire-based process picker)
- `-G` forces gdb mode; otherwise auto-detects `eu-stack` availability
- `-R` (raw) only works with `-G`
- `-t` interval minimum is 0.1s (auto-clamped)
- Stdin input: pass `-` as a positional arg
- Pager auto-enabled unless `-N` or `TERM=dumb`

## Help 输出

clap 中 `-h` 使用短帮助（紧凑，一行一个选项），`--help` 使用长帮助（每个选项的
名称和描述分两行显示）。这是 clap 内置行为，`help_template` 和 `next_line_help`
无法改变。

如果某个选项的 `--help` 描述太长，将 `///` doc comment 拆成多行，并配合
`verbatim_doc_comment` 属性保留换行：

```rust
/// 仅采样两次（before + after）并输出 diff。
/// 配合 -t 可指定间隔秒数，不带 -t 则等待按回车触发第二次。
#[arg(long = "diff-live", ..., verbatim_doc_comment)]
```

当前版本使用 `help_template` 自定义了格式，但选项级别仍为长格式。

## Dist build

Cross-compile targets in CI (via `cross`): `aarch64-unknown-linux-musl`, `x86_64-unknown-linux-musl`, `aarch64-apple-darwin`.

Feature flag: `runtime-agnostic` (empty — marker only).

## Other notes

- No `rustfmt.toml` or `clippy.toml` — uses Rust defaults
- No MSRV declared in `Cargo.toml`
- `.agent-shell/transcripts/` — old session transcripts, not relevant for development
- `images/` and `src/images/` exist but are empty
- 如果变更了 `args.rs` （例如，增加或者修改了参数），要更新 `README.org` 里面的使用说明示例。
