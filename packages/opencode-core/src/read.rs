use base64::Engine;
use crate::create_two_files_patch;
use napi::Result;
use regex::Regex;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

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

#[napi(object)]
#[derive(Clone)]
pub struct ReadFullResult {
    pub kind: String,
    pub exists: bool,
    pub content: String,
    pub mime_type: Option<String>,
    pub encoding: Option<String>,
}

#[napi(object)]
#[derive(Clone)]
pub struct UntrackedLineCount {
    pub path: String,
    pub lines: i32,
}

#[napi(object)]
#[derive(Clone)]
pub struct GitStatusEntry {
    pub path: String,
    pub added: i32,
    pub removed: i32,
    pub status: String,
}

#[napi(object)]
#[derive(Clone)]
pub struct ReadDiffSnapshot {
    pub has_diff: bool,
    pub original: String,
}

#[napi(object)]
#[derive(Clone)]
pub struct DiffHunk {
    pub old_start: i32,
    pub old_lines: i32,
    pub new_start: i32,
    pub new_lines: i32,
    pub lines: Vec<String>,
}

#[napi(object)]
#[derive(Clone)]
pub struct DiffPatch {
    pub old_file_name: String,
    pub new_file_name: String,
    pub old_header: Option<String>,
    pub new_header: Option<String>,
    pub hunks: Vec<DiffHunk>,
    pub index: Option<String>,
}

#[napi(object)]
#[derive(Clone)]
pub struct DiffPatchResult {
    pub diff: String,
    pub patch: DiffPatch,
}

#[napi(object)]
#[derive(Clone)]
pub struct GitExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
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
pub fn read_full(path: String, hint_path: Option<String>) -> Result<ReadFullResult> {
    let classification = classify_read_target(path.clone(), hint_path)?;
    if !classification.exists {
        return Ok(ReadFullResult {
            kind: "text".to_string(),
            exists: false,
            content: String::new(),
            mime_type: None,
            encoding: None,
        });
    }

    if classification.mode == "binary" {
        return Ok(ReadFullResult {
            kind: "binary".to_string(),
            exists: true,
            content: String::new(),
            mime_type: classification.mime_type,
            encoding: None,
        });
    }

    let bytes = std::fs::read(Path::new(&path)).unwrap_or_default();
    if classification.mode == "base64" {
        return Ok(ReadFullResult {
            kind: "text".to_string(),
            exists: true,
            content: base64::engine::general_purpose::STANDARD.encode(bytes),
            mime_type: classification.mime_type,
            encoding: Some("base64".to_string()),
        });
    }

    Ok(ReadFullResult {
        kind: "text".to_string(),
        exists: true,
        content: String::from_utf8_lossy(&bytes).trim().to_string(),
        mime_type: None,
        encoding: None,
    })
}

#[napi]
pub fn count_untracked_lines(root: String, files: Vec<String>) -> Result<Vec<UntrackedLineCount>> {
    let out = files
        .into_iter()
        .filter_map(|item| {
            let full = Path::new(&root).join(&item);
            let bytes = std::fs::read(full).ok()?;
            let text = String::from_utf8_lossy(&bytes);
            let lines = text.split('\n').count() as i32;
            Some(UntrackedLineCount { path: item, lines })
        })
        .collect();
    Ok(out)
}

#[napi]
pub fn git_status(root: String) -> Result<Vec<GitStatusEntry>> {
    let mut out = Vec::new();
    let changed = run_git_lines(
        &root,
        ["-c", "core.quotepath=false", "diff", "--numstat", "HEAD"].as_slice(),
    )?;
    for line in changed {
        let parts = line.split('\t').collect::<Vec<_>>();
        if parts.len() < 3 {
            continue;
        }
        out.push(GitStatusEntry {
            path: parts[2].to_string(),
            added: parts[0].parse::<i32>().unwrap_or(0),
            removed: parts[1].parse::<i32>().unwrap_or(0),
            status: "modified".to_string(),
        });
    }

    let untracked = run_git_lines(
        &root,
        ["-c", "core.quotepath=false", "ls-files", "--others", "--exclude-standard"].as_slice(),
    )?;
    for item in count_untracked_lines(root.clone(), untracked)? {
        out.push(GitStatusEntry {
            path: item.path,
            added: item.lines,
            removed: 0,
            status: "added".to_string(),
        });
    }

    let deleted = run_git_lines(
        &root,
        ["-c", "core.quotepath=false", "diff", "--name-only", "--diff-filter=D", "HEAD"].as_slice(),
    )?;
    for path in deleted {
        out.push(GitStatusEntry {
            path,
            added: 0,
            removed: 0,
            status: "deleted".to_string(),
        });
    }

    Ok(out)
}

#[napi]
pub fn read_diff_snapshot(root: String, file: String) -> Result<ReadDiffSnapshot> {
    let unstaged = run_git_text(
        &root,
        ["diff", "--", file.as_str()].as_slice(),
    )?;
    let has_unstaged = !unstaged.trim().is_empty();
    let head_spec = format!("HEAD:{file}");
    if has_unstaged {
        let original = run_git_text(
            &root,
            ["show", head_spec.as_str()].as_slice(),
        )?;
        return Ok(ReadDiffSnapshot {
            has_diff: true,
            original,
        });
    }

    let staged = run_git_text(
        &root,
        ["diff", "--staged", "--", file.as_str()].as_slice(),
    )?;
    if staged.trim().is_empty() {
        return Ok(ReadDiffSnapshot {
            has_diff: false,
            original: String::new(),
        });
    }

    let original = run_git_text(
        &root,
        ["show", head_spec.as_str()].as_slice(),
    )?;
    Ok(ReadDiffSnapshot {
        has_diff: true,
        original,
    })
}

