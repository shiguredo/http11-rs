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
    /// 禁止フィールド (RFC 9112 Section 7.1.2)
    ProhibitedField(String),
}

impl fmt::Display for TrailerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrailerError::Empty => write!(f, "empty Trailer header"),
            TrailerError::InvalidFormat => write!(f, "invalid Trailer header format"),
            TrailerError::InvalidFieldName => write!(f, "invalid Trailer field name"),
            TrailerError::ProhibitedField(name) => {
                write!(f, "prohibited trailer field: {}", name)
            }
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
    ///
    /// RFC 9112 Section 7.1.2: 禁止フィールドを含む場合はエラー
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
            let lower_name = name.to_ascii_lowercase();

            // RFC 9112 Section 7.1.2: 禁止フィールドチェック
            if is_prohibited_trailer_field(&lower_name) {
                return Err(TrailerError::ProhibitedField(lower_name));
            }

            fields.push(lower_name);
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

/// RFC 9112 Section 7.1.2: トレーラーで禁止されているフィールドかどうか
///
/// 禁止フィールド:
/// - Transfer-Encoding
/// - Content-Length
/// - Host
/// - Trailer
/// - Content-Encoding
/// - Content-Type
/// - Content-Range
pub fn is_prohibited_trailer_field(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "transfer-encoding"
            | "content-length"
            | "host"
            | "trailer"
            | "content-encoding"
            | "content-type"
            | "content-range"
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

    #[test]
    fn prohibited_field_transfer_encoding() {
        let result = Trailer::parse("Transfer-Encoding");
        assert!(matches!(
            result,
            Err(TrailerError::ProhibitedField(ref name)) if name == "transfer-encoding"
        ));
    }

    #[test]
    fn prohibited_field_content_length() {
        let result = Trailer::parse("Content-Length");
        assert!(matches!(
            result,
            Err(TrailerError::ProhibitedField(ref name)) if name == "content-length"
        ));
    }

    #[test]
    fn prohibited_field_host() {
        let result = Trailer::parse("Host");
        assert!(matches!(
            result,
            Err(TrailerError::ProhibitedField(ref name)) if name == "host"
        ));
    }

    #[test]
    fn prohibited_field_trailer() {
        let result = Trailer::parse("Trailer");
        assert!(matches!(
            result,
            Err(TrailerError::ProhibitedField(ref name)) if name == "trailer"
        ));
    }

    #[test]
    fn prohibited_field_content_encoding() {
        let result = Trailer::parse("Content-Encoding");
        assert!(matches!(
            result,
            Err(TrailerError::ProhibitedField(ref name)) if name == "content-encoding"
        ));
    }

    #[test]
    fn prohibited_field_content_type() {
        let result = Trailer::parse("Content-Type");
        assert!(matches!(
            result,
            Err(TrailerError::ProhibitedField(ref name)) if name == "content-type"
        ));
    }

    #[test]
    fn prohibited_field_content_range() {
        let result = Trailer::parse("Content-Range");
        assert!(matches!(
            result,
            Err(TrailerError::ProhibitedField(ref name)) if name == "content-range"
        ));
    }

    #[test]
    fn prohibited_field_in_list() {
        // 複数フィールドの中に禁止フィールドがある場合
        let result = Trailer::parse("X-Custom, Content-Length, X-Other");
        assert!(matches!(
            result,
            Err(TrailerError::ProhibitedField(ref name)) if name == "content-length"
        ));
    }

    #[test]
    fn allowed_fields() {
        // 許可されたフィールドは通る
        let trailer = Trailer::parse("Expires, X-Checksum, X-Custom").unwrap();
        assert_eq!(trailer.fields().len(), 3);
    }

    #[test]
    fn is_prohibited_trailer_field_function() {
        assert!(is_prohibited_trailer_field("Transfer-Encoding"));
        assert!(is_prohibited_trailer_field("transfer-encoding"));
        assert!(is_prohibited_trailer_field("CONTENT-LENGTH"));
        assert!(!is_prohibited_trailer_field("X-Custom"));
        assert!(!is_prohibited_trailer_field("Expires"));
    }
}
