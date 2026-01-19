use rustfuck::{Config, EofBehavior, Program};
use std::fs;
use std::path::Path;

fn run_program(source: &str, input: &[u8], config: &Config) -> Vec<u8> {
    let program = Program::from_source(source).expect("failed to compile");
    let mut output = Vec::new();
    let mut input_slice = input;
    program
        .run(
            config,
            None,
            None,
            Some(&mut input_slice),
            Some(&mut output),
        )
        .expect("failed to run");
    output
}

fn load_file(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_default()
}

fn load_str(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

#[test]
fn test_basicops() {
    let source = load_str(Path::new("tests/programs/basicops.b"));
    let input = load_file(Path::new("tests/programs/basicops.in"));
    let expected = load_file(Path::new("tests/programs/basicops.out"));

    let output = run_program(&source, &input, &Config::default());
    assert_eq!(output, expected);
}

#[test]
fn test_comments() {
    let source = load_str(Path::new("tests/programs/comments.b"));
    let expected = load_file(Path::new("tests/programs/comments.out"));

    let output = run_program(&source, &[], &Config::default());
    assert_eq!(output, expected);
}

#[test]
fn test_empty() {
    let source = load_str(Path::new("tests/programs/empty.b"));
    let expected = load_file(Path::new("tests/programs/empty.out"));

    let output = run_program(&source, &[], &Config::default());
    assert_eq!(output, expected);
}

#[test]
fn test_endoffile() {
    let source = load_str(Path::new("tests/programs/endoffile.b"));
    let expected = load_file(Path::new("tests/programs/endoffile.out"));

    let config = Config {
        eof_behavior: EofBehavior::Unchanged,
        ..Default::default()
    };
    let output = run_program(&source, &[], &config);
    assert_eq!(output, expected);
}

#[test]
fn test_factor() {
    let source = load_str(Path::new("tests/programs/factor.b"));
    let input = load_file(Path::new("tests/programs/factor.in"));
    let expected = load_file(Path::new("tests/programs/factor.out"));

    let output = run_program(&source, &input, &Config::default());
    assert_eq!(output, expected);
}

#[test]
fn test_memoryhog() {
    let source = load_str(Path::new("tests/programs/memoryhog.b"));
    let expected = load_file(Path::new("tests/programs/memoryhog.out"));

    let config = Config {
        tape_size: 65536,
        ..Default::default()
    };
    let output = run_program(&source, &[], &config);
    assert_eq!(output, expected);
}
