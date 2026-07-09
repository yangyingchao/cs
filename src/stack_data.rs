use std::collections::HashMap;
use std::sync::LazyLock;

use colored::*;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::match_mode::{MatchMode, StackKey};

// ---- Data Model ----

#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub struct Frame {
    pub depth: u32,
    pub address: String,
    pub function: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniqueStackGroup {
    pub threads: Vec<ThreadIdent>,
    pub frames: Vec<Frame>,
    pub suspicious: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingInfo {
    pub interval: f32,
    pub count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

fn suspicious_pattern() -> String {
    format!("(?i)({})", SUSPICIOUS_KEYWORDS.join("|"))
}

static RE_SUSPICIOUS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&suspicious_pattern()).unwrap());

static RE_SUSPICIOUS_HIGHLIGHT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r#"(?i)(?P<sus>.*({}).*)"#,
        SUSPICIOUS_KEYWORDS.join("|")
    ))
    .unwrap()
});

fn is_suspicious(function: &str) -> bool {
    RE_SUSPICIOUS.is_match(function)
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
        let entry = groups.entry(key).or_insert_with(|| UniqueStackGroup {
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

// ---- Truncation ----

pub const TRUNCATION_LIMIT: usize = 5;

// ---- Exclude filtering ----

pub fn compile_excludes(patterns: &[String]) -> Result<Vec<Regex>, regex::Error> {
    patterns.iter().map(|p| Regex::new(p)).collect()
}

pub fn filter_excluded(
    mut groups: Vec<UniqueStackGroup>,
    patterns: &[Regex],
) -> Vec<UniqueStackGroup> {
    if patterns.is_empty() {
        return groups;
    }
    groups.retain(|group| {
        // All threads in a UniqueStackGroup share the same frames, so if any
        // frame matches an exclude pattern every thread in this group is
        // excluded and the group is dropped.
        !group
            .frames
            .iter()
            .any(|f| patterns.iter().any(|p| p.is_match(&f.function)))
    });
    groups
}

// ---- Formatting ----

pub fn format_text(
    groups: &[UniqueStackGroup],
    sampling_prefix: &str,
    max_groups: Option<usize>,
) -> String {
    let mut outputs = Vec::new();
    let mut all_suspicious = Vec::new();

    let r_match = &RE_SUSPICIOUS_HIGHLIGHT;

    let (visible, hidden) = match max_groups {
        Some(n) if n < groups.len() => (&groups[..n], Some(&groups[n..])),
        _ => (groups, None),
    };

    for group in visible {
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

    let mut body = outputs.join("\n");

    if let Some(hidden_groups) = hidden {
        let hidden_count = hidden_groups.len();
        let suspicious_count = hidden_groups.iter().filter(|g| g.suspicious).count();
        body.push_str(&format!(
            "\n... and {hidden_count} more unique stack groups \
             ({suspicious_count} suspicious). Use --verbose to show all."
        ));
    }

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

    fn make_thread(pid: i32, tid: i32) -> ThreadIdent {
        ThreadIdent {
            pid,
            tid,
            thread_name: String::new(),
        }
    }

    fn make_group(threads: Vec<ThreadIdent>, frames: Vec<Frame>) -> UniqueStackGroup {
        UniqueStackGroup {
            suspicious: any_frame_suspicious(&frames),
            threads,
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
        let stacks = vec![make_stack(1, 100, vec![f1]), make_stack(1, 101, vec![f2])];
        let fuzzy_groups = dedup_stacks(stacks.clone(), MatchMode::Fuzzy);
        assert_eq!(fuzzy_groups.len(), 1);
        assert_eq!(fuzzy_groups[0].threads.len(), 2);

        let precise_groups = dedup_stacks(stacks, MatchMode::Precise);
        assert_eq!(precise_groups.len(), 2);
    }

    #[test]
    fn test_filter_excluded_no_match() {
        let groups = vec![make_group(
            vec![make_thread(1, 100)],
            vec![make_frame(0, "0x1", "func_x")],
        )];
        let patterns = compile_excludes(&["nonexistent".to_string()]).unwrap();
        let result = filter_excluded(groups, &patterns);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_filter_excluded_partial_group() {
        // After dedup, all threads in a UniqueStackGroup share the same frames.
        // If any frame matches an exclude pattern, the entire group is dropped.
        let groups = vec![
            make_group(
                vec![make_thread(1, 100), make_thread(1, 101)],
                vec![make_frame(0, "0x1", "keep_func")],
            ),
            make_group(
                vec![make_thread(1, 200)],
                vec![make_frame(0, "0x2", "drop_func")],
            ),
        ];
        let patterns = compile_excludes(&["drop_func".to_string()]).unwrap();
        let result = filter_excluded(groups, &patterns);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].threads.len(), 2);
        assert!(result[0].frames.iter().all(|f| f.function == "keep_func"));
    }

    #[test]
    fn test_filter_excluded_all_removed() {
        let groups = vec![
            make_group(
                vec![make_thread(1, 100)],
                vec![make_frame(0, "0x1", "bad_func")],
            ),
            make_group(
                vec![make_thread(1, 200)],
                vec![make_frame(0, "0x2", "also_bad")],
            ),
        ];
        let patterns = compile_excludes(&["bad".to_string()]).unwrap();
        let result = filter_excluded(groups, &patterns);
        assert!(result.is_empty());
    }

    #[test]
    fn test_format_text_truncation() {
        let groups: Vec<UniqueStackGroup> = (0..6)
            .map(|i| {
                make_group(
                    vec![make_thread(1, 100 + i)],
                    vec![make_frame(0, "0x1", "func_x")],
                )
            })
            .collect();
        let output = format_text(&groups, "", Some(3));
        assert!(output.contains("... and 3 more unique stack groups"));
        assert!(output.contains("Use --verbose to show all."));
    }

    #[test]
    fn test_format_text_no_truncation_when_under_limit() {
        let groups: Vec<UniqueStackGroup> = (0..2)
            .map(|i| {
                make_group(
                    vec![make_thread(1, 100 + i)],
                    vec![make_frame(0, "0x1", "func_x")],
                )
            })
            .collect();
        let output = format_text(&groups, "", Some(5));
        assert!(!output.contains("... and"));
        assert!(!output.contains("Use --verbose"));
    }

    #[test]
    fn test_format_text_no_truncation_when_none() {
        let groups: Vec<UniqueStackGroup> = (0..6)
            .map(|i| {
                make_group(
                    vec![make_thread(1, 100 + i)],
                    vec![make_frame(0, "0x1", "func_x")],
                )
            })
            .collect();
        let output = format_text(&groups, "", None);
        assert!(!output.contains("... and"));
        assert!(!output.contains("Use --verbose"));
    }

    #[test]
    fn test_format_text_truncation_suspicious_count() {
        let normal = || make_frame(0, "0x1", "func_x");
        let suspicious = || make_frame(0, "0x1", "raise");
        let groups: Vec<UniqueStackGroup> = vec![
            make_group(vec![make_thread(1, 100)], vec![normal()]),
            make_group(vec![make_thread(1, 101)], vec![normal()]),
            make_group(vec![make_thread(1, 102)], vec![normal()]),
            make_group(vec![make_thread(1, 103)], vec![normal()]),
            make_group(vec![make_thread(1, 104)], vec![suspicious()]),
            make_group(vec![make_thread(1, 105)], vec![suspicious()]),
        ];
        let output = format_text(&groups, "", Some(3));
        assert!(output.contains("... and 3 more unique stack groups"));
        assert!(output.contains("(2 suspicious)"));
    }

    #[test]
    fn test_compile_excludes_invalid_regex() {
        let result = compile_excludes(&["[invalid".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_compile_excludes_valid_regex() {
        let result = compile_excludes(&["func_.*".to_string(), "raise".to_string()]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }
}
