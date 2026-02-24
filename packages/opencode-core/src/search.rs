use globset::GlobBuilder;
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

#[napi]
pub fn search_content(
    pattern: String,
    search_path: String,
    include: Option<String>,
    max_results: Option<i32>,
    max_line_length: Option<i32>,
) -> Result<SearchResult> {
    let path = Path::new(&search_path);
    let regex = Regex::new(&pattern).map_err(|e| napi::Error::from_reason(e.to_string()))?;
    let max_line = max_line_length.unwrap_or(2000).max(1) as usize;
    let include_matcher = include
        .as_ref()
        .map(|glob| {
            GlobBuilder::new(glob)
                .literal_separator(true)
                .build()
                .map(|g| g.compile_matcher())
                .map_err(|e| napi::Error::from_reason(e.to_string()))
        })
        .transpose()?;

    let has_errors = AtomicBool::new(false);
    let mut files: Vec<(PathBuf, i64)> = Vec::new();
    let mut walker = WalkBuilder::new(path);
    walker.hidden(false).follow_links(false).require_git(false);

    for entry in walker.build() {
        let Ok(entry) = entry else {
            has_errors.store(true, Ordering::Relaxed);
            continue;
        };
        let entry_path = entry.path();
        if !entry_path.is_file() {
            continue;
        }
        if let Some(matcher) = &include_matcher {
            let rel = entry_path.strip_prefix(path).ok().unwrap_or(entry_path);
            let file_name = entry_path.file_name();
            let matches = matcher.is_match(rel)
                || file_name
                    .map(|f| matcher.is_match(Path::new(f)))
                    .unwrap_or(false);
            if !matches {
                continue;
            }
        }
        let mod_time = std::fs::metadata(entry_path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        files.push((entry_path.to_path_buf(), mod_time));
    }

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
                        path: file.to_string_lossy().to_string(),
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

fn is_binary(bytes: &[u8]) -> bool {
    let len = bytes.len().min(8192);
    bytes[..len].contains(&0)
}
