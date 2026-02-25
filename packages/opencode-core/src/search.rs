use dashmap::DashMap;
use globset::{GlobBuilder, GlobMatcher};
use ignore::WalkBuilder;
use napi::Result;
use rayon::prelude::*;
use regex::Regex;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::time::UNIX_EPOCH;

#[napi(object)]
#[derive(Clone)]
pub struct SearchMatch {
    pub path: String,
    pub mod_time: i64,
    pub line_num: i32,
    pub line_text: String,
    pub absolute_offset: i32,
    pub submatches: Vec<Submatch>,
}

#[napi(object)]
#[derive(Clone)]
pub struct Submatch {
    pub text: String,
    pub start: i32,
    pub end: i32,
}

#[napi(object)]
#[derive(Clone)]
pub struct SearchResult {
    pub matches: Vec<SearchMatch>,
    pub has_errors: bool,
    pub total_matches: i32,
}

#[napi(object)]
#[derive(Clone)]
pub struct SearchRenderedResult {
    pub output: String,
    pub has_errors: bool,
    pub total_matches: i32,
    pub displayed_matches: i32,
    pub truncated: bool,
}

#[napi(object)]
#[derive(Clone)]
pub struct FileListResult {
    pub files: Vec<String>,
    pub has_errors: bool,
}

#[napi(object)]
#[derive(Clone)]
pub struct FileStat {
    pub path: String,
    pub mod_time: i64,
}

#[napi(object)]
#[derive(Clone)]
pub struct FileListSortedResult {
    pub files: Vec<FileStat>,
    pub total: i32,
    pub has_errors: bool,
}

#[napi(object)]
#[derive(Clone)]
pub struct ListTreeResult {
    pub output: String,
    pub count: i32,
    pub truncated: bool,
}

#[napi(object)]
#[derive(Clone)]
pub struct IndexedPaths {
    pub files: Vec<String>,
    pub dirs: Vec<String>,
    pub has_errors: bool,
}

#[napi(object)]
#[derive(Clone)]
pub struct SearchIndexedInput {
    pub files: Vec<String>,
    pub dirs: Vec<String>,
}

#[napi(object)]
#[derive(Clone)]
pub struct GlobalHomeIndex {
    pub files: Vec<String>,
    pub dirs: Vec<String>,
}

#[napi]
pub fn search_content(
    pattern: String,
    search_path: String,
    include: Option<String>,
    max_results: Option<i32>,
    max_line_length: Option<i32>,
) -> Result<SearchResult> {
    let globs = include.map(|item| vec![item]);
    search_content_advanced(
        pattern,
        search_path,
        globs,
        Some(true),
        Some(false),
        None,
        max_results,
        max_line_length,
    )
}

#[napi]
pub fn index_global_home_dirs(search_path: String, platform: Option<String>) -> Result<GlobalHomeIndex> {
    let root = Path::new(&search_path);
    let mut dirs = BTreeSet::new();
    let mut ignore = HashSet::new();
    let os = platform.unwrap_or_default();
    if os == "darwin" {
        ignore.insert("Library".to_string());
    }
    if os == "win32" {
        ignore.insert("AppData".to_string());
    }
    let ignore_nested = HashSet::from([
        "node_modules".to_string(),
        "dist".to_string(),
        "build".to_string(),
        "target".to_string(),
        "vendor".to_string(),
    ]);

    let top = std::fs::read_dir(root)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .collect::<Vec<_>>();
    for entry in top {
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || ignore.contains(&name) {
            continue;
        }
        dirs.insert(format!("{name}/"));

        let base = entry.path();
        let children = std::fs::read_dir(&base)
            .ok()
            .into_iter()
            .flat_map(|items| items.flatten())
            .collect::<Vec<_>>();
        for child in children {
            let Ok(child_ft) = child.file_type() else {
                continue;
            };
            if !child_ft.is_dir() {
                continue;
            }
            let child_name = child.file_name().to_string_lossy().to_string();
            if child_name.starts_with('.') || ignore_nested.contains(&child_name) {
                continue;
            }
            dirs.insert(format!("{name}/{child_name}/"));
        }
    }

    Ok(GlobalHomeIndex {
        files: Vec::new(),
        dirs: dirs.into_iter().collect(),
    })
}

