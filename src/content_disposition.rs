//! Content-Disposition ヘッダーパース (RFC 6266)
//!
//! ## 概要
//!
//! RFC 6266 に基づいた Content-Disposition ヘッダーのパースを提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::content_disposition::{ContentDisposition, DispositionType};
//!
//! // attachment with filename
//! let cd = ContentDisposition::parse("attachment; filename=\"example.txt\"").expect("Content-Disposition のパースは成功するはず (実装バグ)");
//! assert_eq!(cd.disposition_type(), DispositionType::Attachment);
//! assert_eq!(cd.filename(), Some("example.txt"));
//!
//! // inline
//! let cd = ContentDisposition::parse("inline").expect("Content-Disposition のパースは成功するはず (実装バグ)");
//! assert_eq!(cd.disposition_type(), DispositionType::Inline);
//! ```

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::validate::{
    escape_quotes, is_qdtext_char, is_quoted_pair_char, is_valid_token, trim_ows,
};

/// Content-Disposition パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentDispositionError {
    /// 空の入力
    Empty,
    /// 不正な形式
    InvalidFormat,
    /// 不正な disposition-type
    InvalidDispositionType,
    /// 不正なパラメータ
    InvalidParameter,
    /// 不正な RFC 8187 エンコーディング
    InvalidExtValue,
    /// 重複パラメータ (RFC 6266)
    DuplicateParameter(String),
    /// パラメータ数が `MAX_PARAMS` を超えた
    ///
    /// 実用パラメータ数 (RFC 6266 程度) に十分な余裕として 32 を上限とし、
    /// 線形重複検出の CPU 消費を有限に抑える。
    TooManyParameters,
}

impl fmt::Display for ContentDispositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentDispositionError::Empty => write!(f, "empty content-disposition"),
            ContentDispositionError::InvalidFormat => {
                write!(f, "invalid content-disposition format")
            }
            ContentDispositionError::InvalidDispositionType => {
                write!(f, "invalid disposition-type")
            }
            ContentDispositionError::InvalidParameter => write!(f, "invalid parameter"),
            ContentDispositionError::InvalidExtValue => write!(f, "invalid ext-value encoding"),
            ContentDispositionError::DuplicateParameter(name) => {
                write!(f, "duplicate parameter: {}", name)
            }
            ContentDispositionError::TooManyParameters => {
                write!(f, "too many content-disposition parameters")
            }
        }
    }
}

impl core::error::Error for ContentDispositionError {}

/// Content-Disposition のパラメータ数上限
///
/// 実用パラメータ数 (RFC 6266 = 7 程度) に十分な余裕として 32 を上限とする。
/// 重複検出の `Vec` + `iter().any` 線形検索による CPU 消費を有限に抑えるための hard cap。
/// 将来、RFC 拡張で 32 を超えるパラメータが必要になれば再評価する。
const MAX_PARAMS: usize = 32;

/// Disposition タイプ
///
/// RFC 6266 Section 4.1: disposition-type は拡張可能 (拡張トークンを受け入れる)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispositionType {
    /// inline: コンテンツをインラインで表示
    Inline,
    /// attachment: コンテンツをダウンロードとして扱う
    Attachment,
    /// form-data: multipart/form-data のパート用
    FormData,
    /// 拡張 disposition-type (RFC 6266 準拠)
    Unknown(String),
}

impl DispositionType {
    /// disposition-type をパース
    ///
    /// RFC 6266 Section 4.1: 標準タイプ (inline, attachment, form-data) に加えて、
    /// 有効なトークンであれば拡張タイプとして受け入れる
    fn from_str(s: &str) -> Result<Self, ContentDispositionError> {
        let lower = s.to_ascii_lowercase();
        match lower.as_str() {
            "inline" => Ok(DispositionType::Inline),
            "attachment" => Ok(DispositionType::Attachment),
            "form-data" => Ok(DispositionType::FormData),
            _ => {
                // 拡張 disposition-type: 有効なトークンであれば受け入れる
                if is_valid_token(s) {
                    Ok(DispositionType::Unknown(lower))
                } else {
                    Err(ContentDispositionError::InvalidDispositionType)
                }
            }
        }
    }
}

