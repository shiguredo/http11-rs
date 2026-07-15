//! Accept-Query ヘッダーパース (RFC 10008 Section 3 / RFC 9651 Structured Fields)
//!
//! ## 概要
//!
//! RFC 10008 Section 3 に基づいた Accept-Query レスポンスヘッダーのパースを提供します。
//! Accept-Query は RFC 9651 Structured Fields の List として定義され、
//! QUERY メソッドで受け付け可能なクエリ形式メディアタイプを通知します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::accept_query::AcceptQuery;
//!
//! let aq = AcceptQuery::parse("application/sql, \"application/jsonpath\"").unwrap();
//! assert_eq!(aq.items().len(), 2);
//! assert_eq!(aq.items()[0].media_type(), "application");
//! assert_eq!(aq.items()[0].subtype(), "sql");
//! ```

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::validate::{is_token_char, is_valid_token};

/// Accept-Query パースエラー
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptQueryError {
    /// 不正な形式 (trailing comma / 連続カンマ / 未消費残り文字 等)
    InvalidFormat,
    /// 不正なメディアレンジ (type/subtype 形式でない / `*/subtype` /
    /// type/subtype が HTTP token として不正な文字を含む / SF Token として
    /// 不正な文字を含む media range)
    InvalidMediaRange,
    /// 不正なパラメータ (SF Parameter key 不正 / 値なし (Boolean true) /
    /// parameter value が String/Token 以外 / 重複 parameter key)
    InvalidParameter,
    /// SF String の閉じ DQUOTE が見つからない (RFC 9651 Section 4.2.5)
    UnterminatedQuote,
    /// リストメンバーが Token/String 以外の SF 型 (Integer / Decimal / Boolean /
    /// Byte Sequence / Date / Display String / Inner List)
    UnsupportedItemType,
}

impl fmt::Display for AcceptQueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AcceptQueryError::InvalidFormat => write!(f, "invalid Accept-Query format"),
            AcceptQueryError::InvalidMediaRange => write!(f, "invalid media range"),
            AcceptQueryError::InvalidParameter => write!(f, "invalid parameter"),
            AcceptQueryError::UnterminatedQuote => write!(f, "unterminated string"),
            AcceptQueryError::UnsupportedItemType => write!(f, "unsupported item type"),
        }
    }
}

impl core::error::Error for AcceptQueryError {}

/// Accept-Query ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptQuery {
    items: Vec<MediaRangeItem>,
}

impl AcceptQuery {
    /// Accept-Query ヘッダーをパースする
    ///
    /// RFC 9651 Section 4.2 (Parsing Structured Fields) の List としてパースする。
    /// 空入力は空リストとして受理する
    /// (RFC 9651 Section 4.2.1 step 3: 空 List を返す)。
    pub fn parse(input: &str) -> Result<Self, AcceptQueryError> {
        // RFC 9651 Section 4.2 step 1: Convert input_bytes into an ASCII string;
        // if conversion fails, fail parsing.
        // 入力が非 ASCII バイト (0x80-0xFF) を含む場合はパース全体を fail させる。
        let bytes = input.as_bytes();
        if bytes.iter().any(|&b| b >= 0x80) {
            return Err(AcceptQueryError::InvalidFormat);
        }

        // step 2: 先頭の SP を discard (SP のみ。OWS ではない)
        let mut pos = 0;
        while pos < bytes.len() && bytes[pos] == b' ' {
            pos += 1;
        }

        // 空入力は空リストとして受理
        if pos == bytes.len() {
            return Ok(AcceptQuery { items: Vec::new() });
        }

        // RFC 9651 Section 4.2.1: Parsing a List
        let mut items = Vec::new();
        loop {
            // step 2.1: Parsing an Item or Inner List (Section 4.2.1.1)
            let item = parse_item_or_inner_list(bytes, &mut pos)?;
            items.push(item);

            // step 2.2: Discard any leading OWS characters
            // RFC 9651 Section 4.2.1 step 2.2 / 2.5 は OWS (SP / HTAB) を許容する
            discard_ows(bytes, &mut pos);

            // step 2.3: If input_string is empty, return members
            if pos == bytes.len() {
                return Ok(AcceptQuery { items });
            }

            // step 2.4: Consume the first character; if it is not ",", fail parsing
            if bytes[pos] != b',' {
                return Err(AcceptQueryError::InvalidFormat);
            }
            pos += 1;

            // step 2.5: Discard any leading OWS characters
            discard_ows(bytes, &mut pos);

            // step 2.6: If input_string is empty, there is a trailing comma; fail parsing
            if pos == bytes.len() {
                return Err(AcceptQueryError::InvalidFormat);
            }
        }
    }

