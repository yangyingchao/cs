use std::process;
use std::sync::{Arc, Mutex};

use futures::future::join_all;

use crate::args::Cli;
use crate::stack_data::{self, parse_gdb, ThreadStack};
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

fn format_result(
    all_stacks: Vec<ThreadStack>,
    cli: &Cli,
    interval: Option<f32>,
    count: i32,
) -> String {
    let groups = if cli.unique_mode {
        stack_data::dedup_stacks(all_stacks)
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
                eprintln!(
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
