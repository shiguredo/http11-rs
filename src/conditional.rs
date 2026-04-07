//! 条件付きリクエストヘッダー (RFC 9110)
//!
//! ## 概要
//!
//! RFC 9110 Section 13 に基づいた条件付きリクエストヘッダーのパースを提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::conditional::{IfMatch, IfNoneMatch, IfModifiedSince, IfUnmodifiedSince};
//! use shiguredo_http11::etag::EntityTag;
//! use shiguredo_http11::date::HttpDate;
//!
//! // If-Match
//! let if_match = IfMatch::parse("\"abc\", \"def\"").unwrap();
//! let etag = EntityTag::strong("abc").unwrap();
//! assert!(if_match.matches(&etag));
//!
//! // If-None-Match
//! let if_none_match = IfNoneMatch::parse("*").unwrap();
//! assert!(if_none_match.is_any());
//!
//! // If-Modified-Since
//! let if_mod = IfModifiedSince::parse("Sun, 06 Nov 1994 08:49:37 GMT", 2026).unwrap();
//! let _ = if_mod.date();
//! ```

use crate::date::{DateError, HttpDate};
use crate::etag::{ETagList, EntityTag, parse_etag_list};
use core::fmt;

/// 条件付きリクエストエラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConditionalError {
    /// 空の入力
    Empty,
    /// 不正な形式
    InvalidFormat,
    /// ETag パースエラー
    ETagError,
    /// 日付パースエラー
    DateError,
}

impl fmt::Display for ConditionalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConditionalError::Empty => write!(f, "empty conditional header"),
            ConditionalError::InvalidFormat => write!(f, "invalid conditional header format"),
            ConditionalError::ETagError => write!(f, "invalid etag in conditional header"),
            ConditionalError::DateError => write!(f, "invalid date in conditional header"),
        }
    }
}

impl core::error::Error for ConditionalError {}

/// If-Match ヘッダー (RFC 9110 Section 13.1.1)
///
/// リソースの現在の表現が指定された ETag のいずれかと一致する場合のみ
/// リクエストを処理します。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfMatch(ETagList);

impl IfMatch {
    /// If-Match ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, ConditionalError> {
        parse_etag_list(input)
            .map(IfMatch)
            .map_err(|_| ConditionalError::ETagError)
    }

    /// ワイルドカード (*) かどうか
    pub fn is_any(&self) -> bool {
        self.0.is_any()
    }

    /// 指定した ETag が条件を満たすか (Strong 比較)
    ///
    /// If-Match は Strong 比較を使用します (RFC 9110 Section 13.1.1)
    pub fn matches(&self, etag: &EntityTag) -> bool {
        self.0.contains_strong(etag)
    }

    /// 内部の ETag リストを取得
    pub fn etags(&self) -> &ETagList {
        &self.0
    }
}

impl fmt::Display for IfMatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// If-None-Match ヘッダー (RFC 9110 Section 13.1.2)
///
/// リソースの現在の表現が指定された ETag のいずれとも一致しない場合のみ
/// リクエストを処理します。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfNoneMatch(ETagList);

impl IfNoneMatch {
    /// If-None-Match ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, ConditionalError> {
        parse_etag_list(input)
            .map(IfNoneMatch)
            .map_err(|_| ConditionalError::ETagError)
    }

    /// ワイルドカード (*) かどうか
    pub fn is_any(&self) -> bool {
        self.0.is_any()
    }

    /// 指定した ETag が条件を満たすか (Weak 比較)
    ///
    /// If-None-Match は Weak 比較を使用します (RFC 9110 Section 13.1.2)
    /// 戻り値が true = リクエストを処理すべき (条件を満たす)
    /// 戻り値が false = 304/412 を返すべき (条件を満たさない)
    pub fn matches(&self, etag: &EntityTag) -> bool {
        !self.0.contains_weak(etag)
    }

    /// 内部の ETag リストを取得
    pub fn etags(&self) -> &ETagList {
        &self.0
    }
}

impl fmt::Display for IfNoneMatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// If-Modified-Since ヘッダー (RFC 9110 Section 13.1.3)
///
/// 指定した日時以降にリソースが変更された場合のみリクエストを処理します。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfModifiedSince(HttpDate);

