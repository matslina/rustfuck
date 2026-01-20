use crate::{Config, EofBehavior, ExecutionError, ExecutionResult, Op, Span};
use std::io::{Read, Write};

pub(crate) fn execute(
    ops: &[Op],
    spans: &[Span],
    mut tape: Vec<u8>,
    mut pointer: usize,
    config: &Config,
    mut input: Option<&mut dyn Read>,
    mut output: Option<&mut dyn Write>,
) -> Result<ExecutionResult, ExecutionError> {
    let mut ip = 0usize;
    let mut opcount = 0usize;
    let tape_len = tape.len();
    let op_limit = config.op_limit.unwrap_or(usize::MAX);

    while ip < ops.len() {
        let span = spans[ip];
        match &ops[ip] {
            Op::Add(n) => {
                tape[pointer] = tape[pointer].wrapping_add(*n);
            }
            Op::Move(n) => {
                let new_ptr = pointer as i64 + *n as i64;
                if new_ptr < 0 {
                    return Err(ExecutionError::PointerUnderflow { span });
                }
                if new_ptr as usize >= tape_len {
                    return Err(ExecutionError::PointerOverflow {
                        span,
                        pointer: new_ptr as usize,
                        tape_len,
                    });
                }
                pointer = new_ptr as usize;
            }
            Op::Out => {
                if let Some(ref mut out) = output {
                    out.write_all(&[tape[pointer]])
                        .map_err(|source| ExecutionError::IoError { span, source })?;
                    if config.flush_output {
                        out.flush()
                            .map_err(|source| ExecutionError::IoError { span, source })?;
                    }
                }
            }
            Op::In => {
                if let Some(ref mut inp) = input {
                    let mut buffer = [0u8; 1];
                    match inp.read(&mut buffer) {
                        Ok(0) => {
                            // EOF reached
                            match config.eof_behavior {
                                EofBehavior::Zero => tape[pointer] = 0,
                                EofBehavior::Unchanged => {}
                                EofBehavior::MaxValue => tape[pointer] = 255,
                            }
                        }
                        Ok(_) => tape[pointer] = buffer[0],
                        Err(source) => return Err(ExecutionError::IoError { span, source }),
                    }
                }
            }
            Op::Open(offset) => {
                if tape[pointer] == 0 {
                    ip = *offset as usize;
                }
            }
            Op::Close(offset) => {
                if tape[pointer] != 0 {
                    ip = *offset as usize;
                }
            }
            Op::Set(n) => {
                tape[pointer] = *n;
            }
            Op::Mul(offset, factor) => {
                let target = pointer as i64 + *offset as i64;
                if target < 0 {
                    return Err(ExecutionError::PointerUnderflow { span });
                }
                if target as usize >= tape_len {
                    return Err(ExecutionError::PointerOverflow {
                        span,
                        pointer: target as usize,
                        tape_len,
                    });
                }
                let target = target as usize;
                tape[target] = tape[target].wrapping_add(tape[pointer].wrapping_mul(*factor));
            }
            Op::Scan(step) => {
                let new_ptr = if *step == 1 {
                    match memchr::memchr(0, &tape[pointer..]) {
                        Some(i) => pointer + i,
                        None => {
                            return Err(ExecutionError::PointerOverflow {
                                span,
                                pointer: tape_len,
                                tape_len,
                            });
                        }
                    }
                } else if *step == -1 {
                    match memchr::memrchr(0, &tape[..=pointer]) {
                        Some(i) => i,
                        None => {
                            return Err(ExecutionError::PointerUnderflow { span });
                        }
                    }
                } else if *step > 0 {
                    let step = *step as usize;
                    let mut p = pointer;
                    while p < tape_len && tape[p] != 0 {
                        p += step;
                    }
                    if p >= tape_len {
                        return Err(ExecutionError::PointerOverflow {
                            span,
                            pointer: p,
                            tape_len,
                        });
                    }
                    p
                } else {
                    let step = (-*step) as usize;
                    let mut p = pointer;
                    while tape[p] != 0 {
                        if p < step {
                            return Err(ExecutionError::PointerUnderflow { span });
                        }
                        p -= step;
                    }
                    p
                };
                pointer = new_ptr;
            }
        }
        ip += 1;
        opcount += 1;
        if opcount > op_limit {
            return Err(ExecutionError::OperationLimit { span });
        }
    }

    Ok(ExecutionResult { tape, pointer })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;

    const S: Span = Span {
        start: 0,
        end: 0,
        line: 0,
        col: 0,
    };

    fn spans(n: usize) -> Vec<Span> {
        vec![S; n]
    }

    fn cfg() -> Config {
        Config::default()
    }

    // Scans with stride 1
    #[test]
    fn test_scan_stride_1() {
        let sp = spans(1);
        let ops = vec![Op::Scan(1)];

        // stride 1
        let result = execute(&ops, &sp, vec![1, 2, 3, 0, 5], 0, &cfg(), None, None).unwrap();
        assert_eq!(result.pointer, 3);
        let result = execute(&ops, &sp, vec![1, 2, 3, 4, 5], 1, &cfg(), None, None);
        assert!(matches!(
            result,
            Err(ExecutionError::PointerOverflow { .. })
        ));

        // stride -1
        let ops = vec![Op::Scan(-1)];
        let result = execute(&ops, &sp, vec![0, 0, 3, 255, 5], 4, &cfg(), None, None).unwrap();
        assert_eq!(result.pointer, 1);
        let result = execute(&ops, &sp, vec![1, 2, 3, 4, 5], 1, &cfg(), None, None);
        assert!(matches!(
            result,
            Err(ExecutionError::PointerUnderflow { .. })
        ));
    }

    // Scans with stride > 1
    #[test]
    fn test_scan_stride_n() {
        // stride 2
        let sp = spans(1);
        let ops = vec![Op::Scan(2)];
        let result = execute(&ops, &sp, vec![1, 2, 3, 4, 0, 6], 0, &cfg(), None, None).unwrap();
        assert_eq!(result.pointer, 4);
        let result = execute(&ops, &sp, vec![1, 0, 3, 4, 5, 6], 0, &cfg(), None, None);
        assert!(matches!(
            result,
            Err(ExecutionError::PointerOverflow { .. })
        ));

        // stride -2
        let ops = vec![Op::Scan(-2)];
        let result = execute(&ops, &sp, vec![0, 1, 2, 3, 4], 4, &cfg(), None, None).unwrap();
        assert_eq!(result.pointer, 0);
        let result = execute(&ops, &sp, vec![1, 1, 2, 3, 4], 3, &cfg(), None, None);
        assert!(matches!(
            result,
            Err(ExecutionError::PointerUnderflow { .. })
        ));

        // stride -3
        let ops = vec![Op::Scan(-3)];
        let result = execute(&ops, &sp, vec![1, 2, 0, 4, 5, 6], 5, &cfg(), None, None).unwrap();
        assert_eq!(result.pointer, 2);
        let result = execute(&ops, &sp, vec![1, 2, 3, 4, 5, 6], 5, &cfg(), None, None);
        assert!(matches!(
            result,
            Err(ExecutionError::PointerUnderflow { .. })
        ));
    }

    #[test]
    fn test_mul() {
        let ops = vec![Op::Mul(1, 3)];
        let sp = spans(1);
        let result = execute(&ops, &sp, vec![5, 0, 0], 0, &cfg(), None, None).unwrap();
        assert_eq!(result.tape, vec![5, 15, 0]);

        let result = execute(&ops, &sp, vec![5, 10, 0], 0, &cfg(), None, None).unwrap();
        assert_eq!(result.tape, vec![5, 25, 0]);

        let ops = vec![Op::Mul(-1, 2)];
        let result = execute(&ops, &sp, vec![7, 4, 0], 1, &cfg(), None, None).unwrap();
        assert_eq!(result.tape, vec![15, 4, 0]);

        // wrapping
        let ops = vec![Op::Mul(1, 2)];
        let result = execute(&ops, &sp, vec![200, 0, 0], 0, &cfg(), None, None).unwrap();
        assert_eq!(result.tape, vec![200, 144, 0]);

        // [->>++>+++<<<]
        let ops = vec![Op::Mul(2, 2), Op::Mul(3, 3), Op::Set(0)];
        let sp = spans(3);
        let result = execute(&ops, &sp, vec![10, 0, 0, 0], 0, &cfg(), None, None).unwrap();
        assert_eq!(result.tape[0], 0);
        assert_eq!(result.tape[2], 20);
        assert_eq!(result.tape[3], 30);
    }

    #[test]
    fn test_set() {
        let sp = spans(1);
        let result = execute(
            &vec![Op::Set(42)],
            &sp,
            vec![100, 0, 0],
            0,
            &cfg(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(result.tape, vec![42, 0, 0]);

        let result = execute(
            &vec![Op::Set(0)],
            &sp,
            vec![0, 255, 0],
            1,
            &cfg(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(result.tape, vec![0, 0, 0]);

        let sp = spans(3);
        let result = execute(
            &vec![Op::Set(10), Op::Set(20), Op::Set(30)],
            &sp,
            vec![0],
            0,
            &cfg(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(result.tape, vec![30]);
    }

    #[test]
    fn test_error_pointer_overflow() {
        let ops = vec![Op::Move(10)];
        let sp = vec![Span {
            start: 5,
            end: 15,
            line: 2,
            col: 3,
        }];
        let result = execute(&ops, &sp, vec![0; 5], 0, &cfg(), None, None);

        assert_eq!(
            result,
            Err(ExecutionError::PointerOverflow {
                span: Span {
                    start: 5,
                    end: 15,
                    line: 2,
                    col: 3
                },
                pointer: 10,
                tape_len: 5,
            })
        );
    }

    #[test]
    fn test_error_pointer_underflow() {
        let ops = vec![Op::Move(-5)];
        let sp = vec![Span {
            start: 0,
            end: 5,
            line: 1,
            col: 10,
        }];
        let result = execute(&ops, &sp, vec![0; 5], 2, &cfg(), None, None);

        assert_eq!(
            result,
            Err(ExecutionError::PointerUnderflow {
                span: Span {
                    start: 0,
                    end: 5,
                    line: 1,
                    col: 10
                },
            })
        );
    }

    #[test]
    fn test_error_mul_out_of_bounds() {
        // Mul target beyond tape
        let ops = vec![Op::Mul(10, 1)];
        let sp = vec![Span {
            start: 0,
            end: 5,
            line: 1,
            col: 1,
        }];
        let result = execute(&ops, &sp, vec![1, 0, 0], 0, &cfg(), None, None);
        assert!(matches!(
            result,
            Err(ExecutionError::PointerOverflow { .. })
        ));

        // Mul target before tape
        let ops = vec![Op::Mul(-5, 1)];
        let sp = vec![Span {
            start: 0,
            end: 5,
            line: 1,
            col: 1,
        }];
        let result = execute(&ops, &sp, vec![1, 0, 0], 1, &cfg(), None, None);
        assert!(matches!(
            result,
            Err(ExecutionError::PointerUnderflow { .. })
        ));
    }

    #[test]
    fn test_error_scan_overflow() {
        // Forward scan with no zero
        let ops = vec![Op::Scan(1)];
        let sp = vec![Span {
            start: 2,
            end: 5,
            line: 3,
            col: 7,
        }];
        let result = execute(&ops, &sp, vec![1, 2, 3], 0, &cfg(), None, None);
        match result {
            Err(ExecutionError::PointerOverflow {
                span,
                pointer,
                tape_len,
            }) => {
                assert_eq!(span.line, 3);
                assert_eq!(span.col, 7);
                assert_eq!(pointer, 3);
                assert_eq!(tape_len, 3);
            }
            _ => panic!("expected PointerOverflow"),
        }
    }

    #[test]
    fn test_custom_tape_size_overflow() {
        // Move beyond a small tape
        let ops = vec![Op::Move(5)];
        let sp = spans(1);
        let result = execute(&ops, &sp, vec![0; 3], 0, &cfg(), None, None);
        assert!(matches!(
            result,
            Err(ExecutionError::PointerOverflow {
                tape_len: 3,
                pointer: 5,
                ..
            })
        ));

        // Same move succeeds with larger tape
        let result = execute(&ops, &sp, vec![0; 10], 0, &cfg(), None, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().pointer, 5);
    }

    #[test]
    fn test_custom_tape_size_underflow() {
        // Move before tape start
        let ops = vec![Op::Move(-3)];
        let sp = spans(1);
        let result = execute(&ops, &sp, vec![0; 5], 2, &cfg(), None, None);
        assert!(matches!(
            result,
            Err(ExecutionError::PointerUnderflow { .. })
        ));

        // Same move succeeds with different starting pointer
        let result = execute(&ops, &sp, vec![0; 5], 4, &cfg(), None, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().pointer, 1);
    }

    #[test]
    fn test_op_limit_respected() {
        // A simple loop that runs many iterations
        // [->+<] copies cell 0 to cell 1, running 3 ops per iteration
        let ops = vec![
            Op::Open(5),
            Op::Add(255), // decrement
            Op::Move(1),
            Op::Add(1),
            Op::Move(-1),
            Op::Close(0),
        ];
        let sp = spans(6);

        // With cell 0 = 10, loop runs 10 times = 50 ops (5 ops * 10 iterations)
        // Plus final Open check = 51 ops total
        let config_limited = Config {
            op_limit: Some(30),
            ..Default::default()
        };
        let result = execute(&ops, &sp, vec![10, 0], 0, &config_limited, None, None);
        assert!(matches!(result, Err(ExecutionError::OperationLimit { .. })));

        // With higher limit, it succeeds
        let config_ok = Config {
            op_limit: Some(100),
            ..Default::default()
        };
        let result = execute(&ops, &sp, vec![10, 0], 0, &config_ok, None, None);
        assert!(result.is_ok());

        // With no limit (None), it succeeds
        let config_unlimited = Config {
            op_limit: None,
            ..Default::default()
        };
        let result = execute(&ops, &sp, vec![10, 0], 0, &config_unlimited, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_eof_behavior_zero() {
        use crate::EofBehavior;
        let ops = vec![Op::In];
        let sp = spans(1);
        let config = Config {
            eof_behavior: EofBehavior::Zero,
            ..Default::default()
        };

        let mut input: &[u8] = &[];
        let result = execute(&ops, &sp, vec![42], 0, &config, Some(&mut input), None).unwrap();
        assert_eq!(result.tape[0], 0);
    }

    #[test]
    fn test_eof_behavior_unchanged() {
        use crate::EofBehavior;
        let ops = vec![Op::In];
        let sp = spans(1);
        let config = Config {
            eof_behavior: EofBehavior::Unchanged,
            ..Default::default()
        };

        let mut input: &[u8] = &[];
        let result = execute(&ops, &sp, vec![42], 0, &config, Some(&mut input), None).unwrap();
        assert_eq!(result.tape[0], 42);
    }

    #[test]
    fn test_eof_behavior_max_value() {
        use crate::EofBehavior;
        let ops = vec![Op::In];
        let sp = spans(1);
        let config = Config {
            eof_behavior: EofBehavior::MaxValue,
            ..Default::default()
        };

        let mut input: &[u8] = &[];
        let result = execute(&ops, &sp, vec![42], 0, &config, Some(&mut input), None).unwrap();
        assert_eq!(result.tape[0], 255);
    }

    #[test]
    fn test_eof_after_valid_input() {
        use crate::EofBehavior;
        let ops = vec![Op::In, Op::Move(1), Op::In];
        let sp = spans(3);

        let config = Config {
            eof_behavior: EofBehavior::Zero,
            ..Default::default()
        };
        let mut input: &[u8] = &[65]; // only one byte available
        let result = execute(&ops, &sp, vec![0, 99], 0, &config, Some(&mut input), None).unwrap();
        assert_eq!(result.tape[0], 65); // 'A'
        assert_eq!(result.tape[1], 0); // EOF -> Zero
    }

    struct FailingWriter;
    impl std::io::Write for FailingWriter {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(std::io::ErrorKind::Other, "write failed"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::new(std::io::ErrorKind::Other, "flush failed"))
        }
    }

    struct FailingReader;
    impl std::io::Read for FailingReader {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(std::io::ErrorKind::Other, "read failed"))
        }
    }

    struct WriteOkFlushFails {
        written: bool,
    }
    impl std::io::Write for WriteOkFlushFails {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.written = true;
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            if self.written {
                Err(std::io::Error::new(std::io::ErrorKind::Other, "flush failed"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn test_io_error_write_fails() {
        let ops = vec![Op::Out];
        let sp = spans(1);
        let mut writer = FailingWriter;
        let result = execute(&ops, &sp, vec![65], 0, &cfg(), None, Some(&mut writer));
        assert!(matches!(result, Err(ExecutionError::IoError { .. })));
    }

    #[test]
    fn test_io_error_flush_fails() {
        let ops = vec![Op::Out];
        let sp = spans(1);
        let config = Config {
            flush_output: true,
            ..Default::default()
        };
        let mut writer = WriteOkFlushFails { written: false };
        let result = execute(&ops, &sp, vec![65], 0, &config, None, Some(&mut writer));
        assert!(matches!(result, Err(ExecutionError::IoError { .. })));
    }

    #[test]
    fn test_io_error_read_fails() {
        let ops = vec![Op::In];
        let sp = spans(1);
        let mut reader = FailingReader;
        let result = execute(&ops, &sp, vec![0], 0, &cfg(), Some(&mut reader), None);

        // Use == to exercise PartialEq (compares span and error kind)
        let expected = ExecutionError::IoError {
            span: S,
            source: std::io::Error::new(std::io::ErrorKind::Other, "different msg ok"),
        };
        assert_eq!(result.unwrap_err(), expected);
    }
}
