use std::collections::HashMap;

use colored::*;
use regex::Regex;
use serde::Serialize;

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

// ---- Parsing: eu-stack format ----

pub fn parse_eustack(input: &str) -> Vec<ThreadStack> {
    let re_pid = Regex::new(r"PID\s+(?P<pid>\d+)\s+-\s+").unwrap();
    let re_tid = Regex::new(r"TID\s+(?P<tid>\d+):").unwrap();
    let re_frame =
        Regex::new(r"^#(?P<depth>\d+)\s+(?P<addr>0x[0-9a-fA-F]+)\s+(?P<func>.+?)$").unwrap();

    let mut stacks = Vec::new();
    let mut pid = 0;
    let mut tid = 0;
    let mut thread_name = String::new();
    let mut frames = Vec::new();

    for line in input.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        if let Some(caps) = re_pid.captures(line) {
            flush_stack(&mut stacks, pid, tid, &thread_name, &mut frames);
            pid = caps["pid"].parse().unwrap_or(0);
        } else if let Some(caps) = re_tid.captures(line) {
            flush_stack(&mut stacks, pid, tid, &thread_name, &mut frames);
            tid = caps["tid"].parse().unwrap_or(0);
            thread_name.clear();
        } else if let Some(caps) = re_frame.captures(line) {
            frames.push(Frame {
                depth: caps["depth"].parse().unwrap_or(0),
                address: caps["addr"].to_string(),
                function: caps["func"].to_string(),
                library: None,
            });
        }
    }

    flush_stack(&mut stacks, pid, tid, &thread_name, &mut frames);
    stacks
}

// ---- Parsing: gdb format ----

static RE_GDB_TID: &str = r"Thread\s+(?P<threadnum>\d+)\s+.*\(LWP\s+(?P<lwp>\d+).*\):";
static RE_GDB_DETACH: &str = r"Inferior.*detached";
static RE_GDB_SIMPLIFY: &str = r"\s+in\s+(?P<func>.+?)\s+\(.*?\)\s+(at|from)\s+.*";
static RE_GDB_LIBRARY: &str = r"from\s+(?P<lib>\S+)";

pub fn parse_gdb(input: &str, simplify: bool) -> Vec<ThreadStack> {
    let re_tid = Regex::new(RE_GDB_TID).unwrap();
    let re_detach = Regex::new(RE_GDB_DETACH).unwrap();
    let re_simplify = Regex::new(RE_GDB_SIMPLIFY).unwrap();
    let re_frame_num = Regex::new(r"^\s*#\s*(?P<depth>\d+)").unwrap();
    let re_addr = Regex::new(r"0x[0-9a-fA-F]+").unwrap();
    let re_library = Regex::new(RE_GDB_LIBRARY).unwrap();

    let mut stacks = Vec::new();
    let mut tid = 0;
    let mut thread_name = String::new();
    let mut frames = Vec::new();
    let mut match_started = false;

    for line in input.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        if let Some(caps) = re_tid.captures(line) {
            flush_stack(&mut stacks, 0, tid, &thread_name, &mut frames);
            match_started = true;
            tid = caps["lwp"].parse().unwrap_or(0);
            thread_name = caps["threadnum"].to_string();
        } else if re_detach.is_match(line) || !match_started {
            continue;
        } else if let Some(fcaps) = re_frame_num.captures(line) {
            let depth: u32 = fcaps["depth"].parse().unwrap_or(0);
            let addr = re_addr
                .find(line)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let function = if simplify {
                re_simplify
                    .captures(line)
                    .map(|c| c["func"].to_string())
                    .unwrap_or_else(|| line.trim().to_string())
            } else {
                line.trim().to_string()
            };
            let library = re_library.captures(line).map(|c| c["lib"].to_string());
            frames.push(Frame {
                depth,
                address: addr,
                function,
                library,
            });
        } else {
            eprintln!("IGNORE: Failed to parse: {line}");
        }
    }

    flush_stack(&mut stacks, 0, tid, &thread_name, &mut frames);
    stacks
}

fn flush_stack(
    stacks: &mut Vec<ThreadStack>,
    pid: i32,
    tid: i32,
    thread_name: &str,
    frames: &mut Vec<Frame>,
) {
    if !frames.is_empty() {
        stacks.push(ThreadStack {
            pid,
            tid,
            thread_name: thread_name.to_string(),
            frames: std::mem::take(frames),
        });
    }
}

// ---- Dedup ----

