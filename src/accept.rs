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

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::validate::{
    QuotedStringError, escape_quotes, is_token_char, is_valid_language_tag, is_valid_token,
    parse_quoted_string, split_with_quotes, trim_ows,
};

/// Accept 系パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AcceptError {
    /// 空の入力
    Empty,
    /// 不正な形式
    InvalidFormat,
    /// 不正なメディアレンジ
    InvalidMediaRange,
    /// 不正なトークン
    InvalidToken,
    /// 不正なパラメータ (qdtext / quoted-pair の文字種違反を含む)
    InvalidParameter,
    /// quoted-string の閉じ DQUOTE が見つからない (RFC 9110 Section 5.6.4)
    UnterminatedQuote,
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
            AcceptError::UnterminatedQuote => write!(f, "unterminated quoted-string"),
            AcceptError::InvalidQValue => write!(f, "invalid qvalue"),
            AcceptError::InvalidLanguageTag => write!(f, "invalid language tag"),
        }
    }
}

impl core::error::Error for AcceptError {}

impl From<QuotedStringError> for AcceptError {
    fn from(e: QuotedStringError) -> Self {
        match e {
            QuotedStringError::InvalidQdtext | QuotedStringError::InvalidQuotedPair => {
                AcceptError::InvalidParameter
            }
            QuotedStringError::Unterminated => AcceptError::UnterminatedQuote,
        }
    }
}

/// q 値 (0.000 - 1.000)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct QValue(u16);

impl QValue {
    /// q 値をパース
    pub fn parse(input: &str) -> Result<Self, AcceptError> {
        let input = trim_ows(input);
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

        let mut frac = alloc::format!("{:03}", self.0);
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
    /// RFC 9110 Section 5.6.1.2: 受信者は空のリスト要素を無視しなければならない (MUST)。
    /// 空の値は空リストとして受理する。
    pub fn parse(input: &str) -> Result<Self, AcceptError> {
        let input = trim_ows(input);

        let mut items = Vec::new();
        if !input.is_empty() {
            for part in split_with_quotes(input, ',') {
                let part = trim_ows(&part);
                if part.is_empty() {
                    continue;
                }
                items.push(parse_media_range_item(part)?);
            }
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
///
/// 注: RFC 9110 Section 12.5.2 で deprecated とされている。
/// 一般的に使われていないが、後方互換性のために実装を残している。
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
        let param = trim_ows(&param);
        if param.is_empty() {
            continue;
        }
        let (name, value) = param.split_once('=').ok_or(AcceptError::InvalidParameter)?;
        let name = trim_ows(name).to_ascii_lowercase();
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
    let input = trim_ows(input);
    if input == "*/*" {
        return Ok(("*".to_string(), "*".to_string()));
    }

    let (media_type, subtype) = input
        .split_once('/')
        .ok_or(AcceptError::InvalidMediaRange)?;
    let media_type = trim_ows(media_type);
    let subtype = trim_ows(subtype);

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

/// RFC 9110 Section 5.6.1.2: 受信者は空のリスト要素を無視しなければならない (MUST)。
/// 空の値は空リストとして受理する。
fn parse_weighted_tokens(
    input: &str,
    validator: fn(&str) -> bool,
    lowercase: bool,
    allow_wildcard: bool,
) -> Result<Vec<(String, QValue)>, AcceptError> {
    let input = trim_ows(input);
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    for part in split_with_quotes(input, ',') {
        let part = trim_ows(&part);
        if part.is_empty() {
            continue;
        }

        let mut parts = split_with_quotes(part, ';').into_iter();
        let token_raw = parts.next().unwrap_or_default();
        let token = trim_ows(&token_raw);
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
            let param = trim_ows(&param);
            if param.is_empty() {
                continue;
            }
            let (name, value) = param.split_once('=').ok_or(AcceptError::InvalidParameter)?;
            if trim_ows(name).eq_ignore_ascii_case("q") {
                if q_seen {
                    return Err(AcceptError::InvalidQValue);
                }
                qvalue = QValue::parse(trim_ows(value))?;
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

    Ok(items)
}

fn parse_param_value(input: &str) -> Result<String, AcceptError> {
    let input = trim_ows(input);
    if let Some(rest) = input.strip_prefix('"') {
        let (value, remaining) = parse_quoted_string(rest)?;
        if !trim_ows(remaining).is_empty() {
            return Err(AcceptError::InvalidParameter);
        }
        Ok(value)
    } else if !is_valid_token(input) {
        Err(AcceptError::InvalidToken)
    } else {
        Ok(input.to_string())
    }
}

// 引用符付き文字列のパースは `validate::parse_quoted_string` に委譲する。
// `From<QuotedStringError> for AcceptError` で文字種違反は `InvalidParameter`、
// 終端引用符なしは `UnterminatedQuote` にマップする。

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

fn needs_quoting(s: &str) -> bool {
    // 空文字列は token として表現不能 (RFC 9110 Section 5.6.2: token = 1*tchar)
    // のため必ず引用符が必要。
    s.is_empty() || s.bytes().any(|b| !is_token_char(b))
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
