use inquire::{MultiSelect, Select};
use std::{ffi::OsStr, process::Stdio, sync::OnceLock};
use termion::terminal_size;
use tokio::process::Command;

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

fn parse_pid(s: &str) -> String {
    let r_match_pid = regex::Regex::new(r#"\s*(?P<pid>\d+)\s+"#).unwrap();
    let m = r_match_pid.captures(s).expect("capture fails");
    m.name("pid").unwrap().as_str().to_string()
}

async fn get_process_list(users: Option<String>) -> Result<Vec<String>, String> {
    let mut args: Vec<String> = vec!["-o", "pid,user,stime,cmd"]
        .into_iter()
        .map(|s| s.to_owned())
        .collect();
    if let Some(users) = users {
        let new_args = ["-u".to_owned(), users.clone(), "-U".to_owned(), users];
        args.extend(new_args);
    } else {
        args.push("-A".to_owned());
    };

    match execute_command("ps", &args).await {
        Ok((code, out, err)) => {
            if code != 0 {
                return Err(err);
            }
            return Ok(out.split('\n').skip(1).map(|s| s.to_string()).collect());
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

pub async fn choose_process(
    users: Option<String>,
    initial: Option<String>,
    pattern: Option<String>,
    wide: bool,
    multi: bool,
) -> Result<Vec<String>, String> {
    let (width, height) = get_terminal_size();
    let page_size: usize = std::cmp::max(7, height - 2) as usize;
    let columns = width - if multi { 8 } else { 4 };

    match get_process_list(users).await {
        Ok(cands) => {
            let initial = initial.unwrap_or("".to_owned());
            let cands: Vec<String> = if wide {
                cands
            } else {
                cands
                    .into_iter()
                    .map(|s| s.as_str().chars().take(columns).collect::<String>())
                    .collect()
            };

            if let Some(pattern) = pattern {
                let r_match_pattern = regex::Regex::new(&pattern).unwrap();
                let r_match_self = regex::Regex::new(&format!(" {} ", std::process::id())).unwrap();
                let cands: Vec<String> = cands
                    .into_iter()
                    .filter(|s| r_match_pattern.is_match(s) && !r_match_self.is_match(s))
                    .collect();

                if cands.is_empty() {
                    eprintln!("No process matches givn patter: {pattern}");
                    std::process::exit(1);
                }

                Ok(cands.into_iter().map(|s| parse_pid(&s)).collect())
            } else if multi {
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

pub async fn list_process(wide: bool, pattern: Option<String>, users: Option<String>) {
    let (width, _) = get_terminal_size();
    let columns: usize = width - 2;
    match get_process_list(users).await {
        Ok(cands) => {
            let cands: Vec<String> = if wide {
                cands
            } else {
                cands
                    .into_iter()
                    .map(|s| s.as_str().chars().take(columns).collect::<String>())
                    .collect()
            };

            if let Some(pattern) = pattern {
                match regex::Regex::new(&pattern) {
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
}

pub fn ensure_file_exists(file: &str) {
    let path = std::path::Path::new(&file);
    if !path.exists() {
        panic!("File {file} does not exist");
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
        eprintln!("{}", err);
    } else {
        panic!();
    };
}

#[tokio::test]
async fn test_list_process() {
    let result = get_process_list(Some("some_one_does_not_exists".to_owned())).await;
    assert!(result.is_err());

    let result = get_process_list(Some("root".to_owned())).await;
    assert!(result.is_ok_and(|x| !x.is_empty()));

    list_process(false, None, None).await;
    list_process(false, Some("cs".to_owned()), None).await;
}

#[tokio::test]
async fn test_parse_and_get_pid() {
    assert_eq!(
        parse_pid(" 320282 root     15:29 [kworker/0:2-i915-unordered]"),
        "320282"
    );

    let s = terminal_size();
    println!("S: {:?}", s);
}
