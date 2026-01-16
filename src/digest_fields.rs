//! Digest Fields (RFC 9530)
//!
//! ## 概要
//!
//! RFC 9530 に基づいた Content-Digest / Repr-Digest / Want-Content-Digest /
//! Want-Repr-Digest のパースを提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::digest_fields::{ContentDigest, WantContentDigest};
//!
//! let digest = ContentDigest::parse("sha-256=:YWJj:").unwrap();
//! assert_eq!(digest.items()[0].algorithm(), "sha-256");
//!
//! let want = WantContentDigest::parse("sha-256=1, sha-512=3").unwrap();
//! assert_eq!(want.items().len(), 2);
//! ```

use core::fmt;

/// Digest Fields パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DigestFieldsError {
    /// 空の入力
    Empty,
    /// 不正な形式
    InvalidFormat,
    /// 不正なアルゴリズム
    InvalidAlgorithm,
    /// 不正なバイト列
    InvalidByteSequence,
    /// Base64 デコードエラー
    InvalidBase64,
    /// 不正な優先度
    InvalidPreference,
}

impl fmt::Display for DigestFieldsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DigestFieldsError::Empty => write!(f, "empty digest field"),
            DigestFieldsError::InvalidFormat => write!(f, "invalid digest field format"),
            DigestFieldsError::InvalidAlgorithm => write!(f, "invalid digest algorithm"),
            DigestFieldsError::InvalidByteSequence => write!(f, "invalid digest byte sequence"),
            DigestFieldsError::InvalidBase64 => write!(f, "invalid digest base64"),
            DigestFieldsError::InvalidPreference => write!(f, "invalid digest preference"),
        }
    }
}

impl std::error::Error for DigestFieldsError {}

/// Digest 値
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestValue {
    bytes: Vec<u8>,
}

impl DigestValue {
    /// バイト列
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl fmt::Display for DigestValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, ":{}:", base64_encode(&self.bytes))
    }
}

/// Digest エントリ
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestEntry {
    algorithm: String,
    value: DigestValue,
}

impl DigestEntry {
    /// アルゴリズム名
    pub fn algorithm(&self) -> &str {
        &self.algorithm
    }

    /// Digest 値
    pub fn value(&self) -> &DigestValue {
        &self.value
    }
}

impl fmt::Display for DigestEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", self.algorithm, self.value)
    }
}

/// Content-Digest ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentDigest {
    items: Vec<DigestEntry>,
}

impl ContentDigest {
    /// Content-Digest ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, DigestFieldsError> {
        let items = parse_digest_dictionary(input)?;
        Ok(ContentDigest { items })
    }

    /// Digest 一覧
    pub fn items(&self) -> &[DigestEntry] {
        &self.items
    }

    /// アルゴリズム指定で取得
    pub fn get(&self, algorithm: &str) -> Option<&DigestValue> {
        let key = algorithm.to_ascii_lowercase();
        self.items
            .iter()
            .find(|item| item.algorithm == key)
            .map(|item| &item.value)
    }
}

impl fmt::Display for ContentDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let values: Vec<String> = self.items.iter().map(|item| item.to_string()).collect();
        write!(f, "{}", values.join(", "))
    }
}

/// Repr-Digest ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReprDigest {
    items: Vec<DigestEntry>,
}

impl ReprDigest {
    /// Repr-Digest ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, DigestFieldsError> {
        let items = parse_digest_dictionary(input)?;
        Ok(ReprDigest { items })
    }

    /// Digest 一覧
    pub fn items(&self) -> &[DigestEntry] {
        &self.items
    }

    /// アルゴリズム指定で取得
    pub fn get(&self, algorithm: &str) -> Option<&DigestValue> {
        let key = algorithm.to_ascii_lowercase();
        self.items
            .iter()
            .find(|item| item.algorithm == key)
            .map(|item| &item.value)
    }
}

impl fmt::Display for ReprDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let values: Vec<String> = self.items.iter().map(|item| item.to_string()).collect();
        write!(f, "{}", values.join(", "))
    }
}

/// Digest 優先度
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestPreference {
    algorithm: String,
    weight: u8,
}

impl DigestPreference {
    /// アルゴリズム名
    pub fn algorithm(&self) -> &str {
        &self.algorithm
    }

    /// 優先度 (0-10)
    pub fn weight(&self) -> u8 {
        self.weight
    }
}

impl fmt::Display for DigestPreference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", self.algorithm, self.weight)
    }
}

/// Want-Content-Digest ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WantContentDigest {
    items: Vec<DigestPreference>,
}

impl WantContentDigest {
    /// Want-Content-Digest ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, DigestFieldsError> {
        let items = parse_preference_dictionary(input)?;
        Ok(WantContentDigest { items })
    }

    /// 優先度一覧
    pub fn items(&self) -> &[DigestPreference] {
        &self.items
    }

    /// アルゴリズム指定で取得
    pub fn get(&self, algorithm: &str) -> Option<u8> {
        let key = algorithm.to_ascii_lowercase();
        self.items
            .iter()
            .find(|item| item.algorithm == key)
            .map(|item| item.weight)
    }
}

impl fmt::Display for WantContentDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let values: Vec<String> = self.items.iter().map(|item| item.to_string()).collect();
        write!(f, "{}", values.join(", "))
    }
}

/// Want-Repr-Digest ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WantReprDigest {
    items: Vec<DigestPreference>,
}

