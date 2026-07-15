//! Content-Type ヘッダーパース (RFC 9110 Section 8.3)
//!
//! ## 概要
//!
//! RFC 9110 に基づいた Content-Type ヘッダーのパースを提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::content_type::ContentType;
//!
//! // 基本的な Content-Type
//! let ct = ContentType::parse("text/html").unwrap();
//! assert_eq!(ct.media_type(), "text");
//! assert_eq!(ct.subtype(), "html");
//!
//! // パラメータ付き
//! let ct = ContentType::parse("text/html; charset=utf-8").unwrap();
//! assert_eq!(ct.charset(), Some("utf-8"));
//!
//! // multipart/form-data
//! let ct = ContentType::parse("multipart/form-data; boundary=----WebKitFormBoundary").unwrap();
//! assert_eq!(ct.boundary(), Some("----WebKitFormBoundary"));
//! ```

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::validate::{
    QuotedStringError, escape_quotes, is_token_char, is_valid_token, parse_quoted_string, trim_ows,
};

/// Content-Type パースエラー
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentTypeError {
    /// 空の Content-Type
    Empty,
    /// 不正なメディアタイプ形式
    InvalidMediaType,
    /// 不正なパラメータ形式
    InvalidParameter,
    /// 引用符が閉じていない
    UnterminatedQuote,
}

impl fmt::Display for ContentTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentTypeError::Empty => write!(f, "empty Content-Type"),
            ContentTypeError::InvalidMediaType => write!(f, "invalid media type"),
            ContentTypeError::InvalidParameter => write!(f, "invalid parameter"),
            ContentTypeError::UnterminatedQuote => write!(f, "unterminated quote"),
        }
    }
}

impl core::error::Error for ContentTypeError {}

impl From<QuotedStringError> for ContentTypeError {
    fn from(e: QuotedStringError) -> Self {
        match e {
            QuotedStringError::InvalidQdtext | QuotedStringError::InvalidQuotedPair => {
                ContentTypeError::InvalidParameter
            }
            QuotedStringError::Unterminated => ContentTypeError::UnterminatedQuote,
        }
    }
}

/// パース済み Content-Type
///
/// RFC 9110 Section 8.3 に基づいた Content-Type 構造:
/// ```text
/// Content-Type = media-type
/// media-type = type "/" subtype parameters
/// parameters = *( OWS ";" OWS [ parameter ] )
/// parameter = parameter-name "=" parameter-value
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentType {
    /// メディアタイプ (例: "text")
    media_type: String,
    /// サブタイプ (例: "html")
    subtype: String,
    /// パラメータ (name, value) のペア
    parameters: Vec<(String, String)>,
}

impl ContentType {
    /// Content-Type 文字列をパース
    ///
    /// # 例
    ///
    /// ```rust
    /// use shiguredo_http11::content_type::ContentType;
    ///
    /// let ct = ContentType::parse("text/html; charset=utf-8").unwrap();
    /// assert_eq!(ct.media_type(), "text");
    /// assert_eq!(ct.subtype(), "html");
    /// assert_eq!(ct.charset(), Some("utf-8"));
    /// ```
    pub fn parse(input: &str) -> Result<Self, ContentTypeError> {
        let input = trim_ows(input);
        if input.is_empty() {
            return Err(ContentTypeError::Empty);
        }

        // メディアタイプをパース (type/subtype)
        let (media_type_part, rest) = split_at_semicolon(input);
        let (media_type, subtype) = parse_media_type(media_type_part)?;

        // パラメータをパース
        let parameters = parse_parameters(rest)?;

        Ok(ContentType {
            media_type: media_type.to_ascii_lowercase(),
            subtype: subtype.to_ascii_lowercase(),
            parameters,
        })
    }

    /// 新しい ContentType を作成
    pub fn new(media_type: &str, subtype: &str) -> Self {
        ContentType {
            media_type: media_type.to_ascii_lowercase(),
            subtype: subtype.to_ascii_lowercase(),
            parameters: Vec::new(),
        }
    }

    /// パラメータを追加
    pub fn with_parameter(mut self, name: &str, value: &str) -> Self {
        self.parameters
            .push((name.to_ascii_lowercase(), value.to_string()));
        self
    }

    /// メディアタイプを取得 (例: "text")
    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    /// サブタイプを取得 (例: "html")
    pub fn subtype(&self) -> &str {
        &self.subtype
    }

    /// 完全なメディアタイプを取得 (例: "text/html")
    pub fn mime_type(&self) -> String {
        alloc::format!("{}/{}", self.media_type, self.subtype)
    }

    /// パラメータを取得
    pub fn parameter(&self, name: &str) -> Option<&str> {
        let name_lower = name.to_ascii_lowercase();
        self.parameters
            .iter()
            .find(|(n, _)| n == &name_lower)
            .map(|(_, v)| v.as_str())
    }

