use ignore::gitignore::Gitignore;
use napi::Result;
use rayon::prelude::*;
use std::path::Path;

#[napi]
pub fn is_ignored(path: String, patterns: Vec<String>) -> Result<bool> {
    let path = Path::new(&path);

    for pattern in patterns {
        let (matcher, _) = Gitignore::new(&pattern);
        if matcher.matched(path, path.is_dir()).is_ignore() {
            return Ok(true);
        }
    }

    Ok(false)
}

#[napi]
pub fn filter_paths(paths: Vec<String>, patterns: Vec<String>) -> Result<Vec<String>> {
    let compiled: Vec<_> = patterns.iter().map(|p| Gitignore::new(p)).collect();

    let filtered: Vec<String> = paths
        .par_iter()
        .filter(|path| {
            let p = Path::new(path);
            let is_dir = p.is_dir();
            !compiled
                .iter()
                .any(|(m, _)| m.matched(p, is_dir).is_ignore())
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
        for pattern in &self.patterns {
            let (matcher, _) = Gitignore::new(pattern);
            if matcher.matched(path, path.is_dir()).is_ignore() {
                return true;
            }
        }
        false
    }

    #[napi]
    pub fn filter(&self, paths: Vec<String>) -> Vec<String> {
        paths
            .into_iter()
            .filter(|p| !self.is_ignored(p.clone()))
            .collect()
    }
}
