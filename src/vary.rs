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

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::validate::{is_valid_token, trim_ows};

/// Vary パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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

impl core::error::Error for VaryError {}

/// Vary ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vary {
    any: bool,
    fields: Vec<String>,
}

impl Vary {
    /// Vary ヘッダーをパース
    ///
    /// RFC 9110 Section 5.6.1.2: 空フィールド値・空要素は受理する
    /// RFC 9110 Section 12.5.5: リスト内に "*" を含む場合はワイルドカードとして扱う
    pub fn parse(input: &str) -> Result<Self, VaryError> {
        let input = trim_ows(input);

        let mut any = false;
        let mut fields = Vec::new();
        for part in input.split(',') {
            let name = trim_ows(part);
            // RFC 9110 Section 5.6.1.2: 空要素は無視する
            if name.is_empty() {
                continue;
            }
            if name == "*" {
                // RFC 9110 Section 12.5.5: リスト内の "*" はワイルドカード
                any = true;
            } else {
                if !is_valid_token(name) {
                    return Err(VaryError::InvalidFieldName);
                }
                fields.push(name.to_ascii_lowercase());
            }
        }

        // "*" を含むリストではフィールド名は意味を持たない
        if any {
            fields.clear();
        }

        Ok(Vary { any, fields })
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
