# Rust Benchmark Report

Date: 2026-02-24

These are native `@opencode-ai/core` microbench means in ms/op.

| Operation | Mean (ms/op) |
| --- | ---: |
| countTokensFromText | 39.1819 |
| countTokens(file) | 39.9228 |
| glob | 0.3137 |
| globParallel | 0.5279 |
| isIgnored | 0.0116 |
| truncate | 0.0697 |

Notes:
- This is a local microbench run (single machine, warm process).
- Use it for regression tracking, not absolute cross-machine comparisons.
