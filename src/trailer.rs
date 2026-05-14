//! Trailer フィールドパース (RFC 9110 Section 6.6.2)
//!
//! ## 概要
//!
//! RFC 9112 に基づいた Trailer ヘッダーのパースを提供します。
//!
//! 注: trailer フィールドは一般的に使われていない (RFC 9110 Section 6.5.1 が
//! "trailers are often ignored" と明記)。RFC 準拠のために実装している。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::trailer::Trailer;
//!
//! let trailer = Trailer::parse("X-Checksum, X-Test").unwrap();
//! assert_eq!(trailer.fields().len(), 2);
//! ```

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::validate::is_valid_token;

/// Trailer パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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

impl core::error::Error for TrailerError {}

/// Trailer ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trailer {
    fields: Vec<String>,
}

impl Trailer {
    /// Trailer ヘッダーをパース
    ///
    /// RFC 9110 Section 5.6.1.2: 空フィールド値・空要素は受理する
    /// RFC 9112 Section 7.1.2: 禁止フィールドを含む場合はエラー
    pub fn parse(input: &str) -> Result<Self, TrailerError> {
        let input = input.trim();

        let mut fields = Vec::new();
        for part in input.split(',') {
            let name = part.trim();
            // RFC 9110 Section 5.6.1.2: 空要素は無視する
            if name.is_empty() {
                continue;
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

/// RFC 9110 Section 6.5.1: トレーラーに置けないカテゴリのフィールドかどうか
///
/// RFC 9110 Section 6.5.1 は trailer section に含めてはならないフィールドとして
/// 以下のカテゴリを規定している。本関数はそれぞれの代表的なフィールド名を
/// 大文字小文字を区別せずに判定する。
///
/// - メッセージフレーミング: `Transfer-Encoding`, `Content-Length`
/// - ルーティング: `Host`
/// - リクエスト修飾子: `If-Match`, `If-None-Match`, `If-Modified-Since`,
///   `If-Unmodified-Since`, `If-Range`, `Range`, `Expect`, `TE`
/// - 認証: `Authorization`, `Proxy-Authorization`, `WWW-Authenticate`,
///   `Proxy-Authenticate`
/// - レスポンス制御: `Cache-Control`, `Vary`, `Date`, `Expires`, `Age`,
///   `Set-Cookie`
/// - コンテンツ形式: `Content-Encoding`, `Content-Type`, `Content-Range`
/// - 接続管理: `Connection`, `Upgrade`, `Trailer`
///
/// 本関数で `true` を返すフィールドは、たとえ `Trailer:` ヘッダーで sender が
/// 事前申告していても受理してはならない (RFC 9110 Section 6.5.1 の MUST NOT)。
///
/// なお、本関数で `false` を返すフィールドであっても、自動的に trailer として
/// 許可されるわけではない。`Trailer:` ヘッダーで sender が事前申告したフィールド
/// のみが受理対象になる (ホワイトリスト方式、RFC 9110 Section 6.5.1)。
/// 受信側のホワイトリスト判定は `decoder/body.rs::process_trailers` で行う。
pub fn is_prohibited_trailer_field(name: &str) -> bool {
    // framing
    name.eq_ignore_ascii_case("transfer-encoding")
        || name.eq_ignore_ascii_case("content-length")
        // routing
        || name.eq_ignore_ascii_case("host")
        // request modifiers
        || name.eq_ignore_ascii_case("if-match")
        || name.eq_ignore_ascii_case("if-none-match")
        || name.eq_ignore_ascii_case("if-modified-since")
        || name.eq_ignore_ascii_case("if-unmodified-since")
        || name.eq_ignore_ascii_case("if-range")
        || name.eq_ignore_ascii_case("range")
        || name.eq_ignore_ascii_case("expect")
        || name.eq_ignore_ascii_case("te")
        // authentication
        || name.eq_ignore_ascii_case("authorization")
        || name.eq_ignore_ascii_case("proxy-authorization")
        || name.eq_ignore_ascii_case("www-authenticate")
        || name.eq_ignore_ascii_case("proxy-authenticate")
        // response controls
        || name.eq_ignore_ascii_case("cache-control")
        || name.eq_ignore_ascii_case("vary")
        || name.eq_ignore_ascii_case("date")
        || name.eq_ignore_ascii_case("expires")
        || name.eq_ignore_ascii_case("age")
        || name.eq_ignore_ascii_case("set-cookie")
        // content format
        || name.eq_ignore_ascii_case("content-encoding")
        || name.eq_ignore_ascii_case("content-type")
        || name.eq_ignore_ascii_case("content-range")
        // connection management
        || name.eq_ignore_ascii_case("connection")
        || name.eq_ignore_ascii_case("upgrade")
        || name.eq_ignore_ascii_case("trailer")
}
