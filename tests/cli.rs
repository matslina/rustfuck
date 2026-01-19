use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use tempfile::NamedTempFile;

fn cmd() -> Command {
    assert_cmd::cargo::cargo_bin_cmd!("rustfuck").into()
}

fn batch_results(output: &[u8]) -> Vec<Value> {
    String::from_utf8_lossy(output)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn batch_input(items: &[Value]) -> String {
    items.iter()
        .map(|v| serde_json::to_string(v).unwrap() + "\n")
        .collect()
}

// =============================================================================
// Help!
// =============================================================================

#[test]
fn test_help() {
    cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("brainfuck interpreter"))
        .stdout(predicate::str::contains("--batch"));
}

// =============================================================================
// Input and output
// =============================================================================

#[test]
fn test_io_stdin_stdout() {
    cmd()
        .arg("tests/programs/echo.b")
        .write_stdin("X")
        .assert()
        .success()
        .stdout("X");
}

#[test]
fn test_io_file_in() {
    let mut input_file = NamedTempFile::new().unwrap();
    write!(input_file, "Y").unwrap();

    cmd()
        .arg("tests/programs/echo.b")
        .arg("-i")
        .arg(input_file.path())
        .assert()
        .success()
        .stdout("Y");
}

#[test]
fn test_io_file_out() {
    let output_file = NamedTempFile::new().unwrap();

    cmd()
        .arg("tests/programs/echo.b")
        .write_stdin("Z")
        .arg("-o")
        .arg(output_file.path())
        .assert()
        .success();

    let output = fs::read(output_file.path()).unwrap();
    assert_eq!(output, b"Z");
}

#[test]
fn test_io_file_in_and_out() {
    let mut input_file = NamedTempFile::new().unwrap();
    write!(input_file, "W").unwrap();
    let output_file = NamedTempFile::new().unwrap();

    cmd()
        .arg("tests/programs/echo.b")
        .arg("-i")
        .arg(input_file.path())
        .arg("-o")
        .arg(output_file.path())
        .assert()
        .success();

    let output = fs::read(output_file.path()).unwrap();
    assert_eq!(output, b"W");
}

// =============================================================================
// Runtime configuration flags
// =============================================================================

#[test]
fn test_cfg_memory_success() {
    cmd()
        .arg("tests/programs/memoryhog.b")
        .arg("-m")
        .arg("65536")
        .assert()
        .success();
}

#[test]
fn test_cfg_memory_exceeded() {
    cmd()
        .arg("tests/programs/memoryhog.b")
        .arg("-m")
        .arg("65534")
        .assert()
        .failure()
        .stderr(predicate::str::contains("pointer overflow"));
}

#[test]
fn test_cfg_op_limit_success() {
    cmd()
        .arg("tests/programs/basicops.b")
        .arg("-l")
        .arg("100")
        .assert()
        .success();
}

#[test]
fn test_cfg_op_limit_exceeded() {
    cmd()
        .arg("tests/programs/basicops.b")
        .arg("-l")
        .arg("30")
        .assert()
        .failure()
        .stderr(predicate::str::contains("operation limit exceeded"));
}

#[test]
fn test_cfg_eof_default() {
    let mut program = NamedTempFile::new().unwrap();
    write!(program, "++++,.").unwrap();

    cmd()
        .arg(program.path())
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::eq(vec![4u8]));
}

#[test]
fn test_cfg_eof_zero() {
    cmd()
        .arg("tests/programs/echo.b")
        .arg("-e")
        .arg("zero")
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::eq(vec![0u8]));
}

#[test]
fn test_cfg_eof_unchanged() {
    cmd()
        .arg("tests/programs/endoffile.b")
        .arg("-e")
        .arg("unchanged")
        .assert()
        .success();
}

#[test]
fn test_cfg_eof_max() {
    cmd()
        .arg("tests/programs/echo.b")
        .arg("-e")
        .arg("max")
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::eq(vec![255u8]));
}

// =============================================================================
// Batch mode
// =============================================================================

#[test]
fn test_batch_empty() {
    let out = cmd()
        .arg("tests/programs/empty.b")
        .arg("--batch")
        .write_stdin("{}\n")
        .output()
        .unwrap();

    assert_eq!(
        batch_results(&out.stdout),
        vec![json!({"ok": true, "tape": [], "pointer": 0, "output": []})]
    );
}

#[test]
fn test_batch_id_preserved() {
    let out = cmd()
        .arg("tests/programs/empty.b")
        .arg("--batch")
        .write_stdin(batch_input(&[json!({"id": "foo"})]))
        .output()
        .unwrap();

    assert_eq!(
        batch_results(&out.stdout),
        vec![json!({"id": "foo", "ok": true, "tape": [], "pointer": 0, "output": []})]
    );
}

#[test]
fn test_batch_multiple_lines() {
    let out = cmd()
        .arg("tests/programs/echo.b")
        .arg("--batch")
        .write_stdin(batch_input(&[
            json!({"id": "a", "input": [65]}),
            json!({"id": "b", "input": [66]}),
            json!({"id": "c", "input": [67]}),
        ]))
        .output()
        .unwrap();

    assert_eq!(
        batch_results(&out.stdout),
        vec![
            json!({"id": "a", "ok": true, "tape": [65], "pointer": 0, "output": [65]}),
            json!({"id": "b", "ok": true, "tape": [66], "pointer": 0, "output": [66]}),
            json!({"id": "c", "ok": true, "tape": [67], "pointer": 0, "output": [67]}),
        ]
    );
}

