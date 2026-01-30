//! Accept 系ヘッダーパース (RFC 9110 Section 12.5)
//!
//! ## 概要
//!
//! RFC 9110 に基づいた Accept / Accept-Charset / Accept-Encoding / Accept-Language のパースを提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::accept::{Accept, AcceptEncoding};
//!
//! let accept = Accept::parse("text/html; q=0.5, */*; q=0.1").unwrap();
//! assert_eq!(accept.items().len(), 2);
//!
//! let encoding = AcceptEncoding::parse("gzip, identity;q=0.2").unwrap();
//! assert_eq!(encoding.items().len(), 2);
//! ```

use core::fmt;

/// Accept 系パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptError {
    /// 空の入力
    Empty,
    /// 不正な形式
    InvalidFormat,
    /// 不正なメディアレンジ
    InvalidMediaRange,
    /// 不正なトークン
    InvalidToken,
    /// 不正なパラメータ
    InvalidParameter,
    /// 不正な q 値
    InvalidQValue,
    /// 不正な言語タグ
    InvalidLanguageTag,
}

impl fmt::Display for AcceptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AcceptError::Empty => write!(f, "empty Accept header"),
            AcceptError::InvalidFormat => write!(f, "invalid Accept header format"),
            AcceptError::InvalidMediaRange => write!(f, "invalid media range"),
            AcceptError::InvalidToken => write!(f, "invalid token"),
            AcceptError::InvalidParameter => write!(f, "invalid parameter"),
            AcceptError::InvalidQValue => write!(f, "invalid qvalue"),
            AcceptError::InvalidLanguageTag => write!(f, "invalid language tag"),
        }
    }
}

impl std::error::Error for AcceptError {}

/// q 値 (0.000 - 1.000)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct QValue(u16);

impl QValue {
    /// q 値をパース
    pub fn parse(input: &str) -> Result<Self, AcceptError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(AcceptError::InvalidQValue);
        }

        if input == "1" {
            return Ok(QValue(1000));
        }

        if let Some(rest) = input.strip_prefix("1.") {
            if rest.is_empty() {
                return Ok(QValue(1000));
            }
            if rest.len() > 3 || !rest.chars().all(|c| c == '0') {
                return Err(AcceptError::InvalidQValue);
            }
            return Ok(QValue(1000));
        }

        if input == "0" {
            return Ok(QValue(0));
        }

        if let Some(rest) = input.strip_prefix("0.") {
            if rest.len() > 3 || !rest.chars().all(|c| c.is_ascii_digit()) {
                return Err(AcceptError::InvalidQValue);
            }
            let mut value = 0u16;
            for (idx, c) in rest.chars().enumerate() {
                let digit = c.to_digit(10).ok_or(AcceptError::InvalidQValue)? as u16;
                value += digit * 10u16.pow(2 - idx as u32);
            }
            return Ok(QValue(value));
        }

        Err(AcceptError::InvalidQValue)
    }

    /// ミリ単位の q 値 (0-1000)
    pub fn value(&self) -> u16 {
        self.0
    }

    /// f32 に変換
    pub fn as_f32(&self) -> f32 {
        self.0 as f32 / 1000.0
    }
}

impl Default for QValue {
    fn default() -> Self {
        QValue(1000)
    }
}

impl fmt::Display for QValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == 1000 {
            return write!(f, "1");
        }
        if self.0 == 0 {
            return write!(f, "0");
        }

        let mut frac = format!("{:03}", self.0);
        while frac.ends_with('0') {
            frac.pop();
        }
        write!(f, "0.{}", frac)
    }
}

/// Accept ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accept {
    items: Vec<MediaRange>,
}

impl Accept {
    /// Accept ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, AcceptError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(AcceptError::Empty);
        }

        let mut items = Vec::new();
        for part in split_with_quotes(input, ',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            items.push(parse_media_range_item(part)?);
        }

        if items.is_empty() {
            return Err(AcceptError::Empty);
        }

        Ok(Accept { items })
    }

    /// メディアレンジ一覧
    pub fn items(&self) -> &[MediaRange] {
        &self.items
    }
}

impl fmt::Display for Accept {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let values: Vec<String> = self.items.iter().map(|item| item.to_string()).collect();
        write!(f, "{}", values.join(", "))
    }
}

