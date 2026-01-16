//! Content-Location ヘッダーパース (RFC 9110 Section 8.6)
//!
//! ## 概要
//!
//! RFC 9110 に基づいた Content-Location ヘッダーのパースを提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::content_location::ContentLocation;
//!
//! let cl = ContentLocation::parse("/assets/logo.png").unwrap();
//! assert_eq!(cl.uri().path(), "/assets/logo.png");
//! ```

use crate::uri::Uri;
use core::fmt;

/// Content-Location パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentLocationError {
    /// 空の入力
    Empty,
    /// 不正な URI
    InvalidUri,
}

impl fmt::Display for ContentLocationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentLocationError::Empty => write!(f, "empty Content-Location"),
            ContentLocationError::InvalidUri => write!(f, "invalid Content-Location URI"),
        }
    }
}

impl std::error::Error for ContentLocationError {}

/// Content-Location ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentLocation {
    uri: Uri,
}

impl ContentLocation {
    /// Content-Location ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, ContentLocationError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ContentLocationError::Empty);
        }

        let uri = Uri::parse(input).map_err(|_| ContentLocationError::InvalidUri)?;
        Ok(ContentLocation { uri })
    }

    /// パース済み URI
    pub fn uri(&self) -> &Uri {
        &self.uri
    }
}

impl fmt::Display for ContentLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.uri)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_absolute() {
        let cl = ContentLocation::parse("https://example.com/path").unwrap();
        assert_eq!(cl.uri().as_str(), "https://example.com/path");
    }

    #[test]
    fn parse_relative() {
        let cl = ContentLocation::parse("/assets/logo.png").unwrap();
        assert_eq!(cl.uri().path(), "/assets/logo.png");
    }

    #[test]
    fn parse_invalid() {
        assert!(ContentLocation::parse("").is_err());
        assert!(ContentLocation::parse("http://[::1").is_err());
    }
}
