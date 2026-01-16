//! Trailer フィールドパース (RFC 9112 Section 7.1.2)
//!
//! ## 概要
//!
//! RFC 9112 に基づいた Trailer ヘッダーのパースを提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::trailer::Trailer;
//!
//! let trailer = Trailer::parse("Expires, X-Test").unwrap();
//! assert_eq!(trailer.fields().len(), 2);
//! ```

use core::fmt;

/// Trailer パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrailerError {
    /// 空の入力
    Empty,
    /// 不正な形式
    InvalidFormat,
    /// 不正なフィールド名トークン
    InvalidFieldName,
}

impl fmt::Display for TrailerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrailerError::Empty => write!(f, "empty Trailer header"),
            TrailerError::InvalidFormat => write!(f, "invalid Trailer header format"),
            TrailerError::InvalidFieldName => write!(f, "invalid Trailer field name"),
        }
    }
}

impl std::error::Error for TrailerError {}

/// Trailer ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trailer {
    fields: Vec<String>,
}

impl Trailer {
    /// Trailer ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, TrailerError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(TrailerError::Empty);
        }

        let mut fields = Vec::new();
        for part in input.split(',') {
            let name = part.trim();
            if name.is_empty() {
                return Err(TrailerError::InvalidFormat);
            }
            if !is_valid_token(name) {
                return Err(TrailerError::InvalidFieldName);
            }
            fields.push(name.to_ascii_lowercase());
        }

        if fields.is_empty() {
            return Err(TrailerError::Empty);
        }

        Ok(Trailer { fields })
    }

    /// Trailer フィールド名 (小文字)
    pub fn fields(&self) -> &[String] {
        &self.fields
    }
}

impl fmt::Display for Trailer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.fields.join(", "))
    }
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
    fn parse_fields() {
        let trailer = Trailer::parse("Expires, X-Test").unwrap();
        assert_eq!(
            trailer.fields(),
            &["expires".to_string(), "x-test".to_string()]
        );
    }

    #[test]
    fn parse_invalid() {
        assert!(Trailer::parse("").is_err());
        assert!(Trailer::parse("bad value").is_err());
    }

    #[test]
    fn display() {
        let trailer = Trailer::parse("Expires, X-Test").unwrap();
        assert_eq!(trailer.to_string(), "expires, x-test");
    }
}
