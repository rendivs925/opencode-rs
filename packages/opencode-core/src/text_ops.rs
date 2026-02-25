use napi::Result;
use std::fs::File;
use std::io::Read;

const BINARY_SAMPLE_BYTES: usize = 8192;

#[napi]
pub fn count_lines_fast(text: String) -> u32 {
    if text.is_empty() {
        return 0;
    }
    let bytes = text.as_bytes();
    let count = if cfg!(target_arch = "x86_64") && is_avx2_available() {
        // SAFETY: gated by runtime AVX2 feature detection and x86_64 target.
        unsafe { count_byte_avx2(bytes, b'\n') }
    } else if cfg!(target_arch = "aarch64") && is_neon_available() {
        // SAFETY: gated by runtime NEON feature detection and aarch64 target.
        unsafe { count_byte_neon(bytes, b'\n') }
    } else {
        memchr::memchr_iter(b'\n', bytes).count()
    };
    count as u32 + 1
}

#[napi]
pub fn find_whitespace_indices(text: String) -> Vec<u32> {
    let bytes = text.as_bytes();
    if text.is_ascii() {
        if cfg!(target_arch = "x86_64") && is_avx2_available() {
            // SAFETY: gated by runtime AVX2 feature detection and x86_64 target.
            return unsafe { find_ascii_whitespace_indices_avx2(bytes) };
        }
        if cfg!(target_arch = "aarch64") && is_neon_available() {
            // SAFETY: gated by runtime NEON feature detection and aarch64 target.
            return unsafe { find_ascii_whitespace_indices_neon(bytes) };
        }
        return find_ascii_whitespace_indices_scalar(bytes);
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
    let sample = &bytes[..read];
    let has_null = if cfg!(target_arch = "x86_64") && is_avx2_available() {
        // SAFETY: gated by runtime AVX2 feature detection and x86_64 target.
        unsafe { has_zero_byte_avx2(sample) }
    } else if cfg!(target_arch = "aarch64") && is_neon_available() {
        // SAFETY: gated by runtime NEON feature detection and aarch64 target.
        unsafe { has_zero_byte_neon(sample) }
    } else {
        memchr::memchr(0, sample).is_some()
    };
    Ok(has_null)
}

#[napi]
pub fn case_fold_ascii(text: String) -> String {
    if text.is_empty() {
        return text;
    }
    let mut out = text.into_bytes();
    if cfg!(target_arch = "x86_64") && is_avx2_available() {
        // SAFETY: runtime AVX2 feature check gates x86_64 SIMD usage.
        unsafe { lowercase_ascii_avx2(&mut out) };
    } else if cfg!(target_arch = "aarch64") && is_neon_available() {
        // SAFETY: runtime NEON feature check gates aarch64 SIMD usage.
        unsafe { lowercase_ascii_neon(&mut out) };
    } else {
        lowercase_ascii_scalar(&mut out);
    }
    // SAFETY: ASCII lowercase transform preserves valid UTF-8.
    unsafe { String::from_utf8_unchecked(out) }
}

fn is_ascii_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\n' | b'\r' | b'\t' | 0x0b | 0x0c)
}

