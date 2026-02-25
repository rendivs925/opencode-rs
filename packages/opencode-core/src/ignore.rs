use dashmap::DashMap;
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use napi::Result;
use rayon::prelude::*;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::sync::atomic::{AtomicU64, Ordering};

const CACHE_LIMIT: usize = 512;
static CACHE_TICK: AtomicU64 = AtomicU64::new(1);

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
    let cache = IGNORE_CACHE.get_or_init(DashMap::new);
    let k = key(patterns);
    if let Some(mut item) = cache.get_mut(&k) {
        let (matcher, used) = item.value_mut();
        *used = CACHE_TICK.fetch_add(1, Ordering::Relaxed);
        return Ok(Arc::clone(matcher));
    }

    let built = Arc::new(build_matcher(patterns)?);
    if cache.len() >= CACHE_LIMIT {
        evict_lru_ignore(cache);
    }
    cache.insert(k, (Arc::clone(&built), CACHE_TICK.fetch_add(1, Ordering::Relaxed)));
    Ok(built)
}

fn cached_globset(patterns: Option<&Vec<String>>) -> Result<Arc<GlobSet>> {
    let Some(patterns) = patterns else {
        return Ok(Arc::new(GlobSetBuilder::new().build().map_err(|e| napi::Error::from_reason(e.to_string()))?));
    };
    if patterns.is_empty() {
        return Ok(Arc::new(GlobSetBuilder::new().build().map_err(|e| napi::Error::from_reason(e.to_string()))?));
    }

    let cache = GLOB_CACHE.get_or_init(DashMap::new);
    let k = key(patterns);
    if let Some(mut item) = cache.get_mut(&k) {
        let (set, used) = item.value_mut();
        *used = CACHE_TICK.fetch_add(1, Ordering::Relaxed);
        return Ok(Arc::clone(set));
    }

    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|e| napi::Error::from_reason(e.to_string()))?;
        builder.add(glob);
    }
    let set = Arc::new(builder.build().map_err(|e| napi::Error::from_reason(e.to_string()))?);
    if cache.len() >= CACHE_LIMIT {
        evict_lru_glob(cache);
    }
    cache.insert(k, (Arc::clone(&set), CACHE_TICK.fetch_add(1, Ordering::Relaxed)));
    Ok(set)
}

fn is_whitelisted(path: &str, patterns: Option<&Vec<String>>) -> Result<bool> {
    if patterns.is_none() {
        return Ok(false);
    }
    let set = cached_globset(patterns)?;
    Ok(set.is_match(path))
}

fn evict_lru_ignore(cache: &DashMap<String, (Arc<Gitignore>, u64)>) {
    if let Some(item) = cache.iter().min_by_key(|item| item.value().1) {
        let key = item.key().clone();
        drop(item);
        cache.remove(&key);
    }
}

fn evict_lru_glob(cache: &DashMap<String, (Arc<GlobSet>, u64)>) {
    if let Some(item) = cache.iter().min_by_key(|item| item.value().1) {
        let key = item.key().clone();
        drop(item);
        cache.remove(&key);
    }
}

static IGNORE_CACHE: OnceLock<DashMap<String, (Arc<Gitignore>, u64)>> = OnceLock::new();
static GLOB_CACHE: OnceLock<DashMap<String, (Arc<GlobSet>, u64)>> = OnceLock::new();