    /// メディアレンジ一覧
    pub fn items(&self) -> &[MediaRangeItem] {
        &self.items
    }
}

impl fmt::Display for AcceptQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let values: Vec<String> = self.items.iter().map(|item| item.to_string()).collect();
        write!(f, "{}", values.join(", "))
    }
}

/// Accept-Query のメディアレンジアイテム
///
/// Token/String の区別は保持せず、メディアレンジ値とパラメータを文字列として保持する
/// (RFC 10008 Section 3: "The choice of Token versus String is semantically insignificant")。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaRangeItem {
    media_type: String,
    subtype: String,
    parameters: Vec<(String, String)>,
}

impl MediaRangeItem {
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
}

impl fmt::Display for MediaRangeItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // type/subtype は is_valid_token で tchar only 検査済みのため、
        // SF Token 表現可能性は type の先頭文字 (ALPHA/`*`) だけで決まる
        let range_is_token = is_alpha_or_star(self.media_type.as_bytes()[0]);
        if range_is_token {
            write!(f, "{}/{}", self.media_type, self.subtype)?;
        } else {
            // 先頭数字等で SF Token として表現不可能な場合は SF String 形式
            write_sf_string(f, &format!("{}/{}", self.media_type, self.subtype))?;
        }
        for (name, value) in &self.parameters {
            // RFC 10008 Section 3 の例 (行 447): `application/sql;charset="UTF-8"`
            // パラメータセパレータは `;` (SP なし)
            write!(f, ";{}=", name)?;
            if can_be_token(value) {
                write!(f, "{}", value)?;
            } else {
                // 空パラメータ値や SF Token 不可な文字を含む場合は SF String
                write_sf_string(f, value)?;
            }
        }
        Ok(())
    }
}

/// SF String として formatter に書き込む (RFC 9651 Section 4.2.5)
///
/// `"` と `\` をエスケープし、DQUOTE で囲む。
fn write_sf_string(f: &mut fmt::Formatter<'_>, s: &str) -> fmt::Result {
    write!(f, "\"")?;
    for c in s.chars() {
        if c == '"' || c == '\\' {
            write!(f, "\\")?;
        }
        write!(f, "{}", c)?;
    }
    write!(f, "\"")
}

// ========================================
// RFC 9651 Structured Fields パーサー (Accept-Query 向け最小実装)
// ========================================

/// OWS (SP / HTAB) を discard する (RFC 9651 Section 4.2.1 step 2.2 / 2.5)
///
/// RFC 9651 Section 4.2.1 (Parsing a List) のメンバー間 OWS は SP / HTAB 両方を許容する。
/// Section 4.2 step 2 (先頭) は SP のみなので本関数は使わない。
fn discard_ows(bytes: &[u8], pos: &mut usize) {
    while *pos < bytes.len() && (bytes[*pos] == b' ' || bytes[*pos] == b'\t') {
        *pos += 1;
    }
}

/// SP のみを discard する (RFC 9651 Section 4.2.3.2 step 2.3 / 4.2.1.2 step 3.1)
fn discard_sp(bytes: &[u8], pos: &mut usize) {
    while *pos < bytes.len() && bytes[*pos] == b' ' {
        *pos += 1;
    }
}

/// RFC 9651 Section 4.2.1.1: Parsing an Item or Inner List
fn parse_item_or_inner_list(
    bytes: &[u8],
    pos: &mut usize,
) -> Result<MediaRangeItem, AcceptQueryError> {
    if *pos < bytes.len() && bytes[*pos] == b'(' {
        // Inner List は Accept-Query では許可されない (RFC 10008 Section 3)
        return Err(AcceptQueryError::UnsupportedItemType);
    }
    // Parsing an Item (Section 4.2.3)
    parse_item(bytes, pos)
}

