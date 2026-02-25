use napi::Result;

#[napi]
pub fn count_lines_fast(text: String) -> u32 {
    if text.is_empty() {
        return 0;
    }
    let mut count = 1u32;
    for b in text.as_bytes() {
        if *b == b'\n' {
            count += 1;
        }
    }
    count
}

#[napi]
pub fn find_whitespace_indices(text: String) -> Vec<u32> {
    text.char_indices()
        .filter_map(|(idx, c)| c.is_whitespace().then_some(idx as u32))
        .collect()
}

#[napi]
pub fn is_binary_file(path: String) -> Result<bool> {
    let bytes = std::fs::read(path).map_err(|err| napi::Error::from_reason(err.to_string()))?;
    Ok(bytes.iter().take(8192).any(|item| *item == 0))
}

#[napi]
pub fn case_fold_ascii(text: String) -> String {
    text.to_ascii_lowercase()
}
