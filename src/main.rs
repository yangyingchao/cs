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

    if let Some(file) = cli.file.clone() {
        ensure_file_exists(&file);
        match fs::read_to_string(file) {
            Ok(contents) => {
                handle_content(&contents, cli.raw_mode, cli.unique_mode);
                exit(0);
            }
            Err(err) => {
                panic!("{err}");
            }
        }
    };

    if cli.stdin {
        let stdin = std::io::stdin();
        let mut lines = Vec::new();

        for line in std::io::BufRead::lines(stdin.lock()) {
            if let Ok(line) = line {
                lines.push(line);
            } else {
                panic!("Error reading line");
            }
        }

        let contents = lines.join("\n");
        handle_content(&contents, cli.raw_mode, cli.unique_mode);
        exit(0);
    };

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
                panic!("{err}");
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