#[napi]
pub fn search_content_rendered(
    pattern: String,
    search_path: String,
    include: Option<String>,
    max_results: Option<i32>,
    max_line_length: Option<i32>,
) -> Result<SearchRenderedResult> {
    let result = search_content(
        pattern,
        search_path,
        include,
        max_results,
        max_line_length,
    )?;
    let displayed = result.matches.len() as i32;
    let truncated = result.total_matches > displayed;
    if displayed == 0 {
        return Ok(SearchRenderedResult {
            output: "No files found".to_string(),
            has_errors: result.has_errors,
            total_matches: result.total_matches,
            displayed_matches: 0,
            truncated: false,
        });
    }

    let mut lines = vec![format!(
        "Found {} matches{}",
        result.total_matches,
        if truncated {
            format!(" (showing first {displayed})")
        } else {
            String::new()
        }
    )];

    let mut current = String::new();
    for item in result.matches {
        if current != item.path {
            if !current.is_empty() {
                lines.push(String::new());
            }
            current = item.path.clone();
            lines.push(format!("{}:", item.path));
        }
        lines.push(format!("  Line {}: {}", item.line_num, item.line_text));
    }

    if truncated {
        lines.push(String::new());
        lines.push(format!(
            "(Results truncated: showing {} of {} matches ({} hidden). Consider using a more specific path or pattern.)",
            displayed,
            result.total_matches,
            result.total_matches - displayed
        ));
    }

    if result.has_errors {
        lines.push(String::new());
        lines.push("(Some paths were inaccessible and skipped)".to_string());
    }

    Ok(SearchRenderedResult {
        output: lines.join("\n"),
        has_errors: result.has_errors,
        total_matches: result.total_matches,
        displayed_matches: displayed,
        truncated,
    })
}

#[napi]
pub fn list_files(
    search_path: String,
    globs: Option<Vec<String>>,
    include_hidden: Option<bool>,
    follow_links: Option<bool>,
    max_depth: Option<i32>,
) -> Result<FileListResult> {
    let root = Path::new(&search_path);
    let hidden = include_hidden.unwrap_or(true);
    let follow = follow_links.unwrap_or(false);
    let depth = max_depth.map(|d| d.max(0) as usize);
    let has_errors = AtomicBool::new(false);
    let files = collect_files(root, globs.as_ref(), hidden, follow, depth, &has_errors)?
        .into_iter()
        .map(|(file, _)| normalize_relative(root, &file))
        .collect();
    Ok(FileListResult {
        files,
        has_errors: has_errors.load(AtomicOrdering::Relaxed),
    })
}

#[napi]
pub fn list_files_sorted(
    search_path: String,
    globs: Option<Vec<String>>,
    include_hidden: Option<bool>,
    follow_links: Option<bool>,
    max_depth: Option<i32>,
    limit: Option<i32>,
) -> Result<FileListSortedResult> {
    let root = Path::new(&search_path);
    let hidden = include_hidden.unwrap_or(true);
    let follow = follow_links.unwrap_or(false);
    let depth = max_depth.map(|d| d.max(0) as usize);
    let has_errors = AtomicBool::new(false);
    let mut files = collect_files(root, globs.as_ref(), hidden, follow, depth, &has_errors)?;
    files.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let total = files.len() as i32;
    if let Some(max) = limit {
        let max = max.max(0) as usize;
        if files.len() > max {
            files.truncate(max);
        }
    }
    let files = files
        .into_iter()
        .map(|(p, m)| FileStat {
            path: normalize_relative(root, &p),
            mod_time: m,
        })
        .collect();
    Ok(FileListSortedResult {
        files,
        total,
        has_errors: has_errors.load(AtomicOrdering::Relaxed),
    })
}

