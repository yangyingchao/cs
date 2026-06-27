# Phase 1: Match Mode (--match) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `--match <precise|fuzzy>` flag for configurable stack matching granularity, shared between dedup and future diff.

**Architecture:**
- New `MatchMode` enum in dedicated module, used as HashMap key selector in `dedup_stacks`
- Auto-selection logic in `Cli` (delayed to execution phase to handle interactive mode)
- Three backends pass `Cli::effective_match_mode()` to `dedup_stacks`

**Tech Stack:** Rust, clap derive, serde

---

### Task 1: Define `MatchMode` and `StackKey`

**Files:**
- Create: `src/match_mode.rs`
- Modify: `src/main.rs` (add `mod match_mode`)

- [ ] **Step 1: Create `src/match_mode.rs`**

```rust
use crate::stack_data::Frame;

/// Stack matching granularity
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum MatchMode {
    /// Match using all frame fields (address, depth, function, library)
    Precise,
    /// Match using function names only
    Fuzzy,
}

/// Key for dedup HashMap, supporting both match modes.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum StackKey {
    Full(Vec<Frame>),
    Signature(String),
}

impl MatchMode {
    pub fn build_key(&self, frames: &[Frame]) -> StackKey {
        match self {
            MatchMode::Precise => StackKey::Full(frames.to_vec()),
            MatchMode::Fuzzy => {
                let sig: Vec<&str> = frames.iter().map(|f| f.function.as_str()).collect();
                StackKey::Signature(sig.join(";"))
            }
        }
    }
}
```

- [ ] **Step 2: Add module to `main.rs`**

After `mod utils;` add:

```rust
mod match_mode;
```

- [ ] **Step 3: Verify build**

Run: `cargo build`
Expected: Build succeeds, no warnings.

- [ ] **Step 4: Commit**

```bash
git add src/match_mode.rs src/main.rs
git commit -m "feat: add MatchMode enum and StackKey with build_key"
```

---

### Task 2: Add `--match` flag and auto-selection to `Cli`

**Files:**
- Modify: `src/args.rs`

- [ ] **Step 1: Add imports and `match_mode` field to `Cli` struct

Add `use crate::match_mode::MatchMode;` to the import section of `args.rs`.

Add the field to `Cli` struct:**

Add the field alongside existing flags:

```rust
#[arg(long = "match", value_enum)]
pub match_mode: Option<MatchMode>,
```

Add to `Cli::default()`:

```rust
match_mode: None,
```

- [ ] **Step 2: Add `effective_match_mode()` method**

Implement the auto-selection logic on `Cli`:

```rust
impl Cli {
    pub fn effective_match_mode(&self) -> MatchMode {
        if let Some(mode) = self.match_mode {
            return mode;
        }
        if !self.files.is_empty()
            || self.parent.is_some()
            || self.pattern.is_some()
            || self.pids.as_ref().is_some_and(|p| p.len() > 1)
        {
            MatchMode::Fuzzy
        } else {
            MatchMode::Precise
        }
    }
}
```

- [ ] **Step 3: Add conflict warning method**

```rust
impl Cli {
    /// Returns true if in multi-source mode irrespective of explicit --match.
    fn is_multi_source(&self) -> bool {
        !self.files.is_empty()
            || self.parent.is_some()
            || self.pattern.is_some()
            || self.pids.as_ref().is_some_and(|p| p.len() > 1)
    }

    /// Print warning when --match precise conflicts with multi-source input.
    pub fn warn_if_match_conflict(&self) {
        if self.match_mode == Some(MatchMode::Precise) && self.is_multi_source() {
            eprintln!(
                "warning: --match precise with multiple input sources may cause\n\
                 identical stacks to appear different due to ASLR.\n\
                 Consider using --match fuzzy (or omit --match for auto)."
            );
        }
    }
}
```

- [ ] **Step 4: Add tests for `effective_match_mode()`**

Add to the `#[cfg(test)]` section of `args.rs`:

