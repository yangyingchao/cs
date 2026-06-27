use std::process;
use std::sync::{Arc, Mutex};

use futures::future::join_all;

use regex::Regex;

use crate::args::Cli;
use crate::stack_data::{self, Frame, ThreadStack};
use crate::utils::{
    display_final, ensure_file_exists, execute_command, get_sampling_info, setup_pager,
};

async fn do_run_eustack(
    args: &[String],
    interval: Option<f32>,
    count: i32,
) -> Result<Vec<ThreadStack>, String> {
    let mut raw_outputs = vec![];
    let effective_count = if interval.is_none() { 1 } else { count };
    let sleep = interval.unwrap_or(0.0);

    let mut remaining = effective_count;
    loop {
        match execute_command("eu-stack", args).await {
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
    for raw in &raw_outputs {
        all_stacks.extend(parse_eustack(raw));
    }
    Ok(all_stacks)
}

pub fn parse_eustack(input: &str) -> Vec<ThreadStack> {
    let re_pid = Regex::new(r"PID\s+(?P<pid>\d+)\s+-\s+").unwrap();
    let re_tid = Regex::new(r"TID\s+(?P<tid>\d+):").unwrap();
    let re_frame =
        Regex::new(r"^#(?P<depth>\d+)\s+(?P<addr>0x[0-9a-fA-F]+)\s+(?P<func>.+?)$").unwrap();

    let mut stacks = Vec::new();
    let mut pid = 0;
    let mut tid = 0;
    let mut thread_name = String::new();
    let mut frames = Vec::new();

    for line in input.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        if let Some(caps) = re_pid.captures(line) {
            flush_stack(&mut stacks, pid, tid, &thread_name, &mut frames);
            pid = caps["pid"].parse().unwrap_or(0);
        } else if let Some(caps) = re_tid.captures(line) {
            flush_stack(&mut stacks, pid, tid, &thread_name, &mut frames);
            tid = caps["tid"].parse().unwrap_or(0);
            thread_name.clear();
        } else if let Some(caps) = re_frame.captures(line) {
            frames.push(Frame {
                depth: caps["depth"].parse().unwrap_or(0),
                address: caps["addr"].to_string(),
                function: caps["func"].to_string(),
                library: None,
            });
        }
    }

    flush_stack(&mut stacks, pid, tid, &thread_name, &mut frames);
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
    tool: &str,
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
        stack_data::format_json(&groups, tool, sampling)
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

pub async fn run_eustack(cli: &Cli) {
    if let Some(corefile) = &cli.core {
        let mut args = vec![];
        args.push("--core".into());
        ensure_file_exists(corefile);
        args.push(corefile.to_owned());
        if let Some(executable) = &cli.executable {
            args.push("-e".to_owned());
            ensure_file_exists(executable);
            args.push(executable.to_owned());
        }

        setup_pager(cli);
        match do_run_eustack(&args, None, 1).await {
            Ok(stacks) => {
                let output = format_result(stacks, cli, "eu-stack", None, 1);
                println!("{output}");
                process::exit(0);
            }
            Err(err) => {
                eprintln!("{err}");
                process::exit(2);
            }
        }
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
            let frames = cli.frames;
            handles.push(tokio::spawn(async move {
                let args = vec![
                    "-n".to_string(),
                    frames.to_string(),
                    "-p".to_string(),
                    format!("{pid}"),
                ];
                println!(
                    "Run for process: {pid:?} in thread: {:?}",
                    std::thread::current().id()
                );
                match do_run_eustack(&args, interval, count).await {
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
        let output = format_result(stacks, cli, "eu-stack", cli.interval, cli.count);
        display_final(cli, &output, &errors);
    }

    eprintln!("Needs pid or core file.");
    process::exit(2);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_eustack_single_thread() {
        let input = "PID 1234 - process\n\
                      TID 1234:\n\
                      #0  0x7f83df80a3ec func_a\n\
                      #1  0x7f83df14f421 func_b\n";
        let stacks = parse_eustack(input);
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].pid, 1234);
        assert_eq!(stacks[0].tid, 1234);
        assert_eq!(stacks[0].frames.len(), 2);
    }

    #[test]
    fn test_parse_eustack_multi_thread() {
        let input = "PID 14794 - process\n\
                      TID 14794:\n\
                      #0  0x7f83df80a3ec func_a\n\
                      TID 14818:\n\
                      #0  0x7f83ddba6fea func_b\n";
        let stacks = parse_eustack(input);
        assert_eq!(stacks.len(), 2);
        assert_eq!(stacks[1].tid, 14818);
    }
}
