# rustfuck

[![CI](https://github.com/matslina/rustfuck/actions/workflows/ci.yml/badge.svg)](https://github.com/matslina/rustfuck/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/matslina/rustfuck/graph/badge.svg)](https://codecov.io/gh/matslina/rustfuck)

A brainfuck interpreter in Rust.

## Build

```
cargo build --release
```

## Usage

```
rustfuck run program.b
```

Input/output can be redirected to files:

```
rustfuck run program.b -i input.txt -o output.txt
```

### Options

- `-m, --memory <SIZE>` - Tape size (default: 30000)
- `-l, --limit <OPS>` - Max operations before aborting
- `-e, --eof <MODE>` - EOF behavior: `zero`, `unchanged` (default), or `max`

### Batch mode

For running multiple inputs against the same program:

```
echo '{"input": [72, 101, 108, 108, 111]}' | rustfuck run program.b --batch
```

Reads newline-delimited JSON from stdin, outputs one JSON result per line.
