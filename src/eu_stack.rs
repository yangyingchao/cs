use futures::future::join_all;
use std::sync::{Arc, Mutex};

use crate::{
    args::Cli,
    uniquify::uniquify_eustack,
    utils::{ensure_file_exists, execute_command},
};

async fn do_run_eustack(args: Vec<String>, unique: bool) -> Result<(), String> {
    match execute_command("eu-stack", args).await {
        Ok(result) => {
            let (code, out, err) = result;
            if code <= 1 {
                if unique {
                    uniquify_eustack(&out).expect("Uniquify fail..");
                } else {
                    println!("{}", out);
                }
                if !err.is_empty() {
                    eprintln!("Warnings reported: {err}");
                }
                Ok(())
            } else {
                Err(err)
            }
        }
        Err(err) => Err(err.to_string()),
    }
}

pub async fn run_eustack(cli: &Cli) -> Result<(), String> {
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

        return do_run_eustack(args, cli.unique_mode).await;
    }

    if let Some(pids) = &cli.pids {
        let mut handles = vec![];
        let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));

        let unique = cli.unique_mode;
        for pid in pids.clone() {
            let error_ref = errors.clone();
            handles.push(tokio::spawn(async move {
                let args = vec!["-p".to_string(), pid.to_string()];
                match do_run_eustack(args, unique).await {
                    Ok(_) => {}
                    Err(err) => {
                        eprintln!("Process {pid} returns error: {err}");
                        error_ref.lock().unwrap().push(pid);
                    }
                }
            }));
        }

        join_all(handles).await;
        if !errors.lock().unwrap().is_empty() {
            return Err(format!(
                "error detected on process: {}",
                errors.lock().unwrap().join(",")
            ));
        } else {
            return Ok(());
        }
    }

    panic!("Needs pid or core file.");
}
