//! Vary ヘッダーパース (RFC 9110 Section 12.5.5)
//!
//! ## 概要
//!
//! RFC 9110 に基づいた Vary ヘッダーのパースを提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::vary::Vary;
//!
//! let vary = Vary::parse("Accept-Encoding, User-Agent").unwrap();
//! assert_eq!(vary.fields().len(), 2);
//! ```

use core::fmt;

/// Vary パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaryError {
    /// 空の入力
    Empty,
    /// 不正な形式
    InvalidFormat,
    /// 不正なフィールド名トークン
    InvalidFieldName,
}

impl fmt::Display for VaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VaryError::Empty => write!(f, "empty Vary header"),
            VaryError::InvalidFormat => write!(f, "invalid Vary header format"),
            VaryError::InvalidFieldName => write!(f, "invalid Vary header field name"),
        }
    }
}

impl std::error::Error for VaryError {}

/// Vary ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vary {
    any: bool,
    fields: Vec<String>,
}

impl Vary {
    /// Vary ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, VaryError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(VaryError::Empty);
        }

        if input == "*" {
            return Ok(Vary {
                any: true,
                fields: Vec::new(),
            });
        }

        let mut fields = Vec::new();
        for part in input.split(',') {
            let name = part.trim();
            if name.is_empty() {
                return Err(VaryError::InvalidFormat);
            }
            if !is_valid_token(name) {
                return Err(VaryError::InvalidFieldName);
            }
            fields.push(name.to_ascii_lowercase());
        }

        if fields.is_empty() {
            return Err(VaryError::Empty);
        }

        Ok(Vary { any: false, fields })
    }

    /// Vary が "*" かどうか
    pub fn is_any(&self) -> bool {
        self.any
    }

    /// フィールド名 (小文字)
    pub fn fields(&self) -> &[String] {
        &self.fields
    }
}

impl fmt::Display for Vary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.any {
            return write!(f, "*");
        }
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
    fn parse_any() {
        let vary = Vary::parse("*").unwrap();
        assert!(vary.is_any());
    }

    #[test]
    fn parse_fields() {
        let vary = Vary::parse("Accept-Encoding, User-Agent").unwrap();
        assert_eq!(
            vary.fields(),
            &["accept-encoding".to_string(), "user-agent".to_string()]
        );
    }

    #[test]
    fn parse_invalid() {
        assert!(Vary::parse("").is_err());
        assert!(Vary::parse("bad value").is_err());
    }
}
