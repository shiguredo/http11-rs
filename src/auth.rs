//! HTTP 認証 (Basic / Digest / Bearer)
//!
//! ## 概要
//!
//! Basic / Digest / Bearer 認証のエンコード/デコードを提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::auth::{BasicAuth, BearerToken, WwwAuthenticate};
//!
//! // クライアント: Authorization ヘッダーの作成 (Basic)
//! let auth = BasicAuth::new("user", "password");
//! let header = auth.to_header_value();
//! assert!(header.starts_with("Basic "));
//!
//! // サーバー: Authorization ヘッダーのパース (Basic)
//! let auth = BasicAuth::parse("Basic dXNlcjpwYXNzd29yZA==").unwrap();
//! assert_eq!(auth.username(), "user");
//! assert_eq!(auth.password(), "password");
//!
//! // サーバー: WWW-Authenticate ヘッダーの作成 (Basic)
//! let challenge = WwwAuthenticate::basic("example.com");
//! assert_eq!(challenge.to_string(), "Basic realm=\"example.com\"");
//!
//! // クライアント: Authorization ヘッダーのパース (Bearer)
//! let token = BearerToken::parse("Bearer abc.def").unwrap();
//! assert_eq!(token.token(), "abc.def");
//! ```

use core::fmt;

/// Basic 認証エラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// 空の入力
    Empty,
    /// 不正な形式
    InvalidFormat,
    /// Basic スキームでない
    NotBasicScheme,
    /// Digest スキームでない
    NotDigestScheme,
    /// Bearer スキームでない
    NotBearerScheme,
    /// Base64 デコードエラー
    Base64DecodeError,
    /// UTF-8 デコードエラー
    Utf8Error,
    /// コロンが見つからない (user:password 形式でない)
    MissingColon,
    /// 不正なパラメータ
    InvalidParameter,
    /// 必須パラメータが足りない
    MissingParameter,
    /// 不正なトークン
    InvalidToken,
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::Empty => write!(f, "empty authorization header"),
            AuthError::InvalidFormat => write!(f, "invalid authorization format"),
            AuthError::NotBasicScheme => write!(f, "not basic authentication scheme"),
            AuthError::NotDigestScheme => write!(f, "not digest authentication scheme"),
            AuthError::NotBearerScheme => write!(f, "not bearer authentication scheme"),
            AuthError::Base64DecodeError => write!(f, "base64 decode error"),
            AuthError::Utf8Error => write!(f, "utf-8 decode error"),
            AuthError::MissingColon => write!(f, "missing colon in credentials"),
            AuthError::InvalidParameter => write!(f, "invalid auth parameter"),
            AuthError::MissingParameter => write!(f, "missing required auth parameter"),
            AuthError::InvalidToken => write!(f, "invalid auth token"),
        }
    }
}

impl std::error::Error for AuthError {}

/// Basic 認証
///
/// RFC 7617 Section 2
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicAuth {
    /// ユーザー名
    username: String,
    /// パスワード
    password: String,
}

impl BasicAuth {
    /// 新しい Basic 認証を作成
    pub fn new(username: &str, password: &str) -> Self {
        BasicAuth {
            username: username.to_string(),
            password: password.to_string(),
        }
    }

