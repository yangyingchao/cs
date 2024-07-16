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
    let mut cli = parse_args(std::env::args());
    let _ = utils::get_terminal_size(); // must be done before setup pager

    if let Some(mut pattern) = cli.list {
        if pattern.is_empty() && cli.initial.is_some() {
            pattern.push(cli.initial.unwrap());
        }
        list_process(cli.wide_mode, pattern.first().cloned(), cli.users).await;
        exit(0);
    };

    // read and parse from either files or stdin
    if !cli.files.is_empty() {
        let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
        if cli.files.len() == 1 && cli.files[0] == "-" {
            println!("Reading stack from STDIN.");
            let stdin = std::io::stdin();
            for line in std::io::BufRead::lines(stdin.lock()) {
                if let Ok(line) = line {
                    lines.lock().unwrap().push(line);
                } else {
                    panic!("Error reading line.");
                }
            }
        } else {
            let n = cli.files.len();
            let mut handles = vec![];
            println!("Reading stack from {n} file(s).");
            for file in cli.files {
                let line_ref = lines.clone();
                handles.push(tokio::spawn(async move {
                    ensure_file_exists(&file);
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
        match choose_process(
            cli.users.clone(),
            cli.initial.clone(),
            cli.pattern.clone(),
            cli.wide_mode,
            cli.multi_mode,
        )
        .await
        {
            Ok(pids) => {
                if pids.is_empty() {
                    eprintln!("\nNo process is selected.");
                    exit(1);
                }
                cli.pids.replace(pids);
            }
            Err(err) => {
                panic!("Abort: {err}");
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

    match if cli.gdb_mode {
        if let Ok((code, _out, _err)) = execute_command("which", ["gdb"]).await {
            if code != 0 {
                panic!("Failed to find gdb");
            }
        };
        run_gdb(&cli).await
    } else {
        run_eustack(&cli).await
    } {
        Ok(_) => {}
        Err(err) => {
            eprintln!("cs fails with: {err}");
            exit(1);
        }
    }
}
