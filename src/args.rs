use std::process::exit;

use clap::Parser;

#[derive(Parser, Clone)]
#[command(long_about = None, about = "Tool to show call stack of process(es)",
          arg_required_else_help = true, version, trailing_var_arg=true)]
pub struct Cli {
    #[arg(short = 'p', long = "pid", help = "Show stack of process PID")]
    pub pids: Option<Vec<String>>,

    /// Show stack found in COREFILE
    #[arg(short = 'c', long = "core", conflicts_with = "pids")]
    pub core: Option<String>,

    /// (optional) EXECUTABLE that produced COREFILE
    #[arg(short = 'e', long = "executable", conflicts_with = "pids")]
    pub executable: Option<String>,

    /// Show processes of users (separated by \",\") when listing/choosing processes
    #[arg(short = 'u', long = "users")]
    pub users: Option<String>,

    /// List processes
    #[arg(short = 'l', long = "list", default_value_t = false)]
    pub list: bool,

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

    /// Raw mode: do not try to simplify callstacks (works only in GDB mode)
    #[arg(short = 'R', long = "raw", default_value_t = false)]
    pub raw_mode: bool,

    /// Disable pager
    #[arg(short = 'N', long = "no-pager", default_value_t = false)]
    pub no_pager: bool,

    /// Show call stacks of processes whose name matches PATTERN.
    #[arg(short = 'P', long = "pattern")]
    pub pattern: Option<String>,

    #[clap(allow_hyphen_values=true, num_args=0.., help=r#"Purpose of these args may change based other options:
1. When listing/choosing processes, this is default initial value or pattern to filter process.
2. When uniquifying stacks, this could be files to read stack from ("-" for stdin); multiple
   files will be merged together."#)]
    pub args: Vec<String>,
}

impl Cli {
    pub fn default() -> Cli {
        Self {
            pids: None,
            core: None,
            executable: None,
            users: None,
            list: false,
            wide_mode: false,
            multi_mode: false,
            unique_mode: false,
            gdb_mode: false,
            raw_mode: true,
            args: vec![],
            no_pager: false,
            pattern: None,
        }
    }
}

pub fn parse_args<T, S>(args: T) -> Cli
where
    T: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args = args.into_iter().map(|x| x.into()).collect::<Vec<String>>();
    if args.len() == 1 {
        Cli::default()
    } else {
        let cli = Cli::parse_from(args);
        if cli.args.len() > 1 && cli.args.contains(&"-".to_owned()) {
            eprintln!("stdin should not be used together with other files");
            exit(2);
        } else if cli.args.len() > 1 {
            for arg in cli.args.clone() {
                if arg.starts_with('-') {
                    eprintln!("Failed to parse arg: {arg}");
                    exit(2);
                }
            }
        }
        cli
    }
}

#[test]
fn test_parse_args() {
    let cli = parse_args(vec!["st", "--pid", "1000"]);
    assert_eq!(cli.pids.unwrap().first().unwrap(), "1000");
    assert!(!cli.unique_mode);
    assert!(cli.users.is_none());
    assert!(cli.gdb_mode == false);
    assert!(cli.args.is_empty());

    let cli = parse_args(vec!["st", "-U", "-c", "corefile"]);
    assert!(cli.unique_mode);
    assert_eq!(cli.core, Some("corefile".to_owned()));
    assert!(!cli.list);
    assert!(cli.executable.is_none());

    // -c & -e should be able to work together
    let cli = parse_args(vec!["st", "-c", "corefile", "-e", "executable"]);
    assert_eq!(cli.core, Some("corefile".to_owned()));
    assert_eq!(cli.executable, Some("executable".to_owned()));

    let cli = parse_args(vec!["st", "-l", "-u", "someone"]);
    assert!(cli.list);
    assert_eq!(cli.users.unwrap(), "someone");

    // conflict options
    for args in vec![vec!["st", "-c", "corefile", "-p", "1000"]] {
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
    let cli = parse_args(vec!["st", "file-1", "file-2"]);
    assert!(cli.args.len() == 2);
    println!("{:?}", cli.args);

    let cli = parse_args(vec!["st", "-"]);
    assert!(cli.args.len() == 1);
    println!("{:?}", cli.args);
}
