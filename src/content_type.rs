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

use core::fmt;

/// Content-Type パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
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

impl std::error::Error for ContentTypeError {}

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
        let input = input.trim();
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
        format!("{}/{}", self.media_type, self.subtype)
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
        (input[..pos].trim(), input[pos + 1..].trim())
    } else {
        (input.trim(), "")
    }
}

/// メディアタイプをパース
fn parse_media_type(input: &str) -> Result<(&str, &str), ContentTypeError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ContentTypeError::InvalidMediaType);
    }

    let slash_pos = input.find('/').ok_or(ContentTypeError::InvalidMediaType)?;

    let media_type = input[..slash_pos].trim();
    let subtype = input[slash_pos + 1..].trim();

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
    let mut rest = input.trim();

    while !rest.is_empty() {
        // セミコロンをスキップ
        rest = rest.trim_start_matches(';').trim();
        if rest.is_empty() {
            break;
        }

        // name=value をパース
        let eq_pos = rest.find('=').ok_or(ContentTypeError::InvalidParameter)?;
        let name = rest[..eq_pos].trim();

        if name.is_empty() || !is_valid_token(name) {
            return Err(ContentTypeError::InvalidParameter);
        }

        rest = rest[eq_pos + 1..].trim();

        // 値をパース (引用符付きまたはトークン)
        let (value, remaining) = if let Some(after_quote) = rest.strip_prefix('"') {
            parse_quoted_string(after_quote)?
        } else {
            parse_token_value(rest)
        };

        parameters.push((name.to_ascii_lowercase(), value));
        rest = remaining.trim_start_matches(';').trim();
    }

    Ok(parameters)
}

/// 引用符付き文字列をパース
fn parse_quoted_string(input: &str) -> Result<(String, &str), ContentTypeError> {
    let mut result = String::new();
    let mut escaped = false;

    for (i, c) in input.char_indices() {
        if escaped {
            result.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            return Ok((result, &input[i + 1..]));
        } else {
            result.push(c);
        }
    }

    Err(ContentTypeError::UnterminatedQuote)
}

/// トークン値をパース
fn parse_token_value(input: &str) -> (String, &str) {
    let end = input
        .find(|c: char| c == ';' || c.is_whitespace())
        .unwrap_or(input.len());
    (input[..end].to_string(), &input[end..])
}

/// 有効なトークン文字かどうか
fn is_valid_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_token_char)
}

/// RFC 7230 のトークン文字
fn is_token_char(b: u8) -> bool {
    matches!(b,
        b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.' |
        b'0'..=b'9' | b'A'..=b'Z' | b'^' | b'_' | b'`' | b'a'..=b'z' | b'|' | b'~'
    )
}

/// 引用符で囲む必要があるかどうか
fn needs_quoting(s: &str) -> bool {
    s.bytes().any(|b| !is_token_char(b))
}

/// 引用符をエスケープ
fn escape_quotes(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let ct = ContentType::parse("text/html").unwrap();
        assert_eq!(ct.media_type(), "text");
        assert_eq!(ct.subtype(), "html");
        assert_eq!(ct.mime_type(), "text/html");
        assert!(ct.parameters().is_empty());
    }

    #[test]
    fn test_parse_with_charset() {
        let ct = ContentType::parse("text/html; charset=utf-8").unwrap();
        assert_eq!(ct.media_type(), "text");
        assert_eq!(ct.subtype(), "html");
        assert_eq!(ct.charset(), Some("utf-8"));
    }

    #[test]
    fn test_parse_with_quoted_charset() {
        let ct = ContentType::parse("text/html; charset=\"utf-8\"").unwrap();
        assert_eq!(ct.charset(), Some("utf-8"));
    }

    #[test]
    fn test_parse_multipart() {
        let ct =
            ContentType::parse("multipart/form-data; boundary=----WebKitFormBoundary").unwrap();
        assert!(ct.is_form_data());
        assert_eq!(ct.boundary(), Some("----WebKitFormBoundary"));
    }

    #[test]
    fn test_parse_case_insensitive() {
        let ct = ContentType::parse("TEXT/HTML; CHARSET=UTF-8").unwrap();
        assert_eq!(ct.media_type(), "text");
        assert_eq!(ct.subtype(), "html");
        assert_eq!(ct.charset(), Some("UTF-8")); // 値は大文字小文字を保持
    }

    #[test]
    fn test_parse_multiple_parameters() {
        let ct = ContentType::parse("text/plain; charset=utf-8; boundary=something").unwrap();
        assert_eq!(ct.charset(), Some("utf-8"));
        assert_eq!(ct.boundary(), Some("something"));
    }

    #[test]
    fn test_parse_json() {
        let ct = ContentType::parse("application/json").unwrap();
        assert!(ct.is_json());
    }

    #[test]
    fn test_parse_form_urlencoded() {
        let ct = ContentType::parse("application/x-www-form-urlencoded").unwrap();
        assert!(ct.is_form_urlencoded());
    }

    #[test]
    fn test_parse_with_spaces() {
        let ct = ContentType::parse("  text/html  ;  charset = utf-8  ").unwrap();
        assert_eq!(ct.media_type(), "text");
        assert_eq!(ct.subtype(), "html");
    }

    #[test]
    fn test_parse_quoted_with_escape() {
        let ct = ContentType::parse("text/plain; name=\"hello\\\"world\"").unwrap();
        assert_eq!(ct.parameter("name"), Some("hello\"world"));
    }

    #[test]
    fn test_parse_empty() {
        assert!(ContentType::parse("").is_err());
    }

    #[test]
    fn test_parse_no_subtype() {
        assert!(ContentType::parse("text").is_err());
    }

    #[test]
    fn test_parse_empty_subtype() {
        assert!(ContentType::parse("text/").is_err());
    }

    #[test]
    fn test_display() {
        let ct = ContentType::new("text", "html").with_parameter("charset", "utf-8");
        assert_eq!(ct.to_string(), "text/html; charset=utf-8");
    }

    #[test]
    fn test_display_quoted() {
        let ct = ContentType::new("text", "plain").with_parameter("name", "hello world");
        assert_eq!(ct.to_string(), "text/plain; name=\"hello world\"");
    }

    #[test]
    fn test_is_text() {
        assert!(ContentType::parse("text/plain").unwrap().is_text());
        assert!(ContentType::parse("text/html").unwrap().is_text());
        assert!(!ContentType::parse("application/json").unwrap().is_text());
    }

    #[test]
    fn test_is_multipart() {
        assert!(
            ContentType::parse("multipart/form-data")
                .unwrap()
                .is_multipart()
        );
        assert!(
            ContentType::parse("multipart/mixed")
                .unwrap()
                .is_multipart()
        );
        assert!(!ContentType::parse("text/plain").unwrap().is_multipart());
    }
}
