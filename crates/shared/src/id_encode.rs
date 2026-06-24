//! Movie ID encoding: `mv` prefix + base36 integer.
//!
//! Public URL format: `/movie/mv16` (ID 42), `/movie/mvrs` (ID 1000).
//! The underlying integer primary key is never exposed directly in URLs.
//! This module is `no_std`-friendly: no external deps, works in WASM.
//!
//! Design notes:
//! - `mv` prefix has no coupling to the app name — it stands for "movie"
//! - base36 (digits + lowercase letters) makes IDs short and URL-safe
//! - Decoding is built into the Rust std: `u64::from_str_radix(s, 36)`

/// Encode a database integer ID to the external `mv`+base36 format.
///
/// ```
/// use gem_finder_shared::id_encode::encode_movie_id;
/// assert_eq!(encode_movie_id(42),    "mv16");
/// assert_eq!(encode_movie_id(1000),  "mvrs");
/// assert_eq!(encode_movie_id(1),     "mv1");
/// assert_eq!(encode_movie_id(0),     "mv0");
/// ```
pub fn encode_movie_id(id: i64) -> String {
    format!("mv{}", to_base36(id as u64))
}

/// Decode an `mv`+base36 string back to the integer ID.
/// Returns `None` if the string is missing the `mv` prefix or contains
/// characters outside base36.
///
/// ```
/// use gem_finder_shared::id_encode::decode_movie_id;
/// assert_eq!(decode_movie_id("mv16"),  Some(42));
/// assert_eq!(decode_movie_id("mvrs"),  Some(1000));
/// assert_eq!(decode_movie_id("42"),    None);  // no prefix
/// assert_eq!(decode_movie_id("mv"),    None);  // empty after prefix
/// assert_eq!(decode_movie_id("mvZZ!"), None);  // invalid chars
/// ```
pub fn decode_movie_id(s: &str) -> Option<i64> {
    let base36 = s.strip_prefix("mv")?;
    if base36.is_empty() {
        return None;
    }
    u64::from_str_radix(base36, 36).ok().map(|n| n as i64)
}

fn to_base36(mut n: u64) -> String {
    if n == 0 {
        return "0".to_string();
    }
    const CHARS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut result = Vec::new();
    while n > 0 {
        result.push(CHARS[(n % 36) as usize] as char);
        n /= 36;
    }
    result.reverse();
    result.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for id in [0, 1, 42, 999, 1000, 65535, 100_000, i32::MAX as i64] {
            let encoded = encode_movie_id(id);
            assert!(encoded.starts_with("mv"), "no mv prefix: {}", encoded);
            assert_eq!(
                decode_movie_id(&encoded),
                Some(id),
                "roundtrip failed for {}",
                id
            );
        }
    }

    #[test]
    fn rejects_invalid() {
        assert_eq!(decode_movie_id("42"), None);
        assert_eq!(decode_movie_id("mv"), None);
        assert_eq!(decode_movie_id("gf0000042"), None);
        assert_eq!(decode_movie_id(""), None);
    }
}
