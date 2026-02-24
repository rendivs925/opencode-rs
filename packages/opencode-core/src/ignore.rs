use ignore::gitignore::{Gitignore, GitignoreBuilder};
use napi::Result;
use rayon::prelude::*;
use std::path::Path;

#[napi]
pub fn is_ignored(path: String, patterns: Vec<String>) -> Result<bool> {
    let path = Path::new(&path);
    let matcher = build_matcher(&patterns)?;
    Ok(matcher.matched(path, path.is_dir()).is_ignore())
}

#[napi]
pub fn filter_paths(paths: Vec<String>, patterns: Vec<String>) -> Result<Vec<String>> {
    let matcher = build_matcher(&patterns)?;

    let filtered: Vec<String> = paths
        .par_iter()
        .filter(|path| {
            let p = Path::new(path);
            let is_dir = p.is_dir();
            !matcher.matched(p, is_dir).is_ignore()
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
        let path = Path::new(&path);
        let matcher = build_matcher(&self.patterns);
        if matcher.is_err() {
            return false;
        }
        matcher
            .ok()
            .map(|m| m.matched(path, path.is_dir()).is_ignore())
            .unwrap_or(false)
    }

    #[napi]
    pub fn filter(&self, paths: Vec<String>) -> Vec<String> {
        paths
            .into_iter()
            .filter(|p| !self.is_ignored(p.clone()))
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