/// Accept メディアレンジ
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaRange {
    media_type: String,
    subtype: String,
    parameters: Vec<(String, String)>,
    q: QValue,
}

impl MediaRange {
    /// メディアタイプ (type)
    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    /// メディアサブタイプ
    pub fn subtype(&self) -> &str {
        &self.subtype
    }

    /// パラメータ
    pub fn parameters(&self) -> &[(String, String)] {
        &self.parameters
    }

    /// q 値
    pub fn qvalue(&self) -> QValue {
        self.q
    }
}

impl fmt::Display for MediaRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.media_type, self.subtype)?;
        for (name, value) in &self.parameters {
            if needs_quoting(value) {
                write!(f, "; {}=\"{}\"", name, escape_quotes(value))?;
            } else {
                write!(f, "; {}={}", name, value)?;
            }
        }
        if self.q.value() < 1000 {
            write!(f, "; q={}", self.q)?;
        }
        Ok(())
    }
}

/// Accept-Charset ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptCharset {
    items: Vec<CharsetRange>,
}

impl AcceptCharset {
    /// Accept-Charset ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, AcceptError> {
        let items = parse_weighted_tokens(input, validate_token_or_star, true, true)?
            .into_iter()
            .map(|(value, q)| CharsetRange { charset: value, q })
            .collect();
        Ok(AcceptCharset { items })
    }

    /// 文字セットレンジ一覧
    pub fn items(&self) -> &[CharsetRange] {
        &self.items
    }
}

impl fmt::Display for AcceptCharset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let values: Vec<String> = self.items.iter().map(|item| item.to_string()).collect();
        write!(f, "{}", values.join(", "))
    }
}

/// Accept-Charset レンジ
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharsetRange {
    charset: String,
    q: QValue,
}

impl CharsetRange {
    /// 文字セット
    pub fn charset(&self) -> &str {
        &self.charset
    }

    /// q 値
    pub fn qvalue(&self) -> QValue {
        self.q
    }
}

impl fmt::Display for CharsetRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.charset)?;
        if self.q.value() < 1000 {
            write!(f, "; q={}", self.q)?;
        }
        Ok(())
    }
}

/// Accept-Encoding ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptEncoding {
    items: Vec<EncodingRange>,
}

impl AcceptEncoding {
    /// Accept-Encoding ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, AcceptError> {
        let items = parse_weighted_tokens(input, validate_token_or_star, true, true)?
            .into_iter()
            .map(|(value, q)| EncodingRange { coding: value, q })
            .collect();
        Ok(AcceptEncoding { items })
    }

    /// エンコーディングレンジ一覧
    pub fn items(&self) -> &[EncodingRange] {
        &self.items
    }
}

impl fmt::Display for AcceptEncoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let values: Vec<String> = self.items.iter().map(|item| item.to_string()).collect();
        write!(f, "{}", values.join(", "))
    }
}

/// Accept-Encoding レンジ
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodingRange {
    coding: String,
    q: QValue,
}

impl EncodingRange {
    /// エンコーディング
    pub fn coding(&self) -> &str {
        &self.coding
    }

    /// q 値
    pub fn qvalue(&self) -> QValue {
        self.q
    }
}

impl fmt::Display for EncodingRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.coding)?;
        if self.q.value() < 1000 {
            write!(f, "; q={}", self.q)?;
        }
        Ok(())
    }
}

/// Accept-Language ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptLanguage {
    items: Vec<LanguageRange>,
}

impl AcceptLanguage {
    /// Accept-Language ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, AcceptError> {
        let items = parse_weighted_tokens(input, validate_language_range, false, true)?
            .into_iter()
            .map(|(value, q)| LanguageRange { language: value, q })
            .collect();
        Ok(AcceptLanguage { items })
    }

    /// 言語レンジ一覧
    pub fn items(&self) -> &[LanguageRange] {
        &self.items
    }
}

impl fmt::Display for AcceptLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let values: Vec<String> = self.items.iter().map(|item| item.to_string()).collect();
        write!(f, "{}", values.join(", "))
    }
}

/// Accept-Language レンジ
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageRange {
    language: String,
    q: QValue,
}

impl LanguageRange {
    /// 言語レンジ
    pub fn language(&self) -> &str {
        &self.language
    }

    /// q 値
    pub fn qvalue(&self) -> QValue {
        self.q
    }
}

