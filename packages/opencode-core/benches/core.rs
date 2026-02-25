use criterion::{black_box, criterion_group, criterion_main, Criterion};
use opencode_core::{
    count_lines_fast, count_tokens_from_text, create_two_files_patch, fast_hash, find_whitespace_indices, replace_content,
    truncate,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn bench_token(c: &mut Criterion) {
    let text = "lorem ipsum ".repeat(10_000);
    c.bench_function("token.count_tokens_from_text", |b| {
        b.iter(|| {
            let _ = count_tokens_from_text(black_box(text.clone()), "gpt-4".to_string()).ok();
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
            let _ = truncate(black_box(text.clone()), Some(2_000), Some(50 * 1024), "head".to_string());
        })
    });
}

fn bench_diff_small(c: &mut Criterion) {
    let old = "line1\nline2\nline3\n".repeat(100);
    let new = "line1\nmodified\nline3\n".repeat(100);
    c.bench_function("diff.create_two_files_patch_100", |b| {
        b.iter(|| {
            let _ = create_two_files_patch(
                "a.txt".to_string(),
                "a.txt".to_string(),
                black_box(old.clone()),
                black_box(new.clone()),
            );
        })
    });
}

fn bench_diff_large(c: &mut Criterion) {
    let old = "line1\nline2\nline3\n".repeat(10_000);
    let new = "line1\nmodified\nline3\n".repeat(10_000);
    c.bench_function("diff.create_two_files_patch_10k", |b| {
        b.iter(|| {
            let _ = create_two_files_patch(
                "a.txt".to_string(),
                "a.txt".to_string(),
                black_box(old.clone()),
                black_box(new.clone()),
            );
        })
    });
}

fn bench_edit_replace(c: &mut Criterion) {
    let text = "fn foo() {\n    let x = 1;\n    let y = 2;\n}\n".repeat(100);
    c.bench_function("edit.replace_content", |b| {
        b.iter(|| {
            let _ = replace_content(
                black_box(text.clone()),
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
            let _ = count_lines_fast(black_box(text.clone()));
        })
    });
}

fn bench_whitespace(c: &mut Criterion) {
    let text = "line content here\n".repeat(10_000);
    c.bench_function("text_ops.find_whitespace_indices", |b| {
        b.iter(|| {
            let _ = find_whitespace_indices(black_box(text.clone()));
        })
    });
}

fn bench_hash_mem(c: &mut Criterion) {
    let content = "data to hash".repeat(1000);
    c.bench_function("hash.fast_hash_memory", |b| {
        b.iter(|| {
            let _ = fast_hash(black_box(content.clone()));
        })
    });
}

fn bench_hash_file(c: &mut Criterion) {
    let root = temp_root("opencode-bench-hash");
    let file = root.join("input.txt");
    let _ = fs::create_dir_all(&root);
    let _ = fs::write(&file, "data to hash".repeat(10_000));
    c.bench_function("hash.fast_hash_file", |b| {
        b.iter(|| {
            let _ = opencode_core::file_hash(black_box(file.to_string_lossy().to_string()));
        })
    });
    let _ = fs::remove_file(&file);
    let _ = fs::remove_dir(&root);
}

fn temp_root(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ))
}

criterion_group!(
    benches,
    bench_token,
    bench_truncate,
    bench_diff_small,
    bench_diff_large,
    bench_edit_replace,
    bench_lines,
    bench_whitespace,
    bench_hash_mem,
    bench_hash_file
);
criterion_main!(benches);
