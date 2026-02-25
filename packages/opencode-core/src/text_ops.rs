use napi::Result;
use std::fs::File;
use std::io::Read;

const BINARY_SAMPLE_BYTES: usize = 8192;

#[napi]
pub fn count_lines_fast(text: String) -> u32 {
    if text.is_empty() {
        return 0;
    }
    memchr::memchr_iter(b'\n', text.as_bytes()).count() as u32 + 1
}

#[napi]
pub fn find_whitespace_indices(text: String) -> Vec<u32> {
    let bytes = text.as_bytes();
    if text.is_ascii() {
        return bytes
            .iter()
            .enumerate()
            .filter_map(|(idx, byte)| {
                is_ascii_whitespace(*byte).then_some(idx as u32)
            })
            .collect();
    }

    text.char_indices()
        .filter_map(|(idx, item)| item.is_whitespace().then_some(idx as u32))
        .collect()
}

#[napi]
pub fn is_binary_file(path: String) -> Result<bool> {
    let mut file = File::open(path).map_err(|err| napi::Error::from_reason(err.to_string()))?;
    let mut bytes = [0u8; BINARY_SAMPLE_BYTES];
    let read = file
        .read(&mut bytes)
        .map_err(|err| napi::Error::from_reason(err.to_string()))?;
    Ok(memchr::memchr(0, &bytes[..read]).is_some())
}

#[napi]
pub fn case_fold_ascii(text: String) -> String {
    text.to_ascii_lowercase()
}

fn is_ascii_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\n' | b'\r' | b'\t' | 0x0b | 0x0c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_lines_matches_expected() {
        assert_eq!(count_lines_fast(String::new()), 0);
        assert_eq!(count_lines_fast("one".to_string()), 1);
        assert_eq!(count_lines_fast("one\ntwo\n".to_string()), 3);
        assert_eq!(count_lines_fast("one\ntwo".to_string()), 2);
    }

    #[test]
    fn finds_whitespace_for_ascii_and_unicode() {
        assert_eq!(
            find_whitespace_indices("a b\tc\nd".to_string()),
            vec![1, 3, 5]
        );
        assert_eq!(find_whitespace_indices("x\u{00a0}y".to_string()), vec![1]);
    }

    #[test]
    fn binary_detection_checks_sample() {
        let root = std::env::temp_dir().join(format!(
            "opencode-text-ops-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("temp dir");
        let text = root.join("text.txt");
        let binary = root.join("binary.bin");
        std::fs::write(&text, b"hello world\n").expect("write text");
        std::fs::write(&binary, b"hello\0world").expect("write binary");
        assert!(!is_binary_file(text.to_string_lossy().to_string()).expect("text check"));
        assert!(is_binary_file(binary.to_string_lossy().to_string()).expect("binary check"));
        let _ = std::fs::remove_file(text);
        let _ = std::fs::remove_file(binary);
        let _ = std::fs::remove_dir(root);
    }
}
