# Rust Benchmark Report

Date: 2026-02-25

These are native `@opencode-ai/core` microbench means in ms/op.

| Operation | Mean (ms/op) |
| --- | ---: |
| countTokensFromText | 277.5354 |
| countTokens(file) | 281.7966 |
| glob | 1.9664 |
| globParallel | 2.5547 |
| isIgnored | 0.0028 |
| truncate | 0.4043 |

Notes:
- This is a local microbench run (single machine, warm process).
- Use it for regression tracking, not absolute cross-machine comparisons.
