//! ETag ヘッダーパース (RFC 9110)
//!
//! ## 概要
//!
//! RFC 9110 Section 8.8.3 に基づいた ETag ヘッダーのパースを提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::etag::EntityTag;
//!
//! // Strong ETag
//! let etag = EntityTag::parse("\"abc123\"").unwrap();
//! assert!(etag.is_strong());
//! assert_eq!(etag.tag(), "abc123");
//!
//! // Weak ETag
//! let etag = EntityTag::parse("W/\"abc123\"").unwrap();
//! assert!(etag.is_weak());
//! assert_eq!(etag.tag(), "abc123");
//! ```

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

/// ETag パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ETagError {
    /// 空の入力
    Empty,
    /// 不正な形式
    InvalidFormat,
    /// 引用符が見つからない
    MissingQuote,
    /// 不正な文字
    InvalidCharacter,
}

impl fmt::Display for ETagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ETagError::Empty => write!(f, "empty etag"),
            ETagError::InvalidFormat => write!(f, "invalid etag format"),
            ETagError::MissingQuote => write!(f, "missing quote in etag"),
            ETagError::InvalidCharacter => write!(f, "invalid character in etag"),
        }
    }
}

impl core::error::Error for ETagError {}

/// Entity Tag (ETag)
///
/// RFC 9110 Section 8.8.3
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityTag {
    /// Weak フラグ
    weak: bool,
    /// タグ値 (引用符なし)
    tag: String,
}

impl EntityTag {
    /// ETag ヘッダー文字列をパース
    ///
    /// # 例
    ///
    /// ```rust
    /// use shiguredo_http11::etag::EntityTag;
    ///
    /// // Strong ETag
    /// let etag = EntityTag::parse("\"v1.0\"").unwrap();
    /// assert!(etag.is_strong());
    ///
    /// // Weak ETag
    /// let etag = EntityTag::parse("W/\"v1.0\"").unwrap();
    /// assert!(etag.is_weak());
    /// ```
    pub fn parse(input: &str) -> Result<Self, ETagError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ETagError::Empty);
        }

        // RFC 9110 Section 8.8.3: weak = %s"W/" (case-sensitive)
        // 小文字 w/ は許可しない
        let (weak, rest) = if let Some(rest) = input.strip_prefix("W/") {
            (true, rest)
        } else {
            (false, input)
        };

        // 引用符で囲まれていることを確認
        if !rest.starts_with('"') {
            return Err(ETagError::MissingQuote);
        }

        let end_quote = rest[1..].find('"').ok_or(ETagError::MissingQuote)?;
        let tag = &rest[1..1 + end_quote];

        // タグの文字を検証 (etagc: %x21 / %x23-7E / obs-text)
        for b in tag.bytes() {
            if !is_etagc(b) {
                return Err(ETagError::InvalidCharacter);
            }
        }

        // 閉じ引用符の後に余剰文字がないことを確認
        let after_quote = &rest[2 + end_quote..];
        if !after_quote.is_empty() {
            return Err(ETagError::InvalidFormat);
        }

        Ok(EntityTag {
            weak,
            tag: tag.to_string(),
        })
    }

    /// 新しい Strong ETag を作成
    pub fn strong(tag: &str) -> Result<Self, ETagError> {
        for b in tag.bytes() {
            if !is_etagc(b) {
                return Err(ETagError::InvalidCharacter);
            }
        }
        Ok(EntityTag {
            weak: false,
            tag: tag.to_string(),
        })
    }

    /// 新しい Weak ETag を作成
    pub fn weak(tag: &str) -> Result<Self, ETagError> {
        for b in tag.bytes() {
            if !is_etagc(b) {
                return Err(ETagError::InvalidCharacter);
            }
        }
        Ok(EntityTag {
            weak: true,
            tag: tag.to_string(),
        })
    }

    /// Weak ETag かどうか
    pub fn is_weak(&self) -> bool {
        self.weak
    }

    /// Strong ETag かどうか
    pub fn is_strong(&self) -> bool {
        !self.weak
    }

    /// タグ値を取得 (引用符なし)
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// Strong 比較 (RFC 9110 Section 8.8.3.2)
    ///
    /// 両方が Strong ETag で、タグ値が同一の場合に true
    pub fn strong_compare(&self, other: &EntityTag) -> bool {
        !self.weak && !other.weak && self.tag == other.tag
    }

    /// Weak 比較 (RFC 9110 Section 8.8.3.2)
    ///
    /// タグ値が同一の場合に true (weak フラグは無視)
    pub fn weak_compare(&self, other: &EntityTag) -> bool {
        self.tag == other.tag
    }
}

impl fmt::Display for EntityTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.weak {
            write!(f, "W/\"{}\"", self.tag)
        } else {
            write!(f, "\"{}\"", self.tag)
        }
    }
}

/// etagc 文字 (RFC 9110)
/// %x21 / %x23-7E / obs-text
fn is_etagc(b: u8) -> bool {
    b == 0x21 || (0x23..=0x7E).contains(&b) || b >= 0x80
}

/// ETag リストを引用符を考慮してカンマ分割する
///
/// etagc (RFC 9110 Section 8.8.3) はカンマを含み得るため、
/// DQUOTE 内のカンマはデリミタとして扱わない
fn split_etag_list_raw(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_quotes = false;
    let bytes = input.as_bytes();

    for i in 0..bytes.len() {
        match bytes[i] {
            b'"' => in_quotes = !in_quotes,
            b',' if !in_quotes => {
                parts.push(&input[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&input[start..]);
    parts
}

/// ETag リストをパース (If-Match, If-None-Match 用)
///
/// カンマ区切りの ETag リストをパースします。
/// `*` (ワイルドカード) もサポートします。
pub fn parse_etag_list(input: &str) -> Result<ETagList, ETagError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ETagError::Empty);
    }

    if input == "*" {
        return Ok(ETagList::Any);
    }

    let mut etags = Vec::new();
    for part in split_etag_list_raw(input) {
        let part = part.trim();
        if !part.is_empty() {
            etags.push(EntityTag::parse(part)?);
        }
    }

    if etags.is_empty() {
        return Err(ETagError::Empty);
    }

    Ok(ETagList::Tags(etags))
}

/// ETag リスト
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ETagList {
    /// ワイルドカード (*)
    Any,
    /// ETag のリスト
    Tags(Vec<EntityTag>),
}

impl ETagList {
    /// ワイルドカードかどうか
    pub fn is_any(&self) -> bool {
        matches!(self, ETagList::Any)
    }

    /// 指定した ETag が含まれるか (Weak 比較)
    pub fn contains_weak(&self, etag: &EntityTag) -> bool {
        match self {
            ETagList::Any => true,
            ETagList::Tags(tags) => tags.iter().any(|t| t.weak_compare(etag)),
        }
    }

    /// 指定した ETag が含まれるか (Strong 比較)
    pub fn contains_strong(&self, etag: &EntityTag) -> bool {
        match self {
            ETagList::Any => true,
            ETagList::Tags(tags) => tags.iter().any(|t| t.strong_compare(etag)),
        }
    }
}

impl fmt::Display for ETagList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ETagList::Any => write!(f, "*"),
            ETagList::Tags(tags) => {
                let s: Vec<String> = tags.iter().map(|t| t.to_string()).collect();
                write!(f, "{}", s.join(", "))
            }
        }
    }
}
