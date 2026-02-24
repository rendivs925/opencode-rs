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
) -> Result<Vec<String>> {
    let cwd_path = Path::new(&cwd);
    let mut walker = WalkBuilder::new(cwd_path);

    walker
        .hidden(!include_hidden)
        .max_depth(max_depth.map(|d| d as usize))
        .require_git(false)
        .filter_entry(move |entry| {
            let path = entry.path();
            !path
                .file_name()
                .map(|f| f.to_string_lossy().starts_with('.'))
                .unwrap_or(false)
                || include_hidden
        });

    let pattern_path = Path::new(&pattern);
    let is_glob_pattern = pattern.contains('*') || pattern.contains('?');

    let results: Vec<String> = if is_glob_pattern {
        walker
            .build()
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| matches_glob_pattern(pattern_path, e.path()))
            .map(|e| e.path().to_string_lossy().to_string())
            .collect()
    } else {
        walker
            .build()
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().to_string_lossy().contains(&pattern))
            .map(|e| e.path().to_string_lossy().to_string())
            .collect()
    };

    Ok(results)
}

fn matches_glob_pattern(pattern: &Path, path: &Path) -> bool {
    let pattern_str = pattern.to_string_lossy();
    let path_str = path.to_string_lossy();

    if pattern_str.contains("**") {
        let base = pattern_str.split("**").next().unwrap_or("");
        path_str.contains(&base.replace('/', ""))
    } else {
        let file_name = path
            .file_name()
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();
        glob_match(
            &pattern_str.replace("/*", "").replace("/*/", ""),
            &file_name,
        )
    }
}

fn glob_match(pattern: &str, text: &str) -> bool {
    let mut pattern_chars = pattern.chars().peekable();
    let mut text_chars = text.chars().peekable();

    while pattern_chars.peek().is_some() || text_chars.peek().is_some() {
        match (pattern_chars.peek(), text_chars.peek()) {
            (Some('*'), _) => {
                pattern_chars.next();
                if pattern_chars.peek().is_none() {
                    return true;
                }
                while text_chars.peek().is_some() {
                    if glob_match(
                        &pattern_chars.clone().collect::<String>(),
                        &text_chars.clone().collect::<String>(),
                    ) {
                        return true;
                    }
                    text_chars.next();
                }
                return false;
            }
            (Some('?'), Some(_)) => {
                pattern_chars.next();
                text_chars.next();
            }
            (Some(p), Some(t)) if p == t => {
                pattern_chars.next();
                text_chars.next();
            }
            (None, None) => return true,
            _ => return false,
        }
    }
    true
}

#[napi]
pub fn glob_parallel(
    pattern: String,
    cwd: String,
    max_depth: Option<u32>,
    include_hidden: bool,
) -> Result<Vec<String>> {
    let cwd_path = Path::new(&cwd);
    let entries: Vec<_> = WalkBuilder::new(cwd_path)
        .hidden(!include_hidden)
        .max_depth(max_depth.map(|d| d as usize))
        .require_git(false)
        .build()
        .into_iter()
        .filter_map(|e| e.ok())
        .collect();

    let pattern_path = Path::new(&pattern);
    let filtered: Vec<String> = entries
        .par_iter()
        .filter(|e| matches_glob_pattern(pattern_path, e.path()))
        .map(|e| e.path().to_string_lossy().to_string())
        .collect();

    Ok(filtered)
}