impl fmt::Display for DispositionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DispositionType::Inline => write!(f, "inline"),
            DispositionType::Attachment => write!(f, "attachment"),
            DispositionType::FormData => write!(f, "form-data"),
            DispositionType::Unknown(s) => write!(f, "{}", s),
        }
    }
}

/// Content-Disposition ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentDisposition {
    /// disposition-type
    disposition_type: DispositionType,
    /// filename パラメータ (ASCII)
    filename: Option<String>,
    /// filename* パラメータ (RFC 8187 エンコード済み、デコード後の値)
    filename_ext: Option<String>,
    /// name パラメータ (form-data 用)
    name: Option<String>,
    /// その他のパラメータ
    parameters: Vec<(String, String)>,
}

impl ContentDisposition {
    /// Content-Disposition ヘッダー文字列をパース
    ///
    /// # 例
    ///
    /// ```rust
    /// use shiguredo_http11::content_disposition::{ContentDisposition, DispositionType};
    ///
    /// let cd = ContentDisposition::parse("attachment; filename=\"report.pdf\"").expect("Content-Disposition のパースは成功するはず (実装バグ)");
    /// assert_eq!(cd.disposition_type(), DispositionType::Attachment);
    /// assert_eq!(cd.filename(), Some("report.pdf"));
    /// ```
    pub fn parse(input: &str) -> Result<Self, ContentDispositionError> {
        let input = trim_ows(input);
        if input.is_empty() {
            return Err(ContentDispositionError::Empty);
        }

        // 引用符を考慮してパラメータを分割
        let parts = split_params(input);

        // disposition-type
        let type_str = parts
            .first()
            .ok_or(ContentDispositionError::InvalidFormat)?;
        let disposition_type = DispositionType::from_str(trim_ows(type_str))?;

        let mut cd = ContentDisposition {
            disposition_type,
            filename: None,
            filename_ext: None,
            name: None,
            parameters: Vec::new(),
        };

        // パラメータをパース
        // RFC 6266: 同名パラメータの複数出現は無効
        let mut seen_params = Vec::new();

        for part in parts.iter().skip(1) {
            let part = trim_ows(part);
            if part.is_empty() {
                continue;
            }

            if let Some(eq_pos) = part.find('=') {
                let param_name = trim_ows(&part[..eq_pos]).to_ascii_lowercase();
                let param_value = trim_ows(&part[eq_pos + 1..]);

                // 重複パラメータチェック
                if seen_params.iter().any(|n: &String| n == &param_name) {
                    return Err(ContentDispositionError::DuplicateParameter(param_name));
                }
                // パラメータ数 hard cap (`MAX_PARAMS = 32`)。
                // 線形重複検出の CPU 消費を有限に抑える。
                if seen_params.len() >= MAX_PARAMS {
                    return Err(ContentDispositionError::TooManyParameters);
                }
                seen_params.push(param_name.clone());

                match param_name.as_str() {
                    "filename" => {
                        cd.filename = Some(parse_param_value(param_value)?);
                    }
                    "filename*" => {
                        cd.filename_ext = Some(parse_ext_value(param_value)?);
                    }
                    "name" => {
                        cd.name = Some(parse_param_value(param_value)?);
                    }
                    _ => {
                        cd.parameters
                            .push((param_name, parse_param_value(param_value)?));
                    }
                }
            }
        }

        Ok(cd)
    }

    /// 新しい ContentDisposition を作成
    pub fn new(disposition_type: DispositionType) -> Self {
        ContentDisposition {
            disposition_type,
            filename: None,
            filename_ext: None,
            name: None,
            parameters: Vec::new(),
        }
    }