#[napi]
pub fn list_tree(
    search_path: String,
    globs: Option<Vec<String>>,
    limit: Option<i32>,
) -> Result<ListTreeResult> {
    let root = Path::new(&search_path);
    let limit = limit.unwrap_or(100).max(1) as usize;
    let has_errors = AtomicBool::new(false);
    let files = collect_files(root, globs.as_ref(), true, false, None, &has_errors)?
        .into_iter()
        .map(|(p, _)| normalize_relative(root, &p))
        .take(limit)
        .collect::<Vec<_>>();

    let mut dirs = BTreeSet::new();
    let mut files_by_dir = HashMap::<String, Vec<String>>::new();

    for file in files.iter() {
        let dir = file
            .rsplit_once('/')
            .map(|(d, _)| d.to_string())
            .unwrap_or_else(|| ".".to_string());
        let parts = if dir == "." {
            Vec::new()
        } else {
            dir.split('/').map(|s| s.to_string()).collect::<Vec<_>>()
        };

        for i in 0..=parts.len() {
            let key = if i == 0 {
                ".".to_string()
            } else {
                parts[..i].join("/")
            };
            dirs.insert(key);
        }

        let name = file
            .rsplit_once('/')
            .map(|(_, n)| n.to_string())
            .unwrap_or_else(|| file.to_string());
        files_by_dir.entry(dir).or_default().push(name);
    }

    fn render_dir(
        dir_path: &str,
        depth: usize,
        dirs: &BTreeSet<String>,
        files_by_dir: &HashMap<String, Vec<String>>,
    ) -> String {
        let indent = "  ".repeat(depth);
        let mut out = String::new();

        if depth > 0 {
            let name = dir_path.rsplit('/').next().unwrap_or(dir_path);
            out.push_str(&format!("{indent}{name}/\n"));
        }

        let child_indent = "  ".repeat(depth + 1);
        let mut children = dirs
            .iter()
            .filter(|d| {
                if d.as_str() == dir_path {
                    return false;
                }
                let parent = d
                    .rsplit_once('/')
                    .map(|(p, _)| p.to_string())
                    .unwrap_or_else(|| ".".to_string());
                parent == dir_path
            })
            .cloned()
            .collect::<Vec<_>>();
        children.sort();

        for child in children {
            out.push_str(&render_dir(&child, depth + 1, dirs, files_by_dir));
        }

        let mut names = files_by_dir
            .get(dir_path)
            .cloned()
            .unwrap_or_default();
        names.sort();
        for name in names {
            out.push_str(&format!("{child_indent}{name}\n"));
        }

        out
    }

    let output = format!("{}/\n{}", search_path, render_dir(".", 0, &dirs, &files_by_dir));
    Ok(ListTreeResult {
        output,
        count: files.len() as i32,
        truncated: files.len() >= limit,
    })
}

#[napi]
pub fn index_paths(
    search_path: String,
    include_hidden: Option<bool>,
    follow_links: Option<bool>,
    max_depth: Option<i32>,
) -> Result<IndexedPaths> {
    let root = Path::new(&search_path);
    let hidden = include_hidden.unwrap_or(true);
    let follow = follow_links.unwrap_or(false);
    let depth = max_depth.map(|d| d.max(0) as usize);
    let has_errors = AtomicBool::new(false);
    let files = collect_files(root, None, hidden, follow, depth, &has_errors)?
        .into_iter()
        .map(|(file, _)| normalize_relative(root, &file))
        .collect::<Vec<_>>();
    let dirs = build_dirs(&files);

    Ok(IndexedPaths {
        files,
        dirs,
        has_errors: has_errors.load(AtomicOrdering::Relaxed),
    })
}

#[napi]
pub fn index_paths_cached(
    search_path: String,
    include_hidden: Option<bool>,
    follow_links: Option<bool>,
    max_depth: Option<i32>,
    refresh: Option<bool>,
) -> Result<IndexedPaths> {
    let key = format!(
        "{}|{}|{}|{}",
        search_path,
        include_hidden.unwrap_or(true),
        follow_links.unwrap_or(false),
        max_depth.unwrap_or(-1)
    );
    let cache = INDEX_CACHE.get_or_init(DashMap::new);
    if !refresh.unwrap_or(false) {
        if let Some(cached) = cache.get(&key) {
            return Ok(cached.value().clone());
        }
    }

    let indexed = index_paths(search_path, include_hidden, follow_links, max_depth)?;
    cache.insert(key, indexed.clone());
    Ok(indexed)
}