```rust
#[test]
fn test_match_mode_default_precise() {
    let cli = Cli {
        files: vec![],
        pids: None,
        parent: None,
        pattern: None,
        match_mode: None,
        ..Cli::default()
    };
    assert_eq!(cli.effective_match_mode(), MatchMode::Precise);
}

#[test]
fn test_match_mode_files_implies_fuzzy() {
    let cli = Cli {
        files: vec!["file.stack".into()],
        match_mode: None,
        ..Cli::default()
    };
    assert_eq!(cli.effective_match_mode(), MatchMode::Fuzzy);
}

#[test]
fn test_match_mode_multi_pid_implies_fuzzy() {
    let cli = Cli {
        pids: Some(vec![100, 101]),
        match_mode: None,
        ..Cli::default()
    };
    assert_eq!(cli.effective_match_mode(), MatchMode::Fuzzy);
}

#[test]
fn test_match_mode_explicit_override() {
    let cli = Cli {
        files: vec!["f.stack".into()],
        match_mode: Some(MatchMode::Precise),
        ..Cli::default()
    };
    assert_eq!(cli.effective_match_mode(), MatchMode::Precise);
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test args::tests::test_match_mode_ -- --include-ignored`
Or: `cargo test`
Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/args.rs
git commit -m "feat: add --match flag with auto-selection logic"
```

---

### Task 3: Update `dedup_stacks` to accept `MatchMode`

**Files:**
- Modify: `src/stack_data.rs`

- [ ] **Step 1: Add import and modify `dedup_stacks`**

Add `use crate::match_mode::{MatchMode, StackKey};` to the import section of `stack_data.rs`.

Replace the existing `dedup_stacks` function with:

```rust
use crate::match_mode::{MatchMode, StackKey};

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
```

- [ ] **Step 2: Update dedup tests to pass `MatchMode`**

Update `test_dedup_identical_stacks`:

```rust
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
```

Update `test_dedup_multi_pid`:

```rust
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
    assert!(groups[0].threads.iter().any(|t| t.pid == 100 && t.tid == 1000));
    assert!(groups[0].threads.iter().any(|t| t.pid == 200 && t.tid == 2000));
}
```

- [ ] **Step 3: Add fuzzy dedup tests**

```rust
#[test]
fn test_dedup_fuzzy_ignores_address() {
    // Same function, different addresses → Precise would NOT match, Fuzzy SHOULD
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
```

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/stack_data.rs
git commit -m "feat: dedup_stacks accepts MatchMode, fuzzy/precise key selection"
```

---

### Task 4: Update three backends to pass `MatchMode`

**Files:**
- Modify: `src/input_eustack.rs`
- Modify: `src/input_gdb.rs`
- Modify: `src/input_file.rs`

- [ ] **Step 1: Update `input_eustack.rs`**

Find the dedup call in `format_result`:

```rust
let groups = if cli.unique_mode {
    stack_data::dedup_stacks(all_stacks, cli.effective_match_mode())
} else {
    stack_data::to_groups(all_stacks)
};
```

- [ ] **Step 2: Update `input_gdb.rs`**

Same change:

```rust
let groups = if cli.unique_mode {
    stack_data::dedup_stacks(all_stacks, cli.effective_match_mode())
} else {
    stack_data::to_groups(all_stacks)
};
```

- [ ] **Step 3: Update `input_file.rs`**

Two places where dedup is called (unique mode and non-unique mode). Same change:

```rust
let groups = if cli.unique_mode {
    stack_data::dedup_stacks(stacks, cli.effective_match_mode())
} else {
    stack_data::to_groups(stacks)
};
```

- [ ] **Step 4: Build and test**

Run: `cargo build && cargo test`
Expected: Build succeeds, all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/input_eustack.rs src/input_gdb.rs src/input_file.rs
git commit -m "feat: pass effective_match_mode to dedup_stacks in all backends"
```

---

### Task 5: Wire conflict warning in execution path

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Add `warn_if_match_conflict()` call in execution paths**

Place the call after CLI is fully resolved (including interactive mode). Add to `main()` before the dispatch section, after the `choose_process` block:

```rust
// After interactive pid selection (around line 44-50)
cli.warn_if_match_conflict();
```

- [ ] **Step 2: Build and test**

Run: `cargo build && cargo test`
Expected: Build succeeds, all tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: add match mode conflict warning in execution path"
```
