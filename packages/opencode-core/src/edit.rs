use aho_corasick::AhoCorasickBuilder;
use napi::Result;
use rayon::prelude::*;
use regex::Regex;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const MULTIPLE_MATCHES_ERROR: &str =
    "Found multiple matches for oldString. Provide more surrounding context to make the match unique.";
const NOT_FOUND_ERROR: &str =
    "Could not find oldString in the file. It must match exactly, including whitespace, indentation, and line endings.";
const CACHE_LIMIT: usize = 256;

static AC_CACHE: OnceLock<Mutex<HashMap<String, aho_corasick::AhoCorasick>>> = OnceLock::new();
static REGEX_CACHE: OnceLock<Mutex<HashMap<String, Regex>>> = OnceLock::new();

#[napi(string_enum)]
pub enum ReplaceStrategy {
    Simple,
    LineTrimmed,
    BlockAnchor,
    WhitespaceNormalized,
    IndentationFlexible,
    EscapeNormalized,
    TrimmedBoundary,
    ContextAware,
}

#[napi(object)]
#[derive(Clone)]
pub struct ReplaceResult {
    pub content: String,
    pub replaced: bool,
    pub multiple_matches: bool,
}

#[napi(object)]
#[derive(Clone)]
pub struct ReplacementMatch {
    pub start: u32,
    pub end: u32,
    pub text: String,
}

#[napi]
pub fn levenshtein_distance(a: String, b: String) -> u32 {
    if a.is_empty() {
        return b.chars().count() as u32;
    }
    if b.is_empty() {
        return a.chars().count() as u32;
    }

    let a = a.chars().collect::<Vec<_>>();
    let b = b.chars().collect::<Vec<_>>();
    let mut row = (0..=b.len()).collect::<Vec<_>>();

    for (i, ca) in a.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let old = row[j + 1];
            let cost = usize::from(ca != cb);
            row[j + 1] = (row[j + 1] + 1).min(row[j] + 1).min(prev + cost);
            prev = old;
        }
    }

    row[b.len()] as u32
}

#[napi]
pub fn find_replacements(content: String, old_string: String, strategy: String) -> Vec<ReplacementMatch> {
    let mode = match strategy.as_str() {
        "simple" => ReplaceStrategy::Simple,
        "line_trimmed" => ReplaceStrategy::LineTrimmed,
        "block_anchor" => ReplaceStrategy::BlockAnchor,
        "whitespace_normalized" => ReplaceStrategy::WhitespaceNormalized,
        "indentation_flexible" => ReplaceStrategy::IndentationFlexible,
        "escape_normalized" => ReplaceStrategy::EscapeNormalized,
        "trimmed_boundary" => ReplaceStrategy::TrimmedBoundary,
        "context_aware" => ReplaceStrategy::ContextAware,
        _ => ReplaceStrategy::Simple,
    };
    collect_by_strategy(&content, &old_string, mode)
        .into_iter()
        .map(|item| ReplacementMatch {
            start: item.0 as u32,
            end: item.1 as u32,
            text: content[item.0..item.1].to_string(),
        })
        .collect()
}

#[napi]
pub fn replace_content(
    content: String,
    old_string: String,
    new_string: String,
    replace_all: bool,
) -> Result<ReplaceResult> {
    if old_string == new_string {
        return Err(napi::Error::from_reason(
            "No changes to apply: oldString and newString are identical.".to_string(),
        ));
    }

    let all = [
        ReplaceStrategy::Simple,
        ReplaceStrategy::LineTrimmed,
        ReplaceStrategy::BlockAnchor,
        ReplaceStrategy::WhitespaceNormalized,
        ReplaceStrategy::IndentationFlexible,
        ReplaceStrategy::EscapeNormalized,
        ReplaceStrategy::TrimmedBoundary,
        ReplaceStrategy::ContextAware,
    ];

    for mode in all {
        let mut ranges = collect_by_strategy(&content, &old_string, mode);
        if ranges.is_empty() {
            continue;
        }

        ranges.sort_by_key(|item| item.0);
        ranges.dedup();

        if !replace_all && ranges.len() > 1 {
            return Err(napi::Error::from_reason(MULTIPLE_MATCHES_ERROR.to_string()));
        }

        let out = if replace_all {
            apply_ranges(&content, &ranges, &new_string)
        } else {
            apply_ranges(&content, &[ranges[0]], &new_string)
        };

        return Ok(ReplaceResult {
            content: out,
            replaced: true,
            multiple_matches: ranges.len() > 1,
        });
    }

    Err(napi::Error::from_reason(NOT_FOUND_ERROR.to_string()))
}

