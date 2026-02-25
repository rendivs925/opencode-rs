use crate::apply_patch as apply_unified_patch;
use crate::create_two_files_patch;
use napi::Result;
use std::fs;

#[napi(object)]
#[derive(Clone)]
pub struct PatchUpdateChunk {
    pub old_lines: Vec<String>,
    pub new_lines: Vec<String>,
    pub change_context: Option<String>,
    pub is_end_of_file: Option<bool>,
}

#[napi(object)]
#[derive(Clone)]
pub struct PatchHunk {
    pub hunk_type: String,
    pub path: String,
    pub move_path: Option<String>,
    pub contents: Option<String>,
    pub chunks: Option<Vec<PatchUpdateChunk>>,
}

#[napi(object)]
#[derive(Clone)]
pub struct ParsedApplyPatch {
    pub hunks: Vec<PatchHunk>,
}

#[napi(object)]
#[derive(Clone)]
pub struct PatchFileUpdate {
    pub unified_diff: String,
    pub content: String,
}

#[napi]
pub fn parse_apply_patch(patch_text: String) -> Result<ParsedApplyPatch> {
    let cleaned = strip_heredoc(patch_text.trim());
    let lines = cleaned.lines().collect::<Vec<_>>();
    let begin = lines
        .iter()
        .position(|line| line.trim() == "*** Begin Patch")
        .ok_or_else(|| napi::Error::from_reason("Invalid patch format: missing Begin/End markers".to_string()))?;
    let end = lines
        .iter()
        .position(|line| line.trim() == "*** End Patch")
        .ok_or_else(|| napi::Error::from_reason("Invalid patch format: missing Begin/End markers".to_string()))?;
    if begin >= end {
        return Err(napi::Error::from_reason(
            "Invalid patch format: missing Begin/End markers".to_string(),
        ));
    }

    let mut i = begin + 1;
    let mut hunks = Vec::<PatchHunk>::new();
    while i < end {
        let line = lines[i];
        if let Some(path) = line.strip_prefix("*** Add File:") {
            let path = path.trim().to_string();
            if path.is_empty() {
                i += 1;
                continue;
            }
            i += 1;
            let mut content = String::new();
            while i < end && !lines[i].starts_with("***") {
                if let Some(value) = lines[i].strip_prefix('+') {
                    content.push_str(value);
                    content.push('\n');
                }
                i += 1;
            }
            if content.ends_with('\n') {
                content.pop();
            }
            hunks.push(PatchHunk {
                hunk_type: "add".to_string(),
                path,
                move_path: None,
                contents: Some(content),
                chunks: None,
            });
            continue;
        }

        if let Some(path) = line.strip_prefix("*** Delete File:") {
            let path = path.trim().to_string();
            if path.is_empty() {
                i += 1;
                continue;
            }
            hunks.push(PatchHunk {
                hunk_type: "delete".to_string(),
                path,
                move_path: None,
                contents: None,
                chunks: None,
            });
            i += 1;
            continue;
        }

        if let Some(path) = line.strip_prefix("*** Update File:") {
            let path = path.trim().to_string();
            if path.is_empty() {
                i += 1;
                continue;
            }
            i += 1;
            let mut move_path = None;
            if i < end {
                if let Some(next) = lines[i].strip_prefix("*** Move to:") {
                    let next = next.trim().to_string();
                    if !next.is_empty() {
                        move_path = Some(next);
                    }
                    i += 1;
                }
            }

            let mut chunks = Vec::<PatchUpdateChunk>::new();
            while i < end && !lines[i].starts_with("***") {
                if !lines[i].starts_with("@@") {
                    i += 1;
                    continue;
                }
                let context = lines[i].trim_start_matches("@@").trim().to_string();
                i += 1;
                let mut old_lines = Vec::<String>::new();
                let mut new_lines = Vec::<String>::new();
                let mut eof = false;
                while i < end && !lines[i].starts_with("@@") && !lines[i].starts_with("***") {
                    let row = lines[i];
                    if row == "*** End of File" {
                        eof = true;
                        i += 1;
                        break;
                    }
                    if let Some(value) = row.strip_prefix(' ') {
                        old_lines.push(value.to_string());
                        new_lines.push(value.to_string());
                        i += 1;
                        continue;
                    }
                    if let Some(value) = row.strip_prefix('-') {
                        old_lines.push(value.to_string());
                        i += 1;
                        continue;
                    }
                    if let Some(value) = row.strip_prefix('+') {
                        new_lines.push(value.to_string());
                        i += 1;
                        continue;
                    }
                    i += 1;
                }
                chunks.push(PatchUpdateChunk {
                    old_lines,
                    new_lines,
                    change_context: (!context.is_empty()).then_some(context),
                    is_end_of_file: eof.then_some(true),
                });
            }

            hunks.push(PatchHunk {
                hunk_type: "update".to_string(),
                path,
                move_path,
                contents: None,
                chunks: Some(chunks),
            });
            continue;
        }

        i += 1;
    }

    Ok(ParsedApplyPatch { hunks })
}

