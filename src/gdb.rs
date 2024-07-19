use futures::future::join_all;
use std::sync::{Arc, Mutex};

use crate::{
    args::Cli,
    uniquify::{simplify_stack, uniquify_gdb},
    utils::{execute_command, setup_pager},
};

async fn do_run_gdb(args: Vec<&str>, unique: bool, raw: bool) -> Result<String, String> {
    match execute_command("gdb", args).await {
        Ok((code, out, err)) => {
            if code <= 1 {
                if !err.is_empty() {
                    eprintln!("Warnings reported: {err}");
                }

                let out = if raw { out } else { simplify_stack(out) };
                if unique {
                    uniquify_gdb(&out)
                } else {
                    Ok(out)
                }
            } else {
                Err(err)
            }
        }
        Err(err) => Err(err.to_string()),
    }
}

pub async fn run_gdb(cli: &Cli) {
    if let Some(_corefile) = &cli.core {
        panic!("not impl");
    }

    if let Some(pids) = &cli.pids {
        let mut handles = vec![];
        let outputs = Arc::new(Mutex::new(vec![]));
        let errors = Arc::new(Mutex::new(vec![]));

        let unique = cli.unique_mode;
        let raw = cli.raw_mode;
        for pid in pids.clone() {
            let output_ref = outputs.clone();
            let error_ref = errors.clone();
            handles.push(tokio::spawn(async move {
                let args = vec![
                    "--batch",
                    "-p",
                    pid.as_str(),
                    "-ex",
                    "thread apply all backtrace",
                ];
                println!(
                    "Run for process: {:?} in thread: {:?}",
                    pid,
                    std::thread::current().id()
                );
                match do_run_gdb(args, unique, raw).await {
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
        setup_pager(cli);

        if !errors.lock().unwrap().is_empty() {
            eprintln!(
                "error detected on process: {}",
                errors.lock().unwrap().join(",")
            );
            std::process::exit(2);
        } else {
            println!("{}", outputs.lock().unwrap().join("\n"));
            std::process::exit(0);
        }
    }

    eprintln!("Needs pid or core file.");
    std::process::exit(2);
}
