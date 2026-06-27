use std::collections::HashMap;
use std::time::Duration;

use serde::Serialize;

use crate::args::Cli;
use crate::match_mode::{MatchMode, StackKey};
use crate::stack_data::{OutputData, ThreadStack, UniqueStackGroup};
use crate::utils::execute_command;

// ---- Data Model ----

#[derive(Debug, Clone, Serialize)]
pub struct StackDiffEntry {
    pub signature: String,
    pub frames: Vec<crate::stack_data::Frame>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangedEntry {
    pub signature: String,
    pub frames: Vec<crate::stack_data::Frame>,
    pub before_count: usize,
    pub after_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffResult {
    pub added: Vec<StackDiffEntry>,
    pub removed: Vec<StackDiffEntry>,
    pub changed: Vec<ChangedEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffOutput {
    pub tool: String,
    pub timestamp: String,
    pub before_label: String,
    pub after_label: String,
    pub added: Vec<StackDiffEntry>,
    pub removed: Vec<StackDiffEntry>,
    pub changed: Vec<ChangedEntry>,
}

// ---- Helpers ----

fn group_signature(frames: &[crate::stack_data::Frame]) -> String {
    frames
        .iter()
        .map(|f| f.function.as_str())
        .collect::<Vec<_>>()
        .join(";")
}

fn build_group_map(
    groups: Vec<UniqueStackGroup>,
    mode: MatchMode,
) -> HashMap<
    StackKey,
    (
        Vec<crate::stack_data::Frame>,
        Vec<crate::stack_data::ThreadIdent>,
    ),
> {
    let mut map = HashMap::new();
    for group in groups {
        let key = mode.build_key(&group.frames);
        map.insert(key, (group.frames, group.threads));
    }
    map
}

// ---- Core Algorithm ----

pub fn compute_diff(
    before: Vec<UniqueStackGroup>,
    after: Vec<UniqueStackGroup>,
    mode: MatchMode,
) -> DiffResult {
    let before_map = build_group_map(before, mode);
    let after_map = build_group_map(after, mode);

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();

    for (key, (frames, threads)) in &after_map {
        if !before_map.contains_key(key) {
            added.push(StackDiffEntry {
                signature: group_signature(frames),
                frames: frames.clone(),
                count: threads.len(),
            });
        }
    }

    for (key, (frames, threads)) in &before_map {
        if !after_map.contains_key(key) {
            removed.push(StackDiffEntry {
                signature: group_signature(frames),
                frames: frames.clone(),
                count: threads.len(),
            });
        }
    }

    for (key, (frames, before_threads)) in &before_map {
        if let Some((_, after_threads)) = after_map.get(key) {
            let before_count = before_threads.len();
            let after_count = after_threads.len();
            if before_count != after_count {
                changed.push(ChangedEntry {
                    signature: group_signature(frames),
                    frames: frames.clone(),
                    before_count,
                    after_count,
                });
            }
        }
    }

    added.sort_by_key(|b| std::cmp::Reverse(b.count));
    removed.sort_by_key(|b| std::cmp::Reverse(b.count));
    changed.sort_by_key(|b| std::cmp::Reverse(b.after_count));

    DiffResult {
        added,
        removed,
        changed,
    }
}

// ---- Formatting ----

fn format_entry_frames(entry: &StackDiffEntry, suffix: &str) -> String {
    let mut out = String::new();
    for frame in &entry.frames {
        out.push_str(&format!(
            "    #{}  {} {}\n",
            frame.depth, frame.address, frame.function
        ));
    }
    out.push_str(&format!("    ({})", suffix));
    out
}

pub fn format_diff_text(result: &DiffResult) -> String {
    let mut lines = Vec::new();
    lines.push("=== Stack Diff ===".to_string());
    lines.push(String::new());

    if !result.added.is_empty() {
        lines.push(format!("[+] added: {}", result.added.len()));
        for entry in &result.added {
            lines.push(format_entry_frames(
                entry,
                &format!(
                    "{} thread{}",
                    entry.count,
                    if entry.count > 1 { "s" } else { "" }
                ),
            ));
        }
        lines.push(String::new());
    }

    if !result.removed.is_empty() {
        lines.push(format!("[-] removed: {}", result.removed.len()));
        for entry in &result.removed {
            lines.push(format_entry_frames(
                entry,
                &format!(
                    "{} thread{}",
                    entry.count,
                    if entry.count > 1 { "s" } else { "" }
                ),
            ));
        }
        lines.push(String::new());
    }

    if !result.changed.is_empty() {
        lines.push(format!("[~] changed: {}", result.changed.len()));
        for entry in &result.changed {
            for frame in &entry.frames {
                lines.push(format!(
                    "    #{}  {} {}",
                    frame.depth, frame.address, frame.function
                ));
            }
            lines.push(format!(
                "    before: {} thread{}  after: {} thread{}",
                entry.before_count,
                if entry.before_count > 1 { "s" } else { "" },
                entry.after_count,
                if entry.after_count > 1 { "s" } else { "" },
            ));
        }
        lines.push(String::new());
    }

    if result.added.is_empty() && result.removed.is_empty() && result.changed.is_empty() {
        lines.push("    (no differences)".to_string());
        lines.push(String::new());
    }

    lines.join("\n")
}

pub fn format_diff_json(result: &DiffResult, before_label: &str, after_label: &str) -> String {
    let output = DiffOutput {
        tool: "cs diff".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        before_label: before_label.to_string(),
        after_label: after_label.to_string(),
        added: result.added.clone(),
        removed: result.removed.clone(),
        changed: result.changed.clone(),
    };
    serde_json::to_string_pretty(&output).unwrap()
}

// ---- Entry Point ----

pub async fn run_diff(before_path: &str, after_path: &str, cli: &Cli) {
    let before_content = tokio::fs::read_to_string(before_path)
        .await
        .unwrap_or_else(|e| {
            eprintln!("error: failed to read '{}': {}", before_path, e);
            std::process::exit(1);
        });
    let after_content = tokio::fs::read_to_string(after_path)
        .await
        .unwrap_or_else(|e| {
            eprintln!("error: failed to read '{}': {}", after_path, e);
            std::process::exit(1);
        });

    let before_data: OutputData = serde_json::from_str(&before_content).unwrap_or_else(|e| {
        eprintln!("error: failed to parse '{}': {}", before_path, e);
        std::process::exit(1);
    });
    let after_data: OutputData = serde_json::from_str(&after_content).unwrap_or_else(|e| {
        eprintln!("error: failed to parse '{}': {}", after_path, e);
        std::process::exit(1);
    });

    let mode = cli.effective_match_mode();
    let result = compute_diff(before_data.stacks, after_data.stacks, mode);

    if cli.json_mode {
        println!("{}", format_diff_json(&result, before_path, after_path));
    } else {
        println!("{}", format_diff_text(&result));
    }
}

// ---- Live Diff ----

async fn sample_once(cli: &Cli, pid: i32) -> Vec<ThreadStack> {
    if cli.gdb_mode {
        let frames = if cli.frames == 0 {
            "full".to_string()
        } else {
            cli.frames.to_string()
        };
        let cmd = format!("thread apply all backtrace {frames}");
        let args = vec![
            "--batch".to_string(),
            "-p".to_string(),
            pid.to_string(),
            "-ex".to_string(),
            cmd,
        ];
        match execute_command("gdb", &args).await {
            Ok((code, out, err)) => {
                if code <= 1 {
                    if !err.is_empty() {
                        eprintln!("Warnings reported: {err}");
                    }
                    crate::stack_data::parse_gdb(&out, !cli.raw_mode)
                } else {
                    eprintln!("gdb failed:\n{err}");
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("gdb failed: {e}");
                std::process::exit(1);
            }
        }
    } else {
        let args = vec![
            "-n".to_string(),
            cli.frames.to_string(),
            "-p".to_string(),
            pid.to_string(),
        ];
        match execute_command("eu-stack", &args).await {
            Ok((code, out, err)) => {
                if code <= 1 {
                    if !err.is_empty() {
                        eprintln!("Warnings reported: {err}");
                    }
                    crate::stack_data::parse_eustack(&out)
                } else {
                    eprintln!("eu-stack failed:\n{err}");
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("eu-stack failed: {e}");
                std::process::exit(1);
            }
        }
    }
}

pub async fn run_diff_live(cli: &Cli) {
    let pids = cli.pids.as_ref().expect("pids required for live diff");
    let pid = pids[0];

    eprintln!("Capturing before snapshot...");
    let before = sample_once(cli, pid).await;

    if let Some(secs) = cli.interval {
        eprintln!("Waiting {secs}s before after snapshot...");
        tokio::time::sleep(Duration::from_secs_f32(secs)).await;
    } else {
        eprintln!("Press ENTER to capture after snapshot...");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
    }

    eprintln!("Capturing after snapshot...");
    let after = sample_once(cli, pid).await;

    let mode = cli.effective_match_mode();
    let before = crate::stack_data::dedup_stacks(before, mode);
    let after = crate::stack_data::dedup_stacks(after, mode);
    let result = compute_diff(before, after, mode);

    if cli.json_mode {
        println!(
            "{}",
            format_diff_json(&result, "(live before)", "(live after)")
        );
    } else {
        println!("{}", format_diff_text(&result));
    }
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack_data::Frame;

    fn make_frame(depth: u32, address: &str, function: &str) -> Frame {
        Frame {
            depth,
            address: address.to_string(),
            function: function.to_string(),
            library: None,
        }
    }

    fn make_group(frames: Vec<Frame>, threads: usize) -> UniqueStackGroup {
        UniqueStackGroup {
            threads: (0..threads)
                .map(|i| crate::stack_data::ThreadIdent {
                    pid: 1,
                    tid: i as i32,
                    thread_name: String::new(),
                })
                .collect(),
            frames,
            suspicious: false,
        }
    }

    #[test]
    fn test_diff_all_added() {
        let before = vec![];
        let after = vec![make_group(vec![make_frame(0, "0x1", "func_a")], 2)];
        let result = compute_diff(before, after, MatchMode::Precise);
        assert_eq!(result.added.len(), 1);
        assert_eq!(result.removed.len(), 0);
        assert_eq!(result.changed.len(), 0);
        assert_eq!(result.added[0].count, 2);
    }

    #[test]
    fn test_diff_all_removed() {
        let before = vec![make_group(vec![make_frame(0, "0x1", "func_a")], 1)];
        let after = vec![];
        let result = compute_diff(before, after, MatchMode::Precise);
        assert_eq!(result.added.len(), 0);
        assert_eq!(result.removed.len(), 1);
        assert_eq!(result.changed.len(), 0);
    }

    #[test]
    fn test_diff_no_change() {
        let group = make_group(vec![make_frame(0, "0x1", "func_a")], 1);
        let result = compute_diff(vec![group.clone()], vec![group], MatchMode::Precise);
        assert_eq!(result.added.len(), 0);
        assert_eq!(result.removed.len(), 0);
        assert_eq!(result.changed.len(), 0);
    }

    #[test]
    fn test_diff_changed_count() {
        let frames = vec![make_frame(0, "0x1", "func_a")];
        let before = vec![make_group(frames.clone(), 3)];
        let after = vec![make_group(frames, 5)];
        let result = compute_diff(before, after, MatchMode::Precise);
        assert_eq!(result.added.len(), 0);
        assert_eq!(result.removed.len(), 0);
        assert_eq!(result.changed.len(), 1);
        assert_eq!(result.changed[0].before_count, 3);
        assert_eq!(result.changed[0].after_count, 5);
    }

    #[test]
    fn test_diff_fuzzy_matches_across_addresses() {
        let before = vec![make_group(vec![make_frame(0, "0xaaa", "func_a")], 2)];
        let after = vec![make_group(vec![make_frame(0, "0xbbb", "func_a")], 3)];
        let precise = compute_diff(before.clone(), after.clone(), MatchMode::Precise);
        assert_eq!(precise.added.len(), 1);
        assert_eq!(precise.removed.len(), 1);

        let fuzzy = compute_diff(before.clone(), after, MatchMode::Fuzzy);
        assert_eq!(fuzzy.added.len(), 0);
        assert_eq!(fuzzy.removed.len(), 0);
        assert_eq!(fuzzy.changed.len(), 1);
        assert_eq!(fuzzy.changed[0].before_count, 2);
        assert_eq!(fuzzy.changed[0].after_count, 3);
    }

    #[test]
    fn test_diff_mixed_scenario() {
        let before = vec![
            make_group(vec![make_frame(0, "0x1", "gone")], 1),
            make_group(vec![make_frame(0, "0x2", "stable")], 3),
            make_group(vec![make_frame(0, "0x3", "growing")], 2),
        ];
        let after = vec![
            make_group(vec![make_frame(0, "0x4", "new")], 1),
            make_group(vec![make_frame(0, "0x2", "stable")], 3),
            make_group(vec![make_frame(0, "0x3", "growing")], 5),
        ];
        let result = compute_diff(before, after, MatchMode::Precise);
        assert_eq!(result.added.len(), 1);
        assert_eq!(result.added[0].signature, "new");
        assert_eq!(result.removed.len(), 1);
        assert_eq!(result.removed[0].signature, "gone");
        assert_eq!(result.changed.len(), 1);
        assert_eq!(result.changed[0].signature, "growing");
        assert_eq!(result.changed[0].before_count, 2);
        assert_eq!(result.changed[0].after_count, 5);
    }

    #[test]
    fn test_format_text_no_diff() {
        let result = DiffResult {
            added: vec![],
            removed: vec![],
            changed: vec![],
        };
        let text = format_diff_text(&result);
        assert!(text.contains("(no differences)"));
    }

    #[test]
    fn test_format_text_added_and_removed() {
        let frames_added = vec![make_frame(0, "0x1", "new_func")];
        let frames_removed = vec![make_frame(0, "0x2", "old_func")];
        let result = DiffResult {
            added: vec![StackDiffEntry {
                signature: "new_func".into(),
                frames: frames_added,
                count: 2,
            }],
            removed: vec![StackDiffEntry {
                signature: "old_func".into(),
                frames: frames_removed,
                count: 1,
            }],
            changed: vec![],
        };
        let text = format_diff_text(&result);
        assert!(text.contains("[+] added: 1"));
        assert!(text.contains("#0  0x1 new_func"));
        assert!(text.contains("(2 threads)"));
        assert!(text.contains("[-] removed: 1"));
        assert!(text.contains("#0  0x2 old_func"));
        assert!(text.contains("(1 thread)"));
    }

    #[test]
    fn test_format_text_changed() {
        let frames = vec![make_frame(0, "0xa", "top"), make_frame(1, "0xb", "middle")];
        let result = DiffResult {
            added: vec![],
            removed: vec![],
            changed: vec![ChangedEntry {
                signature: "top;middle".into(),
                frames,
                before_count: 2,
                after_count: 5,
            }],
        };
        let text = format_diff_text(&result);
        assert!(text.contains("[~] changed: 1"));
        assert!(text.contains("#0  0xa top"));
        assert!(text.contains("#1  0xb middle"));
        assert!(text.contains("before: 2 threads  after: 5 threads"));
    }

    #[test]
    fn test_format_json_basic() {
        let frames = vec![make_frame(0, "0x1", "f1")];
        let result = DiffResult {
            added: vec![StackDiffEntry {
                signature: "f1".into(),
                frames: frames.clone(),
                count: 1,
            }],
            removed: vec![],
            changed: vec![],
        };
        let json = format_diff_json(&result, "before.json", "after.json");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["tool"], "cs diff");
        assert_eq!(parsed["before_label"], "before.json");
        assert_eq!(parsed["after_label"], "after.json");
        assert!(parsed["timestamp"].is_string());
        assert_eq!(parsed["added"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["removed"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["changed"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["added"][0]["count"], 1);
    }
}