impl IfModifiedSince {
    /// If-Modified-Since ヘッダーをパース
    ///
    /// `reference_year` は RFC 850 形式の 2 桁年解決に使う現在年
    /// (RFC 9110 §5.6.7)。
    pub fn parse(input: &str, reference_year: u16) -> Result<Self, ConditionalError> {
        HttpDate::parse(input)
            .or_else(|e| match e {
                DateError::Rfc850Date => HttpDate::parse_rfc850(input, reference_year),
                other => Err(other),
            })
            .map(IfModifiedSince)
            .map_err(|_| ConditionalError::DateError)
    }

    /// 日時を取得
    pub fn date(&self) -> &HttpDate {
        &self.0
    }

    /// 指定した Last-Modified が条件を満たすか (RFC 9110 Section 13.1.3)
    ///
    /// 戻り値が true = リクエストを処理すべき (変更されている)
    /// 戻り値が false = 304 を返すべき (変更されていない)
    ///
    /// RFC 9110: last-modified が if-modified-since 以前または等しい場合 false
    pub fn is_modified(&self, last_modified: &HttpDate) -> bool {
        last_modified > &self.0
    }
}

impl fmt::Display for IfModifiedSince {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// If-Unmodified-Since ヘッダー (RFC 9110 Section 13.1.4)
///
/// 指定した日時以降にリソースが変更されていない場合のみリクエストを処理します。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfUnmodifiedSince(HttpDate);

impl IfUnmodifiedSince {
    /// If-Unmodified-Since ヘッダーをパース
    ///
    /// `reference_year` は RFC 850 形式の 2 桁年解決に使う現在年
    /// (RFC 9110 §5.6.7)。
    pub fn parse(input: &str, reference_year: u16) -> Result<Self, ConditionalError> {
        HttpDate::parse(input)
            .or_else(|e| match e {
                DateError::Rfc850Date => HttpDate::parse_rfc850(input, reference_year),
                other => Err(other),
            })
            .map(IfUnmodifiedSince)
            .map_err(|_| ConditionalError::DateError)
    }

    /// 日時を取得
    pub fn date(&self) -> &HttpDate {
        &self.0
    }
}

impl fmt::Display for IfUnmodifiedSince {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// If-Range ヘッダー (RFC 9110 Section 13.1.5)
///
/// Range リクエストで使用され、ETag または日時を指定できます。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IfRange {
    /// ETag による条件
    ETag(EntityTag),
    /// 日時による条件
    Date(HttpDate),
}

impl IfRange {
    /// If-Range ヘッダーをパース
    ///
    /// `reference_year` は RFC 850 形式の 2 桁年解決に使う現在年
    /// (RFC 9110 §5.6.7)。
    pub fn parse(input: &str, reference_year: u16) -> Result<Self, ConditionalError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ConditionalError::Empty);
        }

        // ETag は引用符で始まる、または W/ で始まる
        if input.starts_with('"') || input.starts_with("W/") || input.starts_with("w/") {
            EntityTag::parse(input)
                .map(IfRange::ETag)
                .map_err(|_| ConditionalError::ETagError)
        } else {
            HttpDate::parse(input)
                .or_else(|e| match e {
                    DateError::Rfc850Date => HttpDate::parse_rfc850(input, reference_year),
                    other => Err(other),
                })
                .map(IfRange::Date)
                .map_err(|_| ConditionalError::DateError)
        }
    }

    /// ETag かどうか
    pub fn is_etag(&self) -> bool {
        matches!(self, IfRange::ETag(_))
    }

    /// 日時かどうか
    pub fn is_date(&self) -> bool {
        matches!(self, IfRange::Date(_))
    }

    /// ETag を取得
    pub fn etag(&self) -> Option<&EntityTag> {
        match self {
            IfRange::ETag(e) => Some(e),
            IfRange::Date(_) => None,
        }
    }

    /// 日時を取得
    pub fn date(&self) -> Option<&HttpDate> {
        match self {
            IfRange::ETag(_) => None,
            IfRange::Date(d) => Some(d),
        }
    }
}