#[napi]
pub fn build_diff_patch(file: String, original: String, content: String) -> Result<DiffPatchResult> {
    let diff = create_two_files_patch(file.clone(), file.clone(), original, content);
    let mut old_file_name = file.clone();
    let mut new_file_name = file.clone();
    let mut hunks = Vec::<DiffHunk>::new();
    let mut current: Option<DiffHunk> = None;

    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("--- ") {
            old_file_name = rest.to_string();
            continue;
        }
        if let Some(rest) = line.strip_prefix("+++ ") {
            new_file_name = rest.to_string();
            continue;
        }
        if line.starts_with("@@ ") {
            if let Some(hunk) = current.take() {
                hunks.push(hunk);
            }
            let (old_start, old_lines, new_start, new_lines) = parse_hunk_header(line)?;
            current = Some(DiffHunk {
                old_start: old_start as i32,
                old_lines: old_lines as i32,
                new_start: new_start as i32,
                new_lines: new_lines as i32,
                lines: Vec::new(),
            });
            continue;
        }
        if let Some(hunk) = current.as_mut() {
            if line.starts_with('+') || line.starts_with('-') || line.starts_with(' ') {
                hunk.lines.push(line.to_string());
            }
        }
    }

    if let Some(hunk) = current.take() {
        hunks.push(hunk);
    }

    let patch = DiffPatch {
        old_file_name,
        new_file_name,
        old_header: None,
        new_header: None,
        hunks,
        index: None,
    };

    Ok(DiffPatchResult { diff, patch })
}

#[napi]
pub fn resolve_git_dir(root: String) -> Result<Option<String>> {
    let mut cmd = Command::new("git");
    cmd.args(["rev-parse", "--git-dir"]).current_dir(&root);
    let output = cmd.output().map_err(|e| napi::Error::from_reason(e.to_string()))?;
    if !output.status.success() {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Ok(None);
    }
    let path = Path::new(&root).join(text);
    Ok(Some(path.to_string_lossy().to_string()))
}

#[napi]
pub fn git_exec(cwd: String, args: Vec<String>) -> Result<GitExecResult> {
    run_git_exec(&cwd, &args, None, None, None)
}

#[napi]
pub fn git_exec_env(
    cwd: String,
    args: Vec<String>,
    git_dir: Option<String>,
    git_work_tree: Option<String>,
    git_config_global: Option<String>,
) -> Result<GitExecResult> {
    run_git_exec(
        &cwd,
        &args,
        git_dir.as_deref(),
        git_work_tree.as_deref(),
        git_config_global.as_deref(),
    )
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

fn run_git_lines(root: &str, args: &[&str]) -> Result<Vec<String>> {
    Ok(run_git_text(root, args)?
        .lines()
        .map(|line| line.to_string())
        .filter(|line| !line.trim().is_empty())
        .collect())
}

fn run_git_text(root: &str, args: &[&str]) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(root);
    let output = cmd.output().map_err(|e| napi::Error::from_reason(e.to_string()))?;
    if !output.status.success() {
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn run_git_exec(
    cwd: &str,
    args: &[String],
    git_dir: Option<&str>,
    git_work_tree: Option<&str>,
    git_config_global: Option<&str>,
) -> Result<GitExecResult> {
    let mut cmd = Command::new("git");
    cmd.current_dir(cwd);
    cmd.args(args);
    if let Some(item) = git_dir {
        cmd.env("GIT_DIR", item);
    }
    if let Some(item) = git_work_tree {
        cmd.env("GIT_WORK_TREE", item);
    }
    if let Some(item) = git_config_global {
        cmd.env("GIT_CONFIG_GLOBAL", item);
    }
    let output = cmd.output().map_err(|e| napi::Error::from_reason(e.to_string()))?;
    let code = output.status.code().unwrap_or(1);
    Ok(GitExecResult {
        exit_code: code,
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn hunk_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@")
            .expect("valid read hunk regex")
    })
}

fn parse_hunk_header(line: &str) -> Result<(usize, usize, usize, usize)> {
    let caps = hunk_regex()
        .captures(line)
        .ok_or_else(|| napi::Error::from_reason("invalid hunk header".to_string()))?;
    let old_start = caps
        .get(1)
        .and_then(|m| m.as_str().parse::<usize>().ok())
        .ok_or_else(|| napi::Error::from_reason("invalid old start".to_string()))?;
    let old_lines = caps
        .get(2)
        .and_then(|m| m.as_str().parse::<usize>().ok())
        .unwrap_or(1);
    let new_start = caps
        .get(3)
        .and_then(|m| m.as_str().parse::<usize>().ok())
        .ok_or_else(|| napi::Error::from_reason("invalid new start".to_string()))?;
    let new_lines = caps
        .get(4)
        .and_then(|m| m.as_str().parse::<usize>().ok())
        .unwrap_or(1);
    Ok((old_start, old_lines, new_start, new_lines))
}