impl fmt::Display for LanguageRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.language)?;
        if self.q.value() < 1000 {
            write!(f, "; q={}", self.q)?;
        }
        Ok(())
    }
}

fn parse_media_range_item(input: &str) -> Result<MediaRange, AcceptError> {
    let mut parts = split_with_quotes(input, ';').into_iter();
    let media = parts.next().unwrap_or_default().trim().to_string();
    let (media_type, subtype) = parse_media_range(&media)?;

    let mut params = Vec::new();
    let mut qvalue = QValue::default();
    let mut q_seen = false;

    for param in parts {
        let param = param.trim();
        if param.is_empty() {
            continue;
        }
        let (name, value) = param.split_once('=').ok_or(AcceptError::InvalidParameter)?;
        let name = name.trim().to_ascii_lowercase();
        let value = parse_param_value(value)?;

        if name == "q" {
            if q_seen {
                return Err(AcceptError::InvalidQValue);
            }
            qvalue = QValue::parse(&value)?;
            q_seen = true;
        } else {
            params.push((name, value));
        }
    }

    Ok(MediaRange {
        media_type,
        subtype,
        parameters: params,
        q: qvalue,
    })
}

fn parse_media_range(input: &str) -> Result<(String, String), AcceptError> {
    let input = input.trim();
    if input == "*/*" {
        return Ok(("*".to_string(), "*".to_string()));
    }

    let (media_type, subtype) = input
        .split_once('/')
        .ok_or(AcceptError::InvalidMediaRange)?;
    let media_type = media_type.trim();
    let subtype = subtype.trim();

    if media_type == "*" {
        if subtype != "*" {
            return Err(AcceptError::InvalidMediaRange);
        }
        return Ok(("*".to_string(), "*".to_string()));
    }

    if subtype == "*" {
        if !is_valid_token(media_type) {
            return Err(AcceptError::InvalidMediaRange);
        }
        return Ok((media_type.to_ascii_lowercase(), "*".to_string()));
    }

    if !is_valid_token(media_type) || !is_valid_token(subtype) {
        return Err(AcceptError::InvalidMediaRange);
    }

    Ok((
        media_type.to_ascii_lowercase(),
        subtype.to_ascii_lowercase(),
    ))
}

fn parse_weighted_tokens(
    input: &str,
    validator: fn(&str) -> bool,
    lowercase: bool,
    allow_wildcard: bool,
) -> Result<Vec<(String, QValue)>, AcceptError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(AcceptError::Empty);
    }

    let mut items = Vec::new();
    for part in split_with_quotes(input, ',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        let mut parts = split_with_quotes(part, ';').into_iter();
        let token_raw = parts.next().unwrap_or_default();
        let token = token_raw.trim();
        if token.is_empty() {
            return Err(AcceptError::InvalidFormat);
        }
        if token == "*" && !allow_wildcard {
            return Err(AcceptError::InvalidFormat);
        }
        if token != "*" && !validator(token) {
            return Err(AcceptError::InvalidToken);
        }

        let mut qvalue = QValue::default();
        let mut q_seen = false;

        for param in parts {
            let param = param.trim();
            if param.is_empty() {
                continue;
            }
            let (name, value) = param.split_once('=').ok_or(AcceptError::InvalidParameter)?;
            if name.trim().eq_ignore_ascii_case("q") {
                if q_seen {
                    return Err(AcceptError::InvalidQValue);
                }
                qvalue = QValue::parse(value.trim())?;
                q_seen = true;
            } else {
                return Err(AcceptError::InvalidParameter);
            }
        }

        let mut token_value = token.to_string();
        if lowercase && token_value != "*" {
            token_value = token_value.to_ascii_lowercase();
        }

        items.push((token_value, qvalue));
    }

    if items.is_empty() {
        return Err(AcceptError::Empty);
    }

    Ok(items)
}

fn parse_param_value(input: &str) -> Result<String, AcceptError> {
    let input = input.trim();
    if let Some(rest) = input.strip_prefix('"') {
        let (value, remaining) = parse_quoted_string(rest)?;
        if !remaining.trim().is_empty() {
            return Err(AcceptError::InvalidParameter);
        }
        Ok(value)
    } else {
        if !is_valid_token(input) {
            return Err(AcceptError::InvalidToken);
        }
        Ok(input.to_string())
    }
}

