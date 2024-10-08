use colored::*;
use futures::future::join_all;
use pager::Pager;
use regex::Regex;
use std::{
    collections::HashMap,
    process::exit,
    sync::{Arc, Mutex},
};
use tokio::fs;

use crate::{args::Cli, utils::ensure_file_exists};

fn sort_and_print_stack(cache: HashMap<String, String>) -> Result<String, String> {
    let mut ordered_cache: HashMap<usize, Vec<(&String, &String)>> = HashMap::new();
    for (key, val) in cache.iter() {
        let count = val.chars().filter(|&c| c == ',').count() + 1;
        let mut v = match ordered_cache.get(&count) {
            Some(v) => v.clone(),
            None => vec![],
        };

        v.push((val, key));
        ordered_cache.insert(count, v);
    }

    let keywords = [
        "__assert_fail",
        "fatal.*signals",
        "raise",
        "segfault",
        "segment fault",
        "segmentfault",
        "signal handler called",
    ];

    let pattern = format!(r#"(?i)(?P<sus>.*({}).*)"#, keywords.join("|"));
    let r_match_suspicious = Regex::new(&pattern).unwrap();
    let mut suspicious: Vec<String> = vec![];

    let mut outputs = vec![];
    let pairs: Vec<_> = ordered_cache.iter().collect();
    for (key, val) in pairs.iter().rev() {
        for (pids, stack) in *val {
            if r_match_suspicious.is_match(stack) {
                suspicious.push((*pids).clone());
                let stack = r_match_suspicious
                    .replace_all(stack, |captures: &regex::Captures| {
                        let matched_text = captures.name("sus").unwrap().as_str();
                        format!(
                            "{}{}",
                            matched_text.blue(),
                            "                           <---- HERE ".red().bold()
                        )
                    })
                    .to_string();
                outputs.push(format!("Number of thread: {key} -- {pids}:\n{stack}"));
            } else {
                outputs.push(format!("Number of thread: {key} -- {pids}:\n{stack}"));
            }
        }
    }

    if !suspicious.is_empty() {
        outputs.push(format!(
            "Suspicious threads: {}",
            suspicious.join(", ").red()
        ));
    }

    Ok(outputs.join("\n"))
}

pub fn simplify_stack(input: String) -> String {
    let re = regex::Regex::new(r"\s+in\s+(?P<func>.*?)\s+\(.*?\)\s+(at|from)\s+.*").unwrap();
    re.replace_all(&input, |captures: &regex::Captures| {
        let matched_text = captures.name("func").unwrap().as_str();
        format!(" {}", matched_text)
    })
    .to_string()
}

pub fn uniquify_eustack(input: &str) -> Result<String, String> {
    let r_match_pid = Regex::new(r#"PID\s+(?P<pid>\d+)\s+-\s+process"#).unwrap();
    if r_match_pid.captures(input).is_none() {
        return Err("not generated by eu-stack".to_owned());
    };

    let r_match_tid = Regex::new(r#"TID\s+(?P<tid>\d+):"#).unwrap();
    let r_match_empty = Regex::new(r#"^$"#).unwrap();
    let r_match_entry = Regex::new(r#"^#\d+\s+0x.*?$"#).unwrap();

    let mut cache: HashMap<String, String> = HashMap::new();
    let mut tid = "";
    let mut stack = "".to_owned();

    for s in input.split('\n') {
        if r_match_pid.captures(s).is_some() {
            continue;
        } else if r_match_empty.is_match(s) {
        } else if let Some(m) = r_match_tid.captures(s) {
            // start of new stack
            if !tid.is_empty() {
                let tids = match cache.get(&stack) {
                    Some(existing) => format!("{}, {}", existing, tid.to_owned()),
                    None => tid.to_owned(),
                };
                cache.insert(stack.to_string(), tids);
            }

            tid = m.name("tid").unwrap().as_str();
            stack = "".to_string();
        } else if r_match_entry.is_match(s) {
            stack = stack + s + "\n";
        }
    }

    // now handle last tid
    let tids = match cache.get(&stack) {
        Some(existing) => format!("{}, {}", existing, tid.to_owned()),
        None => tid.to_owned(),
    };
    cache.insert(stack.to_string(), tids);

    sort_and_print_stack(cache)
}

const RE_MATCH_GDB_TID: &str = r#"Thread\s+(?P<tid>\d+)\s+.*\(LWP\s+(?P<lwp>\d+).*\):"#;

pub fn uniquify_gdb(input: &str) -> Result<String, String> {
    let r_match_tid = Regex::new(RE_MATCH_GDB_TID).unwrap();
    if r_match_tid.captures(input).is_none() {
        return Err(format!("not generated by gdb:\n{}", input));
    };

    let r_match_empty = Regex::new(r#"^$"#).unwrap();
    let r_match_entry = Regex::new(r#"\s*#\s*\d+\s+"#).unwrap();
    let r_match_detach = Regex::new(r#"Inferior.*detached"#).unwrap();

    let mut cache: HashMap<String, String> = HashMap::new();
    let mut tid = "";
    let mut stack = "".to_owned();
    let mut match_started = false;

    for s in input.split('\n') {
        if r_match_empty.find(s).is_some() {
        } else if let Some(m) = r_match_tid.captures(s) {
            match_started = true;
            // start of new stack
            if !tid.is_empty() {
                let tids = match cache.get(&stack) {
                    Some(existing) => format!("{}, {}", existing, tid.to_owned()),
                    None => tid.to_owned(),
                };
                cache.insert(stack.to_string(), tids);
            }

            tid = m.name("lwp").unwrap().as_str();
            stack = "".to_string();
        } else if r_match_entry.is_match(s) {
            stack = stack + s + "\n";
        } else if r_match_detach.is_match(s) || !match_started {
            continue;
        } else {
            eprintln!("IGNORE: Failed to parse: {s}");
        }
    }

    // now handle last tid
    let tids = match cache.get(&stack) {
        Some(existing) => format!("{}, {}", existing, tid.to_owned()),
        None => tid.to_owned(),
    };
    cache.insert(stack.to_string(), tids);
    sort_and_print_stack(cache)
}

fn handle_content(contents: &str, raw: bool, unique: bool) {
    Pager::new().setup();
    let contents = if raw {
        contents.to_owned()
    } else {
        simplify_stack(contents.to_owned())
    };

    if unique {
        match (uniquify_eustack(&contents), uniquify_gdb(&contents)) {
            (Ok(result), _) | (_, Ok(result)) => {
                println!("{}", result);
            }
            (_, Err(err)) => {
                eprintln!("Failed to handle content: {}", err);
                std::process::exit(2);
            }
        }
    } else {
        println!("{contents}");
    }
}

pub async fn uniquify_stack_files(cli: Cli) {
    let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
    if cli.files.len() == 1 && cli.files[0] == "-" {
        println!("Reading stack from STDIN.");
        let stdin = std::io::stdin();
        for line in std::io::BufRead::lines(stdin.lock()) {
            if let Ok(line) = line {
                lines.lock().unwrap().push(line);
            } else {
                eprint!("Error reading line.");
                exit(2);
            }
        }
    } else {
        let n = cli.files.len();
        let mut handles = vec![];
        println!("Reading stack from {n} file(s).");
        for file in cli.files {
            ensure_file_exists(&file);
            let line_ref = lines.clone();
            handles.push(tokio::spawn(async move {
                match fs::read_to_string(&file).await {
                    Ok(contents) => {
                        line_ref.lock().unwrap().push(contents);
                    }
                    Err(err) => {
                        eprint!("failed to read from file {}, reason: {}", file, err);
                    }
                }
            }));
        }

        join_all(handles).await;
    }

    let contents = lines.lock().unwrap().join("\n");
    handle_content(&contents, cli.raw_mode, cli.unique_mode);
    exit(0);
}

#[test]
fn test_regex_tid() {
    let re = regex::Regex::new(RE_MATCH_GDB_TID).unwrap();

    if let Some(m) = re.captures(r#"Thread 15 (Thread 0x7fa1aea006c0 (LWP 1175) "waybar"):"#) {
        assert_eq!(m.name("tid").unwrap().as_str(), "15");
        assert_eq!(m.name("lwp").unwrap().as_str(), "1175");
    } else {
        assert!(false);
    };

    if let Some(m) = re.captures(r#"Thread 13 (LWP 258729 "tokio-runtime-w"):"#) {
        assert_eq!(m.name("tid").unwrap().as_str(), "13");
        assert_eq!(m.name("lwp").unwrap().as_str(), "258729");
    } else {
        assert!(false);
    };
}

#[test]
fn test_unquify() {
    let input = r#"
PID 14794 - process
TID 14794:
#0  0x00007f83df80a3ec g_type_check_instance_is_a
#1  0x00007f83df14f421 gdk_frame_clock_request_phase
#19 0x00007f83ddb902e0
#20 0x00007f83ddb90399 __libc_start_main
#21 0x0000557b62938905 _start
TID 14818:
#0  0x00007f83ddba6fea __sigtimedwait
#1  0x00007f83ddba666c sigwait
#2  0x0000557b62997e8b signalThread(void*)
#3  0x00007f83ddbf4359
TID 14820:
#0  0x00007f83ddc5363f __poll
#1  0x00007f83de32a8d7
#2  0x00007f83de32afa0 g_main_context_iteration
#3  0x00007f83de32aff1
#4  0x00007f83de3581a1
TID 14822:
#0  0x00007f83ddc5363f __poll
#1  0x00007f83de32a8d7
#2  0x00007f83de32afa0 g_main_context_iteration
#3  0x00007f83de32aff1
#4  0x00007f83de3581a1

"#
    .to_owned();

    assert!(uniquify_eustack(&input).is_ok());
    assert!(uniquify_gdb(&input).is_err());

    let input = r#"
Thread 3 (Thread 0x7f29ce816740 (LWP 37746) "test"):
#0  0x00007f29ce8db9e7 in clock_nanosleep () from /usr/lib64/libc.so.6
#1  0x00007f29ce8e6a47 in nanosleep () from /usr/lib64/libc.so.6
#2  0x00007f29ce8f7bce in sleep () from /usr/lib64/libc.so.6
#3  0x000055723be89162 in func2 () at test.c:5
#4  0x000055723be8917d in func1 () at test.c:10
#7  0x00007f29ce83f320 in ?? () from /usr/lib64/libc.so.6
#8  0x00007f29ce83f3d9 in __libc_start_main () from /usr/lib64/libc.so.6
#9  0x000055723be89085 in _start ()

Thread 2 (Thread 0x7f29ce816740 (LWP 37748) "test"):
#0  0x00007f29ce8db9e7 in clock_nanosleep () from /usr/lib64/libc.so.6
#1  0x00007f29ce8e6a47 in nanosleep () from /usr/lib64/libc.so.6
#2  0x00007f29ce8f7bce in sleep () from /usr/lib64/libc.so.6
#3  0x000055723be89162 in func2 () at test.c:5
#4  0x000055723be8917d in func1 () at test.c:10
#5  0x000055723be8918d in func () at test.c:15
#6  0x000055723be891af in main (argc=1, argv=0x7ffec118b6f8) at test.c:19
#7  0x00007f29ce83f320 in ?? () from /usr/lib64/libc.so.6
#8  0x00007f29ce83f3d9 in __libc_start_main () from /usr/lib64/libc.so.6
#9  0x000055723be89085 in _start ()

Thread 1 (Thread 0x7f29ce816740 (LWP 37747) "test"):
#0  0x00007f29ce8db9e7 in clock_nanosleep () from /usr/lib64/libc.so.6
#1  0x00007f29ce8e6a47 in nanosleep () from /usr/lib64/libc.so.6
#2  0x00007f29ce8f7bce in sleep () from /usr/lib64/libc.so.6
#3  0x000055723be89162 in func2 () at test.c:5
#4  0x000055723be8917d in func1 () at test.c:10
#5  0x000055723be8918d in func () at test.c:15
#6  0x000055723be891af in main (argc=1, argv=0x7ffec118b6f8) at test.c:19
#7  0x00007f29ce83f320 in ?? () from /usr/lib64/libc.so.6
#8  0x00007f29ce83f3d9 in __libc_start_main () from /usr/lib64/libc.so.6
#9  0x000055723be89085 in _start ()

"#
    .to_owned();

    assert!(uniquify_gdb(&input).is_ok());
    assert!(uniquify_eustack(&input).is_err());
}

#[test]
fn test_regex_replace() {
    let input = r#"
Thread 1 (Thread 0x7f29ce816740 (LWP 37747) "test"):
#0  0x00007f29ce8db9e7 in clock_nanosleep () from /usr/lib64/libc.so.6
#1  0x00007f29ce8e6a47 in nanosleep () from /usr/lib64/libc.so.6
#2  0x00007f29ce8f7bce in sleep () from /usr/lib64/libc.so.6
#3  0x000055723be89162 in func2 () at test.c:5
#4  0x000055723be8917d in func1 () at test.c:10
#5  0x000055723be8918d in func () at test.c:15
#6  0x000055723be891af in main (argc=1, argv=0x7ffec118b6f8) at test.c:19
#7  0x00007f29ce83f320 in ?? () from /usr/lib64/libc.so.6
#8  0x00007f29ce83f3d9 in __libc_start_main () from /usr/lib64/libc.so.6
#9  0x000055723be89085 in _start ()

"#
    .to_owned();

    let result = simplify_stack(input);
    println!("{result}");
    assert!(result.find("in func1 () at").is_none());
}