    /// すべてのパラメータを取得
    pub fn parameters(&self) -> &[(String, String)] {
        &self.parameters
    }

    /// charset パラメータを取得
    pub fn charset(&self) -> Option<&str> {
        self.parameter("charset")
    }

    /// boundary パラメータを取得
    pub fn boundary(&self) -> Option<&str> {
        self.parameter("boundary")
    }

    /// text/* かどうか
    pub fn is_text(&self) -> bool {
        self.media_type == "text"
    }

    /// application/json かどうか
    pub fn is_json(&self) -> bool {
        self.media_type == "application" && self.subtype == "json"
    }

    /// multipart/* かどうか
    pub fn is_multipart(&self) -> bool {
        self.media_type == "multipart"
    }

    /// multipart/form-data かどうか
    pub fn is_form_data(&self) -> bool {
        self.media_type == "multipart" && self.subtype == "form-data"
    }

    /// application/x-www-form-urlencoded かどうか
    pub fn is_form_urlencoded(&self) -> bool {
        self.media_type == "application" && self.subtype == "x-www-form-urlencoded"
    }
}

impl fmt::Display for ContentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.media_type, self.subtype)?;
        for (name, value) in &self.parameters {
            // 値に特殊文字が含まれる場合は引用符で囲む
            if needs_quoting(value) {
                write!(f, "; {}=\"{}\"", name, escape_quotes(value))?;
            } else {
                write!(f, "; {}={}", name, value)?;
            }
        }
        Ok(())
    }
}

/// セミコロンで分割 (最初のセミコロンのみ)
fn split_at_semicolon(input: &str) -> (&str, &str) {
    if let Some(pos) = input.find(';') {
        (trim_ows(&input[..pos]), trim_ows(&input[pos + 1..]))
    } else {
        (trim_ows(input), "")
    }
}

/// メディアタイプをパース
fn parse_media_type(input: &str) -> Result<(&str, &str), ContentTypeError> {
    let input = trim_ows(input);
    if input.is_empty() {
        return Err(ContentTypeError::InvalidMediaType);
    }

    let slash_pos = input.find('/').ok_or(ContentTypeError::InvalidMediaType)?;

    let media_type = trim_ows(&input[..slash_pos]);
    let subtype = trim_ows(&input[slash_pos + 1..]);

    if media_type.is_empty() || subtype.is_empty() {
        return Err(ContentTypeError::InvalidMediaType);
    }

    // トークン文字の検証
    if !is_valid_token(media_type) || !is_valid_token(subtype) {
        return Err(ContentTypeError::InvalidMediaType);
    }

    Ok((media_type, subtype))
}

/// パラメータをパース
fn parse_parameters(input: &str) -> Result<Vec<(String, String)>, ContentTypeError> {
    let mut parameters = Vec::new();
    let mut rest = trim_ows(input);

    while !rest.is_empty() {
        // セミコロンをスキップ
        rest = trim_ows(rest.trim_start_matches(';'));
        if rest.is_empty() {
            break;
        }

        // name=value をパース
        let eq_pos = rest.find('=').ok_or(ContentTypeError::InvalidParameter)?;
        let name = trim_ows(&rest[..eq_pos]);

        if name.is_empty() || !is_valid_token(name) {
            return Err(ContentTypeError::InvalidParameter);
        }

        rest = trim_ows(&rest[eq_pos + 1..]);

        // 値をパース (引用符付きまたはトークン)
        let (value, remaining) = if let Some(after_quote) = rest.strip_prefix('"') {
            parse_quoted_string(after_quote)?
        } else {
            parse_token_value(rest)?
        };

        parameters.push((name.to_ascii_lowercase(), value));
        rest = trim_ows(remaining.trim_start_matches(';'));
    }

    Ok(parameters)
}

// 引用符付き文字列のパースは `validate::parse_quoted_string` に委譲する。
// `From<QuotedStringError> for ContentTypeError` で文字種違反は `InvalidParameter`、
// 終端引用符なしは `UnterminatedQuote` にマップする。

/// トークン値をパース
///
/// RFC 9110 Section 5.6.6: パラメータ値がトークンの場合、
/// トークン文字 (tchar) のみで構成されている必要がある
fn parse_token_value(input: &str) -> Result<(String, &str), ContentTypeError> {
    let end = input
        .find(|c: char| c == ';' || c.is_whitespace())
        .unwrap_or(input.len());
    let token = &input[..end];

    // トークン値の検証 (RFC 9110 Section 5.6.2)
    if !is_valid_token(token) {
        return Err(ContentTypeError::InvalidParameter);
    }

    Ok((token.to_string(), &input[end..]))
}

/// 引用符で囲む必要があるかどうか
fn needs_quoting(s: &str) -> bool {
    // 空文字列は token として表現不能 (RFC 9110 Section 5.6.2: token = 1*tchar)
    // のため必ず引用符が必要。
    s.is_empty() || s.bytes().any(|b| !is_token_char(b))
}
