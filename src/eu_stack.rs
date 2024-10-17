use futures::future::join_all;
use std::sync::{Arc, Mutex};

use crate::{
    args::Cli,
    uniquify::uniquify_eustack,
    utils::{display_result, ensure_file_exists, execute_command, setup_pager},
};

async fn do_run_eustack(
    args: Vec<String>,
    unique: bool,
    interval: Option<f32>,
    count: i32,
) -> Result<String, String> {
    let mut output = vec![];
    let mut count = if interval.is_none() { 1 } else { count };
    let sleep = interval.unwrap_or(0.0);
    let prefix = if count == 1 {
        "".to_owned()
    } else {
        format!("Interval: {}, Count: {}", sleep, count)
    };

    loop {
        match execute_command("eu-stack", &args).await {
            Ok((code, out, err)) => {
                if code <= 1 {
                    if !err.is_empty() {
                        eprintln!("Warnings reported: {err}");
                    }

                    output.push(out);
                } else {
                    return Err(err);
                }
            }
            Err(err) => {
                return Err(err.to_string());
            }
        }

        count -= 1;
        if count == 0 {
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_secs_f32(sleep)).await;
    }

    let result = if unique {
        match uniquify_eustack(&output.join("\n")) {
            Ok(o) => format!("{}\n{}", prefix, o),
            Err(err) => return Err(err.to_string()),
        }
    } else {
        format!("{}\n{}", prefix, output.join("\n"))
    };

    Ok(result)
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
        };

        setup_pager(cli);
        match do_run_eustack(args, cli.unique_mode, None, 1).await {
            Ok(result) => {
                println!("{result}");
                std::process::exit(0);
            }
            Err(err) => {
                eprintln!("{err}");
                std::process::exit(2);
            }
        }
    }

    if let Some(pids) = &cli.pids {
        let mut handles = vec![];
        let outputs = Arc::new(Mutex::new(vec![]));
        let errors = Arc::new(Mutex::new(vec![]));

        let unique = cli.unique_mode;
        for pid in pids.clone() {
            let output_ref = outputs.clone();
            let error_ref = errors.clone();
            let interval = cli.interval;
            let count = cli.count;
            handles.push(tokio::spawn(async move {
                let args = vec!["-p".to_string(), pid.to_string()];
                println!(
                    "Run for process: {:?} in thread: {:?}",
                    pid,
                    std::thread::current().id()
                );
                match do_run_eustack(args, unique, interval, count).await {
                    Ok(output) => {
                        output_ref.lock().unwrap().push(output);
                    }
                    Err(err) => {
                        eprintln!("Process {pid} returns error: {err}");
                        error_ref.lock().unwrap().push(pid);
                    }
                }
            }));
        }

        join_all(handles).await;
        display_result(cli, errors, outputs);
    }

    eprintln!("Needs pid or core file.");
    std::process::exit(2);
}
