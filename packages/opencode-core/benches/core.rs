use criterion::{criterion_group, criterion_main, Criterion};
use opencode_core::{count_lines_fast, count_tokens_from_text, diff_lines, fast_hash, replace_content, truncate};

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

fn bench_diff_small(c: &mut Criterion) {
    let old = "line1\nline2\nline3\n".repeat(100);
    let new = "line1\nmodified\nline3\n".repeat(100);
    c.bench_function("diff.diff_lines_100", |b| {
        b.iter(|| {
            let _ = diff_lines(old.clone(), new.clone());
        })
    });
}

fn bench_edit_replace(c: &mut Criterion) {
    let text = "fn foo() {\n    let x = 1;\n    let y = 2;\n}\n".repeat(100);
    c.bench_function("edit.replace_content", |b| {
        b.iter(|| {
            let _ = replace_content(
                text.clone(),
                "let x = 1;".to_string(),
                "let x = 999;".to_string(),
                false,
            );
        })
    });
}

fn bench_lines(c: &mut Criterion) {
    let text = "line content here\n".repeat(10_000);
    c.bench_function("text_ops.count_lines_fast", |b| {
        b.iter(|| {
            let _ = count_lines_fast(text.clone());
        })
    });
}

fn bench_hash(c: &mut Criterion) {
    let content = "data to hash".repeat(1000);
    c.bench_function("hash.fast_hash", |b| {
        b.iter(|| {
            let _ = fast_hash(content.clone());
        })
    });
}

criterion_group!(
    benches,
    bench_token,
    bench_truncate,
    bench_diff_small,
    bench_edit_replace,
    bench_lines,
    bench_hash
);
criterion_main!(benches);