pub fn dedup_stacks(stacks: Vec<ThreadStack>) -> Vec<UniqueStackGroup> {
    let mut groups: HashMap<Vec<Frame>, UniqueStackGroup> = HashMap::new();

    for stack in stacks {
        let entry = groups
            .entry(stack.frames.clone())
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

    #[test]
    fn test_parse_eustack_single_thread() {
        let input = "PID 1234 - process\n\
                      TID 1234:\n\
                      #0  0x7f83df80a3ec func_a\n\
                      #1  0x7f83df14f421 func_b\n";
        let stacks = parse_eustack(input);
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].pid, 1234);
        assert_eq!(stacks[0].tid, 1234);
        assert_eq!(stacks[0].frames.len(), 2);
    }

    #[test]
    fn test_parse_eustack_multi_thread() {
        let input = "PID 14794 - process\n\
                      TID 14794:\n\
                      #0  0x7f83df80a3ec func_a\n\
                      TID 14818:\n\
                      #0  0x7f83ddba6fea func_b\n";
        let stacks = parse_eustack(input);
        assert_eq!(stacks.len(), 2);
        assert_eq!(stacks[1].tid, 14818);
    }

    #[test]
    fn test_parse_gdb() {
        let input = r#"Thread 1 (Thread 0x7f... (LWP 1234) "test"):
 #0  0x7f... in clock_nanosleep () from /usr/lib64/libc.so.6
 #1  0x7f... in func_a () at test.c:10
"#;
        let stacks = parse_gdb(input, false);
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].tid, 1234);
        assert_eq!(stacks[0].frames.len(), 2);
    }

    #[test]
    fn test_parse_gdb_simplify() {
        let input = r#"Thread 1 (Thread 0x7f... (LWP 1234) "test"):
 #0  0x7f... in clock_nanosleep () from /usr/lib64/libc.so.6
"#;
        let stacks = parse_gdb(input, true);
        assert_eq!(stacks.len(), 1);
        assert!(stacks[0].frames[0].function.contains("clock_nanosleep"));
        assert!(!stacks[0].frames[0].function.contains("libc.so.6"));
    }

    #[test]
    fn test_dedup_identical_stacks() {
        let input = "PID 1 - proc\n\
                      TID 100:\n\
                      #0  0x1 func_x\n\
                      TID 101:\n\
                      #0  0x1 func_x\n";
        let stacks = parse_eustack(input);
        let groups = dedup_stacks(stacks);
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
    fn test_parse_gdb_library() {
        let input = r#"Thread 1 (Thread 0x7f... (LWP 1234) "test"):
 #0  0x7f... in clock_nanosleep () from /usr/lib64/libc.so.6
 #1  0x7f... in func_a () at test.c:10
"#;
        let stacks = parse_gdb(input, true);
        assert_eq!(stacks.len(), 1);
        assert_eq!(
            stacks[0].frames[0].library,
            Some("/usr/lib64/libc.so.6".into())
        );
        assert_eq!(stacks[0].frames[1].library, None);
    }

    #[test]
    fn test_parse_gdb_full_stack_and_dedup() {
        let input = r#"Thread 1 (Thread 0x7f29ce816740 (LWP 37746) "test"):
#0  0x00007f29ce8db9e7 in clock_nanosleep () from /usr/lib64/libc.so.6
#1  0x00007f29ce8e6a47 in nanosleep () from /usr/lib64/libc.so.6
#2  0x00007f29ce8f7bce in sleep () from /usr/lib64/libc.so.6
#3  0x000055723be89162 in func2 () at test.c:5
#4  0x000055723be8917d in func1 () at test.c:10

Thread 2 (Thread 0x7f29ce816740 (LWP 37748) "test"):
#0  0x00007f29ce8db9e7 in clock_nanosleep () from /usr/lib64/libc.so.6
#1  0x00007f29ce8e6a47 in nanosleep () from /usr/lib64/libc.so.6
#2  0x00007f29ce8f7bce in sleep () from /usr/lib64/libc.so.6
#3  0x000055723be89162 in func2 () at test.c:5
#4  0x000055723be8917d in func1 () at test.c:10
"#;
        let stacks = parse_gdb(input, true);
        assert_eq!(stacks.len(), 2);
        let groups = dedup_stacks(stacks);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].threads.len(), 2);
        assert_eq!(groups[0].frames.len(), 5);
    }

    #[test]
    fn test_simplify_gdb_frame() {
        let input = r#"Thread 1 (Thread 0x7f... (LWP 1234) "test"):
 #0  0x7f... in clock_nanosleep () from /usr/lib64/libc.so.6
"#;
        let raw = parse_gdb(input, false);
        let simplified = parse_gdb(input, true);
        assert!(raw[0].frames[0].function.contains("libc.so.6"));
        assert!(!simplified[0].frames[0].function.contains("libc.so.6"));
        assert!(simplified[0].frames[0].function.contains("clock_nanosleep"));
    }

    #[test]
    fn test_dedup_multi_pid() {
        let input = "PID 100 - proc\n\
                      TID 1000:\n\
                      #0  0x1 func_x\n\
                      PID 200 - proc\n\
                      TID 2000:\n\
                      #0  0x1 func_x\n";
        let stacks = parse_eustack(input);
        assert_eq!(stacks.len(), 2);
        let groups = dedup_stacks(stacks);
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
}