    /// Authorization ヘッダー値をパース
    ///
    /// # 例
    ///
    /// ```rust
    /// use shiguredo_http11::auth::BasicAuth;
    ///
    /// let auth = BasicAuth::parse("Basic dXNlcjpwYXNzd29yZA==").unwrap();
    /// assert_eq!(auth.username(), "user");
    /// assert_eq!(auth.password(), "password");
    /// ```
    pub fn parse(input: &str) -> Result<Self, AuthError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(AuthError::Empty);
        }

        // RFC 9110 Section 11.1: 認証スキームは case-insensitive
        let credentials = strip_scheme(input, "Basic").ok_or(AuthError::NotBasicScheme)?;
        if credentials.is_empty() {
            return Err(AuthError::InvalidFormat);
        }

        // Base64 デコード
        let decoded = base64_decode(credentials)?;

        // UTF-8 としてデコード
        let decoded_str = String::from_utf8(decoded).map_err(|_| AuthError::Utf8Error)?;

        // user:password 形式をパース
        let colon_pos = decoded_str.find(':').ok_or(AuthError::MissingColon)?;
        let username = &decoded_str[..colon_pos];
        let password = &decoded_str[colon_pos + 1..];

        Ok(BasicAuth {
            username: username.to_string(),
            password: password.to_string(),
        })
    }

    /// ユーザー名を取得
    pub fn username(&self) -> &str {
        &self.username
    }

    /// パスワードを取得
    pub fn password(&self) -> &str {
        &self.password
    }

    /// Authorization ヘッダー値を生成
    ///
    /// # 例
    ///
    /// ```rust
    /// use shiguredo_http11::auth::BasicAuth;
    ///
    /// let auth = BasicAuth::new("user", "password");
    /// assert_eq!(auth.to_header_value(), "Basic dXNlcjpwYXNzd29yZA==");
    /// ```
    pub fn to_header_value(&self) -> String {
        let credentials = format!("{}:{}", self.username, self.password);
        format!("Basic {}", base64_encode(credentials.as_bytes()))
    }
}

impl fmt::Display for BasicAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_header_value())
    }
}

/// WWW-Authenticate ヘッダー (Basic 認証用)
///
/// RFC 7617 Section 2
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WwwAuthenticate {
    /// realm パラメータ
    realm: String,
    /// charset パラメータ (オプション)
    charset: Option<String>,
}

impl WwwAuthenticate {
    /// Basic 認証チャレンジを作成
    pub fn basic(realm: &str) -> Self {
        WwwAuthenticate {
            realm: realm.to_string(),
            charset: None,
        }
    }

    /// charset パラメータを設定
    pub fn with_charset(mut self, charset: &str) -> Self {
        self.charset = Some(charset.to_string());
        self
    }

    /// WWW-Authenticate ヘッダーをパース
    ///
    /// # 例
    ///
    /// ```rust
    /// use shiguredo_http11::auth::WwwAuthenticate;
    ///
    /// let auth = WwwAuthenticate::parse("Basic realm=\"example.com\"").unwrap();
    /// assert_eq!(auth.realm(), "example.com");
    /// ```
    pub fn parse(input: &str) -> Result<Self, AuthError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(AuthError::Empty);
        }

        // RFC 9110 Section 11.1: 認証スキームは case-insensitive
        let params = strip_scheme(input, "Basic").ok_or(AuthError::NotBasicScheme)?;
        if params.is_empty() {
            return Err(AuthError::InvalidFormat);
        }

        let mut realm = None;
        let mut charset = None;

        // パラメータをパース
        for param in params.split(',') {
            let param = param.trim();
            if let Some((key, value)) = param.split_once('=') {
                let key = key.trim().to_lowercase();
                let value = value.trim();

                // 引用符を除去
                let value = if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
                    &value[1..value.len() - 1]
                } else {
                    value
                };

                match key.as_str() {
                    "realm" => realm = Some(value.to_string()),
                    "charset" => charset = Some(value.to_string()),
                    _ => {} // 未知のパラメータは無視
                }
            }
        }

        let realm = realm.ok_or(AuthError::InvalidFormat)?;

        Ok(WwwAuthenticate { realm, charset })
    }

    /// realm を取得
    pub fn realm(&self) -> &str {
        &self.realm
    }

    /// charset を取得
    pub fn charset(&self) -> Option<&str> {
        self.charset.as_deref()
    }

    /// ヘッダー値を生成
    pub fn to_header_value(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for WwwAuthenticate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Basic realm=\"{}\"", self.realm)?;
        if let Some(charset) = &self.charset {
            write!(f, ", charset=\"{}\"", charset)?;
        }
        Ok(())
    }
}

