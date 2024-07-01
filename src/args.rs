use clap::Parser;

const ST_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(long_about = None, about = "Tool to show call stack of process(es)",
    arg_required_else_help = true, version=ST_VERSION )]
pub struct Cli {
    #[arg(short = 'p', long = "pid", help = "Show stack of process PID")]
    pub pids: Option<Vec<String>>,

    #[arg(
        short = 'c',
        long = "core",
        help = "Show stack found in COREFILE",
        conflicts_with = "pids"
    )]
    pub core: Option<String>,

    #[arg(
        short = 'e',
        long = "executable",
        help = "(optional) EXECUTABLE that produced COREFILE",
        conflicts_with = "pids"
    )]
    pub executable: Option<String>,

    #[arg(
        short = 'u',
        long = "users",
        help = "Show processes of users(separated by \",\"), effective when listing and choosing processes"
    )]
    pub users: Option<String>,

    #[arg(short='l', long = "list", help = "List processes", num_args=0..2,)]
    pub list: Option<Vec<String>>,

    #[arg(
        short = 'i',
        long = "initial",
        help = "Initial value to filter process"
    )]
    pub initial: Option<String>,

    #[arg(
        short = 'f',
        long = "file",
        help = "read call stacks from file",
        conflicts_with = "gdb_mode"
    )]
    pub file: Option<String>,

    #[arg(
        short,
        long = "stdin",
        help = "read call stacks from file",
        conflicts_with = "pids",
        conflicts_with = "core"
    )]
    pub stdin: bool,

    #[arg(
        short = 'W',
        long = "Wide",
        help = "Wide mode: when showing processes, show all chars in a line",
        default_value_t = false
    )]
    pub wide_mode: bool,

    #[arg(
        short = 'M',
        long = "multi",
        help = "Multi mode: when choosing processes, to select multiple processes",
        default_value_t = false
    )]
    pub multi_mode: bool,

    #[arg(
        short = 'U',
        long = "unique",
        help = "Unique mode: when showing call stack, show only unique ones",
        default_value_t = false
    )]
    pub unique_mode: bool,

    #[arg(
        short = 'G',
        long = "gdb",
        help = "gdb mode: use gdb to get call stack (default to eu-stack)",
        default_value_t = false
    )]
    pub gdb_mode: bool,

    #[arg(
        short = 'R',
        long = "raw",
        help = "Row mode: do not try to simpilfy callstacks (works only in GDB mode)",
        default_value_t = false
    )]
    pub raw_mode: bool,
}

impl Cli {
    fn default() -> Cli {
        Self {
            pids: None,
            core: None,
            executable: None,
            users: None,
            list: None,
            initial: None,
            file: None,
            stdin: false,
            wide_mode: false,
            multi_mode: false,
            unique_mode: false,
            gdb_mode: false,
            raw_mode: true,
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
        Cli::parse_from(args)
    }
}

#[test]
fn test_parse_args() {
    let cli = parse_args(vec!["st", "--pid", "1000"]);
    assert_eq!(cli.pids.unwrap().first().unwrap(), "1000");
    assert!(!cli.unique_mode);
    assert!(cli.initial.is_none());
    assert!(cli.users.is_none());
    assert!(cli.gdb_mode == false);

    let cli = parse_args(vec!["st", "-U", "-c", "corefile"]);
    assert!(cli.unique_mode);
    assert_eq!(cli.core, Some("corefile".to_owned()));
    assert!(cli.list.is_none());
    assert!(cli.executable.is_none());

    // -c & -e should be able to work together
    let cli = parse_args(vec!["st", "-c", "corefile", "-e", "executable"]);
    assert_eq!(cli.core, Some("corefile".to_owned()));
    assert_eq!(cli.executable, Some("executable".to_owned()));

    let cli = parse_args(vec!["st", "-l", "-u", "someone"]);
    assert!(cli.list.is_some() && cli.list.unwrap().is_empty());
    assert_eq!(cli.users.unwrap(), "someone");

    let cli = parse_args(vec!["st", "-l", "emacs"]);
    match cli.list {
        Some(v) => {
            assert_eq!(v.len(), 1);
            assert_eq!(v[0], "emacs");
        }
        None => {
            panic!();
        }
    }

    let cli = parse_args(vec!["st", "-i", "pattern1"]);
    assert!(cli.initial.unwrap() == "pattern1");

    // conflict options
    for args in vec![
        vec!["st", "-c", "corefile", "-p", "1000"],
        vec!["st", "--stdin", "-p", "1000"],
        vec!["st", "--stdin", "-c", "core"],
        vec!["st", "-G", "-f", "somefile"],
    ] {
        match Cli::try_parse_from(args) {
            Ok(_) => {
                panic!();
            }
            Err(err) => {
                assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
            }
        }
    }
}