impl WantReprDigest {
    /// Want-Repr-Digest ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, DigestFieldsError> {
        let items = parse_preference_dictionary(input)?;
        Ok(WantReprDigest { items })
    }

    /// 優先度一覧
    pub fn items(&self) -> &[DigestPreference] {
        &self.items
    }

    /// アルゴリズム指定で取得
    pub fn get(&self, algorithm: &str) -> Option<u8> {
        let key = algorithm.to_ascii_lowercase();
        self.items
            .iter()
            .find(|item| item.algorithm == key)
            .map(|item| item.weight)
    }
}

impl fmt::Display for WantReprDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let values: Vec<String> = self.items.iter().map(|item| item.to_string()).collect();
        write!(f, "{}", values.join(", "))
    }
}

fn parse_digest_dictionary(input: &str) -> Result<Vec<DigestEntry>, DigestFieldsError> {
    let entries = parse_dictionary(input, parse_byte_sequence)?;
    Ok(entries
        .into_iter()
        .map(|(algorithm, value)| DigestEntry { algorithm, value })
        .collect())
}

fn parse_preference_dictionary(input: &str) -> Result<Vec<DigestPreference>, DigestFieldsError> {
    let entries = parse_dictionary(input, parse_preference)?;
    Ok(entries
        .into_iter()
        .map(|(algorithm, weight)| DigestPreference { algorithm, weight })
        .collect())
}

fn parse_dictionary<T>(
    input: &str,
    value_parser: fn(&str) -> Result<T, DigestFieldsError>,
) -> Result<Vec<(String, T)>, DigestFieldsError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(DigestFieldsError::Empty);
    }

    let mut entries = Vec::new();
    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err(DigestFieldsError::InvalidFormat);
        }

        let (algorithm, value) = part
            .split_once('=')
            .ok_or(DigestFieldsError::InvalidFormat)?;
        let algorithm = algorithm.trim();
        if algorithm.is_empty() {
            return Err(DigestFieldsError::InvalidAlgorithm);
        }
        if !is_valid_token(algorithm) {
            return Err(DigestFieldsError::InvalidAlgorithm);
        }
        let value = value_parser(value)?;
        entries.push((algorithm.to_ascii_lowercase(), value));
    }

    if entries.is_empty() {
        return Err(DigestFieldsError::Empty);
    }

    Ok(entries)
}

fn parse_byte_sequence(input: &str) -> Result<DigestValue, DigestFieldsError> {
    let input = input.trim();
    let rest = input
        .strip_prefix(':')
        .ok_or(DigestFieldsError::InvalidByteSequence)?;
    let end = rest
        .find(':')
        .ok_or(DigestFieldsError::InvalidByteSequence)?;
    let base64 = &rest[..end];
    if !rest[end + 1..].trim().is_empty() {
        return Err(DigestFieldsError::InvalidByteSequence);
    }
    let bytes = base64_decode(base64)?;
    Ok(DigestValue { bytes })
}

fn parse_preference(input: &str) -> Result<u8, DigestFieldsError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(DigestFieldsError::InvalidPreference);
    }
    if !input.chars().all(|c| c.is_ascii_digit()) {
        return Err(DigestFieldsError::InvalidPreference);
    }
    let value: u8 = input
        .parse()
        .map_err(|_| DigestFieldsError::InvalidPreference)?;
    if value > 10 {
        return Err(DigestFieldsError::InvalidPreference);
    }
    Ok(value)
}

fn is_valid_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_token_char)
}

fn is_token_char(b: u8) -> bool {
    matches!(
        b,
        b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.' |
        b'0'..=b'9' | b'A'..=b'Z' | b'^' | b'_' | b'`' | b'a'..=b'z' | b'|' | b'~'
    )
}

// Base64 エンコード/デコード (依存なし実装)

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 エンコード
fn base64_encode(input: &[u8]) -> String {
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
fn base64_decode(input: &str) -> Result<Vec<u8>, DigestFieldsError> {
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
            ' ' | '\t' | '\n' | '\r' => continue, // 空白は無視
            _ => return Err(DigestFieldsError::InvalidBase64),
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
    fn parse_content_digest() {
        let digest = ContentDigest::parse("sha-256=:YWJj:").unwrap();
        assert_eq!(digest.items().len(), 1);
        assert_eq!(digest.items()[0].algorithm(), "sha-256");
        assert_eq!(digest.items()[0].value().bytes(), b"abc");
    }

    #[test]
    fn parse_repr_digest_multiple() {
        let digest = ReprDigest::parse("sha-256=:YWJj:, sha-512=:Zg==:").unwrap();
        assert_eq!(digest.items().len(), 2);
    }

    #[test]
    fn parse_want_digest() {
        let want = WantContentDigest::parse("sha-512=3, sha-256=10, unixsum=0").unwrap();
        assert_eq!(want.items().len(), 3);
        assert_eq!(want.get("sha-256"), Some(10));
        assert_eq!(want.get("unixsum"), Some(0));
    }

    #[test]
    fn parse_invalid() {
        assert!(ContentDigest::parse("").is_err());
        assert!(ContentDigest::parse("sha-256=YWJj").is_err());
        assert!(ContentDigest::parse("sha-256=:bad*:").is_err());
        assert!(WantReprDigest::parse("sha-256=11").is_err());
    }

    #[test]
    fn display() {
        let digest = ContentDigest::parse("sha-256=:YWJj:").unwrap();
        assert_eq!(digest.to_string(), "sha-256=:YWJj:");
    }
}
