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
//! let cd = ContentDisposition::parse("attachment; filename=\"example.txt\"").unwrap();
//! assert_eq!(cd.disposition_type(), DispositionType::Attachment);
//! assert_eq!(cd.filename(), Some("example.txt"));
//!
//! // inline
//! let cd = ContentDisposition::parse("inline").unwrap();
//! assert_eq!(cd.disposition_type(), DispositionType::Inline);
//! ```

use core::fmt;

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
    /// 不正な RFC 5987 エンコーディング
    InvalidExtValue,
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
        }
    }
}

impl std::error::Error for ContentDispositionError {}

/// Disposition タイプ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispositionType {
    /// inline: コンテンツをインラインで表示
    Inline,
    /// attachment: コンテンツをダウンロードとして扱う
    Attachment,
    /// form-data: multipart/form-data のパート用
    FormData,
}

impl DispositionType {
    fn from_str(s: &str) -> Result<Self, ContentDispositionError> {
        match s.to_ascii_lowercase().as_str() {
            "inline" => Ok(DispositionType::Inline),
            "attachment" => Ok(DispositionType::Attachment),
            "form-data" => Ok(DispositionType::FormData),
            _ => Err(ContentDispositionError::InvalidDispositionType),
        }
    }
}

impl fmt::Display for DispositionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DispositionType::Inline => write!(f, "inline"),
            DispositionType::Attachment => write!(f, "attachment"),
            DispositionType::FormData => write!(f, "form-data"),
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
    /// filename* パラメータ (RFC 5987 エンコード済み、デコード後の値)
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
    /// let cd = ContentDisposition::parse("attachment; filename=\"report.pdf\"").unwrap();
    /// assert_eq!(cd.disposition_type(), DispositionType::Attachment);
    /// assert_eq!(cd.filename(), Some("report.pdf"));
    /// ```
    pub fn parse(input: &str) -> Result<Self, ContentDispositionError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ContentDispositionError::Empty);
        }

        // 引用符を考慮してパラメータを分割
        let parts = split_params(input);

        // disposition-type
        let type_str = parts
            .first()
            .ok_or(ContentDispositionError::InvalidFormat)?;
        let disposition_type = DispositionType::from_str(type_str.trim())?;

        let mut cd = ContentDisposition {
            disposition_type,
            filename: None,
            filename_ext: None,
            name: None,
            parameters: Vec::new(),
        };

        // パラメータをパース
        for part in parts.iter().skip(1) {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            if let Some(eq_pos) = part.find('=') {
                let param_name = part[..eq_pos].trim().to_ascii_lowercase();
                let param_value = part[eq_pos + 1..].trim();

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
        self.disposition_type
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
            write!(f, "; name=\"{}\"", escape_quoted_string(name))?;
        }

        if let Some(filename) = &self.filename {
            write!(f, "; filename=\"{}\"", escape_quoted_string(filename))?;
        }

        if let Some(filename_ext) = &self.filename_ext {
            write!(f, "; filename*=UTF-8''{}", encode_ext_value(filename_ext))?;
        }

        for (name, value) in &self.parameters {
            write!(f, "; {}=\"{}\"", name, escape_quoted_string(value))?;
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
fn parse_param_value(value: &str) -> Result<String, ContentDispositionError> {
    let value = value.trim();

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
        // トークン
        Ok(value.to_string())
    }
}

/// 引用符付き文字列をパース (エスケープ処理)
fn parse_quoted_string(s: &str) -> Result<String, ContentDispositionError> {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();

    while let Some(c) = chars.next() {
        if c == '\\' {
            // エスケープシーケンス
            if let Some(escaped) = chars.next() {
                result.push(escaped);
            } else {
                return Err(ContentDispositionError::InvalidParameter);
            }
        } else {
            result.push(c);
        }
    }

    Ok(result)
}

/// RFC 5987 ext-value をパース
///
/// 形式: charset'language'value
/// 例: UTF-8''%E6%97%A5%E6%9C%AC%E8%AA%9E.txt
fn parse_ext_value(value: &str) -> Result<String, ContentDispositionError> {
    let value = value.trim();

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
    let mut bytes = Vec::with_capacity(s.len());
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
            // attr-char (RFC 5987)
            bytes.push(c as u8);
        }
    }

    String::from_utf8(bytes).map_err(|_| ContentDispositionError::InvalidExtValue)
}

/// RFC 5987 ext-value 用にエンコード
fn encode_ext_value(s: &str) -> String {
    let mut result = String::new();
    for byte in s.bytes() {
        if is_attr_char(byte) {
            result.push(byte as char);
        } else {
            result.push('%');
            result.push_str(&format!("{:02X}", byte));
        }
    }
    result
}

