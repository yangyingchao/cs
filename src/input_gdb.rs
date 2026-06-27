use std::process;
use std::sync::{Arc, Mutex};

use futures::future::join_all;

use regex::Regex;

use crate::args::Cli;
use crate::stack_data::{self, Frame, ThreadStack};
use crate::utils::{display_final, execute_command, get_sampling_info};

async fn do_run_gdb(
    args: &[String],
    raw: bool,
    interval: Option<f32>,
    count: i32,
) -> Result<Vec<ThreadStack>, String> {
    let mut raw_outputs = vec![];
    let effective_count = if interval.is_none() { 1 } else { count };
    let sleep = interval.unwrap_or(0.0);

    let mut remaining = effective_count;
    loop {
        match execute_command("gdb", args).await {
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
        remaining -= 1;
        if remaining == 0 {
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_secs_f32(sleep)).await;
    }

    let mut all_stacks = Vec::new();
    for raw_out in &raw_outputs {
        all_stacks.extend(parse_gdb(raw_out, !raw));
    }
    Ok(all_stacks)
}

pub fn parse_gdb(input: &str, simplify: bool) -> Vec<ThreadStack> {
    let re_tid = Regex::new(r"Thread\s+(?P<threadnum>\d+)\s+.*\(LWP\s+(?P<lwp>\d+).*\):").unwrap();
    let re_detach = Regex::new(r"Inferior.*detached").unwrap();
    let re_simplify = Regex::new(r"\s+in\s+(?P<func>.+?)\s+\(.*?\)\s+(at|from)\s+.*").unwrap();
    let re_frame_num = Regex::new(r"^\s*#\s*(?P<depth>\d+)").unwrap();
    let re_addr = Regex::new(r"0x[0-9a-fA-F]+").unwrap();
    let re_library = Regex::new(r"from\s+(?P<lib>\S+)").unwrap();

    let mut stacks = Vec::new();
    let mut tid = 0;
    let mut thread_name = String::new();
    let mut frames = Vec::new();
    let mut match_started = false;

    for line in input.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        if let Some(caps) = re_tid.captures(line) {
            flush_stack(&mut stacks, 0, tid, &thread_name, &mut frames);
            match_started = true;
            tid = caps["lwp"].parse().unwrap_or(0);
            thread_name = caps["threadnum"].to_string();
        } else if re_detach.is_match(line) || !match_started {
            continue;
        } else if let Some(fcaps) = re_frame_num.captures(line) {
            let depth: u32 = fcaps["depth"].parse().unwrap_or(0);
            let addr = re_addr
                .find(line)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let function = if simplify {
                re_simplify
                    .captures(line)
                    .map(|c| c["func"].to_string())
                    .unwrap_or_else(|| line.trim().to_string())
            } else {
                line.trim().to_string()
            };
            let library = re_library.captures(line).map(|c| c["lib"].to_string());
            frames.push(Frame {
                depth,
                address: addr,
                function,
                library,
            });
        } else {
            eprintln!("IGNORE: Failed to parse: {line}");
        }
    }

    flush_stack(&mut stacks, 0, tid, &thread_name, &mut frames);
    stacks
}

fn flush_stack(
    stacks: &mut Vec<ThreadStack>,
    pid: i32,
    tid: i32,
    thread_name: &str,
    frames: &mut Vec<Frame>,
) {
    if !frames.is_empty() {
        stacks.push(ThreadStack {
            pid,
            tid,
            thread_name: thread_name.to_string(),
            frames: std::mem::take(frames),
        });
    }
}

