# Chinese help support for `cs`

Date: 2026-06-23
Status: Approved

## Summary

Default `cs --help` output is Chinese. Add `--en` flag to display help text in English when combined with `--help` or `-h`. No automatic locale detection — explicit flag only.

## Changes

### 1. New flag

Add `--en` (long-only, no short flag) to `Cli` struct:

```rust
/// Use English for help output (requires --help or -h)
#[arg(long = "en", default_value_t = false)]
pub english_mode: bool,
```

### 2. Help interception

Before `Cli::parse_from()`, scan raw args:

```
raw args contains (--help or -h) and does not contain --en
  → return ArgsAction::ChineseHelp

otherwise
  → normal clap parsing (english_mode flag preserved in Cli for future use)
```

### 3. Chinese help text

`print_chinese_help()` prints the full Chinese help: program description, all argument descriptions, usage examples. Maintained as a static string block in `args.rs`.

### 4. File location

All logic in `args.rs` — the `Cli` struct definition, `is_english_mode()` helper, `print_chinese_help()`, and the interception in `parse_args()`.

## Testing

- Assert `print_chinese_help()` output contains key Chinese phrases
- Test `--help` exits 0 with Chinese output by default
- Test `--en --help` exits 0 with English output
- Test `--en` without `--help` does not exit (normal run)

## Rationale

- Simple: one flag, one static function, minimal code change
- Low maintenance: args change infrequently; out-of-date Chinese text still degrades gracefully
- No env var detection — user explicitly asked to drop it
