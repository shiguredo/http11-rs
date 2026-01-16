//! HTTP キャッシュヘッダー (RFC 9111)
//!
//! ## 概要
//!
//! RFC 9111 に基づいたキャッシュ関連ヘッダーのパース/生成を提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::cache::{CacheControl, Age, Expires};
//!
//! // Cache-Control パース
//! let cc = CacheControl::parse("max-age=3600, public").unwrap();
//! assert_eq!(cc.max_age(), Some(3600));
//! assert!(cc.is_public());
//!
//! // Age ヘッダー
//! let age = Age::new(120);
//! assert_eq!(age.seconds(), 120);
//!
//! // Expires ヘッダー
//! let expires = Expires::parse("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
//! ```

use crate::date::HttpDate;
use core::fmt;

/// キャッシュヘッダーパースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheError {
    /// 空の入力
    Empty,
    /// 不正な形式
    InvalidFormat,
    /// 不正な数値
    InvalidNumber,
    /// 日付パースエラー
    InvalidDate,
}

impl fmt::Display for CacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CacheError::Empty => write!(f, "empty cache header"),
            CacheError::InvalidFormat => write!(f, "invalid cache header format"),
            CacheError::InvalidNumber => write!(f, "invalid number in cache header"),
            CacheError::InvalidDate => write!(f, "invalid date in cache header"),
        }
    }
}

impl std::error::Error for CacheError {}

/// Cache-Control ヘッダー
///
/// RFC 9111 Section 5.2
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CacheControl {
    /// max-age ディレクティブ
    max_age: Option<u64>,
    /// s-maxage ディレクティブ
    s_maxage: Option<u64>,
    /// max-stale ディレクティブ
    max_stale: Option<u64>,
    /// min-fresh ディレクティブ
    min_fresh: Option<u64>,
    /// stale-while-revalidate ディレクティブ
    stale_while_revalidate: Option<u64>,
    /// stale-if-error ディレクティブ
    stale_if_error: Option<u64>,
    /// no-cache ディレクティブ
    no_cache: bool,
    /// no-store ディレクティブ
    no_store: bool,
    /// no-transform ディレクティブ
    no_transform: bool,
    /// only-if-cached ディレクティブ
    only_if_cached: bool,
    /// must-revalidate ディレクティブ
    must_revalidate: bool,
    /// proxy-revalidate ディレクティブ
    proxy_revalidate: bool,
    /// must-understand ディレクティブ
    must_understand: bool,
    /// public ディレクティブ
    public: bool,
    /// private ディレクティブ
    private: bool,
    /// immutable ディレクティブ
    immutable: bool,
}

impl CacheControl {
    /// 新しい Cache-Control を作成
    pub fn new() -> Self {
        Self::default()
    }

    /// Cache-Control ヘッダーをパース
    ///
    /// # 例
    ///
    /// ```rust
    /// use shiguredo_http11::cache::CacheControl;
    ///
    /// let cc = CacheControl::parse("max-age=3600, public").unwrap();
    /// assert_eq!(cc.max_age(), Some(3600));
    /// assert!(cc.is_public());
    /// ```
    pub fn parse(input: &str) -> Result<Self, CacheError> {
        let input = input.trim();
        if input.is_empty() {
            // 空文字列はデフォルトの CacheControl として扱う
            return Ok(CacheControl::new());
        }

        let mut cc = CacheControl::new();

        for directive in input.split(',') {
            let directive = directive.trim();
            if directive.is_empty() {
                continue;
            }

            if let Some((name, value)) = directive.split_once('=') {
                let name = name.trim().to_lowercase();
                let value = value.trim().trim_matches('"');

                match name.as_str() {
                    "max-age" => {
                        cc.max_age = Some(parse_seconds(value)?);
                    }
                    "s-maxage" => {
                        cc.s_maxage = Some(parse_seconds(value)?);
                    }
                    "max-stale" => {
                        cc.max_stale = Some(parse_seconds(value)?);
                    }
                    "min-fresh" => {
                        cc.min_fresh = Some(parse_seconds(value)?);
                    }
                    "stale-while-revalidate" => {
                        cc.stale_while_revalidate = Some(parse_seconds(value)?);
                    }
                    "stale-if-error" => {
                        cc.stale_if_error = Some(parse_seconds(value)?);
                    }
                    _ => {} // 未知のディレクティブは無視
                }
            } else {
                let name = directive.to_lowercase();
                match name.as_str() {
                    "no-cache" => cc.no_cache = true,
                    "no-store" => cc.no_store = true,
                    "no-transform" => cc.no_transform = true,
                    "only-if-cached" => cc.only_if_cached = true,
                    "must-revalidate" => cc.must_revalidate = true,
                    "proxy-revalidate" => cc.proxy_revalidate = true,
                    "must-understand" => cc.must_understand = true,
                    "public" => cc.public = true,
                    "private" => cc.private = true,
                    "immutable" => cc.immutable = true,
                    "max-stale" => cc.max_stale = Some(u64::MAX), // 値なしは無制限
                    _ => {}                                       // 未知のディレクティブは無視
                }
            }
        }

        Ok(cc)
    }

