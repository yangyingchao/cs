mod utils;

mod args;
use std::fs;
use std::process::exit;

use gdb::run_gdb;
use uniquify::handle_content;
use utils::{choose_process, ensure_file_exists, execute_command, list_process};

use crate::args::parse_args;

mod eu_stack;
use crate::eu_stack::run_eustack;
mod gdb;
mod uniquify;
use pager::Pager;

fn main() {
    let mut cli = parse_args(std::env::args());

    if let Some(mut pattern) = cli.list {
        if pattern.is_empty() && cli.initial.is_some() {
            pattern.push(cli.initial.unwrap());
        }
        list_process(cli.wide_mode, pattern.first().cloned(), cli.users);
        exit(0);
    };

    // read and parse from either files or stdin
    if !cli.files.is_empty() {
        let mut lines = Vec::new();
        if cli.files.len() == 1 && cli.files[0] == "-" {
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

    if !cli.gdb_mode {
        if let Ok((code, _out, _err)) = execute_command("which", ["eu-stack"]) {
            if code != 0 {
                eprintln!("Failed to find eu-stack, will try gdb instead...");
                cli.gdb_mode = true;
            }
        };
    }

    Pager::new().setup();
    match if cli.gdb_mode {
        if let Ok((code, _out, _err)) = execute_command("which", ["gdb"]) {
            if code != 0 {
                panic!("Failed to find gdb");
            }
        };
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