fn find_ascii_whitespace_indices_scalar(bytes: &[u8]) -> Vec<u32> {
    bytes
        .iter()
        .enumerate()
        .filter_map(|(idx, byte)| is_ascii_whitespace(*byte).then_some(idx as u32))
        .collect()
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
unsafe fn count_byte_avx2(bytes: &[u8], needle: u8) -> usize {
    use std::arch::x86_64::*;

    let mut count = 0usize;
    let mut i = 0usize;
    let step = 32usize;
    let lane = _mm256_set1_epi8(needle as i8);
    while i + step <= bytes.len() {
        let ptr = bytes.as_ptr().add(i) as *const __m256i;
        let chunk = _mm256_loadu_si256(ptr);
        let eq = _mm256_cmpeq_epi8(chunk, lane);
        let mask = _mm256_movemask_epi8(eq) as u32;
        count += mask.count_ones() as usize;
        i += step;
    }
    count + memchr::memchr_iter(needle, &bytes[i..]).count()
}

#[cfg(not(target_arch = "x86_64"))]
unsafe fn count_byte_avx2(bytes: &[u8], needle: u8) -> usize {
    memchr::memchr_iter(needle, bytes).count()
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn count_byte_neon(bytes: &[u8], needle: u8) -> usize {
    use std::arch::aarch64::*;

    let mut count = 0usize;
    let mut i = 0usize;
    let step = 16usize;
    let lane = vdupq_n_u8(needle);
    while i + step <= bytes.len() {
        let ptr = bytes.as_ptr().add(i);
        let chunk = vld1q_u8(ptr);
        let eq = vceqq_u8(chunk, lane);
        let mut tmp = [0u8; 16];
        vst1q_u8(tmp.as_mut_ptr(), eq);
        count += tmp.iter().filter(|item| **item == 0xff).count();
        i += step;
    }
    count + memchr::memchr_iter(needle, &bytes[i..]).count()
}

#[cfg(not(target_arch = "aarch64"))]
unsafe fn count_byte_neon(bytes: &[u8], needle: u8) -> usize {
    memchr::memchr_iter(needle, bytes).count()
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn has_zero_byte_avx2(bytes: &[u8]) -> bool {
    use std::arch::x86_64::*;

    let mut i = 0usize;
    let step = 32usize;
    let zero = _mm256_setzero_si256();
    while i + step <= bytes.len() {
        let ptr = bytes.as_ptr().add(i) as *const __m256i;
        let chunk = _mm256_loadu_si256(ptr);
        let eq = _mm256_cmpeq_epi8(chunk, zero);
        if _mm256_movemask_epi8(eq) != 0 {
            return true;
        }
        i += step;
    }
    memchr::memchr(0, &bytes[i..]).is_some()
}

#[cfg(not(target_arch = "x86_64"))]
unsafe fn has_zero_byte_avx2(bytes: &[u8]) -> bool {
    memchr::memchr(0, bytes).is_some()
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn has_zero_byte_neon(bytes: &[u8]) -> bool {
    use std::arch::aarch64::*;

    let mut i = 0usize;
    let step = 16usize;
    let zero = vdupq_n_u8(0);
    while i + step <= bytes.len() {
        let ptr = bytes.as_ptr().add(i);
        let chunk = vld1q_u8(ptr);
        let eq = vceqq_u8(chunk, zero);
        let mut tmp = [0u8; 16];
        vst1q_u8(tmp.as_mut_ptr(), eq);
        if tmp.iter().any(|item| *item == 0xff) {
            return true;
        }
        i += step;
    }
    memchr::memchr(0, &bytes[i..]).is_some()
}

#[cfg(not(target_arch = "aarch64"))]
unsafe fn has_zero_byte_neon(bytes: &[u8]) -> bool {
    memchr::memchr(0, bytes).is_some()
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn find_ascii_whitespace_indices_avx2(bytes: &[u8]) -> Vec<u32> {
    use std::arch::x86_64::*;

    let mut out = Vec::<u32>::new();
    let mut i = 0usize;
    let step = 32usize;

    let s = _mm256_set1_epi8(b' ' as i8);
    let n = _mm256_set1_epi8(b'\n' as i8);
    let r = _mm256_set1_epi8(b'\r' as i8);
    let t = _mm256_set1_epi8(b'\t' as i8);
    let v = _mm256_set1_epi8(0x0b_i8);
    let f = _mm256_set1_epi8(0x0c_i8);

    while i + step <= bytes.len() {
        let ptr = bytes.as_ptr().add(i) as *const __m256i;
        let chunk = _mm256_loadu_si256(ptr);
        let mut ws = _mm256_cmpeq_epi8(chunk, s);
        ws = _mm256_or_si256(ws, _mm256_cmpeq_epi8(chunk, n));
        ws = _mm256_or_si256(ws, _mm256_cmpeq_epi8(chunk, r));
        ws = _mm256_or_si256(ws, _mm256_cmpeq_epi8(chunk, t));
        ws = _mm256_or_si256(ws, _mm256_cmpeq_epi8(chunk, v));
        ws = _mm256_or_si256(ws, _mm256_cmpeq_epi8(chunk, f));
        let mut mask = _mm256_movemask_epi8(ws) as u32;
        while mask != 0 {
            let bit = mask.trailing_zeros() as usize;
            out.push((i + bit) as u32);
            mask &= mask - 1;
        }
        i += step;
    }

    bytes[i..]
        .iter()
        .enumerate()
        .filter_map(|(idx, byte)| is_ascii_whitespace(*byte).then_some((i + idx) as u32))
        .for_each(|idx| out.push(idx));
    out
}

#[cfg(not(target_arch = "x86_64"))]
unsafe fn find_ascii_whitespace_indices_avx2(bytes: &[u8]) -> Vec<u32> {
    find_ascii_whitespace_indices_scalar(bytes)
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn find_ascii_whitespace_indices_neon(bytes: &[u8]) -> Vec<u32> {
    use std::arch::aarch64::*;

    let mut out = Vec::<u32>::new();
    let mut i = 0usize;
    let step = 16usize;
    let s = vdupq_n_u8(b' ');
    let n = vdupq_n_u8(b'\n');
    let r = vdupq_n_u8(b'\r');
    let t = vdupq_n_u8(b'\t');
    let v = vdupq_n_u8(0x0b);
    let f = vdupq_n_u8(0x0c);
    while i + step <= bytes.len() {
        let ptr = bytes.as_ptr().add(i);
        let chunk = vld1q_u8(ptr);
        let e0 = vceqq_u8(chunk, s);
        let e1 = vceqq_u8(chunk, n);
        let e2 = vceqq_u8(chunk, r);
        let e3 = vceqq_u8(chunk, t);
        let e4 = vceqq_u8(chunk, v);
        let e5 = vceqq_u8(chunk, f);
        let mut ws = vorrq_u8(e0, e1);
        ws = vorrq_u8(ws, e2);
        ws = vorrq_u8(ws, e3);
        ws = vorrq_u8(ws, e4);
        ws = vorrq_u8(ws, e5);
        let mut tmp = [0u8; 16];
        vst1q_u8(tmp.as_mut_ptr(), ws);
        tmp.iter()
            .enumerate()
            .filter_map(|(idx, item)| (*item == 0xff).then_some((i + idx) as u32))
            .for_each(|idx| out.push(idx));
        i += step;
    }
    bytes[i..]
        .iter()
        .enumerate()
        .filter_map(|(idx, byte)| is_ascii_whitespace(*byte).then_some((i + idx) as u32))
        .for_each(|idx| out.push(idx));
    out
}

#[cfg(not(target_arch = "aarch64"))]
unsafe fn find_ascii_whitespace_indices_neon(bytes: &[u8]) -> Vec<u32> {
    find_ascii_whitespace_indices_scalar(bytes)
}

fn lowercase_ascii_scalar(bytes: &mut [u8]) {
    bytes.iter_mut().for_each(|item| {
        if item.is_ascii_uppercase() {
            *item |= 0x20;
        }
    });
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn lowercase_ascii_avx2(bytes: &mut [u8]) {
    use std::arch::x86_64::*;

    let mut i = 0usize;
    let step = 32usize;
    let a = _mm256_set1_epi8((b'A' - 1) as i8);
    let z = _mm256_set1_epi8((b'Z' + 1) as i8);
    let bit = _mm256_set1_epi8(0x20_i8);
    while i + step <= bytes.len() {
        let ptr = bytes.as_mut_ptr().add(i) as *mut __m256i;
        let chunk = _mm256_loadu_si256(ptr);
        let gt_a = _mm256_cmpgt_epi8(chunk, a);
        let lt_z = _mm256_cmpgt_epi8(z, chunk);
        let upper = _mm256_and_si256(gt_a, lt_z);
        let lower = _mm256_or_si256(chunk, _mm256_and_si256(upper, bit));
        _mm256_storeu_si256(ptr, lower);
        i += step;
    }
    lowercase_ascii_scalar(&mut bytes[i..]);
}

#[cfg(not(target_arch = "x86_64"))]
unsafe fn lowercase_ascii_avx2(bytes: &mut [u8]) {
    lowercase_ascii_scalar(bytes);
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn lowercase_ascii_neon(bytes: &mut [u8]) {
    use std::arch::aarch64::*;

    let mut i = 0usize;
    let step = 16usize;
    let a = vdupq_n_u8(b'A');
    let z = vdupq_n_u8(b'Z');
    let bit = vdupq_n_u8(0x20);
    while i + step <= bytes.len() {
        let ptr = bytes.as_mut_ptr().add(i);
        let chunk = vld1q_u8(ptr);
        let ge = vcgeq_u8(chunk, a);
        let le = vcleq_u8(chunk, z);
        let upper = vandq_u8(ge, le);
        let lower = vorrq_u8(chunk, vandq_u8(upper, bit));
        vst1q_u8(ptr, lower);
        i += step;
    }
    lowercase_ascii_scalar(&mut bytes[i..]);
}

#[cfg(not(target_arch = "aarch64"))]
unsafe fn lowercase_ascii_neon(bytes: &mut [u8]) {
    lowercase_ascii_scalar(bytes);
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

    #[test]
    fn case_fold_ascii_lowercases_ascii_only() {
        assert_eq!(case_fold_ascii("ABC xyz 123".to_string()), "abc xyz 123");
        assert_eq!(case_fold_ascii("RuSt-NEON_AVX2".to_string()), "rust-neon_avx2");
    }

    #[test]
    fn case_fold_ascii_leaves_non_ascii_unchanged() {
        assert_eq!(case_fold_ascii("Résumé Σ".to_string()), "résumé Σ");
    }
}
