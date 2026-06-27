use inquire::{MultiSelect, Select};
use pager::Pager;
use std::sync::LazyLock;
use std::{ffi::OsStr, process::Stdio, sync::OnceLock};
use termion::terminal_size;
use tokio::process::Command;

use crate::args::Cli;
use crate::stack_data::SamplingInfo;

pub async fn execute_command<S, I>(
    command: &str,
    args: I,
) -> Result<(i32, String, String), std::io::Error>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let child = Command::new(command)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let output = child.wait_with_output().await.unwrap();
    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok((exit_code, stdout, stderr))
}

pub async fn collect_samples(
    command: &str,
    args: &[String],
    interval: Option<f32>,
    count: i32,
) -> Result<Vec<String>, String> {
    let effective_count = if interval.is_none() { 1 } else { count };
    let sleep = interval.unwrap_or(0.0);
    let mut raw_outputs = Vec::new();

    for i in 0..effective_count {
        match execute_command(command, args).await {
            Ok((code, out, err)) => {
                if code <= 1 {
                    if !err.is_empty() {
                        eprintln!("Warnings reported: {err}");
                    }
                    raw_outputs.push(out);
                } else {
                    return Err(err);
                }
            }
            Err(err) => return Err(err.to_string()),
        }
        if i < effective_count - 1 {
            tokio::time::sleep(tokio::time::Duration::from_secs_f32(sleep)).await;
        }
    }
    Ok(raw_outputs)
}

static RE_PARSE_PID: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r#"\s*(?P<pid>\d+)\s+"#).unwrap());

fn parse_pid(s: &str) -> i32 {
    let m = RE_PARSE_PID.captures(s).expect("capture fails");
    m.name("pid").unwrap().as_str().parse::<i32>().unwrap()
}

async fn get_process_list(users: Option<String>) -> Result<Vec<String>, String> {
    let mut args: Vec<String> = vec!["-o", "pid,ppid,user,stime,cmd"]
        .into_iter()
        .map(|s| s.to_owned())
        .collect();
    if let Some(users) = users {
        args.extend(vec![
            "-u".to_string(),
            users.clone(),
            "-U".to_string(),
            users.clone(),
        ])
    } else {
        args.push("-A".to_owned());
    };

    match execute_command("ps", &args).await {
        Ok((code, out, err)) => {
            if code != 0 {
                return Err(err);
            }
            Ok(out.split('\n').skip(1).map(|s| s.to_string()).collect())
        }
        Err(err) => Err(err.to_string()),
    }
}

/// save_terminal_size  -  save terminal size.
// some function (like pager) may change behaviour of ioctl...
pub fn get_terminal_size() -> &'static (usize, usize) {
    static TERM_SIZE: OnceLock<(usize, usize)> = OnceLock::new();
    TERM_SIZE.get_or_init(|| {
        if let Ok((width, height)) = terminal_size() {
            if width != 0 {
                (width as usize, height as usize)
            } else {
                (80, 24)
            }
        } else {
            (80, 24)
        }
    })
}

pub async fn choose_process(cli: &Cli) -> Result<Vec<i32>, String> {
    let (width, height) = get_terminal_size();
    let page_size: usize = std::cmp::max(7, height - 2);
    let columns = width - if cli.multi_mode { 8 } else { 4 };

    match get_process_list(cli.users.clone()).await {
        Ok(cands) => {
            let initial = cli.initial.clone().unwrap_or("".to_owned());
            let cands: Vec<String> = if cli.wide_mode {
                cands
            } else {
                cands
                    .into_iter()
                    .map(|s| s.as_str().chars().take(columns).collect::<String>())
                    .collect()
            };

            if let Some(parent) = cli.parent {
                let r_match_pattern = regex::Regex::new(&format!(" {} ", parent)).unwrap();
                let r_match_self = regex::Regex::new(&format!(" {} ", std::process::id())).unwrap();
                let cands: Vec<String> = cands
                    .into_iter()
                    .filter(|s| r_match_pattern.is_match(s) && !r_match_self.is_match(s))
                    .collect();

                Ok(cands.into_iter().map(|s| parse_pid(&s)).collect())
            } else if let Some(pattern) = cli.pattern.clone() {
                let r_match_pattern = regex::Regex::new(&pattern).unwrap();
                let r_match_self = regex::Regex::new(&format!(" {} ", std::process::id())).unwrap();
                let cands: Vec<String> = cands
                    .into_iter()
                    .filter(|s| r_match_pattern.is_match(s) && !r_match_self.is_match(s))
                    .collect();

                if cands.is_empty() {
                    eprintln!("No process matches given patter: {pattern}");
                    std::process::exit(1);
                }

                Ok(cands.into_iter().map(|s| parse_pid(&s)).collect())
            } else if cli.multi_mode {
                match MultiSelect::new("Choose process: ", cands)
                    .with_starting_filter_input(&initial)
                    .with_page_size(page_size)
                    .prompt()
                {
                    Ok(choice) => Ok(choice.into_iter().map(|s| parse_pid(&s)).collect()),
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(1);
                    }
                }
            } else {
                match Select::new("Choose process: ", cands)
                    .with_starting_filter_input(&initial)
                    .with_page_size(page_size)
                    .prompt()
                {
                    Ok(choice) => Ok(vec![parse_pid(&choice)]),
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(1);
                    }
                }
            }
        }
        Err(err) => {
            eprintln!("Failed to list processes: {err}");
            std::process::exit(1);
        }
    }
}