    /// disposition-type を取得
    pub fn disposition_type(&self) -> DispositionType {
        self.disposition_type.clone()
    }

    /// filename を取得 (filename* があればそちらを優先)
    ///
    /// RFC 6266 Section 4.3 に従い、filename* が存在する場合はそちらを優先します。
    pub fn filename(&self) -> Option<&str> {
        self.filename_ext.as_deref().or(self.filename.as_deref())
    }

    /// filename パラメータを取得 (ASCII のみ)
    pub fn filename_ascii(&self) -> Option<&str> {
        self.filename.as_deref()
    }

    /// filename* パラメータを取得 (デコード済み)
    pub fn filename_ext(&self) -> Option<&str> {
        self.filename_ext.as_deref()
    }

    /// name パラメータを取得 (form-data 用)
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// パラメータを取得
    pub fn parameter(&self, name: &str) -> Option<&str> {
        let name_lower = name.to_ascii_lowercase();
        for (k, v) in &self.parameters {
            if k == &name_lower {
                return Some(v);
            }
        }
        None
    }

    /// inline かどうか
    pub fn is_inline(&self) -> bool {
        self.disposition_type == DispositionType::Inline
    }

    /// attachment かどうか
    pub fn is_attachment(&self) -> bool {
        self.disposition_type == DispositionType::Attachment
    }

    /// form-data かどうか
    pub fn is_form_data(&self) -> bool {
        self.disposition_type == DispositionType::FormData
    }

    /// filename を設定
    pub fn with_filename(mut self, filename: &str) -> Self {
        self.filename = Some(filename.to_string());
        self
    }

    /// filename* を設定 (UTF-8 でエンコード)
    pub fn with_filename_ext(mut self, filename: &str) -> Self {
        self.filename_ext = Some(filename.to_string());
        self
    }

    /// name を設定 (form-data 用)
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }
}

impl fmt::Display for ContentDisposition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.disposition_type)?;

        if let Some(name) = &self.name {
            write!(f, "; name=\"{}\"", escape_quotes(name))?;
        }

        if let Some(filename) = &self.filename {
            write!(f, "; filename=\"{}\"", escape_quotes(filename))?;
        }

        if let Some(filename_ext) = &self.filename_ext {
            write!(f, "; filename*=UTF-8''{}", encode_ext_value(filename_ext))?;
        }

        for (name, value) in &self.parameters {
            write!(f, "; {}=\"{}\"", name, escape_quotes(value))?;
        }

        Ok(())
    }
}

