use std::process::exit;

use clap::Parser;

#[derive(Parser, Clone)]
#[command(long_about = None, about = "Tool to show call stack of process(es)",
    arg_required_else_help = true, version, trailing_var_arg=true,
    after_help = r"Usages Examples:
  - `cs --help`:                Show Chinese help (default)
  - `cs --en --help`:           Show English help
  - `cs`:                       Choose process interactive and show's its call stack.
  - `cs -l -u user`:            Show processes of USER.
  - `cs -p 905 -U`:             Show uniue stack for process `905`.
  - `cs --parent 105 -U`:       Show uniue stack for all processes whose parent is `105` beside `105` itself.
  - `cs -U -P google.chrome`:   Show unique stack of all processes of google chrome
  - `cs -U -p 905 -t 0.5 -n 3`: Get callstack for PID 905 for 3 times with interval 0.5 seconds, then uniquify the output.

")]
pub struct Cli {
    /// Show stack of process PID
    #[arg(short = 'p', long = "pid")]
    pub pids: Option<Vec<i32>>,

    /// Show stack of all processes of same group (parent process)
    #[arg(long = "parent", conflicts_with = "pids")]
    pub parent: Option<i32>,

    /// Show stack found in COREFILE
    #[arg(short = 'c', long = "core", conflicts_with = "pids")]
    pub core: Option<String>,

    /// (optional) EXECUTABLE that produced COREFILE
    #[arg(short = 'e', long = "executable", conflicts_with = "pids")]
    pub executable: Option<String>,

    /// Show processes of users (separated by ",") when listing/choosing processes
    #[arg(short = 'u', long = "users")]
    pub users: Option<String>,

    /// Initial value to filter process
    #[arg(short = 'i', long = "initial")]
    pub initial: Option<String>,

    /// List processes
    #[arg(short = 'l', long = "list", default_value_t = false)]
    pub list: bool,

    /// Specify  update  interval as seconds, it should not be quicker than 0.1.
    /// Applies only when getting callstack from running app.
    #[arg(short = 't', long = "interval", verbatim_doc_comment)]
    pub interval: Option<f32>,

    /// Specify number of sampling.
    /// Applies only when getting callstack from running app, and `interval`
    /// is specified.
    #[arg(
        short = 'n',
        long = "count",
        default_value_t = 1,
        requires = "interval",
        verbatim_doc_comment
    )]
    pub count: i32,

    /// Specify number of frames when getting call stack, 0 means unlimited.
    #[arg(short = 'f', long = "frames", default_value_t = 2048)]
    pub frames: i32,

    /// Wide mode: when showing processes, show all chars in a line
    #[arg(short = 'W', long = "Wide", default_value_t = false)]
    pub wide_mode: bool,

    /// Multi mode: when choosing processes, to select multiple processes
    #[arg(short = 'M', long = "multi", default_value_t = false)]
    pub multi_mode: bool,

    /// Unique mode: when showing call stack, show only unique ones
    #[arg(short = 'U', long = "unique", default_value_t = false)]
    pub unique_mode: bool,

    /// gdb mode: use gdb to get call stack (default to eu-stack)
    #[arg(short = 'G', long = "gdb", default_value_t = false)]
    pub gdb_mode: bool,

    /// Raw mode: do not try to simplify callstacks (works with `-G` only)
    #[arg(short = 'R', long = "raw", default_value_t = false)]
    pub raw_mode: bool,

    /// Disable pager
    #[arg(short = 'N', long = "no-pager", default_value_t = false)]
    pub no_pager: bool,

    /// Show call stacks of processes whose name matches PATTERN.
    #[arg(short = 'P', long = "pattern")]
    pub pattern: Option<String>,

    /// Use English for help output (requires --help or -h)
    #[arg(long = "en", default_value_t = false)]
    pub english_mode: bool,

    /// files to read stack from, use "-" for stdin; multiple files will be merged together.
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
            raw_mode: true,
            files: vec![],
            no_pager: false,
            pattern: None,
            english_mode: false,
        }
    }
}