/// Digest 認証 (Authorization / Proxy-Authorization)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestAuth {
    params: Vec<(String, String)>,
}

impl DigestAuth {
    /// Digest Authorization ヘッダー値をパース
    pub fn parse(input: &str) -> Result<Self, AuthError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(AuthError::Empty);
        }

        let params = strip_scheme(input, "Digest").ok_or(AuthError::NotDigestScheme)?;
        if params.is_empty() {
            return Err(AuthError::InvalidFormat);
        }

        let params = parse_auth_params(params)?;
        if !has_required_params(&params, &["username", "realm", "nonce", "uri", "response"]) {
            return Err(AuthError::MissingParameter);
        }

        Ok(DigestAuth { params })
    }

    /// パラメータを取得
    pub fn param(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.params
            .iter()
            .find(|(n, _)| n == &name)
            .map(|(_, v)| v.as_str())
    }

    /// username を取得
    pub fn username(&self) -> Option<&str> {
        self.param("username")
    }

    /// realm を取得
    pub fn realm(&self) -> Option<&str> {
        self.param("realm")
    }

    /// nonce を取得
    pub fn nonce(&self) -> Option<&str> {
        self.param("nonce")
    }

    /// uri を取得
    pub fn uri(&self) -> Option<&str> {
        self.param("uri")
    }

    /// response を取得
    pub fn response(&self) -> Option<&str> {
        self.param("response")
    }

    /// ヘッダー値を生成
    pub fn to_header_value(&self) -> String {
        format!("Digest {}", format_auth_params(&self.params))
    }
}

impl fmt::Display for DigestAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_header_value())
    }
}

/// Digest 認証チャレンジ (WWW-Authenticate / Proxy-Authenticate)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestChallenge {
    params: Vec<(String, String)>,
}

impl DigestChallenge {
    /// Digest チャレンジをパース
    pub fn parse(input: &str) -> Result<Self, AuthError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(AuthError::Empty);
        }

        let params = strip_scheme(input, "Digest").ok_or(AuthError::NotDigestScheme)?;
        if params.is_empty() {
            return Err(AuthError::InvalidFormat);
        }

        let params = parse_auth_params(params)?;
        if !has_required_params(&params, &["realm", "nonce"]) {
            return Err(AuthError::MissingParameter);
        }

        Ok(DigestChallenge { params })
    }

    /// パラメータを取得
    pub fn param(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.params
            .iter()
            .find(|(n, _)| n == &name)
            .map(|(_, v)| v.as_str())
    }

    /// realm を取得
    pub fn realm(&self) -> Option<&str> {
        self.param("realm")
    }

    /// nonce を取得
    pub fn nonce(&self) -> Option<&str> {
        self.param("nonce")
    }

    /// ヘッダー値を生成
    pub fn to_header_value(&self) -> String {
        format!("Digest {}", format_auth_params(&self.params))
    }
}

impl fmt::Display for DigestChallenge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_header_value())
    }
}

/// Bearer トークン (Authorization / Proxy-Authorization)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BearerToken {
    token: String,
}

impl BearerToken {
    /// Bearer Authorization ヘッダー値をパース
    pub fn parse(input: &str) -> Result<Self, AuthError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(AuthError::Empty);
        }

        // Bearer スキームかどうかを先に確認（大文字小文字を区別しない）
        // get() を使用して UTF-8 バイト境界を安全にチェック
        let is_bearer_scheme = input
            .get(..6)
            .is_some_and(|s| s.eq_ignore_ascii_case("Bearer"));

        if !is_bearer_scheme {
            return Err(AuthError::NotBearerScheme);
        }

        // "Bearer" のみ（トークンなし）の場合は InvalidFormat
        let token = strip_scheme(input, "Bearer").unwrap_or("");
        if token.is_empty() {
            return Err(AuthError::InvalidFormat);
        }
        if !is_token68(token) {
            return Err(AuthError::InvalidToken);
        }

        Ok(BearerToken {
            token: token.to_string(),
        })
    }

    /// トークンを取得
    pub fn token(&self) -> &str {
        &self.token
    }

    /// ヘッダー値を生成
    pub fn to_header_value(&self) -> String {
        format!("Bearer {}", self.token)
    }
}

