# Rust vs JS Benchmark Report

Date: 2026-02-24

Microbench means in ms/op. `Speedup = JS / Rust`.

| Operation | JS (ms/op) | Rust (ms/op) | Speedup |
| --- | ---: | ---: | ---: |
| Token from text | 0.0001 | 39.1349 | 0x |
| Token from file | 0.0009 | 41.7398 | 0x |
| Glob scan | 0.0142 | 0.4336 | 0.0328x |
| Glob match | 0.0001 | 0.0018 | 0.0575x |
| Truncate | 0.1836 | 0.1049 | 1.7504x |

Notes:
- JS values are local baseline implementations in this script.
- Results are for regression tracking on one machine, not absolute claims.
