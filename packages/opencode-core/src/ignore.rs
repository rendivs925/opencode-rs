use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use napi::Result;
use rayon::prelude::*;
use std::path::Path;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[napi]
pub fn is_ignored(path: String, patterns: Vec<String>) -> Result<bool> {
    let path = Path::new(&path);
    let matcher = cached_matcher(&patterns)?;
    Ok(matcher.matched(path, false).is_ignore())
}

#[napi]
pub fn filter_paths(paths: Vec<String>, patterns: Vec<String>) -> Result<Vec<String>> {
    let matcher = cached_matcher(&patterns)?;

    let filtered: Vec<String> = paths
        .par_iter()
        .filter(|path| {
            let p = Path::new(path);
            !matcher.matched(p, false).is_ignore()
        })
        .cloned()
        .collect();

    Ok(filtered)
}

#[napi]
pub fn compile_patterns(patterns: Vec<String>) -> Result<CompiledIgnore> {
    Ok(CompiledIgnore { patterns })
}

#[napi]
pub struct CompiledIgnore {
    patterns: Vec<String>,
}

#[napi]
impl CompiledIgnore {
    #[napi]
    pub fn is_ignored(&self, path: String) -> bool {
        self.is_ignored_with(path, None, None)
    }

    #[napi]
    pub fn filter(&self, paths: Vec<String>) -> Vec<String> {
        self.filter_with(paths, None, None)
    }

    #[napi]
    pub fn is_ignored_with(
        &self,
        path: String,
        extra_patterns: Option<Vec<String>>,
        whitelist: Option<Vec<String>>,
    ) -> bool {
        if is_whitelisted(&path, whitelist.as_ref()).unwrap_or(false) {
            return false;
        }
        let patterns = merge_patterns(&self.patterns, extra_patterns.as_ref());
        cached_matcher(&patterns)
            .map(|m| m.matched(Path::new(&path), false).is_ignore())
            .unwrap_or(false)
    }

    #[napi]
    pub fn filter_with(
        &self,
        paths: Vec<String>,
        extra_patterns: Option<Vec<String>>,
        whitelist: Option<Vec<String>>,
    ) -> Vec<String> {
        let patterns = merge_patterns(&self.patterns, extra_patterns.as_ref());
        let matcher = cached_matcher(&patterns).ok();
        let whitelist_set = cached_globset(whitelist.as_ref()).ok();

        paths
            .into_iter()
            .filter(|item| {
                if let Some(set) = whitelist_set.as_ref() {
                    if set.is_match(item) {
                        return true;
                    }
                }
                if let Some(m) = matcher.as_ref() {
                    return !m.matched(Path::new(item), false).is_ignore();
                }
                true
            })
            .collect()
    }
}

fn build_matcher(patterns: &[String]) -> Result<Gitignore> {
    let mut builder = GitignoreBuilder::new("");
    for pattern in patterns {
        builder
            .add_line(None, pattern)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    }
    let matcher = builder
        .build()
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    Ok(matcher)
}

fn merge_patterns(base: &[String], extra: Option<&Vec<String>>) -> Vec<String> {
    let mut out = Vec::with_capacity(base.len() + extra.map_or(0, Vec::len));
    out.extend(base.iter().cloned());
    if let Some(extra) = extra {
        out.extend(extra.iter().cloned());
    }
    out
}

fn key(parts: &[String]) -> String {
    parts.join("\u{1f}")
}

fn cached_matcher(patterns: &[String]) -> Result<Arc<Gitignore>> {
    let cache = IGNORE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let k = key(patterns);
    if let Ok(guard) = cache.lock() {
        if let Some(item) = guard.get(&k) {
            return Ok(item.clone());
        }
    }

    let built = Arc::new(build_matcher(patterns)?);
    if let Ok(mut guard) = cache.lock() {
        guard.insert(k, built.clone());
    }
    Ok(built)
}

fn cached_globset(patterns: Option<&Vec<String>>) -> Result<Arc<GlobSet>> {
    let Some(patterns) = patterns else {
        return Ok(Arc::new(GlobSetBuilder::new().build().map_err(|e| napi::Error::from_reason(e.to_string()))?));
    };
    if patterns.is_empty() {
        return Ok(Arc::new(GlobSetBuilder::new().build().map_err(|e| napi::Error::from_reason(e.to_string()))?));
    }

    let cache = GLOB_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let k = key(patterns);
    if let Ok(guard) = cache.lock() {
        if let Some(item) = guard.get(&k) {
            return Ok(item.clone());
        }
    }

    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|e| napi::Error::from_reason(e.to_string()))?;
        builder.add(glob);
    }
    let set = Arc::new(builder.build().map_err(|e| napi::Error::from_reason(e.to_string()))?);
    if let Ok(mut guard) = cache.lock() {
        guard.insert(k, set.clone());
    }
    Ok(set)
}

fn is_whitelisted(path: &str, patterns: Option<&Vec<String>>) -> Result<bool> {
    if patterns.is_none() {
        return Ok(false);
    }
    let set = cached_globset(patterns)?;
    Ok(set.is_match(path))
}

static IGNORE_CACHE: OnceLock<Mutex<HashMap<String, Arc<Gitignore>>>> = OnceLock::new();
static GLOB_CACHE: OnceLock<Mutex<HashMap<String, Arc<GlobSet>>>> = OnceLock::new();