impl fmt::Display for BearerToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_header_value())
    }
}

/// Bearer 認証チャレンジ (WWW-Authenticate / Proxy-Authenticate)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BearerChallenge {
    params: Vec<(String, String)>,
}

impl BearerChallenge {
    /// Bearer チャレンジをパース
    pub fn parse(input: &str) -> Result<Self, AuthError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(AuthError::Empty);
        }

        let params = strip_scheme(input, "Bearer").ok_or(AuthError::NotBearerScheme)?;
        if params.is_empty() {
            return Err(AuthError::InvalidFormat);
        }

        let params = parse_auth_params(params)?;
        Ok(BearerChallenge { params })
    }

    /// パラメータを取得
    pub fn param(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.params
            .iter()
            .find(|(n, _)| n == &name)
            .map(|(_, v)| v.as_str())
    }

    /// ヘッダー値を生成
    pub fn to_header_value(&self) -> String {
        format!("Bearer {}", format_auth_params(&self.params))
    }
}

impl fmt::Display for BearerChallenge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_header_value())
    }
}

/// Authorization ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Authorization {
    Basic(BasicAuth),
    Digest(DigestAuth),
    Bearer(BearerToken),
}

impl Authorization {
    /// Authorization ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, AuthError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(AuthError::Empty);
        }

        // RFC 9110 Section 11.1: 認証スキームは case-insensitive
        if strip_scheme(input, "Basic").is_some() {
            return Ok(Authorization::Basic(BasicAuth::parse(input)?));
        }
        if strip_scheme(input, "Digest").is_some() {
            return Ok(Authorization::Digest(DigestAuth::parse(input)?));
        }
        if strip_scheme(input, "Bearer").is_some() {
            return Ok(Authorization::Bearer(BearerToken::parse(input)?));
        }

        Err(AuthError::InvalidFormat)
    }

    /// ヘッダー値を生成
    pub fn to_header_value(&self) -> String {
        match self {
            Authorization::Basic(auth) => auth.to_header_value(),
            Authorization::Digest(auth) => auth.to_header_value(),
            Authorization::Bearer(auth) => auth.to_header_value(),
        }
    }
}

impl fmt::Display for Authorization {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_header_value())
    }
}

/// WWW-Authenticate / Proxy-Authenticate 用チャレンジ
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthChallenge {
    Basic(WwwAuthenticate),
    Digest(DigestChallenge),
    Bearer(BearerChallenge),
}

impl AuthChallenge {
    /// チャレンジをパース
    pub fn parse(input: &str) -> Result<Self, AuthError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(AuthError::Empty);
        }

        // RFC 9110 Section 11.1: 認証スキームは case-insensitive
        if strip_scheme(input, "Basic").is_some() {
            return Ok(AuthChallenge::Basic(WwwAuthenticate::parse(input)?));
        }
        if strip_scheme(input, "Digest").is_some() {
            return Ok(AuthChallenge::Digest(DigestChallenge::parse(input)?));
        }
        if strip_scheme(input, "Bearer").is_some() {
            return Ok(AuthChallenge::Bearer(BearerChallenge::parse(input)?));
        }

        Err(AuthError::InvalidFormat)
    }

    /// ヘッダー値を生成
    pub fn to_header_value(&self) -> String {
        match self {
            AuthChallenge::Basic(auth) => auth.to_header_value(),
            AuthChallenge::Digest(auth) => auth.to_header_value(),
            AuthChallenge::Bearer(auth) => auth.to_header_value(),
        }
    }
}

impl fmt::Display for AuthChallenge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_header_value())
    }
}

/// Proxy-Authorization ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyAuthorization(Authorization);