#[napi]
pub fn render_tree(
    search_path: String,
    limit: Option<i32>,
    include_hidden: Option<bool>,
    follow_links: Option<bool>,
    max_depth: Option<i32>,
) -> Result<String> {
    let indexed = index_paths(
        search_path,
        include_hidden,
        follow_links,
        max_depth,
    )?;
    let mut children = HashMap::<String, BTreeSet<String>>::new();
    let mut all = HashSet::<String>::new();

    for dir in indexed
        .dirs
        .into_iter()
        .map(|d| d.trim_end_matches('/').to_string())
        .filter(|d| !d.contains(".opencode"))
    {
        all.insert(dir.clone());
        let parent = dir
            .rsplit_once('/')
            .map(|(p, _)| p.to_string())
            .unwrap_or_else(String::new);
        children.entry(parent).or_default().insert(dir);
    }

    let total = all.len();
    let max = limit.unwrap_or(total as i32).max(0) as usize;
    let mut queue = VecDeque::new();
    if let Some(root) = children.get("") {
        for child in root {
            queue.push_back(child.clone());
        }
    }

    let mut lines = Vec::new();
    let mut used = 0usize;

    while let Some(item) = queue.pop_front() {
        if used >= max {
            break;
        }
        lines.push(item.clone());
        used += 1;
        if let Some(next) = children.get(&item) {
            for child in next {
                queue.push_back(child.clone());
            }
        }
    }

    if total > used {
        lines.push(format!("[{} truncated]", total - used));
    }

    Ok(lines.join("\n"))
}

#[napi]
pub fn search_paths(
    search_path: String,
    query: String,
    kind: String,
    limit: Option<i32>,
    include_hidden: Option<bool>,
    follow_links: Option<bool>,
    max_depth: Option<i32>,
) -> Result<Vec<String>> {
    let query = query.trim().to_string();
    let limit = limit.unwrap_or(100).max(1) as usize;
    let indexed = index_paths(search_path, include_hidden, follow_links, max_depth)?;
    let mut dirs = indexed.dirs;
    dirs.sort();

    let prefer_hidden = query.starts_with('.') || query.contains("/.");

    if query.is_empty() {
        if kind == "file" {
            return Ok(indexed.files.into_iter().take(limit).collect());
        }
        if kind == "directory" || kind == "all" {
            return Ok(sort_hidden_last(dirs, prefer_hidden).into_iter().take(limit).collect());
        }
        return Ok(Vec::new());
    }

    let items = if kind == "file" {
        indexed.files
    } else if kind == "directory" {
        dirs.clone()
    } else {
        let mut all = indexed.files;
        all.extend(dirs.clone());
        all
    };

    let mut ranked = items
        .into_iter()
        .filter_map(|item| score_item(&query, &item).map(|score| (item, score)))
        .collect::<Vec<_>>();

    ranked.sort_by(|a, b| compare_ranked(a, b));

    if kind == "directory" {
        let search_limit = if prefer_hidden { limit } else { limit * 20 };
        let top = ranked
            .into_iter()
            .take(search_limit)
            .map(|(item, _)| item)
            .collect::<Vec<_>>();
        return Ok(sort_hidden_last(top, prefer_hidden).into_iter().take(limit).collect());
    }

    Ok(ranked
        .into_iter()
        .take(limit)
        .map(|(item, _)| item)
        .collect())
}

