use globset::{GlobBuilder, GlobMatcher};
use ignore::WalkBuilder;
use napi::Result;
use rayon::prelude::*;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::time::UNIX_EPOCH;

#[napi(object)]
#[derive(Clone)]
pub struct SearchContextSubmatch {
    pub text: String,
    pub start: i32,
    pub end: i32,
}

#[napi(object)]
#[derive(Clone)]
pub struct SearchContextMatch {
    pub path: String,
    pub mod_time: i64,
    pub line_num: i32,
    pub line_text: String,
    pub absolute_offset: i32,
    pub submatches: Vec<SearchContextSubmatch>,
    pub before: Vec<String>,
    pub after: Vec<String>,
}

#[napi(object)]
#[derive(Clone)]
pub struct SearchContextResult {
    pub matches: Vec<SearchContextMatch>,
    pub has_errors: bool,
    pub total_matches: i32,
}

#[napi]
pub fn search_content_context(
    pattern: String,
    search_path: String,
    globs: Option<Vec<String>>,
    include_hidden: Option<bool>,
    follow_links: Option<bool>,
    max_depth: Option<i32>,
    max_results: Option<i32>,
    max_line_length: Option<i32>,
    before_lines: Option<u32>,
    after_lines: Option<u32>,
) -> Result<SearchContextResult> {
    let root = Path::new(&search_path);
    let regex = Regex::new(&pattern).map_err(|item| napi::Error::from_reason(item.to_string()))?;
    let max_line = max_line_length.unwrap_or(2000).max(1) as usize;
    let hidden = include_hidden.unwrap_or(true);
    let follow = follow_links.unwrap_or(false);
    let depth = max_depth.map(|item| item.max(0) as usize);
    let before = before_lines.unwrap_or(2) as usize;
    let after = after_lines.unwrap_or(2) as usize;
    let has_errors = AtomicBool::new(false);
    let files = collect_files(root, globs.as_ref(), hidden, follow, depth, &has_errors)?;

    let mut matches: Vec<SearchContextMatch> = files
        .par_iter()
        .flat_map(|(file, mod_time)| {
            let bytes = match std::fs::read(file) {
                Ok(item) => item,
                Err(_) => {
                    has_errors.store(true, AtomicOrdering::Relaxed);
                    return Vec::new();
                }
            };
            if is_binary(&bytes) {
                return Vec::new();
            }

            let text = String::from_utf8_lossy(&bytes);
            let mut lines = Vec::new();
            let mut offset = 0usize;
            for segment in text.split_inclusive('\n') {
                let no_nl = segment.strip_suffix('\n').unwrap_or(segment);
                let raw = no_nl.strip_suffix('\r').unwrap_or(no_nl);
                lines.push((offset, raw));
                offset += segment.len();
            }
            if lines.is_empty() && !text.is_empty() {
                lines.push((0, text.as_ref()));
            }

            let mut found = Vec::new();
            for (idx, (line_offset, raw)) in lines.iter().enumerate() {
                if !regex.is_match(raw) {
                    continue;
                }
                let line_text = if raw.chars().count() > max_line {
                    raw.chars().take(max_line).collect::<String>() + "..."
                } else {
                    raw.to_string()
                };
                let submatches = regex
                    .find_iter(&line_text)
                    .map(|item| SearchContextSubmatch {
                        text: item.as_str().to_string(),
                        start: item.start() as i32,
                        end: item.end() as i32,
                    })
                    .collect::<Vec<_>>();

                let before_items = lines
                    .iter()
                    .skip(idx.saturating_sub(before))
                    .take(before.min(idx))
                    .map(|item| item.1.to_string())
                    .collect::<Vec<_>>();
                let after_items = lines
                    .iter()
                    .skip(idx + 1)
                    .take(after)
                    .map(|item| item.1.to_string())
                    .collect::<Vec<_>>();

                found.push(SearchContextMatch {
                    path: normalize_relative(root, file),
                    mod_time: *mod_time,
                    line_num: idx as i32 + 1,
                    line_text,
                    absolute_offset: *line_offset as i32,
                    submatches,
                    before: before_items,
                    after: after_items,
                });
            }

            found
        })
        .collect();

    matches.sort_by(|a, b| {
        b.mod_time
            .cmp(&a.mod_time)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.line_num.cmp(&b.line_num))
    });

    let total_matches = matches.len() as i32;
    if let Some(limit) = max_results {
        let limit = limit.max(0) as usize;
        if matches.len() > limit {
            matches.truncate(limit);
        }
    }

    Ok(SearchContextResult {
        matches,
        has_errors: has_errors.load(AtomicOrdering::Relaxed),
        total_matches,
    })
}

fn collect_files(
    root: &Path,
    globs: Option<&Vec<String>>,
    include_hidden: bool,
    follow_links: bool,
    max_depth: Option<usize>,
    has_errors: &AtomicBool,
) -> Result<Vec<(PathBuf, i64)>> {
    let (include_matchers, exclude_matchers) = if let Some(items) = globs {
        compile_matchers(items)?
    } else {
        (Vec::new(), Vec::new())
    };
    let has_include = !include_matchers.is_empty();
    let mut files = Vec::new();
    let mut walker = WalkBuilder::new(root);
    walker
        .hidden(false)
        .follow_links(follow_links)
        .max_depth(max_depth)
        .require_git(false);

    for entry in walker.build() {
        let Ok(entry) = entry else {
            has_errors.store(true, AtomicOrdering::Relaxed);
            continue;
        };
        let entry_path = entry.path();
        if !entry_path.is_file() {
            continue;
        }
        let rel = entry_path.strip_prefix(root).ok().unwrap_or(entry_path);
        if !include_hidden && is_hidden(rel) {
            continue;
        }
        let rel_text = normalize_path(rel);
        if has_include && !include_matchers.iter().any(|item| item.is_match(&rel_text)) {
            continue;
        }
        if exclude_matchers.iter().any(|item| item.is_match(&rel_text)) {
            continue;
        }
        let mod_time = std::fs::metadata(entry_path)
            .and_then(|item| item.modified())
            .ok()
            .and_then(|item| item.duration_since(UNIX_EPOCH).ok())
            .map(|item| item.as_millis() as i64)
            .unwrap_or(0);
        files.push((entry_path.to_path_buf(), mod_time));
    }

    Ok(files)
}

fn compile_matchers(globs: &[String]) -> Result<(Vec<GlobMatcher>, Vec<GlobMatcher>)> {
    let mut include = Vec::new();
    let mut exclude = Vec::new();

    for item in globs {
        let (negated, pattern) = if let Some(rest) = item.strip_prefix('!') {
            (true, rest)
        } else {
            (false, item.as_str())
        };
        let matcher = GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .map_err(|err| napi::Error::from_reason(err.to_string()))?
            .compile_matcher();
        if negated {
            exclude.push(matcher);
            continue;
        }
        include.push(matcher);
    }

    Ok((include, exclude))
}

fn is_hidden(path: &Path) -> bool {
    path.components().any(|item| {
        item.as_os_str()
            .to_str()
            .map(|value| value.starts_with('.'))
            .unwrap_or(false)
    })
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn normalize_relative(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).ok().unwrap_or(path);
    normalize_path(rel)
}

fn is_binary(bytes: &[u8]) -> bool {
    let len = bytes.len().min(8192);
    bytes[..len].contains(&0)
}