fn collect_by_strategy(content: &str, find: &str, mode: ReplaceStrategy) -> Vec<(usize, usize)> {
    match mode {
        ReplaceStrategy::Simple => find_exact(content, find),
        ReplaceStrategy::LineTrimmed => find_line_trimmed(content, find),
        ReplaceStrategy::BlockAnchor => find_block_anchor(content, find),
        ReplaceStrategy::WhitespaceNormalized => find_whitespace_normalized(content, find),
        ReplaceStrategy::IndentationFlexible => find_indentation_flexible(content, find),
        ReplaceStrategy::EscapeNormalized => find_escape_normalized(content, find),
        ReplaceStrategy::TrimmedBoundary => find_trimmed_boundary(content, find),
        ReplaceStrategy::ContextAware => find_context_aware(content, find),
    }
}

fn find_exact(content: &str, find: &str) -> Vec<(usize, usize)> {
    if find.is_empty() {
        return vec![];
    }
    let ac = cached_ac(find);
    ac.find_iter(content)
        .map(|item| (item.start(), item.end()))
        .collect()
}

fn build_lines(content: &str) -> Vec<(usize, usize, &str)> {
    let mut out = vec![];
    let mut pos = 0usize;
    for line in content.split_inclusive('\n') {
        let start = pos;
        pos += line.len();
        let raw = line.strip_suffix('\n').unwrap_or(line);
        out.push((start, start + raw.len(), raw));
    }
    if content.is_empty() {
        return out;
    }
    if !content.ends_with('\n') {
        return out;
    }
    out
}

fn find_line_trimmed(content: &str, find: &str) -> Vec<(usize, usize)> {
    let rows = build_lines(content);
    let mut target = find.lines().collect::<Vec<_>>();
    if target.last() == Some(&"") {
        target.pop();
    }
    if target.is_empty() || target.len() > rows.len() {
        return vec![];
    }

    (0..=rows.len() - target.len())
        .filter_map(|i| {
            let ok = (0..target.len()).all(|j| rows[i + j].2.trim() == target[j].trim());
            if !ok {
                return None;
            }
            let start = rows[i].0;
            let end = rows[i + target.len() - 1].1;
            Some((start, end))
        })
        .collect()
}

fn find_block_anchor(content: &str, find: &str) -> Vec<(usize, usize)> {
    let rows = build_lines(content);
    let mut target = find.lines().collect::<Vec<_>>();
    if target.last() == Some(&"") {
        target.pop();
    }
    if target.len() < 3 || target.len() > rows.len() {
        return vec![];
    }

    let first = target[0].trim();
    let last = target[target.len() - 1].trim();
    let mut out = vec![];
    for i in 0..rows.len() {
        if rows[i].2.trim() != first {
            continue;
        }
        for j in i + 2..rows.len() {
            if rows[j].2.trim() != last {
                continue;
            }
            out.push((rows[i].0, rows[j].1));
            break;
        }
    }
    out
}