    /// max-age を設定
    pub fn with_max_age(mut self, seconds: u64) -> Self {
        self.max_age = Some(seconds);
        self
    }

    /// s-maxage を設定
    pub fn with_s_maxage(mut self, seconds: u64) -> Self {
        self.s_maxage = Some(seconds);
        self
    }

    /// no-cache を設定
    pub fn with_no_cache(mut self) -> Self {
        self.no_cache = true;
        self
    }

    /// no-store を設定
    pub fn with_no_store(mut self) -> Self {
        self.no_store = true;
        self
    }

    /// no-transform を設定
    pub fn with_no_transform(mut self) -> Self {
        self.no_transform = true;
        self
    }

    /// must-revalidate を設定
    pub fn with_must_revalidate(mut self) -> Self {
        self.must_revalidate = true;
        self
    }

    /// proxy-revalidate を設定
    pub fn with_proxy_revalidate(mut self) -> Self {
        self.proxy_revalidate = true;
        self
    }

    /// public を設定
    pub fn with_public(mut self) -> Self {
        self.public = true;
        self
    }

    /// private を設定
    pub fn with_private(mut self) -> Self {
        self.private = true;
        self
    }

    /// immutable を設定
    pub fn with_immutable(mut self) -> Self {
        self.immutable = true;
        self
    }

    /// max-age を取得
    pub fn max_age(&self) -> Option<u64> {
        self.max_age
    }

    /// s-maxage を取得
    pub fn s_maxage(&self) -> Option<u64> {
        self.s_maxage
    }

    /// max-stale を取得
    pub fn max_stale(&self) -> Option<u64> {
        self.max_stale
    }

    /// min-fresh を取得
    pub fn min_fresh(&self) -> Option<u64> {
        self.min_fresh
    }

    /// stale-while-revalidate を取得
    pub fn stale_while_revalidate(&self) -> Option<u64> {
        self.stale_while_revalidate
    }

    /// stale-if-error を取得
    pub fn stale_if_error(&self) -> Option<u64> {
        self.stale_if_error
    }

    /// no-cache かどうか
    pub fn is_no_cache(&self) -> bool {
        self.no_cache
    }

    /// no-store かどうか
    pub fn is_no_store(&self) -> bool {
        self.no_store
    }

    /// no-transform かどうか
    pub fn is_no_transform(&self) -> bool {
        self.no_transform
    }

    /// only-if-cached かどうか
    pub fn is_only_if_cached(&self) -> bool {
        self.only_if_cached
    }

    /// must-revalidate かどうか
    pub fn is_must_revalidate(&self) -> bool {
        self.must_revalidate
    }

    /// proxy-revalidate かどうか
    pub fn is_proxy_revalidate(&self) -> bool {
        self.proxy_revalidate
    }

    /// must-understand かどうか
    pub fn is_must_understand(&self) -> bool {
        self.must_understand
    }

    /// public かどうか
    pub fn is_public(&self) -> bool {
        self.public
    }

    /// private かどうか
    pub fn is_private(&self) -> bool {
        self.private
    }

    /// immutable かどうか
    pub fn is_immutable(&self) -> bool {
        self.immutable
    }

    /// キャッシュ可能かどうか (簡易判定)
    pub fn is_cacheable(&self) -> bool {
        !self.no_store && (self.public || self.max_age.is_some() || self.s_maxage.is_some())
    }