#[napi]
pub fn search_indexed_paths(
    indexed: SearchIndexedInput,
    query: String,
    kind: String,
    limit: Option<i32>,
) -> Result<Vec<String>> {
    let query = query.trim().to_string();
    let limit = limit.unwrap_or(100).max(1) as usize;
    let mut dirs = indexed.dirs;
    dirs.sort();
    let prefer_hidden = query.starts_with('.') || query.contains("/.");

    if query.is_empty() {
        if kind == "file" {
            return Ok(indexed.files.into_iter().take(limit).collect());
        }
        if kind == "directory" || kind == "all" {
            return Ok(sort_hidden_last(dirs, prefer_hidden).into_iter().take(limit).collect());
        }
        return Ok(Vec::new());
    }

    let items = if kind == "file" {
        indexed.files
    } else if kind == "directory" {
        dirs.clone()
    } else {
        let mut all = indexed.files;
        all.extend(dirs.clone());
        all
    };

    let mut ranked = items
        .into_iter()
        .filter_map(|item| score_item(&query, &item).map(|score| (item, score)))
        .collect::<Vec<_>>();

    ranked.sort_by(|a, b| compare_ranked(a, b));
    if kind == "directory" {
        let search_limit = if prefer_hidden { limit } else { limit * 20 };
        let top = ranked
            .into_iter()
            .take(search_limit)
            .map(|(item, _)| item)
            .collect::<Vec<_>>();
        return Ok(sort_hidden_last(top, prefer_hidden).into_iter().take(limit).collect());
    }

    Ok(ranked
        .into_iter()
        .take(limit)
        .map(|(item, _)| item)
        .collect())
}

#[napi]
pub fn search_content_advanced(
    pattern: String,
    search_path: String,
    globs: Option<Vec<String>>,
    include_hidden: Option<bool>,
    follow_links: Option<bool>,
    max_depth: Option<i32>,
    max_results: Option<i32>,
    max_line_length: Option<i32>,
) -> Result<SearchResult> {
    let root = Path::new(&search_path);
    let regex = Regex::new(&pattern).map_err(|e| napi::Error::from_reason(e.to_string()))?;
    let max_line = max_line_length.unwrap_or(2000).max(1) as usize;
    let hidden = include_hidden.unwrap_or(true);
    let follow = follow_links.unwrap_or(false);
    let depth = max_depth.map(|d| d.max(0) as usize);
    let has_errors = AtomicBool::new(false);
    let files = collect_files(root, globs.as_ref(), hidden, follow, depth, &has_errors)?;

    let mut matches: Vec<SearchMatch> = files
        .par_iter()
        .flat_map(|(file, mod_time)| {
            let bytes = match std::fs::read(file) {
                Ok(v) => v,
                Err(_) => {
                    has_errors.store(true, AtomicOrdering::Relaxed);
                    return Vec::new();
                }
            };
            if is_binary(&bytes) {
                return Vec::new();
            }
            let text = String::from_utf8_lossy(&bytes);
            let mut line_num = 0i32;
            let mut offset = 0usize;
            let mut found = Vec::new();

            for segment in text.split_inclusive('\n') {
                line_num += 1;
                let no_nl = segment.strip_suffix('\n').unwrap_or(segment);
                let raw = no_nl.strip_suffix('\r').unwrap_or(no_nl);
                if !regex.is_match(raw) {
                    offset += segment.len();
                    continue;
                }
                let line_text = if raw.chars().count() > max_line {
                    raw.chars().take(max_line).collect::<String>() + "..."
                } else {
                    raw.to_string()
                };
                let submatches = regex
                    .find_iter(&line_text)
                    .map(|m| Submatch {
                        text: m.as_str().to_string(),
                        start: m.start() as i32,
                        end: m.end() as i32,
                    })
                    .collect::<Vec<_>>();
                found.push(SearchMatch {
                    path: normalize_relative(root, file),
                    mod_time: *mod_time,
                    line_num,
                    line_text,
                    absolute_offset: offset as i32,
                    submatches,
                });
                offset += segment.len();
            }

            found
        })
        .collect();

    matches.sort_by(|a, b| {
        b.mod_time
            .cmp(&a.mod_time)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.line_num.cmp(&b.line_num))
    });

    let total_matches = matches.len() as i32;
    if let Some(limit) = max_results {
        let limit = limit.max(0) as usize;
        if matches.len() > limit {
            matches.truncate(limit);
        }
    }

    Ok(SearchResult {
        matches,
        has_errors: has_errors.load(AtomicOrdering::Relaxed),
        total_matches,
    })
}