/// 引用符を考慮してセミコロンで分割
fn split_params(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escape_next = false;

    for c in input.chars() {
        if escape_next {
            current.push(c);
            escape_next = false;
            continue;
        }

        match c {
            '\\' if in_quotes => {
                current.push(c);
                escape_next = true;
            }
            '"' => {
                current.push(c);
                in_quotes = !in_quotes;
            }
            ';' if !in_quotes => {
                parts.push(current);
                current = String::new();
            }
            _ => {
                current.push(c);
            }
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

/// パラメータ値をパース (引用符付きまたはトークン)
///
/// RFC 9110 Section 5.6.6: パラメータ値がトークンの場合、
/// トークン文字 (tchar) のみで構成されている必要がある
fn parse_param_value(value: &str) -> Result<String, ContentDispositionError> {
    let value = trim_ows(value);

    if value.starts_with('"') {
        // 引用符で始まる場合
        if value.ends_with('"') && value.len() >= 2 {
            // 正常な引用符付き文字列
            parse_quoted_string(&value[1..value.len() - 1])
        } else {
            // 閉じ引用符がない
            Err(ContentDispositionError::InvalidParameter)
        }
    } else {
        // トークン: RFC 9110 Section 5.6.2 準拠の検証
        if !is_valid_token(value) {
            return Err(ContentDispositionError::InvalidParameter);
        }
        Ok(value.to_string())
    }
}

/// 引用符付き文字列をパース (RFC 9110 Section 5.6.4 qdtext / quoted-pair)
///
/// 入力は両端の DQUOTE を除いた中身 (= `qdtext / quoted-pair` の連結)。
/// CTL (CR / LF / NUL / 他) は qdtext / quoted-pair のどちらの右辺としても許容しない。
/// 受信側でも CR/LF を含む quoted-string を素通りさせると、上位アプリでの再エンコード経路で
/// response splitting / log injection に至る経路を生むため厳格に reject する。
fn parse_quoted_string(s: &str) -> Result<String, ContentDispositionError> {
    let mut result = String::with_capacity(s.len());
    let mut iter = s.chars();

    while let Some(c) = iter.next() {
        if c == '\\' {
            // quoted-pair: 次の char が HTAB / SP / VCHAR / obs-text (Unicode scalar 拡張) であること
            let next = iter
                .next()
                .ok_or(ContentDispositionError::InvalidParameter)?;
            if !is_quoted_pair_char(next) {
                return Err(ContentDispositionError::InvalidParameter);
            }
            result.push(next);
        } else {
            // qdtext: HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text (Unicode scalar 拡張)
            if !is_qdtext_char(c) {
                return Err(ContentDispositionError::InvalidParameter);
            }
            result.push(c);
        }
    }

    Ok(result)
}

/// RFC 8187 ext-value をパース
///
/// 形式: charset'language'value
/// 例: UTF-8''%E6%97%A5%E6%9C%AC%E8%AA%9E.txt
fn parse_ext_value(value: &str) -> Result<String, ContentDispositionError> {
    let value = trim_ows(value);

    // charset'language'value の形式
    let first_quote = value
        .find('\'')
        .ok_or(ContentDispositionError::InvalidExtValue)?;
    let charset = &value[..first_quote];

    let rest = &value[first_quote + 1..];
    let second_quote = rest
        .find('\'')
        .ok_or(ContentDispositionError::InvalidExtValue)?;
    // language は無視 (オプション)
    let encoded_value = &rest[second_quote + 1..];

    // charset は UTF-8 のみサポート (RFC 6266 推奨)
    if !charset.eq_ignore_ascii_case("UTF-8") {
        return Err(ContentDispositionError::InvalidExtValue);
    }

    // パーセントデコード
    percent_decode(encoded_value)
}

/// パーセントデコード
fn percent_decode(s: &str) -> Result<String, ContentDispositionError> {
    let mut bytes = Vec::new();
    let mut chars = s.chars();

    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() != 2 {
                return Err(ContentDispositionError::InvalidExtValue);
            }
            let byte = u8::from_str_radix(&hex, 16)
                .map_err(|_| ContentDispositionError::InvalidExtValue)?;
            bytes.push(byte);
        } else {
            // RFC 8187 Section 3.2: パーセントエンコード以外は attr-char のみ許可
            if !c.is_ascii() || !is_attr_char(c as u8) {
                return Err(ContentDispositionError::InvalidExtValue);
            }
            bytes.push(c as u8);
        }
    }

    String::from_utf8(bytes).map_err(|_| ContentDispositionError::InvalidExtValue)
}

/// RFC 8187 ext-value 用にエンコード
fn encode_ext_value(s: &str) -> String {
    let mut result = String::new();
    for byte in s.bytes() {
        if is_attr_char(byte) {
            result.push(byte as char);
        } else {
            result.push('%');
            result.push_str(&alloc::format!("{:02X}", byte));
        }
    }
    result
}

/// RFC 8187 attr-char
fn is_attr_char(b: u8) -> bool {
    matches!(b,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' |
        b'!' | b'#' | b'$' | b'&' | b'+' | b'-' | b'.' |
        b'^' | b'_' | b'`' | b'|' | b'~'
    )
}
