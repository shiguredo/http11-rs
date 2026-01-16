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

use core::fmt;

/// ETag パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
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

impl std::error::Error for ETagError {}

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

        let (weak, rest) = if input.starts_with("W/") || input.starts_with("w/") {
            (true, &input[2..])
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
    for part in input.split(',') {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_strong() {
        let etag = EntityTag::parse("\"abc123\"").unwrap();
        assert!(etag.is_strong());
        assert_eq!(etag.tag(), "abc123");
    }

    #[test]
    fn test_parse_weak() {
        let etag = EntityTag::parse("W/\"abc123\"").unwrap();
        assert!(etag.is_weak());
        assert_eq!(etag.tag(), "abc123");
    }

    #[test]
    fn test_parse_weak_lowercase() {
        let etag = EntityTag::parse("w/\"abc123\"").unwrap();
        assert!(etag.is_weak());
    }

    #[test]
    fn test_parse_empty_tag() {
        let etag = EntityTag::parse("\"\"").unwrap();
        assert_eq!(etag.tag(), "");
    }

    #[test]
    fn test_parse_missing_quote() {
        assert!(EntityTag::parse("abc").is_err());
        assert!(EntityTag::parse("\"abc").is_err());
        assert!(EntityTag::parse("abc\"").is_err());
    }

    #[test]
    fn test_parse_empty() {
        assert!(EntityTag::parse("").is_err());
    }

    #[test]
    fn test_display_strong() {
        let etag = EntityTag::strong("v1.0").unwrap();
        assert_eq!(etag.to_string(), "\"v1.0\"");
    }

    #[test]
    fn test_display_weak() {
        let etag = EntityTag::weak("v1.0").unwrap();
        assert_eq!(etag.to_string(), "W/\"v1.0\"");
    }

    #[test]
    fn test_strong_compare() {
        let e1 = EntityTag::strong("abc").unwrap();
        let e2 = EntityTag::strong("abc").unwrap();
        let e3 = EntityTag::weak("abc").unwrap();

        assert!(e1.strong_compare(&e2));
        assert!(!e1.strong_compare(&e3));
        assert!(!e3.strong_compare(&e1));
    }

    #[test]
    fn test_weak_compare() {
        let e1 = EntityTag::strong("abc").unwrap();
        let e2 = EntityTag::weak("abc").unwrap();
        let e3 = EntityTag::strong("xyz").unwrap();

        assert!(e1.weak_compare(&e2));
        assert!(e2.weak_compare(&e1));
        assert!(!e1.weak_compare(&e3));
    }

    #[test]
    fn test_parse_etag_list() {
        let list = parse_etag_list("\"a\", \"b\", W/\"c\"").unwrap();
        match list {
            ETagList::Tags(tags) => {
                assert_eq!(tags.len(), 3);
                assert_eq!(tags[0].tag(), "a");
                assert!(tags[0].is_strong());
                assert_eq!(tags[1].tag(), "b");
                assert_eq!(tags[2].tag(), "c");
                assert!(tags[2].is_weak());
            }
            _ => panic!("expected Tags"),
        }
    }

    #[test]
    fn test_parse_etag_list_any() {
        let list = parse_etag_list("*").unwrap();
        assert!(list.is_any());
    }

    #[test]
    fn test_etag_list_contains() {
        let list = parse_etag_list("\"a\", W/\"b\"").unwrap();
        let etag_a = EntityTag::strong("a").unwrap();
        let etag_b = EntityTag::strong("b").unwrap();
        let etag_c = EntityTag::strong("c").unwrap();

        assert!(list.contains_weak(&etag_a));
        assert!(list.contains_weak(&etag_b));
        assert!(!list.contains_weak(&etag_c));

        assert!(list.contains_strong(&etag_a));
        assert!(!list.contains_strong(&etag_b)); // W/"b" は strong compare で false
    }

    #[test]
    fn test_etag_list_display() {
        let list = parse_etag_list("\"a\", \"b\"").unwrap();
        assert_eq!(list.to_string(), "\"a\", \"b\"");

        let any = parse_etag_list("*").unwrap();
        assert_eq!(any.to_string(), "*");
    }
}
