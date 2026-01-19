use crate::{CompileError, Op, Span};

// Appends an op, and "compacts" it with previous ops if possible.
fn push_and_compact(ops: &mut Vec<Op>, spans: &mut Vec<Span>, op: Op, span: Span) {
    match (ops.last_mut(), spans.last_mut(), op) {
        // Add + Add
        (Some(Op::Add(a)), Some(s), Op::Add(b)) => {
            let sum = a.wrapping_add(b);
            if sum == 0 {
                ops.pop();
                spans.pop();
            } else {
                *a = sum;
                s.end = span.end;
            }
        }
        // Move + Move
        (Some(Op::Move(a)), Some(s), Op::Move(b)) => match a.checked_add(b) {
            Some(0) => {
                ops.pop();
                spans.pop();
            }
            Some(sum) => {
                *a = sum;
                s.end = span.end;
            }
            None => {
                ops.push(Op::Move(b));
                spans.push(span);
            }
        },
        // Set + Set
        (Some(Op::Set(_)), Some(s), Op::Set(b)) => {
            *ops.last_mut().unwrap() = Op::Set(b);
            s.end = span.end;
        }
        // Set + Add
        (Some(Op::Set(a)), Some(s), Op::Add(b)) => {
            *a = a.wrapping_add(b);
            s.end = span.end;
        }
        // Add + Set
        (Some(Op::Add(_)), Some(s), Op::Set(b)) => {
            *ops.last_mut().unwrap() = Op::Set(b);
            s.end = span.end;
        }
        (_, _, op) => {
            ops.push(op);
            spans.push(span);
        }
    }
}

// Checks for "multiplication loops".
//
// If a loop holds only Move and Add, and it returns pointer to the
// original cell, and it subtracts 1 from that cell on each iteration,
// then it can be replaced with one or several Mul instructions, and a
// Set(0).
fn try_mul_loop(ops: &[Op]) -> Option<Vec<(i32, u8)>> {
    let mut offset: i32 = 0;
    let mut muls = Vec::new();
    let mut origin_delta: u8 = 0;

    for op in ops {
        match op {
            Op::Add(n) => {
                if offset == 0 {
                    origin_delta = origin_delta.wrapping_add(*n);
                } else {
                    muls.push((offset, *n));
                }
            }
            Op::Move(n) => offset += n,
            _ => return None,
        }
    }

    if offset == 0 && origin_delta == 255 {
        Some(muls)
    } else {
        None
    }
}

// Skips past a loop. Useful when a dead loop has been found.
// Returns (new position, lines skipped, final column offset).
fn skip_loop(source: &[u8], start: usize) -> (usize, usize, usize) {
    let mut depth = 1;
    let mut i = start;
    let mut lines = 0;
    let mut col = 0;
    while i < source.len() && depth > 0 {
        match source[i] {
            b'[' => depth += 1,
            b']' => depth -= 1,
            b'\n' => {
                lines += 1;
                col = 0;
            }
            _ => {}
        }
        col += 1;
        i += 1;
    }
    (i, lines, col)
}

