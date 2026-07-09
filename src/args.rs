use std::process::exit;

use clap::{Parser, ValueEnum};

use crate::match_mode::MatchMode;

#[derive(ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    /// 文本格式
    Text,
    /// JSON 格式
    Json,
}

#[derive(Parser, Clone)]
#[command(
    long_about = None,
    about = "进程调用栈分析工具",
    arg_required_else_help = true,
    version,
    trailing_var_arg = true,
    after_help = r#"基本示例:
  - `cs --help`:                        显示本帮助信息
  - `cs`:                               交互式选择进程并显示调用栈
  - `cs -p 905`:                        显示进程 905 的调用栈
  - `cs -p 905 -U`:                     显示进程 905 的去重调用栈
  - `cs -U -P google.chrome`:           显示所有 Chrome 进程的去重调用栈

进阶示例:
  - `cs -U -p 905 -t 0.5 -n 3`:         进程 905 采样 3 次（间隔 0.5 秒），去重输出
  - `cs -p 905 --json > snap.json`:     保存进程 905 的快照为 JSON
  - `cs --diff before.json after.json`:	对比两个 JSON 快照文件
  - `cs --diff-live -p 905 -t 2`:       直接采样两次（间隔 2 秒）并输出 diff
  - `cs --diff-live -p 905`:            按回车触发第二次采样，输出 diff
"#)]
pub struct Cli {
    /// 显示指定 PID 进程的调用栈
    #[arg(short = 'p', long = "pid")]
    pub pids: Option<Vec<i32>>,

    /// 显示父进程 PARENT 的所有子进程的调用栈
    #[arg(long = "parent", conflicts_with = "pids")]
    pub parent: Option<i32>,

    /// 显示 coredump 文件中的调用栈
    #[arg(short = 'c', long = "core", conflicts_with = "pids")]
    pub core: Option<String>,

    /// (可选) 产生 coredump 的可执行文件
    #[arg(short = 'e', long = "executable", conflicts_with = "pids")]
    pub executable: Option<String>,

    /// 列出或选择指定用户的进程（多个用户用逗号分隔）
    #[arg(short = 'u', long = "users")]
    pub users: Option<String>,

    /// 进程过滤的初始值
    #[arg(short = 'i', long = "initial")]
    pub initial: Option<String>,

    /// 列出进程
    #[arg(short = 'l', long = "list", default_value_t = false)]
    pub list: bool,

    /// 采样间隔（秒），不应小于 0.1 秒。
    /// 仅在获取运行中进程的调用栈时适用。
    #[arg(short = 't', long = "interval", verbatim_doc_comment)]
    pub interval: Option<f32>,

