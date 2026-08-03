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
//! let digest = ContentDigest::parse("sha-256=:YWJj:").expect("Digest フィールドのパースは成功するはず (実装バグ)");
//! assert_eq!(digest.items()[0].algorithm(), "sha-256");
//!
//! let want = WantContentDigest::parse("sha-256=1, sha-512=3").expect("Digest フィールドのパースは成功するはず (実装バグ)");
//! assert_eq!(want.items().len(), 2);
//! ```

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::base64;
use crate::validate::{is_valid_token, trim_ows};

/// Digest Fields パースエラー
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl core::error::Error for DigestFieldsError {}

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
        write!(f, ":{}:", base64::encode(&self.bytes))
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
    let input = trim_ows(input);
    if input.is_empty() {
        return Err(DigestFieldsError::Empty);
    }

    let mut entries = Vec::new();
    for part in input.split(',') {
        let part = trim_ows(part);
        if part.is_empty() {
            // RFC 9110 Section 5.6.1.2: empty list element MUST be ignored
            continue;
        }

        let (algorithm, value) = part
            .split_once('=')
            .ok_or(DigestFieldsError::InvalidFormat)?;
        let algorithm = trim_ows(algorithm);
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
    let input = trim_ows(input);
    let rest = input
        .strip_prefix(':')
        .ok_or(DigestFieldsError::InvalidByteSequence)?;
    let end = rest
        .find(':')
        .ok_or(DigestFieldsError::InvalidByteSequence)?;
    let encoded = &rest[..end];
    if !trim_ows(&rest[end + 1..]).is_empty() {
        return Err(DigestFieldsError::InvalidByteSequence);
    }
    let bytes = base64::decode(encoded).map_err(|_| DigestFieldsError::InvalidBase64)?;
    Ok(DigestValue { bytes })
}

fn parse_preference(input: &str) -> Result<u8, DigestFieldsError> {
    let input = trim_ows(input);
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