fn collect_files(
    root: &Path,
    globs: Option<&Vec<String>>,
    include_hidden: bool,
    follow_links: bool,
    max_depth: Option<usize>,
    has_errors: &AtomicBool,
) -> Result<Vec<(PathBuf, i64)>> {
    let (include_matchers, exclude_matchers) = if let Some(items) = globs {
        compile_matchers(items)?
    } else {
        (Vec::new(), Vec::new())
    };
    let has_include = !include_matchers.is_empty();
    let mut files = Vec::new();
    let mut walker = WalkBuilder::new(root);
    walker
        .hidden(false)
        .follow_links(follow_links)
        .max_depth(max_depth)
        .require_git(false);

    for entry in walker.build() {
        let Ok(entry) = entry else {
            has_errors.store(true, AtomicOrdering::Relaxed);
            continue;
        };
        let entry_path = entry.path();
        if !entry_path.is_file() {
            continue;
        }
        let rel = entry_path.strip_prefix(root).ok().unwrap_or(entry_path);
        if !include_hidden && is_hidden(rel) {
            continue;
        }
        let rel_text = normalize_path(rel);
        if has_include && !include_matchers.iter().any(|m| m.is_match(&rel_text)) {
            continue;
        }
        if exclude_matchers.iter().any(|m| m.is_match(&rel_text)) {
            continue;
        }
        let mod_time = std::fs::metadata(entry_path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        files.push((entry_path.to_path_buf(), mod_time));
    }

    Ok(files)
}

fn compile_matchers(globs: &[String]) -> Result<(Vec<GlobMatcher>, Vec<GlobMatcher>)> {
    let mut include = Vec::new();
    let mut exclude = Vec::new();

    for item in globs {
        let (negated, pattern) = if let Some(rest) = item.strip_prefix('!') {
            (true, rest)
        } else {
            (false, item.as_str())
        };
        let matcher = GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?
            .compile_matcher();
        if negated {
            exclude.push(matcher);
            continue;
        }
        include.push(matcher);
    }

    Ok((include, exclude))
}

fn is_hidden(path: &Path) -> bool {
    path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .map(|s| s.starts_with('.'))
            .unwrap_or(false)
    })
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn normalize_relative(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).ok().unwrap_or(path);
    normalize_path(rel)
}

fn is_binary(bytes: &[u8]) -> bool {
    let len = bytes.len().min(8192);
    bytes[..len].contains(&0)
}

fn build_dirs(files: &[String]) -> Vec<String> {
    let mut set = BTreeSet::new();
    for file in files {
        let mut current = file.as_str();
        while let Some((dir, _)) = current.rsplit_once('/') {
            if dir.is_empty() {
                break;
            }
            if !set.insert(format!("{dir}/")) {
                current = dir;
                continue;
            }
            current = dir;
        }
    }
    set.into_iter().collect()
}

fn score_item(query: &str, item: &str) -> Option<i32> {
    let q = query.to_lowercase();
    let t = item.to_lowercase();

    if let Some(pos) = t.find(&q) {
        let score = 100_000 - (pos as i32 * 10) - (t.len() as i32 - q.len() as i32).max(0);
        return Some(score);
    }

    let mut q_chars = q.chars();
    let mut target = q_chars.next()?;
    let mut gaps = 0;
    let mut matched = 0;

    for ch in t.chars() {
        if ch == target {
            matched += 1;
            if let Some(next) = q_chars.next() {
                target = next;
                continue;
            }
            let score = 50_000 - gaps - (t.len() as i32 - q.len() as i32).max(0);
            return Some(score + matched);
        }
        gaps += 1;
    }

    None
}

fn compare_ranked(a: &(String, i32), b: &(String, i32)) -> Ordering {
    b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0))
}

fn sort_hidden_last(items: Vec<String>, prefer_hidden: bool) -> Vec<String> {
    if prefer_hidden {
        return items;
    }
    let mut visible = Vec::new();
    let mut hidden = Vec::new();
    for item in items {
        if is_hidden_text(&item) {
            hidden.push(item);
            continue;
        }
        visible.push(item);
    }
    visible.extend(hidden);
    visible
}

fn is_hidden_text(value: &str) -> bool {
    let normalized = value.trim_end_matches('/');
    normalized
        .split('/')
        .any(|part| part.starts_with('.') && part.len() > 1)
}

static INDEX_CACHE: OnceLock<DashMap<String, IndexedPaths>> = OnceLock::new();
