use futures::future::join_all;
use std::sync::{Arc, Mutex};

use crate::{
    args::Cli,
    uniquify::{simplify_stack, uniquify_gdb},
    utils::execute_command,
};

async fn do_run_gdb(args: Vec<&str>, unique: bool, raw: bool) -> Result<(), String> {
    match execute_command("gdb", args).await {
        Ok(result) => {
            let (code, out, err) = result;
            if code <= 1 {
                let out = if raw { out } else { simplify_stack(out) };
                if unique {
                    return uniquify_gdb(&out);
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

pub async fn run_gdb(cli: &Cli) -> Result<(), String> {
    if let Some(_corefile) = &cli.core {
        // let mut args = vec![];
        // args.push("--core".into());
        // ensure_file_exists(corefile);
        // args.push(corefile.to_owned());
        // if let Some(executable) = &cli.executable {
        //     args.push("-e".to_owned());
        //     ensure_file_exists(executable);
        //     args.push(executable.to_owned());
        // };

        // return do_run_gdb(args, cli.unique_mode);
        panic!("not impl");
    }

    if let Some(pids) = &cli.pids {
        // let mut err: Option<String> = None;

        let mut handles = vec![];
        let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));

        let unique = cli.unique_mode;
        let raw = cli.raw_mode;
        for pid in pids.clone() {
            let error_ref = errors.clone();
            handles.push(tokio::spawn(async move {
                let args = vec![
                    "--batch",
                    "-p",
                    pid.as_str(),
                    "-ex",
                    "thread apply all backtrace",
                ];
                println!("Run for process: {:?}", pid);
                match do_run_gdb(args, unique, raw).await {
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
