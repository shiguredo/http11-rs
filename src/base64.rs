//! RFC 4648 Section 4 Base64 エンコード/デコード (依存なし実装)
//!
//! ## 仕様
//!
//! - 標準アルファベット (`A-Z`, `a-z`, `0-9`, `+`, `/`) を使う
//! - エンコード時は 4 文字単位になるよう末尾を `=` でパディングする
//! - デコード時は末尾の `=` を許容し、空白文字 (` `, `\t`, `\n`, `\r`) は無視する

use alloc::string::String;
use alloc::vec::Vec;

/// Base64 デコードのエラー
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Base64Error {
    /// アルファベット外の文字を検出した
    InvalidCharacter,
}

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 エンコード
pub(crate) fn encode(input: &[u8]) -> String {
    let mut result = String::new();
    let mut i = 0;

    while i < input.len() {
        let b0 = input[i];
        let b1 = if i + 1 < input.len() { input[i + 1] } else { 0 };
        let b2 = if i + 2 < input.len() { input[i + 2] } else { 0 };

        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);

        result.push(BASE64_ALPHABET[(n >> 18 & 0x3F) as usize] as char);
        result.push(BASE64_ALPHABET[(n >> 12 & 0x3F) as usize] as char);

        if i + 1 < input.len() {
            result.push(BASE64_ALPHABET[(n >> 6 & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        if i + 2 < input.len() {
            result.push(BASE64_ALPHABET[(n & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        i += 3;
    }

    result
}

/// Base64 デコード
pub(crate) fn decode(input: &str) -> Result<Vec<u8>, Base64Error> {
    let input = input.trim_end_matches('=');
    let mut result = Vec::new();

    let mut buf: u32 = 0;
    let mut bits: u32 = 0;

    for c in input.chars() {
        let val = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' => 62,
            '/' => 63,
            ' ' | '\t' | '\n' | '\r' => continue,
            _ => return Err(Base64Error::InvalidCharacter),
        };

        buf = (buf << 6) | val;
        bits += 6;

        if bits >= 8 {
            bits -= 8;
            result.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_basic() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"foob"), "Zm9vYg==");
        assert_eq!(encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(encode(b"user:password"), "dXNlcjpwYXNzd29yZA==");
    }

    #[test]
    fn decode_basic() {
        assert_eq!(decode("").unwrap(), b"");
        assert_eq!(decode("Zg==").unwrap(), b"f");
        assert_eq!(decode("Zm8=").unwrap(), b"fo");
        assert_eq!(decode("Zm9v").unwrap(), b"foo");
        assert_eq!(decode("Zm9vYg==").unwrap(), b"foob");
        assert_eq!(decode("Zm9vYmE=").unwrap(), b"fooba");
        assert_eq!(decode("Zm9vYmFy").unwrap(), b"foobar");
        assert_eq!(decode("dXNlcjpwYXNzd29yZA==").unwrap(), b"user:password");
    }

    #[test]
    fn decode_ignores_whitespace() {
        assert_eq!(decode("Zm9v\n").unwrap(), b"foo");
        assert_eq!(decode("Z m 9 v").unwrap(), b"foo");
        assert_eq!(decode("Zm9v\r\n").unwrap(), b"foo");
        assert_eq!(decode("Zm9v\t").unwrap(), b"foo");
    }

    #[test]
    fn decode_rejects_invalid_character() {
        assert_eq!(decode("Zm9*"), Err(Base64Error::InvalidCharacter));
        assert_eq!(decode("Z\u{00A0}m9v"), Err(Base64Error::InvalidCharacter));
    }
}