#[test]
fn test_batch_empty_lines_skipped() {
    let out = cmd()
        .arg("tests/programs/empty.b")
        .arg("--batch")
        .write_stdin("{\"id\":\"a\"}\n\n\n{\"id\":\"b\"}\n")
        .output()
        .unwrap();

    assert_eq!(
        batch_results(&out.stdout),
        vec![
            json!({"id": "a", "ok": true, "tape": [], "pointer": 0, "output": []}),
            json!({"id": "b", "ok": true, "tape": [], "pointer": 0, "output": []}),
        ]
    );
}

#[test]
fn test_batch_initial_state() {
    let mut program = NamedTempFile::new().unwrap();
    write!(program, "++++>,.").unwrap();

    let out = cmd()
        .arg(program.path())
        .arg("--batch")
        .write_stdin(batch_input(&[
            json!({"id": "a", "tape": [1,2,3,4], "pointer":2, "input":[42]}),
            json!({"id": "b", "tape": [1,2,3,4], "pointer":0}),
            json!({"id": "c"}),
        ]))
        .output()
        .unwrap();

    assert_eq!(
        batch_results(&out.stdout),
        vec![
            json!({"id": "a", "tape": [1, 2, 7, 42], "pointer": 3, "ok": true, "output": [42]}),
            json!({"id": "b", "tape": [5, 2, 3, 4], "pointer": 1, "ok": true, "output": [2]}),
            json!({"id": "c", "tape": [4], "pointer": 1, "ok": true, "output": [0]}),
        ]
    );
}

// The tape field in batch output has trailing zeros trimmed.
#[test]
fn test_batch_trailing_zeros_trimmed() {
    let mut program = NamedTempFile::new().unwrap();
    write!(program, "++>>+>+>+>+>+>+>+[[-]<]").unwrap();

    let out = cmd()
        .arg(program.path())
        .arg("--batch")
        .write_stdin(batch_input(&[json!({})]))
        .output()
        .unwrap();

    assert_eq!(
        batch_results(&out.stdout),
        vec![json!({"ok": true, "tape": [2], "pointer": 1, "output": []})]
    );
}

// =============================================================================
// Batch mode with runtime config
// =============================================================================

#[test]
fn test_batch_cfg_tape_size() {
    let out = cmd()
        .arg("tests/programs/memoryhog.b")
        .arg("--batch")
        .write_stdin(batch_input(&[json!({"config": {"tape_size": 65536}})]))
        .output()
        .unwrap();

    let results = batch_results(&out.stdout);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["ok"], true);
}

#[test]
fn test_batch_cfg_op_limit() {
    let out = cmd()
        .arg("tests/programs/factor.b")
        .arg("--batch")
        .write_stdin(batch_input(&[json!({"input": [50, 10], "config": {"op_limit": 10}})]))
        .output()
        .unwrap();

    let results = batch_results(&out.stdout);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["ok"], false);
    assert!(results[0]["error"].as_str().unwrap().contains("operation limit exceeded"));
}

#[test]
fn test_batch_cfg_eof() {
    let out = cmd()
        .arg("tests/programs/echo.b")
        .arg("--batch")
        .write_stdin(batch_input(&[json!({"input": [], "config": {"eof_behavior": "max"}})]))
        .output()
        .unwrap();

    assert_eq!(
        batch_results(&out.stdout),
        vec![json!({"ok": true, "tape": [255], "pointer": 0, "output": [255]})]
    );
}

// =============================================================================
// Errors
// =============================================================================

#[test]
fn test_missing_program_arg() {
    cmd()
        .assert()
        .failure()
        .stderr(predicate::str::contains("required arguments"));
}

#[test]
fn test_missing_file() {
    cmd()
        .arg("nonexistent.b")
        .assert()
        .failure()
        .stderr(predicate::str::contains("file not found"));
}

#[test]
fn test_compile_error() {
    cmd()
        .arg("tests/programs/unmatched.b")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Compile error"))
        .stderr(predicate::str::contains("unmatched '['"));
}

#[test]
fn test_runtime_error() {
    cmd()
        .arg("tests/programs/underflow.b")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Runtime error"))
        .stderr(predicate::str::contains("pointer underflow"));
}

#[test]
fn test_batch_invalid_json_with_recovery() {
    let out = cmd()
        .arg("tests/programs/empty.b")
        .arg("--batch")
        .write_stdin("not valid json\n{\"id\":\"after\"}\n")
        .output()
        .unwrap();

    let results = batch_results(&out.stdout);
    assert_eq!(results.len(), 2);

    // Error for the invalid JSON
    assert_eq!(results[0]["ok"], false);
    assert!(results[0]["error"]
        .as_str()
        .unwrap()
        .contains("invalid JSON"));

    // Second is still processed
    assert_eq!(results[1]["id"], "after");
    assert_eq!(results[1]["ok"], true);
}

#[test]
fn test_batch_runtime_error() {
    let out = cmd()
        .arg("tests/programs/underflow.b")
        .arg("--batch")
        .write_stdin(batch_input(&[json!({"id": "err"})]))
        .output()
        .unwrap();

    let results = batch_results(&out.stdout);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["id"], "err");
    assert_eq!(results[0]["ok"], false);
    assert!(results[0]["error"].as_str().unwrap().contains("pointer underflow"));
}
