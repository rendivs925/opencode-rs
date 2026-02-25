use dashmap::DashMap;
use globset::{GlobBuilder, GlobMatcher};
use ignore::WalkBuilder;
use napi::Result;
use rayon::prelude::*;
use std::path::Path;
use std::sync::{Arc, OnceLock};

const CACHE_LIMIT: usize = 256;

static GLOB_CACHE: OnceLock<DashMap<String, Arc<GlobMatcher>>> = OnceLock::new();

#[napi]
pub fn glob_match_compiled(pattern: String, filepath: String) -> Result<bool> {
    let matcher = compiled_matcher(&pattern)?;
    Ok(matcher.is_match(filepath.replace('\\', "/")))
}

#[napi]
pub fn glob_scan_compiled(
    pattern: String,
    cwd: String,
    max_depth: Option<u32>,
    include_hidden: bool,
    follow_links: Option<bool>,
    include_all: Option<bool>,
    absolute: Option<bool>,
    parallel: Option<bool>,
) -> Result<Vec<String>> {
    let cwd_path = Path::new(&cwd);
    let include_all = include_all.unwrap_or(false);
    let absolute = absolute.unwrap_or(false);
    let matcher = compiled_matcher(&pattern)?;

    let mut walker = WalkBuilder::new(cwd_path);
    walker
        .hidden(!include_hidden)
        .max_depth(max_depth.map(|item| item as usize))
        .follow_links(follow_links.unwrap_or(false))
        .require_git(false)
        .filter_entry(move |entry| {
            let path = entry.path();
            !path
                .file_name()
                .map(|item| item.to_string_lossy().starts_with('.'))
                .unwrap_or(false)
                || include_hidden
        });

    let to_output = |path: &Path| {
        if absolute {
            return path.to_string_lossy().to_string();
        }
        path.strip_prefix(cwd_path)
            .ok()
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    };

    if !parallel.unwrap_or(false) {
        let out = walker
            .build()
            .into_iter()
            .filter_map(|item| item.ok())
            .filter_map(|entry| {
                let path = entry.path();
                if path == cwd_path {
                    return None;
                }
                if !include_all && !entry.file_type().map(|item| item.is_file()).unwrap_or(false) {
                    return None;
                }
                let rel = path.strip_prefix(cwd_path).ok().unwrap_or(path);
                if !matcher.is_match(rel) {
                    return None;
                }
                Some(to_output(path))
            })
            .collect();
        return Ok(out);
    }

    let entries: Vec<_> = walker.build().into_iter().filter_map(|item| item.ok()).collect();
    let out = entries
        .par_iter()
        .filter_map(|entry| {
            let path = entry.path();
            if path == cwd_path {
                return None;
            }
            if !include_all && !entry.file_type().map(|item| item.is_file()).unwrap_or(false) {
                return None;
            }
            let rel = path.strip_prefix(cwd_path).ok().unwrap_or(path);
            if !matcher.is_match(rel) {
                return None;
            }
            Some(to_output(path))
        })
        .collect();
    Ok(out)
}

#[napi]
pub fn glob_compiled_cache_size() -> u32 {
    let cache = GLOB_CACHE.get_or_init(DashMap::new);
    cache.len() as u32
}

#[napi]
pub fn glob_compiled_cache_clear() {
    let cache = GLOB_CACHE.get_or_init(DashMap::new);
    cache.clear();
}

fn compiled_matcher(pattern: &str) -> Result<Arc<GlobMatcher>> {
    let cache = GLOB_CACHE.get_or_init(DashMap::new);
    if let Some(item) = cache.get(pattern) {
        return Ok(Arc::clone(item.value()));
    }

    let matcher = Arc::new(
        GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .map_err(|err| napi::Error::from_reason(err.to_string()))?
            .compile_matcher(),
    );
    if cache.len() >= CACHE_LIMIT {
        if let Some(first) = cache.iter().next() {
            let key = first.key().clone();
            cache.remove(&key);
        }
    }
    cache.insert(pattern.to_string(), Arc::clone(&matcher));
    Ok(matcher)
}
