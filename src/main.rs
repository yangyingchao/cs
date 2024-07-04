mod utils;

mod args;
use std::fs;
use std::process::exit;

use gdb::run_gdb;
use uniquify::handle_content;
use utils::{choose_process, ensure_file_exists, list_process};

use crate::args::parse_args;

mod eu_stack;
use crate::eu_stack::run_eustack;
mod gdb;
mod uniquify;

fn main() {
    let mut cli = parse_args(std::env::args());

    if let Some(pattern) = &cli.list {
        list_process(cli.wide_mode, pattern.first().cloned(), cli.users);
        exit(0);
    };

    // read and parse from either files or stdin
    if !cli.files.is_empty() {
        let mut lines = Vec::new();
        if cli.files[0] == "-" {
            assert!(cli.files.len() == 1); // should be the only arg.
            let stdin = std::io::stdin();
            for line in std::io::BufRead::lines(stdin.lock()) {
                if let Ok(line) = line {
                    lines.push(line);
                } else {
                    panic!("Error reading line");
                }
            }
        } else {
            for file in cli.files {
                ensure_file_exists(&file);
                match fs::read_to_string(file) {
                    Ok(contents) => {
                        lines.push(contents);
                    }
                    Err(err) => {
                        panic!("{err}");
                    }
                }
            }
        }

        let contents = lines.join("\n");
        handle_content(&contents, cli.raw_mode, cli.unique_mode);
        exit(0);
    }

    if cli.pids.is_none() && cli.core.is_none() {
        match choose_process(
            cli.users.clone(),
            cli.initial.clone(),
            cli.wide_mode,
            cli.multi_mode,
        ) {
            Ok(pids) => {
                cli.pids.replace(pids);
            }
            Err(err) => {
                panic!("Abort: {err}");
            }
        }
    }
    match if cli.gdb_mode {
        run_gdb(&cli)
    } else {
        run_eustack(&cli)
    } {
        Ok(_) => {}
        Err(err) => {
            eprintln!("cs fails with: {err}");
            exit(1);
        }
    }
}
