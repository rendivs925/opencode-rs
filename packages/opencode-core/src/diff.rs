use napi::Result;
use regex::Regex;
use std::sync::OnceLock;

#[napi(object)]
#[derive(Clone)]
pub struct DiffLine {
    pub old_index: Option<u32>,
    pub new_index: Option<u32>,
    pub content: String,
    pub line_type: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OpKind {
    Context,
    Added,
    Removed,
}

#[derive(Clone)]
struct Op {
    kind: OpKind,
    old_index: Option<usize>,
    new_index: Option<usize>,
    content: String,
}

#[derive(Clone)]
struct Pos {
    old_line: usize,
    new_line: usize,
}

#[derive(Clone)]
struct Hunk {
    start: usize,
    end: usize,
}

#[napi]
pub fn create_two_files_patch(
    old_path: String,
    new_path: String,
    old_content: String,
    new_content: String,
) -> String {
    let old = split_lines(&old_content);
    let new = split_lines(&new_content);
    let ops = build_ops(&old, &new);
    let hunks = build_hunks(&ops, 3);
    let mut out = vec![format!("--- {old_path}"), format!("+++ {new_path}")];

    if hunks.is_empty() {
        return out.join("\n");
    }

    let pos = op_positions(&ops);
    for hunk in hunks {
        let head = &pos[hunk.start];
        let body = &ops[hunk.start..hunk.end];
        let old_count = body
            .iter()
            .filter(|item| item.kind != OpKind::Added)
            .count();
        let new_count = body
            .iter()
            .filter(|item| item.kind != OpKind::Removed)
            .count();
        out.push(format!(
            "@@ -{},{} +{},{} @@",
            head.old_line.max(1),
            old_count,
            head.new_line.max(1),
            new_count
        ));
        for item in body {
            let prefix = match item.kind {
                OpKind::Context => " ",
                OpKind::Added => "+",
                OpKind::Removed => "-",
            };
            out.push(format!("{prefix}{}", item.content));
        }
    }

    out.join("\n")
}

#[napi]
pub fn diff_lines(old: String, new: String) -> Vec<DiffLine> {
    let old = split_lines(&old);
    let new = split_lines(&new);
    build_ops(&old, &new)
        .into_iter()
        .map(|item| DiffLine {
            old_index: item.old_index.map(|v| v as u32),
            new_index: item.new_index.map(|v| v as u32),
            content: item.content,
            line_type: match item.kind {
                OpKind::Context => "context",
                OpKind::Added => "added",
                OpKind::Removed => "removed",
            }
            .to_string(),
        })
        .collect()
}

#[napi]
pub fn apply_patch(original: String, patch: String) -> Result<String> {
    let mut src = split_lines(&original);
    let mut delta = 0isize;
    let mut i = 0usize;
    let lines = patch.lines().collect::<Vec<_>>();

    while i < lines.len() {
        let line = lines[i];
        if !line.starts_with("@@ ") {
            i += 1;
            continue;
        }

        let (old_start, _) = parse_hunk_header(line)?;
        let mut at = old_start.saturating_sub(1) as isize + delta;
        if at < 0 {
            return Err(napi::Error::from_reason("invalid patch offset".to_string()));
        }

        i += 1;
        while i < lines.len() && !lines[i].starts_with("@@ ") {
            let row = lines[i];
            if row.starts_with("--- ") || row.starts_with("+++ ") {
                i += 1;
                continue;
            }
            if row.starts_with("\\ No newline at end of file") {
                i += 1;
                continue;
            }

            if let Some(value) = row.strip_prefix(' ') {
                let pos = at as usize;
                if src.get(pos).map(String::as_str) != Some(value) {
                    return Err(napi::Error::from_reason("patch context mismatch".to_string()));
                }
                at += 1;
                i += 1;
                continue;
            }

            if let Some(value) = row.strip_prefix('-') {
                let pos = at as usize;
                if src.get(pos).map(String::as_str) != Some(value) {
                    return Err(napi::Error::from_reason("patch delete mismatch".to_string()));
                }
                src.remove(pos);
                delta -= 1;
                i += 1;
                continue;
            }

            if let Some(value) = row.strip_prefix('+') {
                let pos = at as usize;
                if pos > src.len() {
                    return Err(napi::Error::from_reason("patch insert mismatch".to_string()));
                }
                src.insert(pos, value.to_string());
                at += 1;
                delta += 1;
                i += 1;
                continue;
            }

            i += 1;
        }
    }

    Ok(src.join("\n"))
}

fn split_lines(value: &str) -> Vec<String> {
    if value.is_empty() {
        return vec![];
    }
    let mut out = value
        .split('\n')
        .map(|item| item.to_string())
        .collect::<Vec<_>>();
    if value.ends_with('\n') && out.last().is_some_and(|item| item.is_empty()) {
        out.pop();
    }
    out
}

fn build_ops(old: &[String], new: &[String]) -> Vec<Op> {
    let n = old.len();
    let m = new.len();
    let mut dp = vec![vec![0usize; m + 1]; n + 1];

    let mut i = n;
    while i > 0 {
        i -= 1;
        let mut j = m;
        while j > 0 {
            j -= 1;
            dp[i][j] = if old[i] == new[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let mut i = 0usize;
    let mut j = 0usize;
    let mut ops = vec![];

    while i < n && j < m {
        if old[i] == new[j] {
            ops.push(Op {
                kind: OpKind::Context,
                old_index: Some(i),
                new_index: Some(j),
                content: old[i].clone(),
            });
            i += 1;
            j += 1;
            continue;
        }

        if dp[i + 1][j] >= dp[i][j + 1] {
            ops.push(Op {
                kind: OpKind::Removed,
                old_index: Some(i),
                new_index: None,
                content: old[i].clone(),
            });
            i += 1;
            continue;
        }

        ops.push(Op {
            kind: OpKind::Added,
            old_index: None,
            new_index: Some(j),
            content: new[j].clone(),
        });
        j += 1;
    }

    while i < n {
        ops.push(Op {
            kind: OpKind::Removed,
            old_index: Some(i),
            new_index: None,
            content: old[i].clone(),
        });
        i += 1;
    }

    while j < m {
        ops.push(Op {
            kind: OpKind::Added,
            old_index: None,
            new_index: Some(j),
            content: new[j].clone(),
        });
        j += 1;
    }

    ops
}

fn op_positions(ops: &[Op]) -> Vec<Pos> {
    let mut old_line = 1usize;
    let mut new_line = 1usize;
    let mut out = Vec::with_capacity(ops.len() + 1);
    out.push(Pos { old_line, new_line });

    for item in ops {
        if item.kind != OpKind::Added {
            old_line += 1;
        }
        if item.kind != OpKind::Removed {
            new_line += 1;
        }
        out.push(Pos { old_line, new_line });
    }

    out
}

fn build_hunks(ops: &[Op], context: usize) -> Vec<Hunk> {
    let mut spans = vec![];
    for (idx, item) in ops.iter().enumerate() {
        if item.kind == OpKind::Context {
            continue;
        }
        spans.push(Hunk {
            start: idx.saturating_sub(context),
            end: (idx + context + 1).min(ops.len()),
        });
    }

    if spans.is_empty() {
        return vec![];
    }

    let mut merged = vec![spans[0].clone()];
    for span in spans.into_iter().skip(1) {
        let tail = merged.last_mut().expect("merged has at least one span");
        if span.start <= tail.end {
            tail.end = tail.end.max(span.end);
            continue;
        }
        merged.push(span);
    }

    merged
}

fn hunk_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@")
            .expect("valid unified hunk regex")
    })
}

fn parse_hunk_header(line: &str) -> Result<(usize, usize)> {
    let caps = hunk_regex()
        .captures(line)
        .ok_or_else(|| napi::Error::from_reason("invalid hunk header".to_string()))?;

    let old_start = caps
        .get(1)
        .and_then(|m| m.as_str().parse::<usize>().ok())
        .ok_or_else(|| napi::Error::from_reason("invalid old hunk start".to_string()))?;

    let old_count = caps
        .get(2)
        .and_then(|m| m.as_str().parse::<usize>().ok())
        .unwrap_or(1);

    Ok((old_start, old_count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_multi_hunk_patch_roundtrip() {
        let before = "a\nb\nc\nd\ne\nf\ng\n".to_string();
        let after = "a\nb\nX\nd\ne\nY\ng\n".to_string();
        let patch = create_two_files_patch("a.txt".to_string(), "a.txt".to_string(), before.clone(), after.clone());
        let applied = apply_patch(before, patch).expect("patch should apply");
        assert_eq!(applied, after.trim_end().to_string());
    }

    #[test]
    fn fails_on_context_mismatch() {
        let before = "a\nb\nc\n".to_string();
        let patch = vec![
            "--- a.txt".to_string(),
            "+++ a.txt".to_string(),
            "@@ -1,1 +1,1 @@".to_string(),
            " z".to_string(),
            "+x".to_string(),
        ]
        .join("\n");
        assert!(apply_patch(before, patch).is_err());
    }

    #[test]
    fn no_changes_patch_has_headers_only() {
        let text = "same\ncontent\n".to_string();
        let patch = create_two_files_patch("a.txt".to_string(), "a.txt".to_string(), text.clone(), text);
        let lines = patch.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("--- "));
        assert!(lines[1].starts_with("+++ "));
    }

    #[test]
    fn applies_insertions_at_start_and_end() {
        let before = "mid\n".to_string();
        let after = "start\nmid\nend\n".to_string();
        let patch = create_two_files_patch("a.txt".to_string(), "a.txt".to_string(), before.clone(), after.clone());
        let applied = apply_patch(before, patch).expect("patch should apply");
        assert_eq!(applied, after.trim_end().to_string());
    }

    #[test]
    fn applies_deletion_only_patch() {
        let before = "a\nb\nc\n".to_string();
        let after = "a\n".to_string();
        let patch = create_two_files_patch("a.txt".to_string(), "a.txt".to_string(), before.clone(), after.clone());
        let applied = apply_patch(before, patch).expect("patch should apply");
        assert_eq!(applied, after.trim_end().to_string());
    }

    #[test]
    fn handles_marker_like_content_lines() {
        let before = "line\n@@ marker\n--- x\n+++ y\n".to_string();
        let after = "line\n@@ marker changed\n--- x\n+++ y\n".to_string();
        let patch = create_two_files_patch("a.txt".to_string(), "a.txt".to_string(), before.clone(), after.clone());
        let applied = apply_patch(before, patch).expect("patch should apply");
        assert_eq!(applied, after.trim_end().to_string());
    }
}
