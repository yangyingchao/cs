use std::process;
use std::sync::{Arc, Mutex};

use futures::future::join_all;
use tokio::fs;
use tokio::io::AsyncBufReadExt;

use crate::args::Cli;
use crate::stack_data::{
    self, parse_eustack, parse_gdb, OutputData, ThreadStack, UniqueStackGroup,
};
use crate::utils::{ensure_file_exists, setup_pager};

pub async fn uniquify_stack_files(cli: Cli) {
    let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));

    if cli.files.len() == 1 && cli.files[0] == "-" {
        println!("Reading stack from STDIN.");
        let reader = tokio::io::BufReader::new(tokio::io::stdin());
        let mut line_stream = reader.lines();
        while let Some(line) = line_stream.next_line().await.unwrap_or_else(|_| {
            eprint!("Error reading line.");
            process::exit(2);
        }) {
            lines.lock().unwrap().push(line);
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

    let has_json_ext = cli.files.iter().any(|f| f.ends_with(".json"));

    let groups: Vec<UniqueStackGroup> = if let Ok(data) =
        serde_json::from_str::<OutputData>(&content)
    {
        data.stacks
    } else {
        if has_json_ext {
            eprintln!("warning: input has .json extension but is not valid JSON, falling back to text parsing");
        }
        if cli.raw_mode {
            let stacks: Vec<ThreadStack> = parse_eustack(&content)
                .into_iter()
                .chain(parse_gdb(&content, false))
                .collect();
            if stacks.is_empty() {
                eprintln!("Failed to parse stack content.");
                process::exit(2);
            }
            if cli.unique_mode {
                stack_data::dedup_stacks(stacks, cli.effective_match_mode())
            } else {
                stack_data::to_groups(stacks)
            }
        } else {
            let stacks = parse_eustack(&content);
            let stacks = if !stacks.is_empty() {
                stacks
            } else {
                parse_gdb(&content, true)
            };
            if stacks.is_empty() {
                eprintln!("Failed to parse stack content.");
                process::exit(2);
            }
            if cli.unique_mode {
                stack_data::dedup_stacks(stacks, cli.effective_match_mode())
            } else {
                stack_data::to_groups(stacks)
            }
        }
    };

    if cli.json_mode {
        println!("{}", stack_data::format_json(&groups, "unknown", None));
    } else {
        println!("{}", stack_data::format_text(&groups, ""));
    }

    process::exit(0);
}
