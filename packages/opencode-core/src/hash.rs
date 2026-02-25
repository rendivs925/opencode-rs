use napi::Result;
use std::hash::{Hash, Hasher};

#[napi]
pub fn fast_hash(content: String) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[napi]
pub fn file_hash(path: String) -> Result<String> {
    let bytes = std::fs::read(path).map_err(|err| napi::Error::from_reason(err.to_string()))?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    Ok(format!("{:016x}", hasher.finish()))
}
