use std::process;
use std::sync::{Arc, Mutex};

use futures::future::join_all;
use tokio::fs;

use crate::args::Cli;
use crate::stack_data::{self, parse_eustack, parse_gdb, ThreadStack};
use crate::utils::{ensure_file_exists, setup_pager};

pub async fn uniquify_stack_files(cli: Cli) {
    let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));

    if cli.files.len() == 1 && cli.files[0] == "-" {
        println!("Reading stack from STDIN.");
        let stdin = std::io::stdin();
        for line in std::io::BufRead::lines(stdin.lock()) {
            if let Ok(line) = line {
                lines.lock().unwrap().push(line);
            } else {
                eprint!("Error reading line.");
                process::exit(2);
            }
        }
    } else {
        let n = cli.files.len();
        let mut handles = vec![];
        println!("Reading stack from {n} file(s).");
        for file in &cli.files {
            ensure_file_exists(file);
            let line_ref = lines.clone();
            let f = file.clone();
            handles.push(tokio::spawn(async move {
                match fs::read_to_string(&f).await {
                    Ok(contents) => {
                        line_ref.lock().unwrap().push(contents);
                    }
                    Err(err) => {
                        eprint!("failed to read from file {f}, reason: {err}");
                    }
                }
            }));
        }
        join_all(handles).await;
    }

    let content = lines.lock().unwrap().join("\n");
    setup_pager(&cli);

    let stacks: Vec<ThreadStack> = if cli.raw_mode {
        parse_eustack(&content)
            .into_iter()
            .chain(parse_gdb(&content, false))
            .collect()
    } else {
        // Try eu-stack first, fall back to gdb (simplified)
        let stacks = parse_eustack(&content);
        if !stacks.is_empty() {
            stacks
        } else {
            parse_gdb(&content, true)
        }
    };

    if stacks.is_empty() {
        eprintln!("Failed to parse stack content.");
        process::exit(2);
    }

    let groups = if cli.unique_mode {
        stack_data::dedup_stacks(stacks)
    } else {
        stack_data::to_groups(stacks)
    };

    if cli.json_mode {
        println!("{}", stack_data::format_json(&groups, "unknown", None));
    } else {
        println!("{}", stack_data::format_text(&groups, ""));
    }

    process::exit(0);
}