fn is_english_mode(raw_args: &[String]) -> bool {
    raw_args.iter().any(|a| a == "--en")
}

fn is_help_requested(raw_args: &[String]) -> bool {
    raw_args.iter().any(|a| a == "-h" || a == "--help")
}

fn chinese_help_text() -> &'static str {
    r#"进程调用栈分析工具

用法: cs [选项] [文件]...

参数:
  [文件]...      读取调用栈的文件，使用 "-" 表示标准输入；多个文件会被合并

选项:
  -p, --pid <PID>               显示指定 PID 进程的调用栈
      --parent <PARENT>         显示指定父进程的所有子进程的调用栈
  -c, --core <COREFILE>         显示 coredump 中的调用栈
  -e, --executable <EXECUTABLE> (可选) 产生 coredump 的可执行文件
  -u, --users <USERS>           列出或选择指定用户的进程（多个用户用逗号分隔）
  -i, --initial <INITIAL>       进程过滤的初始值
  -l, --list                    列出进程
  -t, --interval <INTERVAL>     采样间隔（秒），最小 0.1 秒
  -n, --count <COUNT>           采样次数（需与 -t 配合使用）[默认: 1]
  -f, --frames <FRAMES>         获取调用栈的帧数，0 表示不限制 [默认: 2048]
  -W, --Wide                    宽模式：显示进程时显示所有字符
  -M, --multi                   多选模式：选择进程时支持多选
  -U, --unique                  去重模式：显示调用栈时合并相同栈
  -G, --gdb                     使用 gdb 获取调用栈（默认使用 eu-stack）
  -R, --raw                     原始模式：不对调用栈做简化（仅与 -G 配合使用）
  -N, --no-pager                禁用分页器
  -P, --pattern <PATTERN>       显示名称匹配 PATTERN 的进程的调用栈
      --en                      显示英文帮助（需与 --help 或 -h 配合使用）
  -h, --help                    打印帮助
  -V, --version                 打印版本

使用示例:
  - `cs --help`:                显示中文帮助信息（默认）
  - `cs --en --help`:           显示英文帮助信息 (Show English help)
  - `cs`:                       交互式选择进程并显示调用栈
  - `cs -l -u user`:            显示 USER 用户的进程
  - `cs -p 905 -U`:             显示进程 905 的去重调用栈
  - `cs --parent 105 -U`:       显示父进程 105 的所有子进程的去重调用栈
  - `cs -U -P google.chrome`:   显示所有 google chrome 进程的去重调用栈
  - `cs -U -p 905 -t 0.5 -n 3`: 采样进程 905 的调用栈 3 次（间隔 0.5 秒），然后去重输出
"#
}

pub fn print_chinese_help() {
    println!("{}", chinese_help_text());
}

pub enum ArgsAction {
    Run(Cli),
    ChineseHelp,
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
        if is_help_requested(&args) && !is_english_mode(&args) {
            return ArgsAction::ChineseHelp;
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
        ArgsAction::ChineseHelp => panic!("expected Run, got ChineseHelp"),
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
fn test_chinese_help_contains_key_phrases() {
    let text = chinese_help_text();
    assert!(text.contains("进程调用栈分析工具"));
    assert!(text.contains("用法"));
    assert!(text.contains("--pid"));
    assert!(text.contains("--en"));
    assert!(text.contains("示例"));
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
fn test_default_help_is_chinese() {
    assert!(matches!(
        parse_args(vec!["cs", "--help"]),
        ArgsAction::ChineseHelp
    ));
    assert!(matches!(
        parse_args(vec!["cs", "-h"]),
        ArgsAction::ChineseHelp
    ));
}

#[test]
fn test_en_help_action() {
    assert!(matches!(
        parse_args(vec!["cs", "--en", "--help"]),
        ArgsAction::Run(_)
    ));
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
    assert!(stdout.contains("Tool to show call stack"));
}