pub async fn list_process(cli: Cli) {
    let (width, _) = get_terminal_size();
    let columns: usize = width - 2;
    match get_process_list(cli.users.clone()).await {
        Ok(cands) => {
            let cands: Vec<String> = if cli.wide_mode {
                cands
            } else {
                cands
                    .into_iter()
                    .map(|s| s.as_str().chars().take(columns).collect::<String>())
                    .collect()
            };

            setup_pager(&cli);

            if let Some(pattern) = cli.files.first() {
                match regex::Regex::new(pattern) {
                    Ok(re) => {
                        println!("Listing processes matching '{pattern}'");
                        let mut lines = 0;
                        for s in cands {
                            if re.is_match(&s) {
                                println!("{s}");
                                lines += 1;
                            }
                        }

                        if lines == 0 {
                            println!("Failed to list process matching '{pattern}'.");
                            std::process::exit(1);
                        } else {
                            println!("Total {lines} process found.");
                        }
                    }
                    Err(err) => {
                        eprintln!("Failed to create regex for input '{pattern}': {err}");
                        std::process::exit(1);
                    }
                }
            } else {
                println!("Listing all processes...");
                println!("{}", cands.join("\n"));
            };
        }
        Err(err) => {
            eprintln!("Failed to list processes: {err}");
            std::process::exit(1);
        }
    }

    std::process::exit(0);
}

pub fn ensure_file_exists(file: &str) {
    let path = std::path::Path::new(&file);
    if !path.exists() {
        panic!("File {file} does not exist");
    }
}

pub fn setup_pager(cli: &Cli) {
    if !cli.no_pager && std::env::var("TERM").unwrap_or("xterm".to_string()) != "dumb" {
        Pager::new().setup();
    }
}

pub fn display_final(cli: &Cli, output: &str, errors: &[String]) {
    if !errors.is_empty() {
        if output.is_empty() {
            eprintln!(
                "error detected on process: {}, no stacks to show...",
                errors.join(",")
            );
        } else {
            eprintln!(
                "error detected on process: {}, press ENTER to continue...",
                errors.join(",")
            );
            use std::io::{stdin, Read};
            let mut stdin_handle = stdin().lock();
            let mut byte = [0_u8];
            stdin_handle.read_exact(&mut byte).unwrap();
            setup_pager(cli);
            println!("{output}");
        }
        std::process::exit(2);
    } else {
        setup_pager(cli);
        println!("{output}");
        std::process::exit(0);
    }
}

pub fn get_sampling_info(interval: Option<f32>, count: i32) -> Option<SamplingInfo> {
    let effective = if interval.is_none() { 1 } else { count };
    if effective > 1 {
        Some(SamplingInfo {
            interval: interval.unwrap(),
            count: effective,
        })
    } else {
        None
    }
}

#[tokio::test]
async fn test_command_execution() {
    if let Ok(result) = execute_command("ls", &["-a", "-l"]).await {
        let (code, out, err) = result;
        assert_eq!(code, 0);
        assert!(!out.is_empty());
        assert!(err.is_empty());
    } else {
        panic!();
    };

    if let Ok(result) = execute_command("ls", &["-a", "-l", "/target_dir_does_not_exist"]).await {
        let (code, out, err) = result;
        assert_ne!(code, 0);
        assert!(out.is_empty());
        assert!(!err.is_empty());
    } else {
        panic!();
    };

    if let Err(err) = execute_command("command_not_exist", &["-a", "-l"]).await {
        eprintln!("{err}");
    } else {
        panic!();
    };
}

#[tokio::test]
async fn test_list_process() {
    let result = get_process_list(Some("someone_does_not_exists".to_owned())).await;
    assert!(result.is_err());

    let mut cli = Cli::default();
    cli.no_pager = true;
    list_process(cli).await;

    let mut cli = Cli::default();
    cli.no_pager = true;
    cli.files.push("cs".to_owned());
    list_process(cli).await;
}

#[tokio::test]
async fn test_parse_and_get_pid() {
    assert_eq!(
        parse_pid(" 320282 root     15:29 [kworker/0:2-i915-unordered]"),
        320282
    );

    let s = terminal_size();
    println!("S: {s:?}");
}

#[tokio::test]
async fn test_collect_samples() {
    let result = collect_samples("echo", &["hello".into()], None, 1).await;
    assert!(result.is_ok());
    let outputs = result.unwrap();
    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].contains("hello"));
}

#[test]
fn test_ensure_file_exists() {
    let path = std::env::temp_dir().join("_cs_test_ensure_exists");
    std::fs::write(&path, "test").unwrap();
    ensure_file_exists(path.to_str().unwrap());
    std::fs::remove_file(&path).unwrap();
}

#[test]
#[should_panic(expected = "does not exist")]
fn test_ensure_file_exists_missing() {
    ensure_file_exists("/tmp/_cs_test_file_does_not_exist_12345");
}
