use criterion::{criterion_group, criterion_main, Criterion};
use opencode_core::{count_tokens_from_text, truncate};

fn bench_token(c: &mut Criterion) {
    let text = "lorem ipsum ".repeat(10_000);
    c.bench_function("token.count_tokens_from_text", |b| {
        b.iter(|| {
            let _ = count_tokens_from_text(text.clone(), "gpt-4".to_string()).ok();
        })
    });
}

fn bench_truncate(c: &mut Criterion) {
    let text = (0..20_000)
        .map(|i| format!("line-{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    c.bench_function("truncation.truncate", |b| {
        b.iter(|| {
            let _ = truncate(text.clone(), Some(2_000), Some(50 * 1024), "head".to_string());
        })
    });
}

criterion_group!(benches, bench_token, bench_truncate);
criterion_main!(benches);