#[napi]
pub fn derive_new_contents_from_chunks(file_path: String, chunks: Vec<PatchUpdateChunk>) -> Result<PatchFileUpdate> {
    let original_content =
        fs::read_to_string(&file_path).map_err(|err| napi::Error::from_reason(format!("Failed to read file {file_path}: {err}")))?;
    let mut content = original_content.clone();
    let mut line_index = 0usize;

    for chunk in chunks {
        let lines = split_content(&content);
        if let Some(ctx) = chunk.change_context.clone() {
            let idx = seek_sequence(&lines, &[ctx], line_index, false);
            if idx.is_none() {
                return Err(napi::Error::from_reason(format!(
                    "Failed to find context '{}' in {}",
                    chunk.change_context.unwrap_or_default(),
                    file_path
                )));
            }
            line_index = idx.expect("checked is_some") + 1;
        }

        let mut pattern = chunk.old_lines.clone();
        let mut replacement = chunk.new_lines.clone();

        if pattern.is_empty() {
            let patch = build_unified_patch(&file_path, lines.len(), &pattern, &replacement);
            content = apply_unified_patch(content, patch)?;
            line_index = lines.len() + replacement.len();
            continue;
        }

        let mut found = seek_sequence(
            &lines,
            &pattern,
            line_index,
            chunk.is_end_of_file.unwrap_or(false),
        );
        if found.is_none() && pattern.last().is_some_and(|item| item.is_empty()) {
            pattern.pop();
            if replacement.last().is_some_and(|item| item.is_empty()) {
                replacement.pop();
            }
            found = seek_sequence(
                &lines,
                &pattern,
                line_index,
                chunk.is_end_of_file.unwrap_or(false),
            );
        }
        if found.is_none() {
            return Err(napi::Error::from_reason(format!(
                "Failed to find expected lines in {}:\n{}",
                file_path,
                chunk.old_lines.join("\n")
            )));
        }

        let at = found.expect("checked is_some");
        let matched = lines[at..at + pattern.len()].to_vec();
        let patch = build_unified_patch(&file_path, at, &matched, &replacement);
        content = apply_unified_patch(content, patch)?;
        line_index = at + replacement.len();
    }

    let next = if content.is_empty() || content.ends_with('\n') {
        content
    } else {
        format!("{content}\n")
    };
    let unified_diff = create_two_files_patch(
        file_path.clone(),
        file_path,
        original_content,
        next.clone(),
    );
    Ok(PatchFileUpdate {
        unified_diff,
        content: next,
    })
}

fn split_content(content: &str) -> Vec<String> {
    let mut lines = content.split('\n').map(|item| item.to_string()).collect::<Vec<_>>();
    if lines.last().is_some_and(|item| item.is_empty()) {
        lines.pop();
    }
    lines
}

fn build_unified_patch(file_path: &str, line_start: usize, old_lines: &[String], new_lines: &[String]) -> String {
    let mut out = vec![
        format!("--- {file_path}"),
        format!("+++ {file_path}"),
        format!(
            "@@ -{},{} +{},{} @@",
            line_start + 1,
            old_lines.len(),
            line_start + 1,
            new_lines.len()
        ),
    ];
    out.extend(old_lines.iter().map(|line| format!("-{line}")));
    out.extend(new_lines.iter().map(|line| format!("+{line}")));
    out.join("\n")
}

fn normalize_unicode(value: &str) -> String {
    value
        .replace(['\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'], "'")
        .replace(['\u{201C}', '\u{201D}', '\u{201E}', '\u{201F}'], "\"")
        .replace(
            ['\u{2010}', '\u{2011}', '\u{2012}', '\u{2013}', '\u{2014}', '\u{2015}'],
            "-",
        )
        .replace('\u{2026}', "...")
        .replace('\u{00A0}', " ")
}

fn seek_sequence(lines: &[String], pattern: &[String], start: usize, eof: bool) -> Option<usize> {
    if pattern.is_empty() {
        return None;
    }

    if let Some(exact) = try_match(lines, pattern, start, eof, |a, b| a == b) {
        return Some(exact);
    }
    if let Some(rstrip) = try_match(lines, pattern, start, eof, |a, b| a.trim_end() == b.trim_end()) {
        return Some(rstrip);
    }
    if let Some(trimmed) = try_match(lines, pattern, start, eof, |a, b| a.trim() == b.trim()) {
        return Some(trimmed);
    }
    try_match(lines, pattern, start, eof, |a, b| {
        normalize_unicode(a.trim()) == normalize_unicode(b.trim())
    })
}

fn try_match<F>(lines: &[String], pattern: &[String], start: usize, eof: bool, cmp: F) -> Option<usize>
where
    F: Fn(&str, &str) -> bool,
{
    if pattern.len() > lines.len() {
        return None;
    }

    if eof {
        let from_end = lines.len() - pattern.len();
        if from_end >= start {
            let mut ok = true;
            for j in 0..pattern.len() {
                if !cmp(&lines[from_end + j], &pattern[j]) {
                    ok = false;
                    break;
                }
            }
            if ok {
                return Some(from_end);
            }
        }
    }

    for i in start..=lines.len() - pattern.len() {
        let mut ok = true;
        for j in 0..pattern.len() {
            if !cmp(&lines[i + j], &pattern[j]) {
                ok = false;
                break;
            }
        }
        if ok {
            return Some(i);
        }
    }
    None
}

fn strip_heredoc(input: &str) -> String {
    if !input.starts_with("<<") && !input.starts_with("cat <<") {
        return input.to_string();
    }
    let mut lines = input.lines();
    let first = lines.next().unwrap_or_default();
    let marker = if let Some(idx) = first.find("<<") {
        let head = first[idx + 2..].trim().trim_matches('\'').trim_matches('"');
        if head.is_empty() {
            return input.to_string();
        }
        head.to_string()
    } else {
        return input.to_string();
    };
    let body = lines.collect::<Vec<_>>();
    if body.is_empty() {
        return input.to_string();
    }
    let end_idx = body.iter().rposition(|line| line.trim() == marker);
    let Some(end_idx) = end_idx else {
        return input.to_string();
    };
    body[..end_idx].join("\n")
}