    /// ヘッダー値を生成
    pub fn to_header_value(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for CacheControl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();

        if let Some(max_age) = self.max_age {
            parts.push(format!("max-age={}", max_age));
        }
        if let Some(s_maxage) = self.s_maxage {
            parts.push(format!("s-maxage={}", s_maxage));
        }
        if let Some(max_stale) = self.max_stale {
            if max_stale == u64::MAX {
                parts.push("max-stale".to_string());
            } else {
                parts.push(format!("max-stale={}", max_stale));
            }
        }
        if let Some(min_fresh) = self.min_fresh {
            parts.push(format!("min-fresh={}", min_fresh));
        }
        if let Some(swr) = self.stale_while_revalidate {
            parts.push(format!("stale-while-revalidate={}", swr));
        }
        if let Some(sie) = self.stale_if_error {
            parts.push(format!("stale-if-error={}", sie));
        }
        if self.no_cache {
            parts.push("no-cache".to_string());
        }
        if self.no_store {
            parts.push("no-store".to_string());
        }
        if self.no_transform {
            parts.push("no-transform".to_string());
        }
        if self.only_if_cached {
            parts.push("only-if-cached".to_string());
        }
        if self.must_revalidate {
            parts.push("must-revalidate".to_string());
        }
        if self.proxy_revalidate {
            parts.push("proxy-revalidate".to_string());
        }
        if self.must_understand {
            parts.push("must-understand".to_string());
        }
        if self.public {
            parts.push("public".to_string());
        }
        if self.private {
            parts.push("private".to_string());
        }
        if self.immutable {
            parts.push("immutable".to_string());
        }

        write!(f, "{}", parts.join(", "))
    }
}

/// Age ヘッダー
///
/// RFC 9111 Section 5.1
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Age {
    /// 秒数
    seconds: u64,
}

impl Age {
    /// 新しい Age ヘッダーを作成
    pub fn new(seconds: u64) -> Self {
        Age { seconds }
    }

    /// Age ヘッダーをパース
    ///
    /// # 例
    ///
    /// ```rust
    /// use shiguredo_http11::cache::Age;
    ///
    /// let age = Age::parse("120").unwrap();
    /// assert_eq!(age.seconds(), 120);
    /// ```
    pub fn parse(input: &str) -> Result<Self, CacheError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(CacheError::Empty);
        }

        let seconds = parse_seconds(input)?;
        Ok(Age { seconds })
    }

    /// 秒数を取得
    pub fn seconds(&self) -> u64 {
        self.seconds
    }

    /// ヘッダー値を生成
    pub fn to_header_value(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for Age {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.seconds)
    }
}

/// Expires ヘッダー
///
/// RFC 9111 Section 5.3
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expires {
    /// 日時
    date: HttpDate,
}

impl Expires {
    /// 新しい Expires ヘッダーを作成
    pub fn new(date: HttpDate) -> Self {
        Expires { date }
    }

    /// Expires ヘッダーをパース
    ///
    /// # 例
    ///
    /// ```rust
    /// use shiguredo_http11::cache::Expires;
    ///
    /// let expires = Expires::parse("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
    /// assert_eq!(expires.date().year(), 1994);
    /// ```
    pub fn parse(input: &str) -> Result<Self, CacheError> {
        let date = HttpDate::parse(input).map_err(|_| CacheError::InvalidDate)?;
        Ok(Expires { date })
    }

    /// 日時を取得
    pub fn date(&self) -> &HttpDate {
        &self.date
    }

    /// ヘッダー値を生成
    pub fn to_header_value(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for Expires {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.date)
    }
}

