use ignore::gitignore::{Gitignore, GitignoreBuilder};
use napi::Result;
use std::path::{Path, PathBuf};

#[napi(object)]
#[derive(Clone)]
pub struct ListEntry {
    pub name: String,
    pub path: String,
    pub absolute: String,
    pub entry_type: String,
    pub ignored: bool,
}

#[napi]
pub fn list_directory(
    dir: String,
    base: String,
    exclude: Option<Vec<String>>,
    ignore_patterns: Option<Vec<String>>,
) -> Result<Vec<ListEntry>> {
    let dir_path = Path::new(&dir);
    let base_path = Path::new(&base);
    let exclude = exclude.unwrap_or_default();
    let matcher = build_matcher(ignore_patterns.unwrap_or_default().as_slice())?;

    let mut nodes = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir_path) else {
        return Ok(nodes);
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if exclude.iter().any(|item| item == &name) {
            continue;
        }

        let absolute_path = entry.path();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let rel = relative_path(base_path, &absolute_path);
        let ignored = matcher
            .as_ref()
            .map(|m| m.matched(Path::new(&rel), is_dir).is_ignore())
            .unwrap_or(false);

        nodes.push(ListEntry {
            name,
            path: rel,
            absolute: normalize(&absolute_path),
            entry_type: if is_dir {
                "directory".to_string()
            } else {
                "file".to_string()
            },
            ignored,
        });
    }

    nodes.sort_by(|a, b| {
        if a.entry_type != b.entry_type {
            if a.entry_type == "directory" {
                return std::cmp::Ordering::Less;
            }
            return std::cmp::Ordering::Greater;
        }
        a.name.cmp(&b.name)
    });

    Ok(nodes)
}

#[napi]
pub fn list_directory_project(
    dir: String,
    base: String,
    worktree: String,
    exclude: Option<Vec<String>>,
) -> Result<Vec<ListEntry>> {
    let mut patterns = Vec::new();
    for file in [".gitignore", ".ignore"] {
        let text = std::fs::read_to_string(Path::new(&worktree).join(file)).unwrap_or_default();
        if text.is_empty() {
            continue;
        }
        patterns.extend(text.lines().map(|line| line.to_string()));
    }
    list_directory(dir, base, exclude, Some(patterns))
}

fn build_matcher(patterns: &[String]) -> Result<Option<Gitignore>> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GitignoreBuilder::new("");
    for pattern in patterns {
        builder
            .add_line(None, pattern)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    }
    let matcher = builder
        .build()
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    Ok(Some(matcher))
}

fn relative_path(base: &Path, full: &Path) -> String {
    let rel: PathBuf = full.strip_prefix(base).map(|p| p.to_path_buf()).unwrap_or_else(|_| full.to_path_buf());
    normalize(&rel)
}

fn normalize(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