/// RFC 5987 attr-char
fn is_attr_char(b: u8) -> bool {
    matches!(b,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' |
        b'!' | b'#' | b'$' | b'&' | b'+' | b'-' | b'.' |
        b'^' | b'_' | b'`' | b'|' | b'~'
    )
}

/// 引用符付き文字列用にエスケープ
fn escape_quoted_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '"' || c == '\\' {
            result.push('\\');
        }
        result.push(c);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_inline() {
        let cd = ContentDisposition::parse("inline").unwrap();
        assert_eq!(cd.disposition_type(), DispositionType::Inline);
        assert!(cd.is_inline());
        assert!(!cd.is_attachment());
    }

    #[test]
    fn test_parse_attachment() {
        let cd = ContentDisposition::parse("attachment").unwrap();
        assert_eq!(cd.disposition_type(), DispositionType::Attachment);
        assert!(cd.is_attachment());
    }

    #[test]
    fn test_parse_attachment_with_filename() {
        let cd = ContentDisposition::parse("attachment; filename=\"example.txt\"").unwrap();
        assert!(cd.is_attachment());
        assert_eq!(cd.filename(), Some("example.txt"));
    }

    #[test]
    fn test_parse_filename_without_quotes() {
        let cd = ContentDisposition::parse("attachment; filename=example.txt").unwrap();
        assert_eq!(cd.filename(), Some("example.txt"));
    }

    #[test]
    fn test_parse_filename_with_escape() {
        let cd = ContentDisposition::parse(r#"attachment; filename="file\"name.txt""#).unwrap();
        assert_eq!(cd.filename(), Some("file\"name.txt"));
    }

    #[test]
    fn test_parse_filename_ext() {
        let cd = ContentDisposition::parse(
            "attachment; filename*=UTF-8''%E6%97%A5%E6%9C%AC%E8%AA%9E.txt",
        )
        .unwrap();
        assert_eq!(cd.filename(), Some("日本語.txt"));
        assert_eq!(cd.filename_ext(), Some("日本語.txt"));
    }

    #[test]
    fn test_filename_ext_priority() {
        // filename* が filename より優先される
        let cd = ContentDisposition::parse(
            "attachment; filename=\"fallback.txt\"; filename*=UTF-8''preferred.txt",
        )
        .unwrap();
        assert_eq!(cd.filename(), Some("preferred.txt"));
        assert_eq!(cd.filename_ascii(), Some("fallback.txt"));
    }

    #[test]
    fn test_parse_form_data() {
        let cd = ContentDisposition::parse("form-data; name=\"field1\"").unwrap();
        assert!(cd.is_form_data());
        assert_eq!(cd.name(), Some("field1"));
    }

    #[test]
    fn test_parse_form_data_with_filename() {
        let cd =
            ContentDisposition::parse("form-data; name=\"file\"; filename=\"image.png\"").unwrap();
        assert!(cd.is_form_data());
        assert_eq!(cd.name(), Some("file"));
        assert_eq!(cd.filename(), Some("image.png"));
    }

    #[test]
    fn test_parse_case_insensitive() {
        let cd = ContentDisposition::parse("ATTACHMENT; FILENAME=\"test.txt\"").unwrap();
        assert!(cd.is_attachment());
        assert_eq!(cd.filename(), Some("test.txt"));
    }

    #[test]
    fn test_parse_empty() {
        assert!(ContentDisposition::parse("").is_err());
    }

    #[test]
    fn test_parse_invalid_type() {
        assert!(ContentDisposition::parse("unknown").is_err());
    }

    #[test]
    fn test_display() {
        let cd = ContentDisposition::new(DispositionType::Attachment).with_filename("test.txt");
        assert_eq!(cd.to_string(), "attachment; filename=\"test.txt\"");
    }

    #[test]
    fn test_display_with_filename_ext() {
        let cd = ContentDisposition::new(DispositionType::Attachment)
            .with_filename("fallback.txt")
            .with_filename_ext("日本語.txt");
        let s = cd.to_string();
        assert!(s.contains("attachment"));
        assert!(s.contains("filename=\"fallback.txt\""));
        assert!(s.contains("filename*=UTF-8''"));
    }

    #[test]
    fn test_display_form_data() {
        let cd = ContentDisposition::new(DispositionType::FormData)
            .with_name("field")
            .with_filename("file.txt");
        let s = cd.to_string();
        assert!(s.contains("form-data"));
        assert!(s.contains("name=\"field\""));
        assert!(s.contains("filename=\"file.txt\""));
    }

    #[test]
    fn test_builder() {
        let cd = ContentDisposition::new(DispositionType::Attachment)
            .with_filename("example.txt")
            .with_filename_ext("例.txt");

        assert!(cd.is_attachment());
        assert_eq!(cd.filename_ascii(), Some("example.txt"));
        assert_eq!(cd.filename_ext(), Some("例.txt"));
        assert_eq!(cd.filename(), Some("例.txt")); // filename* 優先
    }
}