pub(crate) fn compile(source: &str) -> Result<(Vec<Op>, Vec<Span>), CompileError> {
    let mut ops = Vec::new();
    let mut spans = Vec::new();
    let mut loop_stack: Vec<(usize, Span)> = Vec::new(); // (ops index, loop start span)
    let source = source.as_bytes();
    let mut i = 0;
    let mut line = 1usize;
    let mut col = 1usize;

    while i < source.len() {
        let span = Span { start: i, end: i + 1, line, col };
        match source[i] {
            b'+' => push_and_compact(&mut ops, &mut spans, Op::Add(1), span),
            b'-' => push_and_compact(&mut ops, &mut spans, Op::Add(255), span),
            b'<' => push_and_compact(&mut ops, &mut spans, Op::Move(-1), span),
            b'>' => push_and_compact(&mut ops, &mut spans, Op::Move(1), span),
            b'.' => {
                ops.push(Op::Out);
                spans.push(span);
            }
            b',' => {
                ops.push(Op::In);
                spans.push(span);
            }
            b'[' => {
                // If previous op is Set(0), Close, or Scan, this loop will
                // never be entered (current cell is guaranteed to be 0).
                let is_dead = matches!(ops.last(), Some(Op::Set(0)) | Some(Op::Close(_)) | Some(Op::Scan(_)));
                if is_dead {
                    let (new_i, lines, new_col) = skip_loop(source, i + 1);
                    i = new_i;
                    line += lines;
                    col = if lines > 0 { new_col } else { col + new_col };
                    continue;
                } else {
                    loop_stack.push((ops.len(), span));
                    ops.push(Op::Open(0));
                    spans.push(span);
                }
            }
            b']' => {
                let Some((start, loop_start_span)) = loop_stack.pop() else {
                    return Err(CompileError::UnmatchedClose { span });
                };
                {
                    let loop_span = Span {
                        start: loop_start_span.start,
                        end: i + 1,
                        line: loop_start_span.line,
                        col: loop_start_span.col,
                    };
                    let loop_body = &ops[start + 1..];
                    if let Some(muls) = try_mul_loop(loop_body) {
                        ops.truncate(start);
                        spans.truncate(start);
                        for (offset, factor) in muls {
                            ops.push(Op::Mul(offset, factor));
                            spans.push(loop_span);
                        }
                        push_and_compact(&mut ops, &mut spans, Op::Set(0), loop_span);
                        i += 1;
                        col += 1;
                        continue;
                    }
                    if ops.len() == start + 2 {
                        if let Some(Op::Move(n)) = ops.last() {
                            let step = *n;
                            ops.pop();
                            ops.pop();
                            spans.pop();
                            spans.pop();
                            ops.push(Op::Scan(step));
                            spans.push(loop_span);
                            i += 1;
                            col += 1;
                            continue;
                        }
                        if let Some(Op::Add(n)) = ops.last() {
                            if n % 2 == 1 {
                                ops.pop();
                                ops.pop();
                                spans.pop();
                                spans.pop();
                                push_and_compact(&mut ops, &mut spans, Op::Set(0), loop_span);
                                i += 1;
                                col += 1;
                                continue;
                            }
                        }
                    }
                    let end = ops.len();
                    ops[start] = Op::Open(end as u32);
                    ops.push(Op::Close(start as u32));
                    spans.push(loop_span);
                }
            }
            b'\n' => {
                line += 1;
                col = 0; // will be incremented to 1 below
            }
            _ => {}
        }
        i += 1;
        col += 1;
    }

    if let Some((_, span)) = loop_stack.pop() {
        return Err(CompileError::UnmatchedOpen { span });
    }

    Ok((ops, spans))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Span;

    // The basic bf instructions
    #[test]
    fn test_basic() {
        let (ops, _) = compile(",[+>-.<]").unwrap();
        assert_eq!(ops, vec![
            Op::In,
            Op::Open(7),
            Op::Add(1),
            Op::Move(1),
            Op::Add(255),
            Op::Out,
            Op::Move(-1),
            Op::Close(1),
        ]);
    }

    // Arithmetic and pointer movement
    #[test]
    fn test_add_and_move() {
        let (ops, _) = compile("++-->><<").unwrap();
        assert_eq!(ops.len(), 0);

        let (ops, _) = compile("++++++++--++>>>>><<>>").unwrap();
        assert_eq!(ops, vec![
            Op::Add(8),
            Op::Move(5),
        ]);

        let (ops, _) = compile(">>>++--++------->><<<<").unwrap();
        assert_eq!(ops, vec![
            Op::Move(3),
            Op::Add(251),
            Op::Move(-2),
        ]);

        let (ops, _) = compile("++-->>>>>------<<+++++++<<<<<").unwrap();
        assert_eq!(ops, vec![
            Op::Move(5),
            Op::Add(250),
            Op::Move(-2),
            Op::Add(7),
            Op::Move(-5),
        ]);
    }

    // Arithmetic around u8 overflow
    #[test]
    fn test_add_u8_boundaries() {
        let src = "+".repeat(254) +
            &">" + &"+".repeat(255) +
            &">" + &"+".repeat(256) +
            &">" + &"+".repeat(257) +
            &">" + &"+".repeat(258);
        let (ops, _) = compile(&src).unwrap();
        assert_eq!(ops, vec![
            Op::Add(254),
            Op::Move(1), Op::Add(255),
            Op::Move(2), Op::Add(1),
            Op::Move(1), Op::Add(2),
        ]);

        let src = "-".repeat(1) +
            &">" + &"-".repeat(2) +
            &">" + &"-".repeat(3) +
            &">" + &"-".repeat(254) +
            &">" + &"-".repeat(255) +
            &">" + &"-".repeat(256) +
            &">" + &"-".repeat(257) +
            &">" + &"-".repeat(258);
        let (ops, _) = compile(&src).unwrap();
        assert_eq!(ops, vec![
            Op::Add(255),
            Op::Move(1), Op::Add(254),
            Op::Move(1), Op::Add(253),
            Op::Move(1), Op::Add(2),
            Op::Move(1), Op::Add(1),
            Op::Move(2),
            Op::Add(255),
            Op::Move(1), Op::Add(254),
        ]);
    }

    // Nested loops
    #[test]
    fn test_nested_loops() {
        let (ops, _) = compile("+[->++[->++++<]<]>.----[------>+<]>.").unwrap();
        assert_eq!(ops, vec![
            Op::Add(1),
            Op::Open(8),
            Op::Add(255),
            Op::Move(1),
            Op::Add(2),
            Op::Mul(1, 4),
            Op::Set(0),
            Op::Move(-1),
            Op::Close(1),
            Op::Move(1),
            Op::Out,
            Op::Add(252),
            Op::Open(17),
            Op::Add(250),
            Op::Move(1),
            Op::Add(1),
            Op::Move(-1),
            Op::Close(12),
            Op::Move(1),
            Op::Out,
        ]);
    }

    // Clear loops -> Set(0)
    #[test]
    fn test_clear_loop() {
        let (ops, _) = compile(",[-],[+],[---],[+++++]").unwrap();
        assert_eq!(ops, vec![
            Op::In, Op::Set(0),
            Op::In, Op::Set(0),
            Op::In, Op::Set(0),
            Op::In, Op::Set(0),
        ]);

        let (ops, _) = compile(",[++],[+++]").unwrap();
        assert_eq!(ops, vec![
            Op::In, Op::Open(3), Op::Add(2), Op::Close(1),
            Op::In, Op::Set(0),
        ]);
    }

    // Clear loops -> Set(0), and Set is merged with Add, and Set.
    #[test]
    fn test_clear_loop_with_add() {
        let (ops, _) = compile(",[-]><++++++++++").unwrap();
        assert_eq!(ops, vec![
            Op::In, Op::Set(10),
        ]);

        let (ops, _) = compile("++++[-]---+").unwrap();
        assert_eq!(ops, vec![
            Op::Set(254),
        ]);

        let (ops, _) = compile("++++[-]---+[+++]+").unwrap();
        assert_eq!(ops, vec![
            Op::Set(1),
        ]);
    }

    // Multiplication loops -> Mul op
    #[test]
    fn test_mul_loop() {
        let (ops, _) = compile(",[->>++>+++>+<<<<]").unwrap();
        assert_eq!(ops, vec![
            Op::In,
            Op::Mul(2, 2),
            Op::Mul(3, 3),
            Op::Mul(4, 1),
            Op::Set(0),
        ]);

        let (ops, _) = compile(",[->+<]").unwrap();
        assert_eq!(ops, vec![Op::In, Op::Mul(1, 1), Op::Set(0)]);

        let (ops, _) = compile(",[>+<-]").unwrap();
        assert_eq!(ops, vec![Op::In, Op::Mul(1, 1), Op::Set(0)]);
    }

    #[test]
    fn test_dead_code_elimination() {
        let (ops, _) = compile(",[-][>>>+>]").unwrap();
        assert_eq!(ops, vec![Op::In, Op::Set(0)]);

        let (ops, _) = compile(",[->>][>+<-]").unwrap();
        assert_eq!(ops, vec![
            Op::In,
            Op::Open(4),
            Op::Add(255),
            Op::Move(2),
            Op::Close(1),
        ]);

        let (ops, _) = compile(",[>][+++]").unwrap();
        assert_eq!(ops, vec![Op::In, Op::Scan(1)]);

        let (ops, _) = compile(",[<][>>>>>+<<<<<-]").unwrap();
        assert_eq!(ops, vec![Op::In, Op::Scan(-1)]);

        let (ops, _) = compile(",[>>][[nested]more]").unwrap();
        assert_eq!(ops, vec![Op::In, Op::Scan(2)]);
    }

    // Scan loop -> Scan
    #[test]
    fn test_scan() {
        let (ops, _) = compile(",[>],[<],[>>],[<<<]").unwrap();
        assert_eq!(ops, vec![
            Op::In, Op::Scan(1),
            Op::In, Op::Scan(-1),
            Op::In, Op::Scan(2),
            Op::In, Op::Scan(-3),
        ]);
    }

    // Compilation error on unmatched open
    #[test]
    fn test_unmatched_open() {
        let err = compile(",\n\n[+").unwrap_err();
        assert_eq!(err, CompileError::UnmatchedOpen {
            span: Span { start: 3, end: 4, line: 3, col: 1 }
        });

        let err = compile(",[[+").unwrap_err();
        assert_eq!(err, CompileError::UnmatchedOpen {
            span: Span { start: 2, end: 3, line: 1, col: 3 }
        });
    }

    // Compilation error on unmatched close
    #[test]
    fn test_unmatched_close() {
        let err = compile(",]").unwrap_err();
        assert_eq!(err, CompileError::UnmatchedClose {
            span: Span { start: 1, end: 2, line: 1, col: 2 }
        });

        let err = compile("+\n\n+]").unwrap_err();
        assert_eq!(err, CompileError::UnmatchedClose {
            span: Span { start: 4, end: 5, line: 3, col: 2 }
        });
    }

    // Few tests for line and column tracking
    #[test]
    fn test_error_line_column() {
        let err = compile("++\n>>\n[").unwrap_err();
        assert_eq!(err, CompileError::UnmatchedOpen {
            span: Span { start: 6, end: 7, line: 3, col: 1 }
        });

        let err = compile("++\n>>]").unwrap_err();
        assert_eq!(err, CompileError::UnmatchedClose {
            span: Span { start: 5, end: 6, line: 2, col: 3 }
        });

        let err = compile("+++\n[\n>+\n]>]").unwrap_err();
        assert_eq!(err, CompileError::UnmatchedClose {
            span: Span { start: 11, end: 12, line: 4, col: 3 }
        });
    }
}
