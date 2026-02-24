use globset::GlobBuilder;
use ignore::WalkBuilder;
use napi::Result;
use rayon::prelude::*;
use std::path::Path;

#[napi]
pub fn glob(
    pattern: String,
    cwd: String,
    max_depth: Option<u32>,
    include_hidden: bool,
    follow_links: Option<bool>,
) -> Result<Vec<String>> {
    let cwd_path = Path::new(&cwd);
    let mut walker = WalkBuilder::new(cwd_path);

    walker
        .hidden(!include_hidden)
        .max_depth(max_depth.map(|d| d as usize))
        .follow_links(follow_links.unwrap_or(false))
        .require_git(false)
        .filter_entry(move |entry| {
            let path = entry.path();
            !path
                .file_name()
                .map(|f| f.to_string_lossy().starts_with('.'))
                .unwrap_or(false)
                || include_hidden
        });

    let matcher = GlobBuilder::new(&pattern)
        .literal_separator(true)
        .build()
        .map_err(|e| napi::Error::from_reason(e.to_string()))?
        .compile_matcher();

    let results: Vec<String> = walker
        .build()
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            if path == cwd_path {
                return None;
            }
            let rel = path.strip_prefix(cwd_path).ok().unwrap_or(path);
            if matcher.is_match(rel) {
                return Some(path.to_string_lossy().to_string());
            }
            None
        })
        .collect();

    Ok(results)
}

#[napi]
pub fn glob_parallel(
    pattern: String,
    cwd: String,
    max_depth: Option<u32>,
    include_hidden: bool,
    follow_links: Option<bool>,
) -> Result<Vec<String>> {
    let cwd_path = Path::new(&cwd);
    let entries: Vec<_> = WalkBuilder::new(cwd_path)
        .hidden(!include_hidden)
        .max_depth(max_depth.map(|d| d as usize))
        .follow_links(follow_links.unwrap_or(false))
        .require_git(false)
        .build()
        .into_iter()
        .filter_map(|e| e.ok())
        .collect();

    let matcher = GlobBuilder::new(&pattern)
        .literal_separator(true)
        .build()
        .map_err(|e| napi::Error::from_reason(e.to_string()))?
        .compile_matcher();

    let filtered: Vec<String> = entries
        .par_iter()
        .filter_map(|e| {
            let path = e.path();
            if path == cwd_path {
                return None;
            }
            let rel = path.strip_prefix(cwd_path).ok().unwrap_or(path);
            if matcher.is_match(rel) {
                return Some(path.to_string_lossy().to_string());
            }
            None
        })
        .collect();

    Ok(filtered)
}
