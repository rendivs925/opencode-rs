use napi::Result;
use std::path::{Component, Path, PathBuf};

#[napi]
pub fn contains_path(parent: String, child: String) -> Result<bool> {
    let parent = resolve_path(Path::new(&parent))?;
    let child = resolve_path(Path::new(&child))?;
    Ok(child.starts_with(&parent))
}

#[napi]
pub fn overlaps_path(a: String, b: String) -> Result<bool> {
    let a = resolve_path(Path::new(&a))?;
    let b = resolve_path(Path::new(&b))?;
    Ok(a.starts_with(&b) || b.starts_with(&a))
}

fn resolve_path(path: &Path) -> Result<PathBuf> {
    let absolute = to_absolute(path)?;
    if let Ok(canonical) = std::fs::canonicalize(&absolute) {
        return Ok(canonical);
    }

    let mut probe = absolute.clone();
    let mut missing = Vec::new();
    while !probe.exists() {
        let Some(name) = probe.file_name() else {
            break;
        };
        missing.push(name.to_os_string());
        if !probe.pop() {
            break;
        }
    }

    if probe.exists() {
        if let Ok(mut canonical) = std::fs::canonicalize(&probe) {
            for part in missing.iter().rev() {
                canonical.push(part);
            }
            return Ok(clean_path(&canonical));
        }
    }

    Ok(clean_path(&absolute))
}

fn to_absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let cwd = std::env::current_dir().map_err(|e| napi::Error::from_reason(e.to_string()))?;
    Ok(cwd.join(path))
}

fn clean_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            Component::RootDir => out.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
        }
    }
    out
}
