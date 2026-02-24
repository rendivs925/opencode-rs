# Rust Benchmark Report

Date: 2026-02-24

These are native `@opencode-ai/core` microbench means in ms/op.

| Operation | Mean (ms/op) |
| --- | ---: |
| countTokensFromText | 37.2608 |
| countTokens(file) | 37.7415 |
| glob | 0.3259 |
| globParallel | 0.5375 |
| isIgnored | 0.0118 |
| truncate | 0.0682 |

Notes:
- This is a local microbench run (single machine, warm process).
- Use it for regression tracking, not absolute cross-machine comparisons.