fn parse_quoted_string(input: &str) -> Result<(String, &str), AcceptError> {
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

    Err(AcceptError::InvalidParameter)
}

fn split_with_quotes(input: &str, delimiter: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_quote = false;
    let mut escaped = false;

    for (i, c) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' && in_quote {
            escaped = true;
            continue;
        }
        if c == '"' {
            in_quote = !in_quote;
            continue;
        }
        if c == delimiter && !in_quote {
            parts.push(input[start..i].to_string());
            start = i + c.len_utf8();
        }
    }
    parts.push(input[start..].to_string());
    parts
}

fn validate_token_or_star(token: &str) -> bool {
    if token == "*" {
        return true;
    }
    is_valid_token(token)
}

fn validate_language_range(token: &str) -> bool {
    if token == "*" {
        return true;
    }
    is_valid_language_tag(token)
}

fn is_valid_language_tag(tag: &str) -> bool {
    if tag.is_empty() {
        return false;
    }
    let mut parts = tag.split('-');

    // BCP 47/RFC 5646: 先頭サブタグは ALPHA のみ (数字不可)
    let Some(primary) = parts.next() else {
        return false;
    };
    if primary.is_empty() || primary.len() > 8 || !primary.chars().all(|c| c.is_ascii_alphabetic())
    {
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

fn is_valid_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_token_char)
}

fn is_token_char(b: u8) -> bool {
    matches!(
        b,
        b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.' |
        b'0'..=b'9' | b'A'..=b'Z' | b'^' | b'_' | b'`' | b'a'..=b'z' | b'|' | b'~'
    )
}

fn needs_quoting(s: &str) -> bool {
    s.bytes().any(|b| !is_token_char(b))
}

fn escape_quotes(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accept_simple() {
        let accept = Accept::parse("text/html").unwrap();
        assert_eq!(accept.items().len(), 1);
        let item = &accept.items()[0];
        assert_eq!(item.media_type(), "text");
        assert_eq!(item.subtype(), "html");
        assert_eq!(item.qvalue().value(), 1000);
    }

    #[test]
    fn parse_accept_wildcard() {
        let accept = Accept::parse("text/*; q=0.5").unwrap();
        let item = &accept.items()[0];
        assert_eq!(item.subtype(), "*");
        assert_eq!(item.qvalue().value(), 500);
    }

    #[test]
    fn parse_accept_with_params() {
        let accept = Accept::parse("text/html; level=1; q=0.7").unwrap();
        let item = &accept.items()[0];
        assert_eq!(item.parameters()[0].0, "level");
        assert_eq!(item.parameters()[0].1, "1");
        assert_eq!(item.qvalue().value(), 700);
    }

    #[test]
    fn parse_accept_invalid_q() {
        assert!(Accept::parse("text/html;q=1.5").is_err());
    }

    #[test]
    fn parse_accept_charset() {
        let ac = AcceptCharset::parse("utf-8, iso-8859-1;q=0.5").unwrap();
        assert_eq!(ac.items().len(), 2);
        assert_eq!(ac.items()[1].qvalue().value(), 500);
    }

    #[test]
    fn parse_accept_encoding() {
        let ae = AcceptEncoding::parse("gzip, identity;q=0.2").unwrap();
        assert_eq!(ae.items()[0].coding(), "gzip");
        assert_eq!(ae.items()[1].qvalue().value(), 200);
    }

    #[test]
    fn parse_accept_language() {
        let al = AcceptLanguage::parse("en-US, ja;q=0.8").unwrap();
        assert_eq!(al.items()[0].language(), "en-US");
        assert_eq!(al.items()[1].qvalue().value(), 800);
    }

    #[test]
    fn display_accept() {
        let accept = Accept::parse("text/html; q=0.5").unwrap();
        assert_eq!(accept.to_string(), "text/html; q=0.5");
    }

    #[test]
    fn parse_accept_language_primary_subtag_alpha_only() {
        // BCP 47/RFC 5646: 先頭サブタグは ALPHA のみ
        // 数字で始まる言語タグは不正
        assert!(AcceptLanguage::parse("123").is_err());
        assert!(AcceptLanguage::parse("1ab").is_err());
        // 後続サブタグは ALPHA / DIGIT OK
        let al = AcceptLanguage::parse("en-123").unwrap();
        assert_eq!(al.items()[0].language(), "en-123");
    }
}
