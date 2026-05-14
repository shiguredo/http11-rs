//! Content-Language ヘッダーパース (RFC 9110 Section 8.5)
//!
//! ## 概要
//!
//! RFC 9110 に基づいた Content-Language ヘッダーのパースを提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::content_language::ContentLanguage;
//!
//! let cl = ContentLanguage::parse("en-US, ja").unwrap();
//! assert_eq!(cl.tags().len(), 2);
//! ```

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

/// Content-Language パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContentLanguageError {
    /// 空の入力
    Empty,
    /// 不正な形式
    InvalidFormat,
    /// 不正な言語タグ
    InvalidLanguageTag,
}

impl fmt::Display for ContentLanguageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentLanguageError::Empty => write!(f, "empty Content-Language"),
            ContentLanguageError::InvalidFormat => {
                write!(f, "invalid Content-Language format")
            }
            ContentLanguageError::InvalidLanguageTag => {
                write!(f, "invalid language tag")
            }
        }
    }
}

impl core::error::Error for ContentLanguageError {}

/// Content-Language ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentLanguage {
    tags: Vec<String>,
}

impl ContentLanguage {
    /// Content-Language ヘッダーをパース
    ///
    /// RFC 9110 Section 5.6.1.2: 空フィールド値・空要素は受理する
    pub fn parse(input: &str) -> Result<Self, ContentLanguageError> {
        let input = input.trim();

        let mut tags = Vec::new();
        for part in input.split(',') {
            let tag = part.trim();
            // RFC 9110 Section 5.6.1.2: 空要素は無視する
            if tag.is_empty() {
                continue;
            }
            if !is_valid_language_tag(tag) {
                return Err(ContentLanguageError::InvalidLanguageTag);
            }
            tags.push(tag.to_string());
        }

        Ok(ContentLanguage { tags })
    }

    /// 言語タグ一覧
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
}

impl fmt::Display for ContentLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.tags.join(", "))
    }
}

fn is_valid_language_tag(tag: &str) -> bool {
    if tag.is_empty() {
        return false;
    }

    let mut parts = tag.split('-');

    // BCP 47/RFC 5646: 先頭サブタグは ALPHA のみ (数字不可)
    if let Some(primary) = parts.next() {
        if primary.is_empty()
            || primary.len() > 8
            || !primary.chars().all(|c| c.is_ascii_alphabetic())
        {
            return false;
        }
    } else {
        return false;
    }

    // 後続サブタグは ALPHA / DIGIT
    for part in parts {
        if part.is_empty() || part.len() > 8 || !part.chars().all(|c| c.is_ascii_alphanumeric()) {
            return false;
        }
    }

    true
}
