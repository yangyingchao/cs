use futures::future::join_all;
use std::sync::{Arc, Mutex};

use crate::{
    args::Cli,
    uniquify::uniquify_eustack,
    utils::{ensure_file_exists, execute_command, setup_pager},
};

async fn do_run_eustack(args: Vec<String>, unique: bool) -> Result<String, String> {
    match execute_command("eu-stack", args).await {
        Ok((code, out, err)) => {
            if code <= 1 {
                if !err.is_empty() {
                    eprintln!("Warnings reported: {err}");
                }

                if unique {
                    uniquify_eustack(&out)
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
        match do_run_eustack(args, cli.unique_mode).await {
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
            handles.push(tokio::spawn(async move {
                let args = vec!["-p".to_string(), pid.to_string()];
                println!(
                    "Run for process: {:?} in thread: {:?}",
                    pid,
                    std::thread::current().id()
                );
                match do_run_eustack(args, unique).await {
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
