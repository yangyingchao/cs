mod utils;

mod args;
use futures::future::join_all;
use std::process::exit;
use std::sync::{Arc, Mutex};
use tokio::fs;

use gdb::run_gdb;
use uniquify::handle_content;
use utils::{choose_process, ensure_file_exists, execute_command, list_process};

use crate::args::parse_args;

mod eu_stack;
use crate::eu_stack::run_eustack;
mod gdb;
mod uniquify;

#[tokio::main]
async fn main() {
    let _ = utils::get_terminal_size(); // must be done before setup pager
    let mut cli = parse_args(std::env::args());

    if cli.list {
        list_process(cli).await;
        exit(0);
    }

    // read and parse from either files or stdin
    if !cli.args.is_empty() {
        let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
        if cli.args.len() == 1 && cli.args[0] == "-" {
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
            let n = cli.args.len();
            let mut handles = vec![];
            println!("Reading stack from {n} file(s).");
            for file in cli.args {
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

    if cli.pids.is_none() && cli.core.is_none() {
        match choose_process(&cli).await {
            Ok(pids) => {
                if pids.is_empty() {
                    eprintln!("\nNo process is selected.");
                    exit(1);
                }
                cli.pids.replace(pids);
            }
            Err(err) => {
                eprintln!("Abort: {err}");
                exit(2);
            }
        }
    }

    if !cli.gdb_mode {
        if let Ok((code, _out, _err)) = execute_command("which", ["eu-stack"]).await {
            if code != 0 {
                eprintln!("Failed to find eu-stack, will try gdb instead...");
                cli.gdb_mode = true;
            }
        };
    }

    if cli.gdb_mode {
        run_gdb(&cli).await;
    } else {
        run_eustack(&cli).await;
    }
}