impl ProxyAuthorization {
    /// Proxy-Authorization ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, AuthError> {
        Authorization::parse(input).map(ProxyAuthorization)
    }

    /// 内部の Authorization を取得
    pub fn authorization(&self) -> &Authorization {
        &self.0
    }

    /// ヘッダー値を生成
    pub fn to_header_value(&self) -> String {
        self.0.to_header_value()
    }
}

impl fmt::Display for ProxyAuthorization {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_header_value())
    }
}

/// Proxy-Authenticate ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyAuthenticate(AuthChallenge);

impl ProxyAuthenticate {
    /// Proxy-Authenticate ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, AuthError> {
        AuthChallenge::parse(input).map(ProxyAuthenticate)
    }

    /// 内部のチャレンジを取得
    pub fn challenge(&self) -> &AuthChallenge {
        &self.0
    }

    /// ヘッダー値を生成
    pub fn to_header_value(&self) -> String {
        self.0.to_header_value()
    }
}

impl fmt::Display for ProxyAuthenticate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_header_value())
    }
}

fn strip_scheme<'a>(input: &'a str, scheme: &str) -> Option<&'a str> {
    let input = input.trim_start();
    let scheme_len = scheme.len();
    if input.len() <= scheme_len {
        return None;
    }
    let prefix = input.get(..scheme_len)?;
    if !prefix.eq_ignore_ascii_case(scheme) {
        return None;
    }
    let rest = input.get(scheme_len..)?;
    if rest.is_empty() {
        return None;
    }
    if !rest.starts_with(' ') && !rest.starts_with('\t') {
        return None;
    }
    Some(rest.trim_start())
}

