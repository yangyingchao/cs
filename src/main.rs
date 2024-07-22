mod utils;

mod args;
mod eu_stack;
mod gdb;
mod uniquify;

use std::process::exit;

use crate::args::parse_args;
use crate::eu_stack::run_eustack;
use gdb::run_gdb;
use uniquify::uniquify_stack_files;
use utils::{choose_process, execute_command, list_process};

#[tokio::main]
async fn main() {
    let _ = utils::get_terminal_size(); // must be done before setup pager
    let mut cli = parse_args(std::env::args());

    if !cli.gdb_mode {
        if let Ok((code, _out, _err)) = execute_command("which", ["eu-stack"]).await {
            if code != 0 {
                eprintln!("Failed to find eu-stack, will try gdb instead...");
                cli.gdb_mode = true;
            }
        }
    }

    if !cli.list && cli.files.is_empty() && cli.pids.is_none() && cli.core.is_none() {
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

    if cli.list {
        list_process(cli).await;
    } else if !cli.files.is_empty() {
        uniquify_stack_files(cli).await;
    } else if cli.gdb_mode {
        run_gdb(&cli).await;
    } else {
        run_eustack(&cli).await;
    }
}