    /// 采样次数。
    /// 仅在获取运行中进程的调用栈时适用，且需指定 `interval`。
    #[arg(
        short = 'n',
        long = "count",
        default_value_t = 1,
        requires = "interval",
        verbatim_doc_comment
    )]
    pub count: i32,

    /// 获取调用栈的帧数，0 表示不限制
    #[arg(short = 'F', long = "frames", default_value_t = 2048)]
    pub frames: i32,

    /// 宽模式：显示进程时显示所有字符
    #[arg(short = 'W', long = "Wide", default_value_t = false)]
    pub wide_mode: bool,

    /// 多选模式：选择进程时支持多选
    #[arg(short = 'M', long = "multi", default_value_t = false)]
    pub multi_mode: bool,

    /// 去重模式：显示调用栈时合并相同栈
    #[arg(short = 'U', long = "unique", default_value_t = false)]
    pub unique_mode: bool,

    /// 使用 gdb 获取调用栈（默认使用 eu-stack）
    #[arg(short = 'G', long = "gdb", default_value_t = false)]
    pub gdb_mode: bool,

    /// 原始模式：不对调用栈做简化（仅与 -G 配合使用）
    #[arg(short = 'R', long = "raw", default_value_t = false)]
    pub raw_mode: bool,

    /// 禁用分页器
    #[arg(short = 'N', long = "no-pager", default_value_t = false)]
    pub no_pager: bool,

    /// 显示名称匹配 PATTERN 的进程的调用栈
    #[arg(short = 'P', long = "pattern")]
    pub pattern: Option<String>,

    /// 显示英文帮助（需与 --help 或 -h 配合使用）
    #[arg(long = "en", default_value_t = false)]
    pub english_mode: bool,

    /// 以 JSON 格式输出堆栈信息
    #[arg(short = 'j', long = "json", default_value_t = false)]
    pub json_mode: bool,

    /// 堆栈去重时的匹配模式：precise（按完整帧信息去重）或 fuzzy（仅按函数名去重）。
    /// 默认 auto：单进程用 precise，多进程/文件/diff 用 fuzzy。
    #[arg(long = "match", value_enum, verbatim_doc_comment)]
    pub match_mode: Option<MatchMode>,

    /// 排除匹配指定正则的堆栈（可重复使用，匹配帧的 function 字段）
    #[arg(short = 'E', long = "exclude", verbatim_doc_comment)]
    pub exclude: Vec<String>,

    /// 输出格式：text 或 json（优先于 --json）
    #[arg(short = 'f', long = "format", value_enum)]
    pub format: Option<OutputFormat>,

    /// 显示完整输出（不截断）
    #[arg(short = 'v', long = "verbose", default_value_t = false)]
    pub verbose: bool,

    /// 对比两个 JSON 格式的堆栈快照文件
    #[arg(long = "diff", num_args = 2, value_names = ["BEFORE", "AFTER"])]
    pub diff: Option<Vec<String>>,

    /// 仅采样两次（before + after）并输出 diff。
    /// 配合 -t 可指定间隔秒数，不带 -t 则等待按回车触发第二次。
    #[arg(
        long = "diff-live",
        conflicts_with = "diff",
        conflicts_with = "count",
        verbatim_doc_comment
    )]
    pub diff_live: bool,

    /// 读取调用栈的文件，使用 "-" 表示标准输入；多个文件会被合并
    #[clap(allow_hyphen_values=true, num_args=0..,)]
    pub files: Vec<String>,
}

impl Cli {
    pub fn default() -> Cli {
        Self {
            pids: None,
            parent: None,
            core: None,
            executable: None,
            users: None,
            list: false,
            initial: None,
            interval: None,
            count: 1,
            frames: 2048,
            wide_mode: false,
            multi_mode: false,
            unique_mode: false,
            gdb_mode: false,
            raw_mode: false,
            files: vec![],
            no_pager: false,
            pattern: None,
            english_mode: false,
            json_mode: false,
            diff: None,
            diff_live: false,
            match_mode: None,
            exclude: vec![],
            format: None,
            verbose: false,
        }
    }

    pub fn effective_json_mode(&self) -> bool {
        match self.format {
            Some(OutputFormat::Json) => true,
            Some(OutputFormat::Text) => false,
            None => self.json_mode,
        }
    }

    pub fn effective_match_mode(&self) -> MatchMode {
        if let Some(mode) = self.match_mode {
            return mode;
        }
        if self.diff.is_some() || self.diff_live || self.is_multi_source() {
            MatchMode::Fuzzy
        } else {
            MatchMode::Precise
        }
    }

    fn is_multi_source(&self) -> bool {
        self.diff.is_some()
            || self.diff_live
            || !self.files.is_empty()
            || self.parent.is_some()
            || self.pattern.is_some()
            || self.pids.as_ref().is_some_and(|p| p.len() > 1)
    }

    pub fn warn_if_match_conflict(&self) {
        if self.match_mode.is_some() && !self.unique_mode && self.diff.is_none() && !self.diff_live
        {
            eprintln!(
                "warning: --match has no effect without --unique (-U).\n\
                 Use -U to enable stack dedup."
            );
        }
        if self.match_mode == Some(MatchMode::Precise) && self.is_multi_source() {
            eprintln!(
                "warning: --match precise with multiple input sources may cause\n\
                 identical stacks to appear different due to ASLR.\n\
                 Consider using --match fuzzy (or omit --match for auto)."
            );
        }
    }
}

fn is_english_mode(raw_args: &[String]) -> bool {
    raw_args.iter().any(|a| a == "--en")
}

fn is_help_requested(raw_args: &[String]) -> bool {
    raw_args.iter().any(|a| a == "-h" || a == "--help")
}

fn english_help_text() -> &'static str {
    r#"Call stack analysis tool for processes

Usage: cs [OPTIONS] [FILES]...

