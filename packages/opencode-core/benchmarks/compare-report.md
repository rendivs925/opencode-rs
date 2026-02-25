# Rust vs JS Benchmark Report

Date: 2026-02-25

Microbench means in ms/op. `Speedup = JS / Rust`.

| Operation | JS (ms/op) | Rust (ms/op) | Speedup |
| --- | ---: | ---: | ---: |
| Token from text | 0.0001 | 293.4037 | 0x |
| Token from file | 0.001 | 291.4979 | 0x |
| Glob scan | 0.0143 | 2.729 | 0.0052x |
| Glob match | 0.0001 | 0.0016 | 0.0774x |
| Truncate | 0.1583 | 0.6106 | 0.2593x |

Notes:
- JS values are local baseline implementations in this script.
- Results are for regression tracking on one machine, not absolute claims.
