use std::process::Command;

fn cs_bin() -> String {
    env!("CARGO_BIN_EXE_cs").to_string()
}

fn run_cs(args: &[&str]) -> std::process::Output {
    Command::new(cs_bin())
        .args(args)
        .output()
        .expect("failed to run cs")
}

#[test]
fn test_diff_text_output() {
    let output = run_cs(&[
        "--diff",
        "tests/fixtures/before.json",
        "tests/fixtures/after.json",
    ]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected = include_str!("fixtures/expected_diff.txt");
    assert_eq!(stdout.trim(), expected.trim());
}

#[test]
fn test_diff_json_output() {
    let output = run_cs(&[
        "--diff",
        "tests/fixtures/before.json",
        "tests/fixtures/after.json",
        "--json",
    ]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed["tool"], "cs diff");
    assert!(parsed["timestamp"].is_string());
    assert_eq!(parsed["added"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["removed"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["changed"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["added"][0]["count"], 1);
    assert_eq!(parsed["added"][0]["signature"], "func_f");
    assert_eq!(parsed["removed"][0]["signature"], "func_e");
    assert_eq!(parsed["changed"][0]["signature"], "func_c;func_d");
    assert_eq!(parsed["changed"][0]["before_count"], 3);
    assert_eq!(parsed["changed"][0]["after_count"], 1);
}

#[test]
fn test_json_file_input() {
    let output = run_cs(&["-U", "tests/fixtures/before.json"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("func_a"));
    assert!(stdout.contains("func_c"));
    assert!(stdout.contains("func_e"));
    assert!(stdout.contains("Number of thread: 2"));
    assert!(stdout.contains("Number of thread: 3"));
    assert!(stdout.contains("Number of thread: 1"));
}

#[test]
fn test_json_still_works_without_u() {
    let output = run_cs(&["tests/fixtures/before.json"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Without -U, JSON input still uses the pre-grouped data
    assert!(stdout.contains("func_a"));
    assert!(stdout.contains("Number of thread: 2"));
}