Arguments:
  [FILES]...      files to read stack from ("-" for stdin); multiple files will be merged

Options:
  -p, --pid <PIDS>               Show stack of process PID
      --parent <PARENT>          Show stack of all child processes of PARENT
  -c, --core <COREFILE>          Show stack found in COREFILE
  -e, --executable <EXECUTABLE>  EXECUTABLE that produced COREFILE (optional)
  -u, --users <USERS>            Filter processes by user list (comma-separated)
  -i, --initial <INITIAL>        Initial value to filter process
  -l, --list                     List processes
  -t, --interval <INTERVAL>      Sampling interval in seconds (min: 0.1).
                                 Applies only when sampling a running process.
  -n, --count <COUNT>            Number of samples to take.
                                 Applies only when sampling a running process with `-t`.
                                 [default: 1]
  -F, --frames <FRAMES>          Frame count to show (0 = unlimited) [default: 2048]
  -W, --Wide                     Wide mode: show full line in process list
  -M, --multi                    Multi mode: allow selecting multiple processes
  -U, --unique                   Unique mode: deduplicate identical stacks
  -G, --gdb                      Use gdb instead of eu-stack
  -R, --raw                      Raw mode: skip stack simplification (works with `-G` only)
  -N, --no-pager                 Disable pager
  -P, --pattern <PATTERN>        Filter processes by name PATTERN
  -E, --exclude <PATTERN>        Exclude stacks matching PATTERN (regex on function field)
      --en                       Show English help (requires --help or -h)
      --diff <BEFORE> <AFTER>    Diff two JSON stack snapshot files
      --diff-live                Live diff: sample twice, show diff
  -j, --json                     JSON output format
  -f, --format <FMT>             Output format: text or json (overrides --json)
  -v, --verbose                  Show full output (no truncation)
      --match <MODE>             Stack dedup match mode:
                                    - precise (full frame info) or
                                    - fuzzy (function names only).
                                    - Auto: precise for single source, fuzzy for multi-source/diff
  -h, --help                     Print help
  -V, --version                  Print version

Basic Usage:
  - `cs --help`:                        Show Chinese help (default)
  - `cs`:                               Choose process and show its call stack.
  - `cs -p 905`:                        Show stack of process 905.
  - `cs -p 905 -U`:                     Show unique stack for process 905.
  - `cs -l -u user`:                    Show processes of USER.
  - `cs -U -P google.chrome`:           Unique stacks of all Chrome processes.

Advanced Usage:
  - `cs -U -p 905 -t 0.5 -n 3`:         Sample 905 3 times (0.5s apart), uniquify.
  - `cs -p 905 --json > snap.json`:     Save stack snapshot as JSON.
  - `cs --diff before.json after.json`:	Diff two JSON snapshot files.
  - `cs --diff-live -p 905 -t 2`:       Sample twice (2s interval), show diff.
  - `cs --diff-live -p 905`:            Press ENTER for second sample, show diff.
"#
}

pub fn print_english_help() {
    println!("{}", english_help_text());
}

#[allow(clippy::large_enum_variant)]
pub enum ArgsAction {
    Run(Cli),
    EnglishHelp,
}

pub fn parse_args<T, S>(args: T) -> ArgsAction
where
    T: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args = args.into_iter().map(|x| x.into()).collect::<Vec<String>>();
    if args.len() == 1 {
        ArgsAction::Run(Cli::default())
    } else {
        if is_help_requested(&args) && is_english_mode(&args) {
            return ArgsAction::EnglishHelp;
        }

        let mut cli = Cli::parse_from(args);
        if cli.files.len() > 1 && cli.files.contains(&"-".to_owned()) {
            eprintln!("stdin should not be used together with other files");
            exit(2);
        } else if cli.files.len() > 1 {
            for arg in cli.files.clone() {
                if arg.starts_with('-') {
                    eprintln!("Failed to parse arg: {arg}");
                    exit(2);
                }
            }
        }

        // check and update interval, minimum value should be 0.1s
        if let Some(interval) = cli.interval {
            if interval < 0.1 {
                cli.interval.replace(0.1);
            }
        };

        ArgsAction::Run(cli)
    }
}

#[cfg(test)]
fn unwrap_run(action: ArgsAction) -> Cli {
    match action {
        ArgsAction::Run(cli) => cli,
        ArgsAction::EnglishHelp => panic!("expected Run, got EnglishHelp"),
    }
}

