# Rust vs JS Benchmark Report

Date: 2026-02-24

Microbench means in ms/op. `Speedup = JS / Rust`.

| Operation | JS (ms/op) | Rust (ms/op) | Speedup |
| --- | ---: | ---: | ---: |
| Token from text | 0.0001 | 40.9048 | 0x |
| Token from file | 0.001 | 38.911 | 0x |
| Glob scan | 0.0151 | 0.4412 | 0.0343x |
| Glob match | 0.0001 | 0.0018 | 0.0532x |
| Truncate | 0.1768 | 0.1091 | 1.6208x |

Notes:
- JS values are local baseline implementations in this script.
- Results are for regression tracking on one machine, not absolute claims.