fn parse_auth_params(input: &str) -> Result<Vec<(String, String)>, AuthError> {
    let mut params = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        while i < bytes.len() && is_ows(bytes[i]) {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b',' {
            i += 1;
            continue;
        }
        while i < bytes.len() && is_ows(bytes[i]) {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        let name_start = i;
        while i < bytes.len() && is_token_char(bytes[i]) {
            i += 1;
        }
        if i == name_start {
            return Err(AuthError::InvalidParameter);
        }
        let name = &input[name_start..i];

        while i < bytes.len() && is_ows(bytes[i]) {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            return Err(AuthError::InvalidParameter);
        }
        i += 1;
        while i < bytes.len() && is_ows(bytes[i]) {
            i += 1;
        }
        if i >= bytes.len() {
            return Err(AuthError::InvalidParameter);
        }

        let value = if bytes[i] == b'"' {
            i += 1;
            let mut value = String::new();
            let mut escaped = false;
            let mut closed = false;
            while i < bytes.len() {
                let b = bytes[i];
                if escaped {
                    value.push(b as char);
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b'"' {
                    i += 1;
                    closed = true;
                    break;
                } else {
                    value.push(b as char);
                }
                i += 1;
            }
            if escaped || !closed {
                return Err(AuthError::InvalidParameter);
            }
            value
        } else {
            let value_start = i;
            while i < bytes.len() && !is_ows(bytes[i]) && bytes[i] != b',' {
                i += 1;
            }
            let token = &input[value_start..i];
            if token.is_empty() || !is_valid_token(token) {
                return Err(AuthError::InvalidParameter);
            }
            token.to_string()
        };

        params.push((name.to_ascii_lowercase(), value));
        while i < bytes.len() && is_ows(bytes[i]) {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b',' {
            i += 1;
        }
    }

    if params.is_empty() {
        return Err(AuthError::InvalidFormat);
    }

    Ok(params)
}

fn has_required_params(params: &[(String, String)], required: &[&str]) -> bool {
    required.iter().all(|name| {
        let name = name.to_ascii_lowercase();
        params.iter().any(|(n, _)| n == &name)
    })
}

fn format_auth_params(params: &[(String, String)]) -> String {
    let mut parts = Vec::new();
    for (name, value) in params {
        if needs_quoting(value) {
            parts.push(format!("{}=\"{}\"", name, escape_quotes(value)));
        } else {
            parts.push(format!("{}={}", name, value));
        }
    }
    parts.join(", ")
}

fn needs_quoting(value: &str) -> bool {
    value.is_empty() || value.bytes().any(|b| !is_token_char(b))
}

fn escape_quotes(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn is_valid_token(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(is_token_char)
}

fn is_token_char(b: u8) -> bool {
    matches!(
        b,
        b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.' |
        b'0'..=b'9' | b'A'..=b'Z' | b'^' | b'_' | b'`' | b'a'..=b'z' | b'|' | b'~'
    )
}

fn is_token68(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(is_token68_char)
}

fn is_token68_char(b: u8) -> bool {
    matches!(
        b,
        b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'.'
            | b'_'
            | b'~'
            | b'+'
            | b'/'
            | b'='
    )
}

fn is_ows(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

// Base64 エンコード/デコード (依存なし実装)

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 エンコード
fn base64_encode(input: &[u8]) -> String {
    let mut result = String::new();
    let mut i = 0;

    while i < input.len() {
        let b0 = input[i];
        let b1 = if i + 1 < input.len() { input[i + 1] } else { 0 };
        let b2 = if i + 2 < input.len() { input[i + 2] } else { 0 };

        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);

        result.push(BASE64_ALPHABET[(n >> 18 & 0x3F) as usize] as char);
        result.push(BASE64_ALPHABET[(n >> 12 & 0x3F) as usize] as char);

        if i + 1 < input.len() {
            result.push(BASE64_ALPHABET[(n >> 6 & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        if i + 2 < input.len() {
            result.push(BASE64_ALPHABET[(n & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        i += 3;
    }

    result
}

/// Base64 デコード
fn base64_decode(input: &str) -> Result<Vec<u8>, AuthError> {
    let input = input.trim_end_matches('=');
    let mut result = Vec::new();

    let mut buf: u32 = 0;
    let mut bits: u32 = 0;

    for c in input.chars() {
        let val = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' => 62,
            '/' => 63,
            ' ' | '\t' | '\n' | '\r' => continue, // 空白は無視
            _ => return Err(AuthError::Base64DecodeError),
        };

        buf = (buf << 6) | val;
        bits += 6;

        if bits >= 8 {
            bits -= 8;
            result.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(b"user:password"), "dXNlcjpwYXNzd29yZA==");
    }

    #[test]
    fn test_base64_decode() {
        assert_eq!(base64_decode("").unwrap(), b"");
        assert_eq!(base64_decode("Zg==").unwrap(), b"f");
        assert_eq!(base64_decode("Zm8=").unwrap(), b"fo");
        assert_eq!(base64_decode("Zm9v").unwrap(), b"foo");
        assert_eq!(base64_decode("Zm9vYg==").unwrap(), b"foob");
        assert_eq!(base64_decode("Zm9vYmE=").unwrap(), b"fooba");
        assert_eq!(base64_decode("Zm9vYmFy").unwrap(), b"foobar");
        assert_eq!(
            base64_decode("dXNlcjpwYXNzd29yZA==").unwrap(),
            b"user:password"
        );
    }

    #[test]
    fn test_basic_auth_new() {
        let auth = BasicAuth::new("user", "pass");
        assert_eq!(auth.username(), "user");
        assert_eq!(auth.password(), "pass");
    }

    #[test]
    fn test_basic_auth_parse() {
        let auth = BasicAuth::parse("Basic dXNlcjpwYXNzd29yZA==").unwrap();
        assert_eq!(auth.username(), "user");
        assert_eq!(auth.password(), "password");
    }

    #[test]
    fn test_basic_auth_parse_lowercase() {
        let auth = BasicAuth::parse("basic dXNlcjpwYXNzd29yZA==").unwrap();
        assert_eq!(auth.username(), "user");
        assert_eq!(auth.password(), "password");
    }

    #[test]
    fn test_basic_auth_parse_empty_password() {
        let auth = BasicAuth::parse("Basic dXNlcjo=").unwrap();
        assert_eq!(auth.username(), "user");
        assert_eq!(auth.password(), "");
    }

    #[test]
    fn test_basic_auth_parse_colon_in_password() {
        // user:pass:word -> dXNlcjpwYXNzOndvcmQ=
        let auth = BasicAuth::parse("Basic dXNlcjpwYXNzOndvcmQ=").unwrap();
        assert_eq!(auth.username(), "user");
        assert_eq!(auth.password(), "pass:word");
    }

    #[test]
    fn test_basic_auth_parse_empty() {
        assert!(BasicAuth::parse("").is_err());
    }

    #[test]
    fn test_basic_auth_parse_not_basic() {
        assert!(BasicAuth::parse("Bearer token").is_err());
        assert!(BasicAuth::parse("Digest abc").is_err());
    }

    #[test]
    fn test_basic_auth_to_header_value() {
        let auth = BasicAuth::new("user", "password");
        assert_eq!(auth.to_header_value(), "Basic dXNlcjpwYXNzd29yZA==");
    }

    #[test]
    fn test_basic_auth_roundtrip() {
        let original = BasicAuth::new("testuser", "testpass123");
        let header = original.to_header_value();
        let parsed = BasicAuth::parse(&header).unwrap();
        assert_eq!(original.username(), parsed.username());
        assert_eq!(original.password(), parsed.password());
    }

    #[test]
    fn test_www_authenticate_basic() {
        let auth = WwwAuthenticate::basic("example.com");
        assert_eq!(auth.realm(), "example.com");
        assert_eq!(auth.charset(), None);
    }

    #[test]
    fn test_www_authenticate_with_charset() {
        let auth = WwwAuthenticate::basic("example.com").with_charset("UTF-8");
        assert_eq!(auth.realm(), "example.com");
        assert_eq!(auth.charset(), Some("UTF-8"));
    }

    #[test]
    fn test_www_authenticate_parse() {
        let auth = WwwAuthenticate::parse("Basic realm=\"example.com\"").unwrap();
        assert_eq!(auth.realm(), "example.com");
        assert_eq!(auth.charset(), None);
    }

    #[test]
    fn test_www_authenticate_parse_with_charset() {
        let auth =
            WwwAuthenticate::parse("Basic realm=\"example.com\", charset=\"UTF-8\"").unwrap();
        assert_eq!(auth.realm(), "example.com");
        assert_eq!(auth.charset(), Some("UTF-8"));
    }

    #[test]
    fn test_www_authenticate_parse_lowercase() {
        let auth = WwwAuthenticate::parse("basic realm=\"test\"").unwrap();
        assert_eq!(auth.realm(), "test");
    }

    #[test]
    fn test_www_authenticate_parse_empty() {
        assert!(WwwAuthenticate::parse("").is_err());
    }

    #[test]
    fn test_www_authenticate_parse_not_basic() {
        assert!(WwwAuthenticate::parse("Digest realm=\"test\"").is_err());
    }

    #[test]
    fn test_www_authenticate_display() {
        let auth = WwwAuthenticate::basic("example.com");
        assert_eq!(auth.to_string(), "Basic realm=\"example.com\"");

        let auth = WwwAuthenticate::basic("example.com").with_charset("UTF-8");
        assert_eq!(
            auth.to_string(),
            "Basic realm=\"example.com\", charset=\"UTF-8\""
        );
    }

    #[test]
    fn test_www_authenticate_roundtrip() {
        let original = WwwAuthenticate::basic("test.example.com");
        let header = original.to_string();
        let parsed = WwwAuthenticate::parse(&header).unwrap();
        assert_eq!(original.realm(), parsed.realm());
    }

    #[test]
    fn test_digest_auth_parse() {
        let header = "Digest username=\"Mufasa\", realm=\"test\", nonce=\"abc\", uri=\"/\", response=\"xyz\"";
        let auth = DigestAuth::parse(header).unwrap();
        assert_eq!(auth.username(), Some("Mufasa"));
        assert_eq!(auth.realm(), Some("test"));
        assert_eq!(auth.nonce(), Some("abc"));
        assert_eq!(auth.uri(), Some("/"));
        assert_eq!(auth.response(), Some("xyz"));
    }

    #[test]
    fn test_digest_auth_missing_param() {
        let header = "Digest username=\"Mufasa\", realm=\"test\"";
        assert!(DigestAuth::parse(header).is_err());
    }

    #[test]
    fn test_digest_challenge_parse() {
        let header = "Digest realm=\"test\", nonce=\"abc\"";
        let challenge = DigestChallenge::parse(header).unwrap();
        assert_eq!(challenge.realm(), Some("test"));
        assert_eq!(challenge.nonce(), Some("abc"));
    }

    #[test]
    fn test_bearer_token_parse() {
        let token = BearerToken::parse("Bearer abc.def.ghi").unwrap();
        assert_eq!(token.token(), "abc.def.ghi");
        assert_eq!(token.to_header_value(), "Bearer abc.def.ghi");
    }

    #[test]
    fn test_bearer_token_parse_non_ascii() {
        // マルチバイト UTF-8 文字を含む入力でパニックしないことを確認
        // Fuzzing で発見されたクラッシュケース: バイト [228, 167, 167, 10, 228, 167, 167]
        let input = "䧧\n䧧";
        let result = BearerToken::parse(input);
        assert!(result.is_err());

        // 6 バイト未満のマルチバイト文字
        let input2 = "日本語";
        let result2 = BearerToken::parse(input2);
        assert!(result2.is_err());

        // 6 バイト以上だがバイト境界が文字境界でない場合
        let input3 = "あいう"; // 9 バイト
        let result3 = BearerToken::parse(input3);
        assert!(result3.is_err());
    }

    #[test]
    fn test_bearer_challenge_parse() {
        let header = "Bearer realm=\"example\", error=\"invalid_token\"";
        let challenge = BearerChallenge::parse(header).unwrap();
        assert_eq!(challenge.param("realm"), Some("example"));
        assert_eq!(challenge.param("error"), Some("invalid_token"));
    }

    #[test]
    fn test_authorization_parse() {
        let auth = Authorization::parse("Bearer token123").unwrap();
        assert!(matches!(auth, Authorization::Bearer(_)));
    }

    #[test]
    fn test_proxy_headers_parse() {
        let auth = ProxyAuthorization::parse("Basic dXNlcjpwYXNzd29yZA==").unwrap();
        assert!(matches!(auth.authorization(), Authorization::Basic(_)));

        let challenge = ProxyAuthenticate::parse("Basic realm=\"proxy\"").unwrap();
        assert!(matches!(challenge.challenge(), AuthChallenge::Basic(_)));
    }

    #[test]
    fn test_digest_auth_non_ascii_input() {
        let input = ")ϓ )ϓ";
        assert!(DigestAuth::parse(input).is_err());
    }

    #[test]
    fn test_basic_auth_case_insensitive() {
        // RFC 9110 Section 11.1: 認証スキームは case-insensitive
        let auth1 = BasicAuth::parse("BASIC dXNlcjpwYXNzd29yZA==").unwrap();
        let auth2 = BasicAuth::parse("BaSiC dXNlcjpwYXNzd29yZA==").unwrap();
        assert_eq!(auth1.username(), "user");
        assert_eq!(auth2.username(), "user");
    }

    #[test]
    fn test_www_authenticate_case_insensitive() {
        // RFC 9110 Section 11.1: 認証スキームは case-insensitive
        let auth = WwwAuthenticate::parse("BASIC realm=\"test\"").unwrap();
        assert_eq!(auth.realm(), "test");
    }

    #[test]
    fn test_authorization_case_insensitive() {
        // RFC 9110 Section 11.1: 認証スキームは case-insensitive
        let auth = Authorization::parse("BEARER token123").unwrap();
        assert!(matches!(auth, Authorization::Bearer(_)));
    }
}