fn normalize_ws(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn find_whitespace_normalized(content: &str, find: &str) -> Vec<(usize, usize)> {
    let target = normalize_ws(find);
    if target.is_empty() {
        return vec![];
    }

    let rows = build_lines(content);
    let mut out = rows
        .iter()
        .filter_map(|item| (normalize_ws(item.2) == target).then_some((item.0, item.1)))
        .collect::<Vec<_>>();

    let words = find.split_whitespace().collect::<Vec<_>>();
    if !words.is_empty() {
        let pat = words
            .iter()
            .map(|item| regex::escape(item))
            .collect::<Vec<_>>()
            .join(r"\s+");
        let re = cached_regex(&pat);
        re.find_iter(content)
            .for_each(|item| out.push((item.start(), item.end())));
    }
    out
}

fn ac_cache() -> &'static Mutex<HashMap<String, aho_corasick::AhoCorasick>> {
    AC_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn regex_cache() -> &'static Mutex<HashMap<String, Regex>> {
    REGEX_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached_ac(pattern: &str) -> aho_corasick::AhoCorasick {
    if let Ok(mut cache) = ac_cache().lock() {
        if let Some(item) = cache.get(pattern) {
            return item.clone();
        }
        let ac = AhoCorasickBuilder::new()
            .build([pattern])
            .expect("single-pattern automaton should build");
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(pattern.to_string(), ac.clone());
        return ac;
    }
    AhoCorasickBuilder::new()
        .build([pattern])
        .expect("single-pattern automaton should build")
}

fn cached_regex(pattern: &str) -> Regex {
    if let Ok(mut cache) = regex_cache().lock() {
        if let Some(item) = cache.get(pattern) {
            return item.clone();
        }
        let re = Regex::new(pattern).expect("escaped whitespace regex should compile");
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(pattern.to_string(), re.clone());
        return re;
    }
    Regex::new(pattern).expect("escaped whitespace regex should compile")
}

fn unindent(value: &str) -> String {
    let rows = value.lines().collect::<Vec<_>>();
    let min = rows
        .iter()
        .filter(|item| !item.trim().is_empty())
        .map(|item| item.len() - item.trim_start().len())
        .min()
        .unwrap_or(0);
    rows.iter()
        .map(|item| {
            if item.trim().is_empty() {
                (*item).to_string()
            } else {
                item.chars().skip(min).collect::<String>()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn find_indentation_flexible(content: &str, find: &str) -> Vec<(usize, usize)> {
    let rows = build_lines(content);
    let target = find.lines().collect::<Vec<_>>();
    if target.is_empty() || target.len() > rows.len() {
        return vec![];
    }
    let cmp = unindent(find);
    (0..=rows.len() - target.len())
        .filter_map(|i| {
            let block = rows[i..i + target.len()]
                .iter()
                .map(|item| item.2)
                .collect::<Vec<_>>()
                .join("\n");
            (unindent(&block) == cmp).then_some((rows[i].0, rows[i + target.len() - 1].1))
        })
        .collect()
}

fn unescape(value: &str) -> String {
    value
        .replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace("\\r", "\r")
        .replace("\\'", "'")
        .replace("\\\"", "\"")
        .replace("\\`", "`")
        .replace("\\\\", "\\")
        .replace("\\$", "$")
}

fn find_escape_normalized(content: &str, find: &str) -> Vec<(usize, usize)> {
    find_exact(content, &unescape(find))
}

fn find_trimmed_boundary(content: &str, find: &str) -> Vec<(usize, usize)> {
    let trimmed = find.trim();
    if trimmed == find {
        return vec![];
    }
    find_exact(content, trimmed)
}

fn find_context_aware(content: &str, find: &str) -> Vec<(usize, usize)> {
    let rows = build_lines(content);
    let mut target = find.lines().collect::<Vec<_>>();
    if target.last() == Some(&"") {
        target.pop();
    }
    if target.len() < 3 {
        return vec![];
    }
    let first = target[0].trim();
    let last = target[target.len() - 1].trim();
    let mut out = (0..rows.len())
        .into_par_iter()
        .filter_map(|i| {
            if rows[i].2.trim() != first {
                return None;
            }
            (i + 2..rows.len()).find_map(|j| {
                if rows[j].2.trim() != last {
                    return None;
                }
                let block = &rows[i..=j];
                if block.len() != target.len() {
                    return None;
                }
                let score = (1..block.len() - 1)
                    .filter(|k| block[*k].2.trim() == target[*k].trim())
                    .count();
                let total = (1..block.len() - 1)
                    .filter(|k| !block[*k].2.trim().is_empty() || !target[*k].trim().is_empty())
                    .count();
                if total == 0 || score as f32 / total as f32 >= 0.5 {
                    Some((rows[i].0, rows[j].1))
                } else {
                    None
                }
            })
        })
        .collect::<Vec<_>>();
    out.sort_by_key(|item| item.0);
    out
}

fn apply_ranges(content: &str, ranges: &[(usize, usize)], value: &str) -> String {
    let mut out = String::new();
    let mut pos = 0usize;
    for (start, end) in ranges {
        if *start < pos || *end < *start {
            continue;
        }
        out.push_str(&content[pos..*start]);
        out.push_str(value);
        pos = *end;
    }
    out.push_str(&content[pos..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_single_exact_match() {
        let out = replace_content(
            "hello world".to_string(),
            "world".to_string(),
            "rust".to_string(),
            false,
        )
        .expect("replace should succeed");
        assert_eq!(out.content, "hello rust");
        assert!(out.replaced);
        assert!(!out.multiple_matches);
    }

    #[test]
    fn replace_all_handles_multiple_matches() {
        let out = replace_content("x x x".to_string(), "x".to_string(), "y".to_string(), true)
            .expect("replace should succeed");
        assert_eq!(out.content, "y y y");
        assert!(out.multiple_matches);
    }

    #[test]
    fn indentation_flexible_strategy_matches() {
        let out = replace_content(
            "a(\n    one,\n    two,\n)\n".to_string(),
            "a(\none,\ntwo,\n)\n".to_string(),
            "a(three)\n".to_string(),
            false,
        )
        .expect("replace should succeed");
        assert_eq!(out.content, "a(three)\n\n");
    }

    #[test]
    fn errors_on_not_found() {
        let err = replace_content(
            "foo bar".to_string(),
            "missing".to_string(),
            "x".to_string(),
            false,
        );
        assert!(err.is_err());
        assert!(
            err.err()
                .map(|e| e.to_string().contains("Could not find oldString"))
                .unwrap_or(false)
        );
    }

    #[test]
    fn errors_on_ambiguous_single_replace() {
        let err = replace_content("hit hit".to_string(), "hit".to_string(), "x".to_string(), false);
        assert!(err.is_err());
        assert!(
            err.err()
                .map(|e| e.to_string().contains("Found multiple matches"))
                .unwrap_or(false)
        );
    }

    #[test]
    fn exact_search_returns_non_overlapping_matches() {
        let matches = find_exact("ababa", "aba");
        assert_eq!(matches, vec![(0, 3)]);
    }

    #[test]
    fn context_aware_order_is_stable() {
        let content = "A\n\nB\nA\n\nB\n";
        let find = "A\n\nB\n";
        let matches = find_context_aware(content, find);
        assert_eq!(matches, vec![(0, 4), (5, 9)]);
    }
}