/// 秒数をパース
fn parse_seconds(s: &str) -> Result<u64, CacheError> {
    s.parse::<u64>().map_err(|_| CacheError::InvalidNumber)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_control_parse_max_age() {
        let cc = CacheControl::parse("max-age=3600").unwrap();
        assert_eq!(cc.max_age(), Some(3600));
    }

    #[test]
    fn test_cache_control_parse_multiple() {
        let cc = CacheControl::parse("max-age=3600, public, no-transform").unwrap();
        assert_eq!(cc.max_age(), Some(3600));
        assert!(cc.is_public());
        assert!(cc.is_no_transform());
    }

    #[test]
    fn test_cache_control_parse_no_store() {
        let cc = CacheControl::parse("no-store").unwrap();
        assert!(cc.is_no_store());
        assert!(!cc.is_cacheable());
    }

    #[test]
    fn test_cache_control_parse_private() {
        let cc = CacheControl::parse("private, max-age=600").unwrap();
        assert!(cc.is_private());
        assert_eq!(cc.max_age(), Some(600));
    }

    #[test]
    fn test_cache_control_parse_s_maxage() {
        let cc = CacheControl::parse("public, s-maxage=86400").unwrap();
        assert!(cc.is_public());
        assert_eq!(cc.s_maxage(), Some(86400));
    }

    #[test]
    fn test_cache_control_parse_must_revalidate() {
        let cc = CacheControl::parse("max-age=0, must-revalidate").unwrap();
        assert_eq!(cc.max_age(), Some(0));
        assert!(cc.is_must_revalidate());
    }

    #[test]
    fn test_cache_control_parse_immutable() {
        let cc = CacheControl::parse("max-age=31536000, immutable").unwrap();
        assert_eq!(cc.max_age(), Some(31536000));
        assert!(cc.is_immutable());
    }

    #[test]
    fn test_cache_control_parse_empty() {
        // 空文字列はデフォルトの CacheControl として扱う
        let cc = CacheControl::parse("").unwrap();
        assert_eq!(cc, CacheControl::default());
        assert_eq!(cc.max_age(), None);
        assert!(!cc.is_public());
    }

    #[test]
    fn test_cache_control_builder() {
        let cc = CacheControl::new()
            .with_max_age(3600)
            .with_public()
            .with_no_transform();

        assert_eq!(cc.max_age(), Some(3600));
        assert!(cc.is_public());
        assert!(cc.is_no_transform());
    }

    #[test]
    fn test_cache_control_display() {
        let cc = CacheControl::new().with_max_age(3600).with_public();

        let s = cc.to_string();
        assert!(s.contains("max-age=3600"));
        assert!(s.contains("public"));
    }

    #[test]
    fn test_cache_control_roundtrip() {
        let original = CacheControl::new()
            .with_max_age(3600)
            .with_public()
            .with_must_revalidate();

        let header = original.to_string();
        let reparsed = CacheControl::parse(&header).unwrap();

        assert_eq!(original.max_age(), reparsed.max_age());
        assert_eq!(original.is_public(), reparsed.is_public());
        assert_eq!(original.is_must_revalidate(), reparsed.is_must_revalidate());
    }

    #[test]
    fn test_cache_control_is_cacheable() {
        let cc1 = CacheControl::new().with_max_age(3600);
        assert!(cc1.is_cacheable());

        let cc2 = CacheControl::new().with_no_store();
        assert!(!cc2.is_cacheable());

        let cc3 = CacheControl::new().with_public();
        assert!(cc3.is_cacheable());
    }

    #[test]
    fn test_age_parse() {
        let age = Age::parse("120").unwrap();
        assert_eq!(age.seconds(), 120);
    }

    #[test]
    fn test_age_parse_zero() {
        let age = Age::parse("0").unwrap();
        assert_eq!(age.seconds(), 0);
    }

    #[test]
    fn test_age_parse_empty() {
        assert!(Age::parse("").is_err());
    }

    #[test]
    fn test_age_parse_invalid() {
        assert!(Age::parse("abc").is_err());
        assert!(Age::parse("-1").is_err());
    }

    #[test]
    fn test_age_display() {
        let age = Age::new(120);
        assert_eq!(age.to_string(), "120");
    }

    #[test]
    fn test_age_roundtrip() {
        let original = Age::new(3600);
        let header = original.to_string();
        let reparsed = Age::parse(&header).unwrap();
        assert_eq!(original.seconds(), reparsed.seconds());
    }

    #[test]
    fn test_expires_parse() {
        let expires = Expires::parse("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
        assert_eq!(expires.date().year(), 1994);
        assert_eq!(expires.date().month(), 11);
        assert_eq!(expires.date().day(), 6);
    }

    #[test]
    fn test_expires_parse_invalid() {
        assert!(Expires::parse("invalid date").is_err());
        assert!(Expires::parse("").is_err());
    }

    #[test]
    fn test_expires_display() {
        let expires = Expires::parse("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
        let s = expires.to_string();
        assert!(s.contains("1994"));
        assert!(s.contains("Nov"));
    }

    #[test]
    fn test_expires_roundtrip() {
        let original = Expires::parse("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
        let header = original.to_string();
        let reparsed = Expires::parse(&header).unwrap();

        assert_eq!(original.date().year(), reparsed.date().year());
        assert_eq!(original.date().month(), reparsed.date().month());
        assert_eq!(original.date().day(), reparsed.date().day());
    }
}
