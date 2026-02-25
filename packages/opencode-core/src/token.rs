use dashmap::DashMap;
use memmap2::Mmap;
use napi::Result;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, OnceLock};

const BPE_CACHE_LIMIT: usize = 32;

static BPE_CACHE: OnceLock<DashMap<String, Arc<tiktoken_rs::CoreBPE>>> = OnceLock::new();

#[napi]
pub fn count_tokens(path: String, encoding: String) -> Result<i32> {
    let path = Path::new(&path);
    let metadata = std::fs::metadata(path).map_err(|e| napi::Error::from_reason(e.to_string()))?;

    let token_count: i32 = if metadata.len() > 100_000_000 {
        count_tokens_mmap(path, &encoding)?
    } else {
        let mut file = File::open(path).map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        count_tokens_from_text_internal(&contents, &encoding)?
    };

    Ok(token_count)
}

fn count_tokens_mmap(path: &Path, encoding: &str) -> Result<i32> {
    let file = File::open(path).map_err(|e| napi::Error::from_reason(e.to_string()))?;
    let mmap = unsafe { Mmap::map(&file).map_err(|e| napi::Error::from_reason(e.to_string()))? };

    let text = String::from_utf8_lossy(&mmap);
    count_tokens_from_text_internal(&text, encoding)
}

#[napi]
pub fn count_tokens_from_text(text: String, encoding: String) -> Result<i32> {
    count_tokens_from_text_internal(&text, &encoding)
}

fn count_tokens_from_text_internal(text: &str, encoding: &str) -> Result<i32> {
    let bpe = bpe_for(encoding)?;
    let tokens = bpe.encode_ordinary(text);
    Ok(tokens.len() as i32)
}

#[napi]
pub fn count_tokens_streaming(path: String, encoding: String, chunk_size: i32) -> Result<i32> {
    let path = Path::new(&path);
    let file = File::open(path).map_err(|e| napi::Error::from_reason(e.to_string()))?;
    let mmap = unsafe { Mmap::map(&file).map_err(|e| napi::Error::from_reason(e.to_string()))? };

    let bpe = bpe_for(&encoding)?;
    let mut total_tokens = 0;

    for chunk in mmap.chunks(chunk_size as usize) {
        let text = String::from_utf8_lossy(chunk);
        let tokens = bpe.encode_ordinary(&text);
        total_tokens += tokens.len();
    }

    Ok(total_tokens as i32)
}

fn bpe_for(encoding: &str) -> Result<Arc<tiktoken_rs::CoreBPE>> {
    let cache = BPE_CACHE.get_or_init(DashMap::new);
    if let Some(item) = cache.get(encoding) {
        return Ok(Arc::clone(item.value()));
    }

    let bpe = Arc::new(
        match encoding {
            "cl100k_base" => tiktoken_rs::cl100k_base(),
            "p50k_base" => tiktoken_rs::p50k_base(),
            "p50k_edit" => tiktoken_rs::p50k_edit(),
            "r50k_base" => tiktoken_rs::r50k_base(),
            _ => tiktoken_rs::get_bpe_from_model(encoding),
        }
        .map_err(|e| napi::Error::from_reason(e.to_string()))?,
    );

    if cache.len() >= BPE_CACHE_LIMIT {
        if let Some(first) = cache.iter().next() {
            let key = first.key().clone();
            cache.remove(&key);
        }
    }
    cache.insert(encoding.to_string(), Arc::clone(&bpe));
    Ok(bpe)
}
