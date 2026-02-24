use globset::{GlobBuilder, GlobMatcher};
use ignore::WalkBuilder;
use napi::Result;
use rayon::prelude::*;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::UNIX_EPOCH;

#[napi(object)]
#[derive(Clone)]
pub struct SearchMatch {
    pub path: String,
    pub mod_time: i64,
    pub line_num: i32,
    pub line_text: String,
}

#[napi(object)]
#[derive(Clone)]
pub struct SearchResult {
    pub matches: Vec<SearchMatch>,
    pub has_errors: bool,
    pub total_matches: i32,
}

#[napi(object)]
#[derive(Clone)]
pub struct FileListResult {
    pub files: Vec<String>,
    pub has_errors: bool,
}

#[napi]
pub fn search_content(
    pattern: String,
    search_path: String,
    include: Option<String>,
    max_results: Option<i32>,
    max_line_length: Option<i32>,
) -> Result<SearchResult> {
    let globs = include.map(|item| vec![item]);
    search_content_advanced(
        pattern,
        search_path,
        globs,
        Some(true),
        Some(false),
        None,
        max_results,
        max_line_length,
    )
}

#[napi]
pub fn list_files(
    search_path: String,
    globs: Option<Vec<String>>,
    include_hidden: Option<bool>,
    follow_links: Option<bool>,
    max_depth: Option<i32>,
) -> Result<FileListResult> {
    let root = Path::new(&search_path);
    let hidden = include_hidden.unwrap_or(true);
    let follow = follow_links.unwrap_or(false);
    let depth = max_depth.map(|d| d.max(0) as usize);
    let has_errors = AtomicBool::new(false);
    let files = collect_files(root, globs.as_ref(), hidden, follow, depth, &has_errors)?
        .into_iter()
        .map(|(file, _)| normalize_relative(root, &file))
        .collect();
    Ok(FileListResult {
        files,
        has_errors: has_errors.load(Ordering::Relaxed),
    })
}

#[napi]
pub fn search_content_advanced(
    pattern: String,
    search_path: String,
    globs: Option<Vec<String>>,
    include_hidden: Option<bool>,
    follow_links: Option<bool>,
    max_depth: Option<i32>,
    max_results: Option<i32>,
    max_line_length: Option<i32>,
) -> Result<SearchResult> {
    let root = Path::new(&search_path);
    let regex = Regex::new(&pattern).map_err(|e| napi::Error::from_reason(e.to_string()))?;
    let max_line = max_line_length.unwrap_or(2000).max(1) as usize;
    let hidden = include_hidden.unwrap_or(true);
    let follow = follow_links.unwrap_or(false);
    let depth = max_depth.map(|d| d.max(0) as usize);
    let has_errors = AtomicBool::new(false);
    let files = collect_files(root, globs.as_ref(), hidden, follow, depth, &has_errors)?;

    let mut matches: Vec<SearchMatch> = files
        .par_iter()
        .flat_map(|(file, mod_time)| {
            let bytes = match std::fs::read(file) {
                Ok(v) => v,
                Err(_) => {
                    has_errors.store(true, Ordering::Relaxed);
                    return Vec::new();
                }
            };
            if is_binary(&bytes) {
                return Vec::new();
            }
            let text = String::from_utf8_lossy(&bytes);
            text.lines()
                .enumerate()
                .filter_map(|(index, line)| {
                    if !regex.is_match(line) {
                        return None;
                    }
                    let line_text = if line.chars().count() > max_line {
                        line.chars().take(max_line).collect::<String>() + "..."
                    } else {
                        line.to_string()
                    };
                    Some(SearchMatch {
                        path: normalize_relative(root, file),
                        mod_time: *mod_time,
                        line_num: (index + 1) as i32,
                        line_text,
                    })
                })
                .collect::<Vec<_>>()
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

    Ok(SearchResult {
        matches,
        has_errors: has_errors.load(Ordering::Relaxed),
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
            has_errors.store(true, Ordering::Relaxed);
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
        if has_include && !include_matchers.iter().any(|m| m.is_match(&rel_text)) {
            continue;
        }
        if exclude_matchers.iter().any(|m| m.is_match(&rel_text)) {
            continue;
        }
        let mod_time = std::fs::metadata(entry_path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
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
            .map_err(|e| napi::Error::from_reason(e.to_string()))?
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
    path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .map(|s| s.starts_with('.'))
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