impl fmt::Display for IfRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IfRange::ETag(e) => write!(f, "{}", e),
            IfRange::Date(d) => write!(f, "{}", d),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_if_match_single() {
        let im = IfMatch::parse("\"abc\"").unwrap();
        let etag = EntityTag::strong("abc").unwrap();
        assert!(im.matches(&etag));

        let other = EntityTag::strong("xyz").unwrap();
        assert!(!im.matches(&other));
    }

    #[test]
    fn test_if_match_multiple() {
        let im = IfMatch::parse("\"a\", \"b\", \"c\"").unwrap();
        assert!(im.matches(&EntityTag::strong("b").unwrap()));
        assert!(!im.matches(&EntityTag::strong("d").unwrap()));
    }

    #[test]
    fn test_if_match_any() {
        let im = IfMatch::parse("*").unwrap();
        assert!(im.is_any());
        assert!(im.matches(&EntityTag::strong("anything").unwrap()));
    }

    #[test]
    fn test_if_match_weak_not_match() {
        // If-Match は Strong 比較を使用するため、Weak ETag は一致しない
        let im = IfMatch::parse("W/\"abc\"").unwrap();
        let etag = EntityTag::strong("abc").unwrap();
        assert!(!im.matches(&etag));
    }

    #[test]
    fn test_if_none_match_single() {
        let inm = IfNoneMatch::parse("\"abc\"").unwrap();
        let etag = EntityTag::strong("abc").unwrap();
        assert!(!inm.matches(&etag)); // 一致するので処理しない

        let other = EntityTag::strong("xyz").unwrap();
        assert!(inm.matches(&other)); // 一致しないので処理する
    }

    #[test]
    fn test_if_none_match_any() {
        let inm = IfNoneMatch::parse("*").unwrap();
        assert!(inm.is_any());
        // * は全てに一致するので、どの ETag でも処理しない
        assert!(!inm.matches(&EntityTag::strong("anything").unwrap()));
    }

    #[test]
    fn test_if_none_match_weak() {
        // If-None-Match は Weak 比較を使用
        let inm = IfNoneMatch::parse("W/\"abc\"").unwrap();
        let etag = EntityTag::strong("abc").unwrap();
        assert!(!inm.matches(&etag)); // Weak 比較で一致するので処理しない
    }

    #[test]
    fn test_if_modified_since() {
        let ims = IfModifiedSince::parse("Sun, 06 Nov 1994 08:49:37 GMT", 2026).unwrap();
        assert_eq!(ims.date().day(), 6);
        assert_eq!(ims.date().month(), 11);
        assert_eq!(ims.date().year(), 1994);
    }

    #[test]
    fn test_if_modified_since_is_modified() {
        // If-Modified-Since: 1994-11-06
        let ims = IfModifiedSince::parse("Sun, 06 Nov 1994 08:49:37 GMT", 2026).unwrap();

        // last-modified が同じ → false (304)
        let same = HttpDate::parse("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
        assert!(!ims.is_modified(&same));

        // last-modified が古い → false (304)
        let older = HttpDate::parse("Sat, 05 Nov 1994 08:49:37 GMT").unwrap();
        assert!(!ims.is_modified(&older));

        // last-modified が新しい → true (処理する)
        let newer = HttpDate::parse("Mon, 07 Nov 1994 08:49:37 GMT").unwrap();
        assert!(ims.is_modified(&newer));
    }

    #[test]
    fn test_if_unmodified_since() {
        let ius = IfUnmodifiedSince::parse("Sun, 06 Nov 1994 08:49:37 GMT", 2026).unwrap();
        assert_eq!(ius.date().day(), 6);
    }

    #[test]
    fn test_if_range_etag() {
        let ir = IfRange::parse("\"abc123\"", 2026).unwrap();
        assert!(ir.is_etag());
        assert_eq!(ir.etag().unwrap().tag(), "abc123");
    }

    #[test]
    fn test_if_range_weak_etag() {
        let ir = IfRange::parse("W/\"abc123\"", 2026).unwrap();
        assert!(ir.is_etag());
        assert!(ir.etag().unwrap().is_weak());
    }

    #[test]
    fn test_if_range_date() {
        let ir = IfRange::parse("Sun, 06 Nov 1994 08:49:37 GMT", 2026).unwrap();
        assert!(ir.is_date());
        assert_eq!(ir.date().unwrap().day(), 6);
    }

    #[test]
    fn test_if_match_display() {
        let im = IfMatch::parse("\"a\", \"b\"").unwrap();
        assert_eq!(im.to_string(), "\"a\", \"b\"");
    }

    #[test]
    fn test_if_range_display() {
        let ir = IfRange::parse("\"abc\"", 2026).unwrap();
        assert_eq!(ir.to_string(), "\"abc\"");
    }
}