#[tokio::test]
async fn test_parse_args() {
    let cli = unwrap_run(parse_args(vec!["cs", "--pid", "1000"]));
    assert_eq!(cli.pids.unwrap().first().unwrap(), &1000);
    assert!(!cli.unique_mode);
    assert!(cli.users.is_none());
    assert!(!cli.gdb_mode);
    assert!(cli.files.is_empty());

    let cli = unwrap_run(parse_args(vec!["cs", "-U", "-c", "corefile"]));
    assert!(cli.unique_mode);
    assert_eq!(cli.core, Some("corefile".to_owned()));
    assert!(!cli.list);
    assert!(cli.executable.is_none());

    // -c & -e should be able to work together
    let cli = unwrap_run(parse_args(vec!["cs", "-c", "corefile", "-e", "executable"]));
    assert_eq!(cli.core, Some("corefile".to_owned()));
    assert_eq!(cli.executable, Some("executable".to_owned()));

    let cli = unwrap_run(parse_args(vec!["cs", "-l", "-u", "someone"]));
    assert!(cli.list);
    assert_eq!(cli.users.unwrap(), "someone");

    // conflict options
    for args in [vec!["cs", "-c", "corefile", "-p", "1000"]] {
        match Cli::try_parse_from(args) {
            Ok(_) => {
                panic!();
            }
            Err(err) => {
                assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
            }
        }
    }

    // trailing args should be files
    let cli = unwrap_run(parse_args(vec!["cs", "file-1", "file-2"]));
    assert!(cli.files.len() == 2);
    println!("{:?}", cli.files);

    let cli = unwrap_run(parse_args(vec!["cs", "-"]));
    assert!(cli.files.len() == 1);
    println!("{:?}", cli.files);

    let cli = unwrap_run(parse_args(vec!["cs", "-t", "0.001", "-n", "3"]));
    assert_eq!(cli.interval.unwrap(), 0.1);
    assert_eq!(cli.count, 3);
}

#[test]
fn test_is_english_mode() {
    assert!(is_english_mode(&["cs".into(), "--en".into()]));
    assert!(!is_english_mode(&["cs".into()]));
    assert!(!is_english_mode(&["cs".into(), "--help".into()]));
}

#[test]
fn test_is_help_requested() {
    assert!(is_help_requested(&["cs".into(), "--help".into()]));
    assert!(is_help_requested(&["cs".into(), "-h".into()]));
    assert!(!is_help_requested(&["cs".into(), "--en".into()]));
}

#[test]
fn test_english_help_contains_key_phrases() {
    let text = english_help_text();
    assert!(text.contains("Call stack analysis tool"));
    assert!(text.contains("Usage"));
    assert!(text.contains("--pid"));
    assert!(text.contains("--en"));
    assert!(text.contains("Basic Usage"));
}

#[test]
fn test_en_flag_with_help_not_present() {
    let cli = unwrap_run(parse_args(vec!["cs", "--en", "--pid", "1000"]));
    assert!(cli.english_mode);
    assert_eq!(cli.pids.unwrap().first().unwrap(), &1000);
}

#[test]
fn test_en_flag_not_present() {
    let cli = unwrap_run(parse_args(vec!["cs", "--pid", "1000"]));
    assert!(!cli.english_mode);
}

#[test]
fn test_en_help_action() {
    assert!(matches!(
        parse_args(vec!["cs", "--en", "--help"]),
        ArgsAction::EnglishHelp
    ));
}

#[test]
fn test_match_mode_default_precise() {
    let cli = Cli {
        files: vec![],
        pids: None,
        parent: None,
        pattern: None,
        match_mode: None,
        ..Cli::default()
    };
    assert_eq!(cli.effective_match_mode(), MatchMode::Precise);
}

#[test]
fn test_match_mode_files_implies_fuzzy() {
    let cli = Cli {
        files: vec!["file.stack".into()],
        match_mode: None,
        ..Cli::default()
    };
    assert_eq!(cli.effective_match_mode(), MatchMode::Fuzzy);
}

#[test]
fn test_match_mode_multi_pid_implies_fuzzy() {
    let cli = Cli {
        pids: Some(vec![100, 101]),
        match_mode: None,
        ..Cli::default()
    };
    assert_eq!(cli.effective_match_mode(), MatchMode::Fuzzy);
}

