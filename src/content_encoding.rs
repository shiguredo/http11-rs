//! Content-Encoding ヘッダーパース (RFC 9110 Section 8.4)
//!
//! ## 概要
//!
//! RFC 9110 に基づいた Content-Encoding ヘッダーのパースを提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::content_encoding::ContentEncoding;
//!
//! let ce = ContentEncoding::parse("gzip, deflate").unwrap();
//! assert!(ce.has_gzip());
//! assert!(ce.has_deflate());
//! ```

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::validate::{is_valid_token, trim_ows};

/// Content-Encoding パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContentEncodingError {
    /// 空の入力
    Empty,
    /// 不正な形式
    InvalidFormat,
    /// 不正なエンコーディングトークン
    InvalidEncoding,
}

impl fmt::Display for ContentEncodingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentEncodingError::Empty => write!(f, "empty Content-Encoding"),
            ContentEncodingError::InvalidFormat => {
                write!(f, "invalid Content-Encoding format")
            }
            ContentEncodingError::InvalidEncoding => {
                write!(f, "invalid Content-Encoding token")
            }
        }
    }
}

impl core::error::Error for ContentEncodingError {}

/// コンテント コーディング (Content Coding)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentCoding {
    Gzip,
    Deflate,
    Compress,
    Identity,
    Other(String),
}

impl ContentCoding {
    /// 正規化したトークン値
    pub fn as_str(&self) -> &str {
        match self {
            ContentCoding::Gzip => "gzip",
            ContentCoding::Deflate => "deflate",
            ContentCoding::Compress => "compress",
            ContentCoding::Identity => "identity",
            ContentCoding::Other(value) => value.as_str(),
        }
    }
}

/// Content-Encoding ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentEncoding {
    encodings: Vec<ContentCoding>,
}

impl ContentEncoding {
    /// Content-Encoding ヘッダーをパース
    ///
    /// RFC 9110 Section 5.6.1.2: 受信者は空のリスト要素を無視しなければならない (MUST)。
    /// 空の値は空リストとして受理する。
    pub fn parse(input: &str) -> Result<Self, ContentEncodingError> {
        let input = trim_ows(input);

        let mut encodings = Vec::new();
        if !input.is_empty() {
            for part in input.split(',') {
                let part = trim_ows(part);
                if part.is_empty() {
                    continue;
                }
                let coding = parse_coding(part)?;
                encodings.push(coding);
            }
        }

        Ok(ContentEncoding { encodings })
    }

    /// エンコーディング一覧
    pub fn encodings(&self) -> &[ContentCoding] {
        &self.encodings
    }

    /// gzip を含むかどうか
    pub fn has_gzip(&self) -> bool {
        self.encodings
            .iter()
            .any(|coding| matches!(coding, ContentCoding::Gzip))
    }

    /// deflate を含むかどうか
    pub fn has_deflate(&self) -> bool {
        self.encodings
            .iter()
            .any(|coding| matches!(coding, ContentCoding::Deflate))
    }

    /// compress を含むかどうか
    pub fn has_compress(&self) -> bool {
        self.encodings
            .iter()
            .any(|coding| matches!(coding, ContentCoding::Compress))
    }

    /// identity を含むかどうか
    pub fn has_identity(&self) -> bool {
        self.encodings
            .iter()
            .any(|coding| matches!(coding, ContentCoding::Identity))
    }
}

impl fmt::Display for ContentEncoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let values: Vec<&str> = self.encodings.iter().map(ContentCoding::as_str).collect();
        write!(f, "{}", values.join(", "))
    }
}

fn parse_coding(token: &str) -> Result<ContentCoding, ContentEncodingError> {
    if token.is_empty() {
        return Err(ContentEncodingError::InvalidFormat);
    }
    if !is_valid_token(token) {
        return Err(ContentEncodingError::InvalidEncoding);
    }

    let normalized = token.to_ascii_lowercase();
    let coding = match normalized.as_str() {
        "gzip" => ContentCoding::Gzip,
        "deflate" => ContentCoding::Deflate,
        "compress" => ContentCoding::Compress,
        "identity" => ContentCoding::Identity,
        _ => ContentCoding::Other(normalized),
    };

    Ok(coding)
}
