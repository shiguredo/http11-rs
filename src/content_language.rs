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

use core::fmt;

/// Content-Language パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
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

impl std::error::Error for ContentLanguageError {}

/// Content-Language ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentLanguage {
    tags: Vec<String>,
}

impl ContentLanguage {
    /// Content-Language ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, ContentLanguageError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ContentLanguageError::Empty);
        }

        let mut tags = Vec::new();
        for part in input.split(',') {
            let tag = part.trim();
            if tag.is_empty() {
                continue;
            }
            if !is_valid_language_tag(tag) {
                return Err(ContentLanguageError::InvalidLanguageTag);
            }
            tags.push(tag.to_string());
        }

        if tags.is_empty() {
            return Err(ContentLanguageError::Empty);
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

    let mut iter = tag.split('-');
    let mut has_part = false;
    for part in &mut iter {
        has_part = true;
        if part.is_empty() || part.len() > 8 || !part.chars().all(|c| c.is_ascii_alphanumeric()) {
            return false;
        }
    }

    has_part
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        let cl = ContentLanguage::parse("en").unwrap();
        assert_eq!(cl.tags(), &["en".to_string()]);
    }

    #[test]
    fn parse_multiple() {
        let cl = ContentLanguage::parse("en-US, ja").unwrap();
        assert_eq!(cl.tags().len(), 2);
        assert_eq!(cl.tags()[1], "ja");
    }

    #[test]
    fn parse_invalid() {
        assert!(ContentLanguage::parse("").is_err());
        assert!(ContentLanguage::parse("en-").is_err());
        assert!(ContentLanguage::parse("en--us").is_err());
    }

    #[test]
    fn display() {
        let cl = ContentLanguage::parse("en-US, ja").unwrap();
        assert_eq!(cl.to_string(), "en-US, ja");
    }
}
