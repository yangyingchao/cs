use inquire::{MultiSelect, Select};
use std::{
    ffi::OsStr,
    process::{Command, Stdio},
};
use termion::terminal_size;

pub fn execute_command<S, I>(
    command: &str,
    args: I,
) -> Result<(i32, String, String), std::io::Error>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let child_process = Command::new(command)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let output = child_process.wait_with_output()?;
    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok((exit_code, stdout, stderr))
}

fn parse_and_get_pid(s: &str) -> String {
    let r_match_pid = regex::Regex::new(r#"\s*(?P<pid>\d+)\s+"#).unwrap();
    let m = r_match_pid.captures(s).expect("capture fails");
    m.name("pid").unwrap().as_str().to_string()
}

fn get_process_list(users: Option<String>) -> Result<Vec<String>, String> {
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

    match execute_command("ps", &args) {
        Ok((code, out, err)) => {
            if code != 0 {
                return Err(err);
            }
            return Ok(out.split('\n').map(|s| s.to_string()).collect());
        }
        Err(err) => Err(err.to_string()),
    }
}

pub fn choose_process(
    users: Option<String>,
    pattern: Option<String>,
    wide: bool,
    multi: bool,
) -> Result<Vec<String>, String> {
    let mut page_size: usize = 20;
    let mut columns: usize = 80;
    if let Ok((width, height)) = terminal_size() {
        page_size = std::cmp::max(7, height - 2) as usize;
        columns = (width - if multi { 8 } else { 4 }) as usize;
    };

    match get_process_list(users) {
        Ok(cands) => {
            let initial = pattern.unwrap_or("".to_owned());
            let cands: Vec<String> = if wide {
                cands
            } else {
                cands
                    .into_iter()
                    .map(|s| s.as_str().chars().take(columns).collect::<String>())
                    .collect()
            };

            if multi {
                match MultiSelect::new("Choose process: ", cands)
                    .with_starting_filter_input(&initial)
                    .with_page_size(page_size)
                    .prompt()
                {
                    Ok(choice) => Ok(choice.into_iter().map(|s| parse_and_get_pid(&s)).collect()),
                    Err(e) => {
                        println!("{e}");
                        std::process::exit(1);
                    }
                }
            } else {
                match Select::new("Choose process: ", cands)
                    .with_starting_filter_input(&initial)
                    .with_page_size(page_size)
                    .prompt()
                {
                    Ok(choice) => Ok(vec![parse_and_get_pid(&choice)]),
                    Err(e) => {
                        println!("{e}");
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

pub fn list_process(wide: bool, pattern: Option<String>, users: Option<String>) {
    let mut columns: usize = 80;
    if let Ok((width, _height)) = terminal_size() {
        columns = (width - 2) as usize;
    };

    match get_process_list(users) {
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
                            if re.find(&s).is_some() {
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

#[test]
fn test_command_execution() {
    // normal command, should not error.
    if let Ok(result) = execute_command("ls", &["-a", "-l"]) {
        let (code, out, err) = result;
        assert_eq!(code, 0);
        assert!(!out.is_empty());
        assert!(err.is_empty());
    } else {
        panic!();
    };

    if let Ok(result) = execute_command("ls", &["-a", "-l", "/target_dir_does_not_exist"]) {
        let (code, out, err) = result;
        assert_ne!(code, 0);
        assert!(out.is_empty());
        assert!(!err.is_empty());
    } else {
        panic!();
    };

    if let Err(err) = execute_command("command_not_exist", &["-a", "-l"]) {
        eprintln!("{}", err);
    } else {
        panic!();
    };
}

#[test]
fn test_list_process() {
    let result = get_process_list(Some("some_one_does_not_exists".to_owned()));
    assert!(result.is_err());

    let result = get_process_list(Some("root".to_owned()));
    assert!(result.is_ok_and(|x| !x.is_empty()));

    list_process(false, None, None);
    list_process(false, Some("boost".to_owned()), None);
}

#[test]
fn test_parse_and_get_pid() {
    assert_eq!(
        parse_and_get_pid(" 320282 root     15:29 [kworker/0:2-i915-unordered]"),
        "320282"
    );
}
