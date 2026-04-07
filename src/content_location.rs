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
    /// フラグメントは許可されない (RFC 9110 Section 8.7)
    FragmentNotAllowed,
}

impl fmt::Display for ContentLocationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentLocationError::Empty => write!(f, "empty Content-Location"),
            ContentLocationError::InvalidUri => write!(f, "invalid Content-Location URI"),
            ContentLocationError::FragmentNotAllowed => {
                write!(f, "Content-Location must not contain a fragment")
            }
        }
    }
}

impl core::error::Error for ContentLocationError {}

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

        // RFC 9110 Section 4.2.1/4.2.2: http/https URI は空 host を含んではならない (MUST NOT)
        if let Some(scheme) = uri.scheme()
            && (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
            && uri.host().is_none_or(|h| h.is_empty())
        {
            return Err(ContentLocationError::InvalidUri);
        }

        // RFC 9110 Section 8.7: Content-Location = absolute-URI / partial-URI
        // absolute-URI はフラグメントを含まない (RFC 3986 Section 4.3)
        // partial-URI もフラグメントを含まない (RFC 9110 Section 4)
        if uri.fragment().is_some() {
            return Err(ContentLocationError::FragmentNotAllowed);
        }

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

    #[test]
    fn parse_fragment_rejected() {
        assert_eq!(
            ContentLocation::parse("https://example.com/path#frag"),
            Err(ContentLocationError::FragmentNotAllowed)
        );
        assert_eq!(
            ContentLocation::parse("/path#frag"),
            Err(ContentLocationError::FragmentNotAllowed)
        );
    }
}