/// RFC 9651 Section 4.2.3: Parsing an Item
fn parse_item(bytes: &[u8], pos: &mut usize) -> Result<MediaRangeItem, AcceptQueryError> {
    // step 1: Parsing a Bare Item (Section 4.2.3.1)
    let bare_item = parse_bare_item(bytes, pos)?;

    // step 2: Parsing Parameters (Section 4.2.3.2)
    let parameters = parse_parameters(bytes, pos)?;

    // step 3: Return the tuple (bare_item, parameters)
    // bare_item は media range 文字列 (Token または String から取り出した値)
    let (media_type, subtype) = parse_media_range(&bare_item)?;

    Ok(MediaRangeItem {
        media_type,
        subtype,
        parameters,
    })
}

/// RFC 9651 Section 4.2.3.1: Parsing a Bare Item
///
/// Accept-Query は Token または String のみを許可する (RFC 10008 Section 3)。
/// それ以外の SF 型 (Integer / Decimal / Boolean / Byte Sequence / Date /
/// Display String) は `UnsupportedItemType` で拒否する。
fn parse_bare_item(bytes: &[u8], pos: &mut usize) -> Result<String, AcceptQueryError> {
    if *pos >= bytes.len() {
        return Err(AcceptQueryError::InvalidFormat);
    }
    let first = bytes[*pos];
    match first {
        b'"' => parse_sf_string(bytes, pos),
        b':' | b'@' | b'%' => Err(AcceptQueryError::UnsupportedItemType),
        b'?' => Err(AcceptQueryError::UnsupportedItemType),
        b'-' | b'0'..=b'9' => Err(AcceptQueryError::UnsupportedItemType),
        c if is_alpha_or_star(c) => parse_sf_token(bytes, pos),
        _ => {
            // 不正な先頭文字
            // ASCII 範囲外 (0x80-0xFF) は RFC 9651 Section 4.2 step 1 で fail
            // ASCII 範囲内で上記以外は認識不能
            Err(AcceptQueryError::InvalidFormat)
        }
    }
}

/// ASCII ALPHA または `*` か判定 (RFC 9651 Section 4.2.6)
fn is_alpha_or_star(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'*'
}

/// RFC 9651 Section 4.2.6: Parsing a Token
///
/// 構文: `( ALPHA / "*" ) *( tchar / ":" / "/" )`
fn parse_sf_token(bytes: &[u8], pos: &mut usize) -> Result<String, AcceptQueryError> {
    // step 1: 先頭は ALPHA または `*`
    debug_assert!(*pos < bytes.len() && is_alpha_or_star(bytes[*pos]));

    let mut output = String::new();
    output.push(bytes[*pos] as char);
    *pos += 1;

    // step 3: 後続は tchar / `:` / `/`
    while *pos < bytes.len() {
        let b = bytes[*pos];
        if is_sf_token_trailing_char(b) {
            output.push(b as char);
            *pos += 1;
        } else {
            break;
        }
    }

    Ok(output)
}

/// SF Token の後続文字 (tchar / `:` / `/`) か判定 (RFC 9651 Section 4.2.6)
fn is_sf_token_trailing_char(b: u8) -> bool {
    is_token_char(b) || b == b':' || b == b'/'
}