fn format_result(
    all_stacks: Vec<ThreadStack>,
    cli: &Cli,
    interval: Option<f32>,
    count: i32,
) -> String {
    let groups = if cli.unique_mode {
        stack_data::dedup_stacks(all_stacks, cli.effective_match_mode())
    } else {
        stack_data::to_groups(all_stacks)
    };

    if cli.json_mode {
        let sampling = get_sampling_info(interval, count);
        stack_data::format_json(&groups, "gdb", sampling)
    } else {
        let prefix = if let Some(sleep) = interval {
            if count > 1 {
                format!("Interval: {sleep}, Count: {count}")
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        stack_data::format_text(&groups, &prefix)
    }
}

pub async fn run_gdb(cli: &Cli) {
    if let Some(_corefile) = &cli.core {
        panic!("not impl");
    }

    if let Some(pids) = &cli.pids {
        let all_stacks: Arc<Mutex<Vec<ThreadStack>>> = Arc::new(Mutex::new(vec![]));
        let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));

        let mut handles = vec![];
        for pid in pids.clone() {
            let stacks_ref = all_stacks.clone();
            let err_ref = errors.clone();
            let interval = cli.interval;
            let count = cli.count;
            let raw = cli.raw_mode;
            let command = format!(
                "thread apply all backtrace {}",
                if cli.frames == 0 {
                    "full".to_string()
                } else {
                    cli.frames.to_string()
                }
            );

            handles.push(tokio::spawn(async move {
                let args = vec![
                    "--batch".to_string(),
                    "-p".to_string(),
                    format!("{pid}"),
                    "-ex".to_string(),
                    command,
                ];
                println!(
                    "Run for process: {pid:?} in thread: {:?}",
                    std::thread::current().id()
                );
                match do_run_gdb(&args, raw, interval, count).await {
                    Ok(stacks) => {
                        stacks_ref.lock().unwrap().extend(stacks);
                    }
                    Err(err) => {
                        eprintln!("Process {pid} returns error: {err}");
                        err_ref.lock().unwrap().push(pid.to_string());
                    }
                }
            }));
        }

        join_all(handles).await;

        let stacks = all_stacks.lock().unwrap().clone();
        let errors = errors.lock().unwrap().clone();
        let output = format_result(stacks, cli, cli.interval, cli.count);
        display_final(cli, &output, &errors);
    }

    eprintln!("Needs pid or core file.");
    process::exit(2);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::match_mode::MatchMode;
    use crate::stack_data;

    #[test]
    fn test_parse_gdb() {
        let input = r#"Thread 1 (Thread 0x7f... (LWP 1234) "test"):
 #0  0x7f... in clock_nanosleep () from /usr/lib64/libc.so.6
 #1  0x7f... in func_a () at test.c:10
"#;
        let stacks = parse_gdb(input, false);
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].tid, 1234);
        assert_eq!(stacks[0].frames.len(), 2);
    }

    #[test]
    fn test_parse_gdb_simplify() {
        let input = r#"Thread 1 (Thread 0x7f... (LWP 1234) "test"):
 #0  0x7f... in clock_nanosleep () from /usr/lib64/libc.so.6
"#;
        let stacks = parse_gdb(input, true);
        assert_eq!(stacks.len(), 1);
        assert!(stacks[0].frames[0].function.contains("clock_nanosleep"));
        assert!(!stacks[0].frames[0].function.contains("libc.so.6"));
    }

    #[test]
    fn test_parse_gdb_library() {
        let input = r#"Thread 1 (Thread 0x7f... (LWP 1234) "test"):
 #0  0x7f... in clock_nanosleep () from /usr/lib64/libc.so.6
 #1  0x7f... in func_a () at test.c:10
"#;
        let stacks = parse_gdb(input, true);
        assert_eq!(stacks.len(), 1);
        assert_eq!(
            stacks[0].frames[0].library,
            Some("/usr/lib64/libc.so.6".into())
        );
        assert_eq!(stacks[0].frames[1].library, None);
    }

    #[test]
    fn test_parse_gdb_full_stack_and_dedup() {
        let input = r#"Thread 1 (Thread 0x7f29ce816740 (LWP 37746) "test"):
 #0  0x00007f29ce8db9e7 in clock_nanosleep () from /usr/lib64/libc.so.6
 #1  0x00007f29ce8e6a47 in nanosleep () from /usr/lib64/libc.so.6
 #2  0x00007f29ce8f7bce in sleep () from /usr/lib64/libc.so.6
 #3  0x000055723be89162 in func2 () at test.c:5
 #4  0x000055723be8917d in func1 () at test.c:10

Thread 2 (Thread 0x7f29ce816740 (LWP 37748) "test"):
 #0  0x00007f29ce8db9e7 in clock_nanosleep () from /usr/lib64/libc.so.6
 #1  0x00007f29ce8e6a47 in nanosleep () from /usr/lib64/libc.so.6
 #2  0x00007f29ce8f7bce in sleep () from /usr/lib64/libc.so.6
 #3  0x000055723be89162 in func2 () at test.c:5
 #4  0x000055723be8917d in func1 () at test.c:10
"#;
        let stacks = parse_gdb(input, true);
        assert_eq!(stacks.len(), 2);
        let groups = stack_data::dedup_stacks(stacks, MatchMode::Precise);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].threads.len(), 2);
        assert_eq!(groups[0].frames.len(), 5);
    }

    #[test]
    fn test_simplify_gdb_frame() {
        let input = r#"Thread 1 (Thread 0x7f... (LWP 1234) "test"):
 #0  0x7f... in clock_nanosleep () from /usr/lib64/libc.so.6
"#;
        let raw = parse_gdb(input, false);
        let simplified = parse_gdb(input, true);
        assert!(raw[0].frames[0].function.contains("libc.so.6"));
        assert!(!simplified[0].frames[0].function.contains("libc.so.6"));
        assert!(simplified[0].frames[0].function.contains("clock_nanosleep"));
    }
}
