use std::collections::HashMap;

use colored::*;
use regex::Regex;
use serde::Serialize;

use crate::match_mode::{MatchMode, StackKey};

// ---- Data Model ----

#[derive(Debug, Clone, Serialize, Hash, Eq, PartialEq)]
pub struct Frame {
    pub depth: u32,
    pub address: String,
    pub function: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThreadIdent {
    pub pid: i32,
    pub tid: i32,
    pub thread_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThreadStack {
    pub pid: i32,
    pub tid: i32,
    pub thread_name: String,
    pub frames: Vec<Frame>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UniqueStackGroup {
    pub threads: Vec<ThreadIdent>,
    pub frames: Vec<Frame>,
    pub suspicious: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SamplingInfo {
    pub interval: f32,
    pub count: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutputData {
    pub tool: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<SamplingInfo>,
    pub stacks: Vec<UniqueStackGroup>,
}

// ---- Helpers ----

fn timestamp_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

// ---- Suspicious detection ----

static SUSPICIOUS_KEYWORDS: &[&str] = &[
    "__assert_fail",
    "fatal.*signals",
    "raise",
    "segfault",
    "segment fault",
    "segmentfault",
    "signal handler called",
];

fn is_suspicious(function: &str) -> bool {
    let pattern = format!("(?i)({})", SUSPICIOUS_KEYWORDS.join("|"));
    let re = Regex::new(&pattern).unwrap();
    re.is_match(function)
}

fn any_frame_suspicious(frames: &[Frame]) -> bool {
    frames.iter().any(|f| is_suspicious(&f.function))
}

pub use crate::input_eustack::parse_eustack;
pub use crate::input_gdb::parse_gdb;

// ---- Dedup ----

pub fn dedup_stacks(stacks: Vec<ThreadStack>, mode: MatchMode) -> Vec<UniqueStackGroup> {
    let mut groups: HashMap<StackKey, UniqueStackGroup> = HashMap::new();

    for stack in stacks {
        let key = mode.build_key(&stack.frames);
        let entry = groups
            .entry(key)
            .or_insert_with(|| UniqueStackGroup {
                threads: Vec::new(),
                frames: stack.frames.clone(),
                suspicious: any_frame_suspicious(&stack.frames),
            });
        entry.threads.push(ThreadIdent {
            pid: stack.pid,
            tid: stack.tid,
            thread_name: stack.thread_name,
        });
    }

    let mut result: Vec<UniqueStackGroup> = groups.into_values().collect();
    result.sort_by_key(|b| std::cmp::Reverse(b.threads.len()));
    result
}

pub fn to_groups(stacks: Vec<ThreadStack>) -> Vec<UniqueStackGroup> {
    stacks
        .into_iter()
        .map(|s| UniqueStackGroup {
            suspicious: any_frame_suspicious(&s.frames),
            threads: vec![ThreadIdent {
                pid: s.pid,
                tid: s.tid,
                thread_name: s.thread_name,
            }],
            frames: s.frames,
        })
        .collect()
}

// ---- Formatting ----

pub fn format_text(groups: &[UniqueStackGroup], sampling_prefix: &str) -> String {
    let mut outputs = Vec::new();
    let mut all_suspicious = Vec::new();

    let keywords = SUSPICIOUS_KEYWORDS;
    let pattern = format!(r#"(?i)(?P<sus>.*({}).*)"#, keywords.join("|"));
    let r_match = Regex::new(&pattern).unwrap();

    for group in groups {
        let tids_str = group
            .threads
            .iter()
            .map(|t| t.tid.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        let mut stack_text = String::new();
        for frame in &group.frames {
            let raw = format!("#{}  {} {}", frame.depth, frame.address, frame.function);
            if group.suspicious && r_match.is_match(&frame.function) {
                stack_text.push_str(&raw.blue());
                stack_text.push_str(&"                           <---- HERE ".red().bold());
                stack_text.push('\n');
            } else {
                stack_text.push_str(&raw);
                stack_text.push('\n');
            }
        }

        outputs.push(format!(
            "Number of thread: {} -- {}:\n{}",
            group.threads.len(),
            tids_str,
            stack_text.trim_end()
        ));

        if group.suspicious {
            for t in &group.threads {
                all_suspicious.push(t.tid.to_string());
            }
        }
    }

    if !all_suspicious.is_empty() {
        outputs.push(format!(
            "Suspicious threads: {}",
            all_suspicious.join(", ").red()
        ));
    }

    let body = outputs.join("\n");
    if sampling_prefix.is_empty() {
        body
    } else {
        format!("{sampling_prefix}\n{body}")
    }
}

pub fn format_json(
    groups: &[UniqueStackGroup],
    tool: &str,
    sampling: Option<SamplingInfo>,
) -> String {
    let output = OutputData {
        tool: tool.to_string(),
        timestamp: timestamp_iso(),
        sampling,
        stacks: groups.to_vec(),
    };
    serde_json::to_string_pretty(&output).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_frame(depth: u32, address: &str, function: &str) -> Frame {
        Frame {
            depth,
            address: address.to_string(),
            function: function.to_string(),
            library: None,
        }
    }

    fn make_stack(pid: i32, tid: i32, frames: Vec<Frame>) -> ThreadStack {
        ThreadStack {
            pid,
            tid,
            thread_name: String::new(),
            frames,
        }
    }

    #[test]
    fn test_dedup_identical_stacks() {
        let f = make_frame(0, "0x1", "func_x");
        let stacks = vec![
            make_stack(1, 100, vec![f.clone()]),
            make_stack(1, 101, vec![f]),
        ];
        let groups = dedup_stacks(stacks, MatchMode::Precise);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].threads.len(), 2);
    }

    #[test]
    fn test_suspicious_detection() {
        assert!(is_suspicious("raise"));
        assert!(is_suspicious("__assert_fail"));
        assert!(is_suspicious("sigwait (signal handler called)"));
        assert!(!is_suspicious("clock_nanosleep"));
    }

    #[test]
    fn test_dedup_multi_pid() {
        let f = make_frame(0, "0x1", "func_x");
        let stacks = vec![
            make_stack(100, 1000, vec![f.clone()]),
            make_stack(200, 2000, vec![f]),
        ];
        let groups = dedup_stacks(stacks, MatchMode::Precise);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].threads.len(), 2);
        assert!(groups[0]
            .threads
            .iter()
            .any(|t| t.pid == 100 && t.tid == 1000));
        assert!(groups[0]
            .threads
            .iter()
            .any(|t| t.pid == 200 && t.tid == 2000));
    }

    #[test]
    fn test_dedup_fuzzy_ignores_address() {
        let f1 = make_frame(0, "0xaaa", "func_a");
        let f2 = make_frame(0, "0xbbb", "func_a");
        let stacks = vec![
            make_stack(1, 100, vec![f1]),
            make_stack(1, 101, vec![f2]),
        ];
        let fuzzy_groups = dedup_stacks(stacks.clone(), MatchMode::Fuzzy);
        assert_eq!(fuzzy_groups.len(), 1);
        assert_eq!(fuzzy_groups[0].threads.len(), 2);

        let precise_groups = dedup_stacks(stacks, MatchMode::Precise);
        assert_eq!(precise_groups.len(), 2);
    }
}