#[test]
fn test_match_mode_explicit_override() {
    let cli = Cli {
        files: vec!["f.stack".into()],
        match_mode: Some(MatchMode::Precise),
        ..Cli::default()
    };
    assert_eq!(cli.effective_match_mode(), MatchMode::Precise);
}

#[test]
fn test_match_mode_diff_implies_fuzzy() {
    let cli = Cli {
        diff: Some(vec!["a.json".into(), "b.json".into()]),
        match_mode: None,
        ..Cli::default()
    };
    assert_eq!(cli.effective_match_mode(), MatchMode::Fuzzy);
}

#[test]
fn test_match_mode_diff_live_implies_fuzzy() {
    let cli = Cli {
        diff_live: true,
        match_mode: None,
        ..Cli::default()
    };
    assert_eq!(cli.effective_match_mode(), MatchMode::Fuzzy);
}

#[test]
fn test_match_mode_diff_can_override_to_precise() {
    let cli = Cli {
        diff: Some(vec!["a.json".into(), "b.json".into()]),
        match_mode: Some(MatchMode::Precise),
        ..Cli::default()
    };
    assert_eq!(cli.effective_match_mode(), MatchMode::Precise);
    // precise with multi-source should trigger warning
    cli.warn_if_match_conflict();
}

#[test]
fn test_effective_json_mode_default() {
    let cli = Cli::default();
    assert!(!cli.effective_json_mode());
}

#[test]
fn test_effective_json_mode_json_flag() {
    let cli = Cli {
        json_mode: true,
        ..Cli::default()
    };
    assert!(cli.effective_json_mode());
}

#[test]
fn test_effective_json_mode_format_overrides() {
    let cli = Cli {
        format: Some(OutputFormat::Json),
        json_mode: false,
        ..Cli::default()
    };
    assert!(cli.effective_json_mode());

    let cli = Cli {
        format: Some(OutputFormat::Text),
        json_mode: true,
        ..Cli::default()
    };
    assert!(!cli.effective_json_mode());
}

#[test]
fn test_format_json_equivalent_to_json() {
    let cli = unwrap_run(parse_args(vec!["cs", "--format", "json"]));
    assert!(cli.effective_json_mode());
}

#[test]
fn test_format_text_overrides_json() {
    let cli = unwrap_run(parse_args(vec!["cs", "--format", "text", "--json"]));
    assert!(!cli.effective_json_mode());
}

#[test]
fn test_format_json_overrides_json() {
    let cli = unwrap_run(parse_args(vec!["cs", "--format", "json", "--json"]));
    assert!(cli.effective_json_mode());
}

#[test]
fn test_short_j_equivalent_to_json() {
    let cli = unwrap_run(parse_args(vec!["cs", "-j"]));
    assert!(cli.json_mode);
    assert!(cli.effective_json_mode());
}

#[test]
fn test_short_f_for_format() {
    let cli = unwrap_run(parse_args(vec!["cs", "-f", "json"]));
    assert!(matches!(cli.format, Some(OutputFormat::Json)));
    assert!(cli.effective_json_mode());
}

#[test]
#[allow(non_snake_case)]
fn test_short_F_for_frames() {
    let cli = unwrap_run(parse_args(vec!["cs", "-F", "10"]));
    assert_eq!(cli.frames, 10);
}

#[test]
fn test_exclude_repeatable() {
    let cli = unwrap_run(parse_args(vec!["cs", "-E", "foo", "-E", "bar"]));
    assert_eq!(cli.exclude.len(), 2);
    assert_eq!(cli.exclude[0], "foo");
    assert_eq!(cli.exclude[1], "bar");
}

#[cfg(test)]
fn run_cs(args: &[&str]) -> std::process::Output {
    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("run").arg("--").args(args);
    cmd.current_dir(env!("CARGO_MANIFEST_DIR"));
    cmd.output().expect("failed to run cargo run")
}

#[test]
fn test_default_help_subprocess() {
    let output = run_cs(&["--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("进程调用栈分析工具"));
}

#[test]
fn test_en_help_subprocess() {
    let output = run_cs(&["--en", "--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Call stack analysis tool"));
}