/// RFC 9651 Section 4.2.5: Parsing a String
///
/// 構文: `DQUOTE *( unescaped / escaped ) DQUOTE`
/// - `unescaped = %x20-21 / %x23-5B / %x5D-7E` (SP / VCHAR から DQUOTE と backslash を除く)
/// - `escape = "\" ( DQUOTE / "\" )`
///
/// HTAB (0x09) / obs-text (0x80-FF) / その他 CTL は不許可 (RFC 9651 は ASCII のみ)。
fn parse_sf_string(bytes: &[u8], pos: &mut usize) -> Result<String, AcceptQueryError> {
    debug_assert!(*pos < bytes.len() && bytes[*pos] == b'"');

    // step 2: 先頭の DQUOTE を消費
    *pos += 1;

    let mut output = String::new();
    while *pos < bytes.len() {
        let c = bytes[*pos];
        // step 4.2: backslash の場合
        if c == b'\\' {
            *pos += 1;
            // step 4.2.1: 入力が尽きた場合は fail
            if *pos >= bytes.len() {
                return Err(AcceptQueryError::InvalidFormat);
            }
            let next = bytes[*pos];
            // step 4.2.3: next_char は DQUOTE または backslash のみ
            if next != b'"' && next != b'\\' {
                return Err(AcceptQueryError::InvalidFormat);
            }
            output.push(next as char);
            *pos += 1;
        } else if c == b'"' {
            // step 4.3: 閉じ DQUOTE
            *pos += 1;
            return Ok(output);
        } else if !(0x20..0x7f).contains(&c) {
            // step 4.4: %x00-1f / %x7f-ff は fail (HTAB も不許可)
            // 0x80-0xFF は RFC 9651 Section 4.2 step 1 の ASCII 変換で
            // 既に排除済みのため、ここでは 0x7f (DEL) のみ到達可能
            return Err(AcceptQueryError::InvalidFormat);
        } else {
            // step 4.5: それ以外 (%x20-21 / %x23-5B / %x5D-7E) は受理
            output.push(c as char);
            *pos += 1;
        }
    }

    // step 5: 閉じ DQUOTE が見つからず入力が尽きた
    Err(AcceptQueryError::UnterminatedQuote)
}
/// RFC 9651 Section 4.2.3.2: Parsing Parameters
///
/// Accept-Query では parameter value は String または Token のみ許可する
/// (RFC 10008 Section 3)。Boolean true (`;key` 値なし) や Integer / Decimal 等は
/// `InvalidParameter` で拒否する。
/// 重複 parameter key は RFC 9651 Section 4.2.3.2 step 7 に従い
/// 最後の値で上書き (last-wins) する。
fn parse_parameters(
    bytes: &[u8],
    pos: &mut usize,
) -> Result<Vec<(String, String)>, AcceptQueryError> {
    let mut parameters: Vec<(String, String)> = Vec::new();

    loop {
        // step 2.1: `;` でなければループを抜ける
        if *pos >= bytes.len() || bytes[*pos] != b';' {
            break;
        }
        // step 2.2: `;` を消費
        *pos += 1;

        // step 2.3: 先頭の SP を discard (SP のみ。OWS ではない)
        discard_sp(bytes, pos);

        // step 2.4: Parsing a Key (Section 4.2.3.3)
        let param_key = parse_key(bytes, pos)?;

        // step 2.5: param_value を Boolean true に設定 (デフォルト)
        // step 2.6: `=` があれば Bare Item をパース
        let param_value: Result<String, AcceptQueryError> =
            if *pos < bytes.len() && bytes[*pos] == b'=' {
                *pos += 1;
                // parameter value は Bare Item としてパース
                // Accept-Query では String / Token のみ許可
                parse_param_bare_item(bytes, pos)
            } else {
                // 値なし (Boolean true) は Accept-Query では許可しない
                Err(AcceptQueryError::InvalidParameter)
            };

        let value = param_value?;

        // step 2.7 / 2.8: 重複 key は最後の値で上書き (RFC 9651 Section 4.2.3.2)
        if let Some(existing) = parameters.iter_mut().find(|(k, _)| k == &param_key) {
            existing.1 = value;
        } else {
            parameters.push((param_key, value));
        }
    }

    Ok(parameters)
}

