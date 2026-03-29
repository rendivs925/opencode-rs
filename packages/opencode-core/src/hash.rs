use napi::Result;
use rayon::prelude::*;
use xxhash_rust::xxh3::xxh3_64;

pub(crate) fn hash_bytes(bytes: &[u8]) -> u64 {
    xxh3_64(bytes)
}

#[napi]
pub fn fast_hash(content: String) -> String {
    format!("{:016x}", hash_bytes(content.as_bytes()))
}

#[napi]
pub fn file_hash(path: String) -> Result<String> {
    let bytes = std::fs::read(path).map_err(|err| napi::Error::from_reason(err.to_string()))?;
    Ok(format!("{:016x}", hash_bytes(&bytes)))
}

#[napi(object)]
pub struct HashResult {
    pub path: String,
    pub hash: Option<String>,
    pub error: Option<String>,
}

#[napi]
pub fn hash_multiple_files(paths: Vec<String>) -> Vec<HashResult> {
    paths
        .into_par_iter()
        .map(|path| match std::fs::read(&path) {
            Ok(bytes) => HashResult {
                path,
                hash: Some(format!("{:016x}", hash_bytes(&bytes))),
                error: None,
            },
            Err(e) => HashResult {
                path,
                hash: None,
                error: Some(e.to_string()),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_for_same_input() {
        let a = fast_hash("hello world".to_string());
        let b = fast_hash("hello world".to_string());
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn hash_changes_for_different_input() {
        let a = fast_hash("hello world".to_string());
        let b = fast_hash("hello rust".to_string());
        assert_ne!(a, b);
    }
}
