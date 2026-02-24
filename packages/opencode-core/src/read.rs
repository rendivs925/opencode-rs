use napi::Result;
use std::path::Path;

const BINARY_EXTENSIONS: &[&str] = &[
    "zip", "tar", "gz", "exe", "dll", "so", "class", "jar", "war", "7z", "doc", "docx", "xls",
    "xlsx", "ppt", "pptx", "odt", "ods", "odp", "bin", "dat", "obj", "o", "a", "lib", "wasm",
    "pyc", "pyo",
];

#[napi(object)]
#[derive(Clone)]
pub struct FileWindow {
    pub lines: Vec<String>,
    pub total_lines: i32,
    pub truncated: bool,
    pub truncated_by_bytes: bool,
    pub next_offset: i32,
}

#[napi(object)]
#[derive(Clone)]
pub struct DirWindow {
    pub entries: Vec<String>,
    pub total_entries: i32,
    pub truncated: bool,
    pub next_offset: i32,
}

#[napi]
pub fn read_file_window(
    path: String,
    offset: Option<i32>,
    limit: Option<i32>,
    max_bytes: Option<i32>,
    max_line_length: Option<i32>,
) -> Result<FileWindow> {
    let path = Path::new(&path);
    let offset = offset.unwrap_or(1);
    let limit = limit.unwrap_or(2000).max(1) as usize;
    let max_bytes = max_bytes.unwrap_or(50 * 1024).max(1) as usize;
    let max_line_length = max_line_length.unwrap_or(2000).max(1) as usize;

    if offset < 1 {
        return Err(napi::Error::from_reason(
            "offset must be greater than or equal to 1",
        ));
    }

    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    if BINARY_EXTENSIONS.contains(&ext.as_str()) {
        return Err(napi::Error::from_reason(format!(
            "Cannot read binary file: {}",
            path.display()
        )));
    }

    let bytes = std::fs::read(path).map_err(|e| napi::Error::from_reason(e.to_string()))?;
    let sample_len = bytes.len().min(4096);
    if bytes[..sample_len].contains(&0) {
        return Err(napi::Error::from_reason(format!(
            "Cannot read binary file: {}",
            path.display()
        )));
    }

    let text = String::from_utf8_lossy(&bytes);
    let all: Vec<&str> = text.lines().collect();
    let total_lines = all.len() as i32;
    let start = (offset - 1) as usize;

    if all.len() < offset as usize && !(all.is_empty() && offset == 1) {
        return Err(napi::Error::from_reason(format!(
            "Offset {} is out of range for this file ({} lines)",
            offset, total_lines
        )));
    }

    let mut lines = Vec::new();
    let mut bytes_used = 0usize;
    let mut truncated_by_bytes = false;
    let mut has_more_lines = false;

    for line in all.iter().skip(start) {
        if lines.len() >= limit {
            has_more_lines = true;
            break;
        }
        let mut value = (*line).to_string();
        if value.chars().count() > max_line_length {
            value = value.chars().take(max_line_length).collect::<String>()
                + &format!("... (line truncated to {} chars)", max_line_length);
        }
        let size = value.as_bytes().len() + usize::from(!lines.is_empty());
        if bytes_used + size > max_bytes {
            truncated_by_bytes = true;
            has_more_lines = true;
            break;
        }
        bytes_used += size;
        lines.push(value);
    }

    let next_offset = offset + lines.len() as i32;
    Ok(FileWindow {
        lines,
        total_lines,
        truncated: has_more_lines || truncated_by_bytes,
        truncated_by_bytes,
        next_offset,
    })
}

#[napi]
pub fn read_dir_window(path: String, offset: Option<i32>, limit: Option<i32>) -> Result<DirWindow> {
    let path = Path::new(&path);
    let offset = offset.unwrap_or(1);
    let limit = limit.unwrap_or(2000).max(1) as usize;
    if offset < 1 {
        return Err(napi::Error::from_reason(
            "offset must be greater than or equal to 1",
        ));
    }

    let mut entries = std::fs::read_dir(path)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?
        .filter_map(|item| item.ok())
        .map(|item| {
            let name = item.file_name().to_string_lossy().to_string();
            let file_type = item.file_type().ok();
            let is_dir = file_type.map(|t| t.is_dir()).unwrap_or(false)
                || file_type
                    .map(|t| t.is_symlink())
                    .unwrap_or(false)
                    && std::fs::metadata(item.path())
                        .map(|m| m.is_dir())
                        .unwrap_or(false);
            if is_dir {
                return format!("{name}/");
            }
            name
        })
        .collect::<Vec<_>>();
    entries.sort();

    let start = (offset - 1) as usize;
    let total_entries = entries.len() as i32;
    let window = entries
        .into_iter()
        .skip(start)
        .take(limit)
        .collect::<Vec<_>>();
    let truncated = start + window.len() < total_entries as usize;
    let next_offset = offset + window.len() as i32;

    Ok(DirWindow {
        entries: window,
        total_entries,
        truncated,
        next_offset,
    })
}
