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

use core::fmt;

/// Content-Encoding パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
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

impl std::error::Error for ContentEncodingError {}

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
    pub fn parse(input: &str) -> Result<Self, ContentEncodingError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ContentEncodingError::Empty);
        }

        let mut encodings = Vec::new();
        for part in input.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let coding = parse_coding(part)?;
            encodings.push(coding);
        }

        if encodings.is_empty() {
            return Err(ContentEncodingError::Empty);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single() {
        let ce = ContentEncoding::parse("gzip").unwrap();
        assert_eq!(ce.encodings().len(), 1);
        assert!(ce.has_gzip());
    }

    #[test]
    fn parse_multiple() {
        let ce = ContentEncoding::parse("gzip, deflate, identity").unwrap();
        assert_eq!(ce.encodings().len(), 3);
        assert!(ce.has_deflate());
        assert!(ce.has_identity());
    }

    #[test]
    fn parse_unknown() {
        let ce = ContentEncoding::parse("br").unwrap();
        assert_eq!(ce.encodings().len(), 1);
        assert_eq!(ce.encodings()[0], ContentCoding::Other("br".to_string()));
    }

    #[test]
    fn parse_invalid() {
        assert!(ContentEncoding::parse("").is_err());
        assert!(ContentEncoding::parse("gzip,").is_ok());
        assert!(ContentEncoding::parse("g zip").is_err());
    }

    #[test]
    fn display() {
        let ce = ContentEncoding::parse("GZIP, Deflate").unwrap();
        assert_eq!(ce.to_string(), "gzip, deflate");
    }
}
