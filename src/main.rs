use clap::{Parser, Subcommand, ValueEnum};
use rustfuck::{Config, EofBehavior, Program};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum EofArg {
    Zero,
    Unchanged,
    Max,
}

impl From<EofArg> for EofBehavior {
    fn from(arg: EofArg) -> Self {
        match arg {
            EofArg::Zero => EofBehavior::Zero,
            EofArg::Unchanged => EofBehavior::Unchanged,
            EofArg::Max => EofBehavior::MaxValue,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "rustfuck")]
#[command(about = "A brainfuck interpreter")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run a brainfuck program
    Run(RunArgs),
}

#[derive(Parser, Debug)]
struct RunArgs {
    /// Path to brainfuck source file
    program: PathBuf,

    /// Read input from file instead of stdin
    #[arg(short, long)]
    input: Option<PathBuf>,

    /// Write output to file instead of stdout
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Tape size
    #[arg(short = 'm', long = "memory", default_value_t = rustfuck::DEFAULT_TAPE_SIZE)]
    tape_size: usize,

    /// Max operations (default: unlimited)
    #[arg(short = 'l', long = "limit")]
    op_limit: Option<usize>,

    /// EOF behavior
    #[arg(short, long, value_enum, default_value_t = EofArg::Unchanged)]
    eof: EofArg,

    /// Enable batch/ndjson mode
    #[arg(long)]
    batch: bool,
}

#[derive(Debug, Deserialize)]
struct BatchConfig {
    tape_size: Option<usize>,
    op_limit: Option<usize>,
    eof_behavior: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BatchInput {
    id: Option<String>,
    tape: Option<Vec<u8>>,
    pointer: Option<usize>,
    input: Option<Vec<u8>>,
    config: Option<BatchConfig>,
}

#[derive(Debug, Serialize)]
struct BatchOutputOk {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    ok: bool,
    tape: Vec<u8>,
    pointer: usize,
    output: Vec<u8>,
}

#[derive(Debug, Serialize)]
struct BatchOutputErr {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    ok: bool,
    error: String,
}

fn trim_tape(mut tape: Vec<u8>) -> Vec<u8> {
    while tape.last() == Some(&0) {
        tape.pop();
    }
    tape
}

fn parse_eof_string(s: &str) -> EofBehavior {
    match s.to_lowercase().as_str() {
        "unchanged" => EofBehavior::Unchanged,
        "max" => EofBehavior::MaxValue,
        _ => EofBehavior::Zero,
    }
}

// Processes batches of input/output for the program, read/written
// from/to stdin/stdout. These are expected to be newline separated
// json objects.
fn run_batch(program: &Program, base_config: &Config) {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                let err = BatchOutputErr {
                    id: None,
                    ok: false,
                    error: format!("failed to read input line: {}", e),
                };
                let _ = serde_json::to_writer(&mut stdout, &err);
                let _ = writeln!(stdout);
                continue;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        let batch_input: BatchInput = match serde_json::from_str(&line) {
            Ok(bi) => bi,
            Err(e) => {
                let err = BatchOutputErr {
                    id: None,
                    ok: false,
                    error: format!("invalid JSON: {}", e),
                };
                let _ = serde_json::to_writer(&mut stdout, &err);
                let _ = writeln!(stdout);
                continue;
            }
        };

        let config = if let Some(bc) = &batch_input.config {
            Config {
                tape_size: bc.tape_size.unwrap_or(base_config.tape_size),
                op_limit: bc.op_limit.or(base_config.op_limit),
                eof_behavior: bc
                    .eof_behavior
                    .as_ref()
                    .map(|s| parse_eof_string(s))
                    .unwrap_or(base_config.eof_behavior),
                flush_output: false,
            }
        } else {
            Config {
                flush_output: false,
                ..base_config.clone()
            }
        };

        let input_bytes = batch_input.input.unwrap_or_default();
        let mut output_buf = Vec::new();
        let mut input_slice = input_bytes.as_slice();

        let result = program.run(
            &config,
            batch_input.tape,
            batch_input.pointer,
            Some(&mut input_slice),
            Some(&mut output_buf),
        );

        match result {
            Ok(exec_result) => {
                let out = BatchOutputOk {
                    id: batch_input.id,
                    ok: true,
                    tape: trim_tape(exec_result.tape),
                    pointer: exec_result.pointer,
                    output: output_buf,
                };
                let _ = serde_json::to_writer(&mut stdout, &out);
                let _ = writeln!(stdout);
            }
            Err(e) => {
                let err = BatchOutputErr {
                    id: batch_input.id,
                    ok: false,
                    error: e.to_string(),
                };
                let _ = serde_json::to_writer(&mut stdout, &err);
                let _ = writeln!(stdout);
            }
        }
    }
}

fn run_normal(program: &Program, config: &Config, args: &RunArgs) -> Result<(), String> {
    let input: Box<dyn io::Read> = if let Some(path) = &args.input {
        Box::new(fs::File::open(path).map_err(|e| format!("failed to open input file: {}", e))?)
    } else {
        Box::new(io::stdin())
    };

    let output: Box<dyn io::Write> = if let Some(path) = &args.output {
        Box::new(
            fs::File::create(path).map_err(|e| format!("failed to create output file: {}", e))?,
        )
    } else {
        Box::new(io::stdout())
    };

    let mut input = input;
    let mut output = output;

    program
        .run(config, None, None, Some(&mut input), Some(&mut output))
        .map_err(|e| e.to_string())?;

    Ok(())
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run(args) => {
            let source = match fs::read_to_string(&args.program) {
                Ok(s) => s,
                Err(e) => {
                    if e.kind() == io::ErrorKind::NotFound {
                        eprintln!("Error: file not found: {}", args.program.display());
                    } else {
                        eprintln!("Error reading {}: {}", args.program.display(), e);
                    }
                    std::process::exit(1);
                }
            };

            let program = match Program::from_source(&source) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Compile error: {}", e);
                    std::process::exit(1);
                }
            };

            let config = Config {
                tape_size: args.tape_size,
                op_limit: args.op_limit,
                eof_behavior: args.eof.into(),
                flush_output: !args.batch,
            };

            if args.batch {
                run_batch(&program, &config);
            } else if let Err(e) = run_normal(&program, &config, &args) {
                eprintln!("Runtime error: {}", e);
                std::process::exit(1);
            }
        }
    }
}
