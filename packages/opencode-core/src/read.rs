use base64::Engine;
use napi::Result;
use std::path::Path;

const BINARY_EXTENSIONS: &[&str] = &[
    "zip", "tar", "gz", "exe", "dll", "so", "class", "jar", "war", "7z", "doc", "docx", "xls",
    "xlsx", "ppt", "pptx", "odt", "ods", "odp", "bin", "dat", "obj", "o", "a", "lib", "wasm",
    "pyc", "pyo",
];
const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "ico", "tif", "tiff", "svg", "svgz", "avif",
    "apng", "jxl", "heic", "heif", "raw", "cr2", "nef", "arw", "dng", "orf", "raf", "pef",
    "x3f",
];
const TEXT_EXTENSIONS: &[&str] = &[
    "ts", "tsx", "mts", "cts", "mtsx", "ctsx", "js", "jsx", "mjs", "cjs", "sh", "bash", "zsh",
    "fish", "ps1", "psm1", "cmd", "bat", "json", "jsonc", "json5", "yaml", "yml", "toml", "md",
    "mdx", "txt", "xml", "html", "htm", "css", "scss", "sass", "less", "graphql", "gql", "sql",
    "ini", "cfg", "conf", "env",
];
const TEXT_NAMES: &[&str] = &[
    "dockerfile",
    "makefile",
    ".gitignore",
    ".gitattributes",
    ".editorconfig",
    ".npmrc",
    ".nvmrc",
    ".prettierrc",
    ".eslintrc",
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

#[napi(object)]
#[derive(Clone)]
pub struct ReadTargetClassification {
    pub mode: String,
    pub exists: bool,
    pub mime_type: Option<String>,
}

#[napi(object)]
#[derive(Clone)]
pub struct ReadAttachment {
    pub is_attachment: bool,
    pub mime_type: Option<String>,
    pub base64: Option<String>,
}

#[napi]
pub fn classify_read_target(path: String, hint_path: Option<String>) -> Result<ReadTargetClassification> {
    let path_obj = Path::new(&path);
    let hint = hint_path.unwrap_or(path.clone());
    let hint_obj = Path::new(&hint);
    let exists = path_obj.exists();

    if is_image_by_extension(hint_obj) {
        if !exists {
            return Ok(ReadTargetClassification {
                mode: "text".to_string(),
                exists,
                mime_type: None,
            });
        }
        return Ok(ReadTargetClassification {
            mode: "base64".to_string(),
            exists,
            mime_type: Some(image_mime_type(hint_obj)),
        });
    }

    let text = is_text_by_extension(hint_obj) || is_text_by_name(hint_obj);
    if is_binary_by_extension(hint_obj) && !text {
        return Ok(ReadTargetClassification {
            mode: "binary".to_string(),
            exists,
            mime_type: None,
        });
    }

    if !exists {
        return Ok(ReadTargetClassification {
            mode: "text".to_string(),
            exists,
            mime_type: None,
        });
    }

    let mime = mime_type(path_obj);
    let encode = if text { false } else { should_encode(&mime) };

    if encode && !mime.starts_with("image/") {
        return Ok(ReadTargetClassification {
            mode: "binary".to_string(),
            exists,
            mime_type: Some(mime),
        });
    }

    if encode {
        return Ok(ReadTargetClassification {
            mode: "base64".to_string(),
            exists,
            mime_type: Some(mime),
        });
    }

    Ok(ReadTargetClassification {
        mode: "text".to_string(),
        exists,
        mime_type: None,
    })
}

#[napi]
pub fn read_attachment(path: String) -> Result<ReadAttachment> {
    let path_obj = Path::new(&path);
    if !path_obj.exists() {
        return Ok(ReadAttachment {
            is_attachment: false,
            mime_type: None,
            base64: None,
        });
    }

    let mime = mime_type(path_obj);
    let image = mime.starts_with("image/") && mime != "image/svg+xml" && mime != "image/vnd.fastbidsheet";
    let pdf = mime == "application/pdf";
    if !image && !pdf {
        return Ok(ReadAttachment {
            is_attachment: false,
            mime_type: None,
            base64: None,
        });
    }

    let bytes = std::fs::read(path_obj).map_err(|e| napi::Error::from_reason(e.to_string()))?;
    Ok(ReadAttachment {
        is_attachment: true,
        mime_type: Some(mime),
        base64: Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
    })
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

fn ext(path: &Path) -> String {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default()
}

fn base(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default()
}

fn is_binary_by_extension(path: &Path) -> bool {
    BINARY_EXTENSIONS.contains(&ext(path).as_str())
}

fn is_image_by_extension(path: &Path) -> bool {
    IMAGE_EXTENSIONS.contains(&ext(path).as_str())
}

fn is_text_by_extension(path: &Path) -> bool {
    TEXT_EXTENSIONS.contains(&ext(path).as_str())
}

fn is_text_by_name(path: &Path) -> bool {
    TEXT_NAMES.contains(&base(path).as_str())
}

fn image_mime_type(path: &Path) -> String {
    let mime = match ext(path).as_str() {
        "png" => "image/png",
        "jpg" => "image/jpeg",
        "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "tif" => "image/tiff",
        "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        "svgz" => "image/svg+xml",
        "avif" => "image/avif",
        "apng" => "image/apng",
        "jxl" => "image/jxl",
        "heic" => "image/heic",
        "heif" => "image/heif",
        _ => "application/octet-stream",
    };
    mime.to_string()
}

fn mime_type(path: &Path) -> String {
    let mime = match ext(path).as_str() {
        "json" => "application/json",
        "pdf" => "application/pdf",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" | "cjs" => "text/javascript",
        "ts" | "tsx" | "jsx" | "md" | "mdx" | "txt" | "yaml" | "yml" | "toml" | "ini" | "cfg"
        | "conf" | "env" | "sql" | "graphql" | "gql" => "text/plain",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "tif" | "tiff" => "image/tiff",
        "svg" | "svgz" => "image/svg+xml",
        "avif" => "image/avif",
        "apng" => "image/apng",
        "jxl" => "image/jxl",
        "heic" => "image/heic",
        "heif" => "image/heif",
        _ => "application/octet-stream",
    };
    mime.to_string()
}

fn should_encode(mime_type: &str) -> bool {
    let mime = mime_type.to_ascii_lowercase();
    if mime.is_empty() {
        return false;
    }
    if mime.starts_with("text/") {
        return false;
    }
    if mime.contains("charset=") {
        return false;
    }
    let top = mime.split('/').next().unwrap_or_default();
    matches!(top, "image" | "audio" | "video" | "font" | "model" | "multipart")
}
