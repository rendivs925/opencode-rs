use bincode;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use napi::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug)]
pub enum CacheValue {
    String(String),
    Bytes(Vec<u8>),
    GlobResults(Vec<String>),
    TokenCount(i32),
    IgnorePatterns(Vec<String>),
}

fn simple_hash(s: &str) -> u64 {
    let mut hash: u64 = 0;
    for (i, byte) in s.bytes().enumerate() {
        hash = hash.wrapping_add((byte as u64).wrapping_mul((i as u64).wrapping_add(1)));
        hash = hash.rotate_left(5);
    }
    hash
}

fn get_cache_path(cache_dir: &str, key: &str) -> PathBuf {
    let hash = format!("{:x}", simple_hash(key));
    PathBuf::from(cache_dir).join(format!("{}.cache", hash))
}

#[napi]
pub fn write_glob_cache(cache_dir: String, key: String, value: Vec<String>) -> Result<()> {
    let path = get_cache_path(&cache_dir, &key);

    fs::create_dir_all(&cache_dir).map_err(|e| napi::Error::from_reason(e.to_string()))?;

    let encoded: Vec<u8> = bincode::serialize(&CacheValue::GlobResults(value))
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    let mut compressor = GzEncoder::new(Vec::new(), Compression::default());
    compressor
        .write_all(&encoded)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    let compressed = compressor
        .finish()
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;

    fs::write(&path, compressed).map_err(|e| napi::Error::from_reason(e.to_string()))?;

    Ok(())
}

#[napi]
pub fn read_glob_cache(cache_dir: String, key: String) -> Result<Option<Vec<String>>> {
    let path = get_cache_path(&cache_dir, &key);

    if !path.exists() {
        return Ok(None);
    }

    let compressed = fs::read(&path).map_err(|e| napi::Error::from_reason(e.to_string()))?;
    let mut decoder = GzDecoder::new(&compressed[..]);
    let mut decompressed = Vec::new();
    decoder
        .read_to_end(&mut decompressed)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;

    match bincode::deserialize::<CacheValue>(&decompressed)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?
    {
        CacheValue::GlobResults(v) => Ok(Some(v)),
        _ => Ok(None),
    }
}

#[napi]
pub fn write_token_cache(cache_dir: String, key: String, value: i32) -> Result<()> {
    let path = get_cache_path(&cache_dir, &key);

    fs::create_dir_all(&cache_dir).map_err(|e| napi::Error::from_reason(e.to_string()))?;

    let encoded: Vec<u8> = bincode::serialize(&CacheValue::TokenCount(value))
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    let mut compressor = GzEncoder::new(Vec::new(), Compression::default());
    compressor
        .write_all(&encoded)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    let compressed = compressor
        .finish()
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;

    fs::write(&path, compressed).map_err(|e| napi::Error::from_reason(e.to_string()))?;

    Ok(())
}

#[napi]
pub fn read_token_cache(cache_dir: String, key: String) -> Result<Option<i32>> {
    let path = get_cache_path(&cache_dir, &key);

    if !path.exists() {
        return Ok(None);
    }

    let compressed = fs::read(&path).map_err(|e| napi::Error::from_reason(e.to_string()))?;
    let mut decoder = GzDecoder::new(&compressed[..]);
    let mut decompressed = Vec::new();
    decoder
        .read_to_end(&mut decompressed)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;

    match bincode::deserialize::<CacheValue>(&decompressed)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?
    {
        CacheValue::TokenCount(v) => Ok(Some(v)),
        _ => Ok(None),
    }
}

#[napi]
pub fn clear_cache(cache_dir: String) -> Result<()> {
    let dir = PathBuf::from(&cache_dir);
    for entry in fs::read_dir(&dir).map_err(|e| napi::Error::from_reason(e.to_string()))? {
        let entry = entry.map_err(|e| napi::Error::from_reason(e.to_string()))?;
        fs::remove_file(entry.path()).map_err(|e| napi::Error::from_reason(e.to_string()))?;
    }
    Ok(())
}

#[napi]
pub fn delete_cache(cache_dir: String, key: String) -> Result<()> {
    let path = get_cache_path(&cache_dir, &key);
    if path.exists() {
        fs::remove_file(path).map_err(|e| napi::Error::from_reason(e.to_string()))?;
    }
    Ok(())
}