/// parameter value 向けの Bare Item パース
///
/// Accept-Query では String または Token のみ許可する (RFC 10008 Section 3)。
/// Integer / Decimal / Boolean / Byte Sequence / Date / Display String は
/// `InvalidParameter` で拒否する (リストメンバーの型違反は `UnsupportedItemType`
/// だが、parameter value の型違反は `InvalidParameter` と使い分ける)。
fn parse_param_bare_item(bytes: &[u8], pos: &mut usize) -> Result<String, AcceptQueryError> {
    if *pos >= bytes.len() {
        return Err(AcceptQueryError::InvalidParameter);
    }
    let first = bytes[*pos];
    match first {
        b'"' => parse_sf_string(bytes, pos).map_err(|e| match e {
            AcceptQueryError::UnterminatedQuote => AcceptQueryError::UnterminatedQuote,
            _ => AcceptQueryError::InvalidParameter,
        }),
        b':' | b'@' | b'%' | b'?' => Err(AcceptQueryError::InvalidParameter),
        b'-' | b'0'..=b'9' => Err(AcceptQueryError::InvalidParameter),
        c if is_alpha_or_star(c) => parse_sf_token(bytes, pos),
        _ => Err(AcceptQueryError::InvalidParameter),
    }
}

/// RFC 9651 Section 4.2.3.3: Parsing a Key
///
/// 構文: `( lcalpha / "*" ) *( lcalpha / DIGIT / "_" / "-" / "." / "*" )`
/// 先頭は小文字 ALPHA または `*`。大文字不可。
fn parse_key(bytes: &[u8], pos: &mut usize) -> Result<String, AcceptQueryError> {
    if *pos >= bytes.len() {
        return Err(AcceptQueryError::InvalidParameter);
    }
    let first = bytes[*pos];
    // step 1: 先頭は lcalpha または `*`
    if !(first.is_ascii_lowercase() || first == b'*') {
        return Err(AcceptQueryError::InvalidParameter);
    }

    let mut output = String::new();
    output.push(first as char);
    *pos += 1;

    // step 3: 後続は lcalpha / DIGIT / `_` / `-` / `.` / `*`
    while *pos < bytes.len() {
        let b = bytes[*pos];
        if b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-' | b'.' | b'*') {
            output.push(b as char);
            *pos += 1;
        } else {
            break;
        }
    }

    Ok(output)
}

/// media range 文字列を type/subtype に分割・検証する
///
/// RFC 10008 Section 3:
/// - ワイルドカードは `*/*` と `xxxx/*` のみ。`*/subtype` は不可
/// - media type / subtype は小文字に正規化する (RFC 9110 Section 8.3.1)
/// - type/subtype は HTTP token (tchar only) として検証する
///   (RFC 9110 Section 8.3.1 + Section 5.6.2)
fn parse_media_range(input: &str) -> Result<(String, String), AcceptQueryError> {
    if input == "*/*" {
        return Ok(("*".to_string(), "*".to_string()));
    }

    let (media_type, subtype) = input
        .split_once('/')
        .ok_or(AcceptQueryError::InvalidMediaRange)?;

    if media_type == "*" {
        // `*/subtype` は不可 (RFC 10008 Section 3)
        return Err(AcceptQueryError::InvalidMediaRange);
    }

    if subtype == "*" {
        // `xxxx/*` は許可。type は HTTP token として検証
        if !is_valid_token(media_type) {
            return Err(AcceptQueryError::InvalidMediaRange);
        }
        return Ok((media_type.to_ascii_lowercase(), "*".to_string()));
    }

    // type / subtype ともに HTTP token (tchar only) として検証
    // SF Token は `:` と `/` を許容するが、RFC 9110 Section 8.3.1 の
    // type/subtype は token (tchar only) のため、`text/html:extra` /
    // `text/html/foo` 等は InvalidMediaRange とする
    if !is_valid_token(media_type) || !is_valid_token(subtype) {
        return Err(AcceptQueryError::InvalidMediaRange);
    }

    Ok((
        media_type.to_ascii_lowercase(),
        subtype.to_ascii_lowercase(),
    ))
}

/// 文字列が SF Token として表現可能か判定する (Display 用)
///
/// SF Token の先頭は ALPHA または `*`、後続は tchar / `:` / `/`。
/// ただし Display では media range `type/subtype` 全体を 1 つの Token として
/// 出力するため、文字集合は SF Token の後続文字集合 (tchar / `:` / `/`) で判定する。
/// 先頭文字が ALPHA / `*` でない場合は String 形式が必要。
fn can_be_token(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let bytes = s.as_bytes();
    if !is_alpha_or_star(bytes[0]) {
        return false;
    }
    bytes[1..].iter().all(|&b| is_sf_token_trailing_char(b))
}
