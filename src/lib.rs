use std::io::{Read, Write};

mod compile;
mod execute;

pub const DEFAULT_TAPE_SIZE: usize = 30000;

/// Behavior when input reaches EOF.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EofBehavior {
    /// Set the cell to 0.
    #[default]
    Zero,
    /// Leave the cell unchanged.
    Unchanged,
    /// Set the cell to 255.
    MaxValue,
}

/// Configuration for program execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Size of the memory tape. Default: 30000.
    pub tape_size: usize,
    /// Maximum number of operations before aborting. None = unlimited.
    pub op_limit: Option<usize>,
    /// Behavior when input reaches EOF. Default: Zero.
    pub eof_behavior: EofBehavior,
    /// Whether to flush output after each write. Default: true.
    pub flush_output: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tape_size: DEFAULT_TAPE_SIZE,
            op_limit: None,
            eof_behavior: EofBehavior::Zero,
            flush_output: true,
        }
    }
}

/// References a location in source code.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
}

/// Runtime error
#[derive(Debug)]
pub enum ExecutionError {
    PointerUnderflow { span: Span },
    PointerOverflow { span: Span, pointer: usize, tape_len: usize },
    OperationLimit { span: Span },
    IoError { span: Span, source: std::io::Error },
}

impl PartialEq for ExecutionError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                ExecutionError::PointerUnderflow { span: a },
                ExecutionError::PointerUnderflow { span: b },
            ) => a == b,
            (
                ExecutionError::PointerOverflow { span: a, pointer: pa, tape_len: ta },
                ExecutionError::PointerOverflow { span: b, pointer: pb, tape_len: tb },
            ) => a == b && pa == pb && ta == tb,
            (
                ExecutionError::OperationLimit { span: a },
                ExecutionError::OperationLimit { span: b },
            ) => a == b,
            (
                ExecutionError::IoError { span: a, source: sa },
                ExecutionError::IoError { span: b, source: sb },
            ) => a == b && sa.kind() == sb.kind(),
            _ => false,
        }
    }
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionError::PointerUnderflow { span } => {
                write!(f, "pointer underflow at line {}, column {}", span.line, span.col)
            }
            ExecutionError::PointerOverflow { span, pointer, tape_len } => {
                write!(
                    f,
                    "pointer overflow: position {} exceeds tape length {} (at line {}, column {})",
                    pointer, tape_len, span.line, span.col
                )
            }
            ExecutionError::OperationLimit { span } => {
                write!(f, "operation limit exceeded at line {}, column {}", span.line, span.col)
            }
            ExecutionError::IoError { span, source } => {
                write!(f, "I/O error at line {}, column {}: {}", span.line, span.col, source)
            }
        }
    }
}

impl std::error::Error for ExecutionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ExecutionError::IoError { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Compilation error
#[derive(Debug, PartialEq)]
pub enum CompileError {
    UnmatchedOpen { span: Span },
    UnmatchedClose { span: Span },
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::UnmatchedOpen { span } => {
                write!(f, "unmatched '[' at line {}, column {}", span.line, span.col)
            }
            CompileError::UnmatchedClose { span } => {
                write!(f, "unmatched ']' at line {}, column {}", span.line, span.col)
            }
        }
    }
}

impl std::error::Error for CompileError {}

/// Bytecode instruction.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Add(u8),
    Move(i32),
    Out,
    In,
    Open(u32),
    Close(u32),
    Set(u8),
    Mul(i32, u8),
    Scan(i32),
}

/// A compiled brainfuck program ready for execution.
pub struct Program {
    pub ops: Vec<Op>,
    pub spans: Vec<Span>,
}

/// State of the machine after execution.
#[derive(Debug, PartialEq)]
pub struct ExecutionResult {
    pub tape: Vec<u8>,
    pub pointer: usize,
}

impl Program {
    /// Compiles source code into a program.
    pub fn from_source(source: &str) -> Result<Self, CompileError> {
        let (ops, spans) = compile::compile(source)?;
        Ok(Self { ops, spans })
    }

    /// Runs the program with the given configuration.
    pub fn run(
        &self,
        config: &Config,
        tape: Option<Vec<u8>>,
        pointer: Option<usize>,
        input: Option<&mut dyn Read>,
        output: Option<&mut dyn Write>,
    ) -> Result<ExecutionResult, ExecutionError> {
        let tape = tape.unwrap_or_else(|| vec![0u8; config.tape_size]);
        let pointer = pointer.unwrap_or(0);
        execute::execute(&self.ops, &self.spans, tape, pointer, config, input, output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hello_world() {
        let program = Program::from_source("++++++++[->++[->++++<]<]>>.----[------>+<]>.").unwrap();
        let mut output = Vec::new();
        program.run(&Config::default(), None, None, None, Some(&mut output)).unwrap();
        assert_eq!(String::from_utf8(output).unwrap(), "@\n");
    }
}
