use napi::Result;

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

pub(crate) fn hash_bytes(bytes: &[u8]) -> u64 {
    bytes.iter().fold(FNV_OFFSET, |h, b| (h ^ (*b as u64)).wrapping_mul(FNV_PRIME))
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
