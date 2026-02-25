use aho_corasick::AhoCorasickBuilder;
use dashmap::DashMap;
use napi::Result;
use rayon::prelude::*;
use regex::Regex;
use std::sync::{Arc, OnceLock};

const MULTIPLE_MATCHES_ERROR: &str =
    "Found multiple matches for oldString. Provide more surrounding context to make the match unique.";
const NOT_FOUND_ERROR: &str =
    "Could not find oldString in the file. It must match exactly, including whitespace, indentation, and line endings.";
const CACHE_LIMIT: usize = 256;

static AC_CACHE: OnceLock<DashMap<String, Arc<aho_corasick::AhoCorasick>>> = OnceLock::new();
static REGEX_CACHE: OnceLock<DashMap<String, Arc<Regex>>> = OnceLock::new();

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

    if a.is_ascii() && b.is_ascii() {
        return levenshtein_ascii_simd(a.as_bytes(), b.as_bytes()) as u32;
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

fn levenshtein_ascii_simd(a: &[u8], b: &[u8]) -> usize {
    let (mut a, mut b) = trim_common_affixes(a, b);
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    if b.len() > a.len() {
        std::mem::swap(&mut a, &mut b);
    }

    let mut row = (0..=b.len()).collect::<Vec<_>>();
    for (i, &ca) in a.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let old = row[j + 1];
            let cost = usize::from(ca != cb);
            row[j + 1] = (row[j + 1] + 1).min(row[j] + 1).min(prev + cost);
            prev = old;
        }
    }
    row[b.len()]
}

fn trim_common_affixes<'a>(a: &'a [u8], b: &'a [u8]) -> (&'a [u8], &'a [u8]) {
    let max = a.len().min(b.len());
    let prefix = common_prefix_len(a, b, max);
    let remain = (a.len() - prefix).min(b.len() - prefix);
    let suffix = common_suffix_len(a, b, remain, prefix);
    (&a[prefix..a.len() - suffix], &b[prefix..b.len() - suffix])
}

fn common_prefix_len(a: &[u8], b: &[u8], max: usize) -> usize {
    let mut i = if cfg!(target_arch = "x86_64") && is_avx2_available() {
        // SAFETY: runtime AVX2-gated and x86_64-only implementation.
        unsafe { common_prefix_avx2(a, b, max) }
    } else if cfg!(target_arch = "aarch64") && is_neon_available() {
        // SAFETY: runtime NEON-gated and aarch64-only implementation.
        unsafe { common_prefix_neon(a, b, max) }
    } else {
        0
    };
    while i < max && a[i] == b[i] {
        i += 1;
    }
    i
}

fn common_suffix_len(a: &[u8], b: &[u8], max: usize, prefix: usize) -> usize {
    let mut i = if cfg!(target_arch = "x86_64") && is_avx2_available() {
        // SAFETY: runtime AVX2-gated and x86_64-only implementation.
        unsafe { common_suffix_avx2(a, b, max) }
    } else if cfg!(target_arch = "aarch64") && is_neon_available() {
        // SAFETY: runtime NEON-gated and aarch64-only implementation.
        unsafe { common_suffix_neon(a, b, max) }
    } else {
        0
    };
    while i < max
        && a.len().saturating_sub(1 + i) >= prefix
        && b.len().saturating_sub(1 + i) >= prefix
        && a[a.len() - 1 - i] == b[b.len() - 1 - i]
    {
        i += 1;
    }
    i
}

#[inline]
fn is_avx2_available() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("avx2")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

#[inline]
fn is_neon_available() -> bool {
    #[cfg(target_arch = "aarch64")]
    {
        std::arch::is_aarch64_feature_detected!("neon")
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        false
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn common_prefix_avx2(a: &[u8], b: &[u8], max: usize) -> usize {
    use std::arch::x86_64::*;
    let mut i = 0usize;
    while i + 32 <= max {
        let va = _mm256_loadu_si256(a.as_ptr().add(i) as *const __m256i);
        let vb = _mm256_loadu_si256(b.as_ptr().add(i) as *const __m256i);
        let eq = _mm256_cmpeq_epi8(va, vb);
        let mask = _mm256_movemask_epi8(eq) as u32;
        if mask == u32::MAX {
            i += 32;
            continue;
        }
        return i + (!mask).trailing_zeros() as usize;
    }
    i
}

#[cfg(not(target_arch = "x86_64"))]
unsafe fn common_prefix_avx2(_a: &[u8], _b: &[u8], _max: usize) -> usize {
    0
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn common_prefix_neon(a: &[u8], b: &[u8], max: usize) -> usize {
    use std::arch::aarch64::*;
    let mut i = 0usize;
    while i + 16 <= max {
        let va = vld1q_u8(a.as_ptr().add(i));
        let vb = vld1q_u8(b.as_ptr().add(i));
        let eq = vceqq_u8(va, vb);
        let mut tmp = [0u8; 16];
        vst1q_u8(tmp.as_mut_ptr(), eq);
        if tmp.iter().all(|item| *item == 0xff) {
            i += 16;
            continue;
        }
        if let Some(pos) = tmp.iter().position(|item| *item != 0xff) {
            return i + pos;
        }
    }
    i
}

#[cfg(not(target_arch = "aarch64"))]
unsafe fn common_prefix_neon(_a: &[u8], _b: &[u8], _max: usize) -> usize {
    0
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn common_suffix_avx2(a: &[u8], b: &[u8], max: usize) -> usize {
    use std::arch::x86_64::*;
    let mut i = 0usize;
    while i + 32 <= max {
        let start_a = a.len() - (i + 32);
        let start_b = b.len() - (i + 32);
        let va = _mm256_loadu_si256(a.as_ptr().add(start_a) as *const __m256i);
        let vb = _mm256_loadu_si256(b.as_ptr().add(start_b) as *const __m256i);
        let eq = _mm256_cmpeq_epi8(va, vb);
        let mask = _mm256_movemask_epi8(eq) as u32;
        if mask == u32::MAX {
            i += 32;
            continue;
        }
        return i + (!mask).leading_zeros() as usize;
    }
    i
}

#[cfg(not(target_arch = "x86_64"))]
unsafe fn common_suffix_avx2(_a: &[u8], _b: &[u8], _max: usize) -> usize {
    0
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn common_suffix_neon(a: &[u8], b: &[u8], max: usize) -> usize {
    use std::arch::aarch64::*;
    let mut i = 0usize;
    while i + 16 <= max {
        let start_a = a.len() - (i + 16);
        let start_b = b.len() - (i + 16);
        let va = vld1q_u8(a.as_ptr().add(start_a));
        let vb = vld1q_u8(b.as_ptr().add(start_b));
        let eq = vceqq_u8(va, vb);
        let mut tmp = [0u8; 16];
        vst1q_u8(tmp.as_mut_ptr(), eq);
        if tmp.iter().all(|item| *item == 0xff) {
            i += 16;
            continue;
        }
        if let Some(pos) = tmp.iter().rposition(|item| *item != 0xff) {
            return i + (15 - pos);
        }
    }
    i
}

#[cfg(not(target_arch = "aarch64"))]
unsafe fn common_suffix_neon(_a: &[u8], _b: &[u8], _max: usize) -> usize {
    0
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

fn ac_cache() -> &'static DashMap<String, Arc<aho_corasick::AhoCorasick>> {
    AC_CACHE.get_or_init(DashMap::new)
}

fn regex_cache() -> &'static DashMap<String, Arc<Regex>> {
    REGEX_CACHE.get_or_init(DashMap::new)
}

fn cached_ac(pattern: &str) -> Arc<aho_corasick::AhoCorasick> {
    if let Some(item) = ac_cache().get(pattern) {
        return Arc::clone(item.value());
    }
    let ac = Arc::new(
        AhoCorasickBuilder::new()
        .build([pattern])
        .expect("single-pattern automaton should build"),
    );
    if ac_cache().len() >= CACHE_LIMIT {
        ac_cache().clear();
    }
    ac_cache().insert(pattern.to_string(), Arc::clone(&ac));
    ac
}

fn cached_regex(pattern: &str) -> Arc<Regex> {
    if let Some(item) = regex_cache().get(pattern) {
        return Arc::clone(item.value());
    }
    let re = Arc::new(Regex::new(pattern).expect("escaped whitespace regex should compile"));
    if regex_cache().len() >= CACHE_LIMIT {
        regex_cache().clear();
    }
    regex_cache().insert(pattern.to_string(), Arc::clone(&re));
    re
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

    #[test]
    fn levenshtein_ascii_examples() {
        assert_eq!(levenshtein_distance("kitten".to_string(), "sitting".to_string()), 3);
        assert_eq!(levenshtein_distance("flaw".to_string(), "lawn".to_string()), 2);
    }

    #[test]
    fn levenshtein_unicode_fallback() {
        assert_eq!(levenshtein_distance("résumé".to_string(), "resume".to_string()), 2);
        assert_eq!(levenshtein_distance("你好世界".to_string(), "你们世界".to_string()), 1);
    }

    #[test]
    fn levenshtein_trims_shared_prefix_suffix() {
        let left = format!("{}x{}", "a".repeat(512), "b".repeat(512));
        let right = format!("{}y{}", "a".repeat(512), "b".repeat(512));
        assert_eq!(levenshtein_distance(left, right), 1);
    }
}
