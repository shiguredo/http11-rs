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
//! let auth = BasicAuth::new("user", "password").unwrap();
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

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::base64;
use crate::validate::{is_qdtext_char, is_quoted_pair_char};

/// Basic 認証エラー
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
    /// パラメータ名が重複している (RFC 9110 Section 11.2)
    DuplicateParameter,
    /// user-id にコロンが含まれている (RFC 7617 Section 2)
    ColonInUserId,
    /// 制御文字が含まれている (RFC 7617 Section 2)
    ControlCharacter,
    /// charset が UTF-8 でない (RFC 7617 Section 2.1)
    InvalidCharset,
    /// `username` と `username*` が同時に送信されている (RFC 7616 Section 3.4)
    ///
    /// RFC 7616 では `username` (ASCII) と `username*` (RFC 8187 ext-value、UTF-8) は
    /// XOR で、両方同時の送信は MUST NOT。
    ConflictingUsernameField,
    /// `username*` の ext-value が不正 (RFC 8187 Section 3.2.1 / RFC 7616 Section 3.4)
    InvalidUsernameExtValue,
    /// auth-param が `MAX_AUTH_PARAMS` を超えた (RFC 9110 Section 11.2 auth-param リスト上限)
    ///
    /// 実用パラメータ数 (RFC 7616 Digest = 12 / RFC 6750 Bearer = 5) に十分な余裕として
    /// 32 を上限とし、線形重複検出の CPU 消費を有限に抑える。
    TooManyParameters,
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
            AuthError::DuplicateParameter => write!(f, "duplicate auth parameter"),
            AuthError::ColonInUserId => write!(f, "colon in user-id"),
            AuthError::ControlCharacter => write!(f, "control character in credentials"),
            AuthError::InvalidCharset => write!(f, "charset must be UTF-8"),
            AuthError::ConflictingUsernameField => {
                write!(
                    f,
                    "both username and username* present (RFC 7616 Section 3.4)"
                )
            }
            AuthError::InvalidUsernameExtValue => write!(f, "invalid username* ext-value"),
            AuthError::TooManyParameters => write!(f, "too many auth parameters"),
        }
    }
}

/// auth-param リストの上限 (issue 0047)
///
/// 実用パラメータ数 (RFC 7616 Digest = 12 / RFC 6750 Bearer = 5) に十分な余裕として
/// 32 を上限とする。重複検出の `Vec` + `iter().any` 線形検索による CPU 消費を有限に
/// 抑えるための hard cap。
/// 将来、認証スキームの拡張で 32 を超えるパラメータが必要になれば再評価する。
const MAX_AUTH_PARAMS: usize = 32;

impl core::error::Error for AuthError {}

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
    ///
    /// RFC 7617 Section 2: user-id にコロンを含めてはならない。
    /// user-id と password に制御文字 (CTL) を含めてはならない。
    pub fn new(username: &str, password: &str) -> Result<Self, AuthError> {
        if username.contains(':') {
            return Err(AuthError::ColonInUserId);
        }
        if has_control_chars(username) || has_control_chars(password) {
            return Err(AuthError::ControlCharacter);
        }
        Ok(BasicAuth {
            username: username.to_string(),
            password: password.to_string(),
        })
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

        // RFC 7617 Section 2: credentials は token68 形式
        if !is_token68(credentials) {
            return Err(AuthError::InvalidToken);
        }

        // Base64 デコード
        let decoded = base64::decode(credentials).map_err(|_| AuthError::Base64DecodeError)?;

        // UTF-8 としてデコード
        let decoded_str = String::from_utf8(decoded).map_err(|_| AuthError::Utf8Error)?;

        // user:password 形式をパース
        let colon_pos = decoded_str.find(':').ok_or(AuthError::MissingColon)?;
        let username = &decoded_str[..colon_pos];
        let password = &decoded_str[colon_pos + 1..];

        // RFC 7617 Section 2: user-id と password に制御文字を含めてはならない
        if has_control_chars(username) || has_control_chars(password) {
            return Err(AuthError::ControlCharacter);
        }

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
    /// let auth = BasicAuth::new("user", "password").unwrap();
    /// assert_eq!(auth.to_header_value(), "Basic dXNlcjpwYXNzd29yZA==");
    /// ```
    pub fn to_header_value(&self) -> String {
        let credentials = alloc::format!("{}:{}", self.username, self.password);
        alloc::format!("Basic {}", base64::encode(credentials.as_bytes()))
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

    /// charset パラメータを UTF-8 に設定
    ///
    /// RFC 7617 Section 2.1: charset の許容値は "UTF-8" のみ
    pub fn with_charset_utf8(mut self) -> Self {
        self.charset = Some("UTF-8".to_string());
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

        // RFC 9110 Section 11.2: quoted-string 内のカンマを正しく処理する
        let parsed_params = parse_auth_params(params)?;

        let mut realm = None;
        let mut charset = None;

        for (key, value) in &parsed_params {
            match key.as_str() {
                "realm" => realm = Some(value.clone()),
                "charset" => {
                    // RFC 7617 Section 2.1: charset の許容値は "UTF-8" のみ
                    if !value.eq_ignore_ascii_case("UTF-8") {
                        return Err(AuthError::InvalidCharset);
                    }
                    charset = Some(value.clone());
                }
                _ => {} // 未知のパラメータは無視
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

        // RFC 7616 §3.4: username と username* は XOR (両方同時送信は MUST NOT)。
        // どちらか一方が必須。username* は RFC 8187 ext-value (UTF-8 ユーザー名用)。
        let has_username = params.iter().any(|(n, _)| n == "username");
        let has_username_ext = params.iter().any(|(n, _)| n == "username*");
        if has_username && has_username_ext {
            return Err(AuthError::ConflictingUsernameField);
        }
        if !has_username && !has_username_ext {
            return Err(AuthError::MissingParameter);
        }

        // username* が指定されている場合は、ext-value として decode 可能か事前検証する。
        // 不正値は早期に reject (`username()` 呼出時に毎回 fallible にしないため)。
        if has_username_ext {
            let raw = params
                .iter()
                .find(|(n, _)| n == "username*")
                .map(|(_, v)| v.as_str())
                .unwrap_or("");
            decode_username_ext_value(raw)?;
        }

        if !has_required_params(&params, &["realm", "nonce", "uri", "response"]) {
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

    /// username (ASCII) を取得
    ///
    /// `username` パラメータがある場合のみその値を返す。`username*` で送られた
    /// UTF-8 ユーザー名は本メソッドでは取得できない。UTF-8 ユーザー名を含めて
    /// 取得するには [`username_decoded`](Self::username_decoded) を使う。
    pub fn username(&self) -> Option<&str> {
        self.param("username")
    }

    /// `username` または `username*` のいずれかから UTF-8 ユーザー名を取得する
    ///
    /// RFC 7616 §3.4 に従い、`username` パラメータがあればその値を、なければ
    /// `username*` を RFC 8187 ext-value としてデコードした値を返す。
    /// 構築時に `username*` の ext-value は検証済みのため本メソッドは infallible。
    pub fn username_decoded(&self) -> Option<String> {
        if let Some(v) = self.param("username") {
            return Some(v.to_string());
        }
        let raw = self.param("username*")?;
        // 構築時 (parse) に検証済みのため decode は必ず成功する。
        decode_username_ext_value(raw).ok()
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
        alloc::format!("Digest {}", format_auth_params(&self.params))
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
        alloc::format!("Digest {}", format_auth_params(&self.params))
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
        alloc::format!("Bearer {}", self.token)
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
        alloc::format!("Bearer {}", format_auth_params(&self.params))
    }
}

impl fmt::Display for BearerChallenge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_header_value())
    }
}

/// Authorization ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
#[non_exhaustive]
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
            // 開く DQUOTE をスキップしてサブスライスから char 単位で走査する。
            // bytes[i] == b'"' は ASCII (1 バイト) なので i+1 は valid な char 境界。
            i += 1;
            let inner = &input[i..];
            let mut iter = inner.chars();
            let mut value = String::new();
            // 走査した value 部分と閉じ DQUOTE が占めるバイト数。
            // 外側 i に反映して quoted-string 全体を消費させる。
            let mut consumed: usize = 0;
            let mut closed = false;
            while let Some(c) = iter.next() {
                if c == '"' {
                    consumed += 1; // 閉じ DQUOTE は ASCII 1 バイト
                    closed = true;
                    break;
                } else if c == '\\' {
                    consumed += 1; // バックスラッシュは ASCII 1 バイト
                    let next_c = iter.next().ok_or(AuthError::InvalidParameter)?;
                    // RFC 9110 Section 5.6.4: quoted-pair = "\" ( HTAB / SP / VCHAR / obs-text )
                    // CTL (CR / LF / NUL / 他) は escape の対象として許容しない。
                    if !is_quoted_pair_char(next_c) {
                        return Err(AuthError::InvalidParameter);
                    }
                    consumed += next_c.len_utf8();
                    value.push(next_c);
                } else {
                    // RFC 9110 Section 5.6.4: qdtext = HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text
                    // DQUOTE と backslash は別経路で処理済み。CR / LF / NUL 等の CTL を reject する。
                    if !is_qdtext_char(c) {
                        return Err(AuthError::InvalidParameter);
                    }
                    consumed += c.len_utf8();
                    value.push(c);
                }
            }
            if !closed {
                return Err(AuthError::InvalidParameter);
            }
            i += consumed;
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

        // RFC 9110 Section 11.2: 各パラメータ名は 1 回のみ
        let key = name.to_ascii_lowercase();
        if params.iter().any(|(n, _)| n == &key) {
            return Err(AuthError::DuplicateParameter);
        }
        // issue 0047: パラメータ数 hard cap (`MAX_AUTH_PARAMS = 32`)。
        // 線形重複検出の CPU 消費を有限に抑える。
        if params.len() >= MAX_AUTH_PARAMS {
            return Err(AuthError::TooManyParameters);
        }
        params.push((key, value));
        while i < bytes.len() && is_ows(bytes[i]) {
            i += 1;
        }
        if i < bytes.len() {
            // RFC 9110 Section 11.2 (auth-param 定義)、Section 11.6.3: auth-param *( OWS "," OWS auth-param )
            // パラメータ間のカンマは必須
            if bytes[i] == b',' {
                i += 1;
            } else {
                return Err(AuthError::InvalidParameter);
            }
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
            parts.push(alloc::format!("{}=\"{}\"", name, escape_quotes(value)));
        } else {
            parts.push(alloc::format!("{}={}", name, value));
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

/// RFC 9110 Section 11.2: token68 = 1*( ALPHA / DIGIT / "-" / "." / "_" / "~" / "+" / "/" ) *"="
fn is_token68(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    let trimmed = value.trim_end_matches('=');
    // 末尾の = を除去した残りが 1 文字以上必要
    !trimmed.is_empty() && trimmed.bytes().all(is_token68_char)
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
    )
}

fn is_ows(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

/// RFC 5234 Appendix B.1: CTL = %x00-1F / %x7F
fn has_control_chars(s: &str) -> bool {
    s.bytes().any(|b| b <= 0x1F || b == 0x7F)
}

/// RFC 8187 Section 3.2.1 / RFC 7616 Section 3.4: `username*` の ext-value をデコードする
///
/// ext-value = charset "'" [ language ] "'" value-chars
/// 例: `UTF-8''%E3%83%A6%E3%83%BC%E3%82%B6` → `ユーザ`
///
/// charset は UTF-8 のみサポート (RFC 7616 §3.4 で UTF-8 を要求するため、
/// ISO-8859-1 等は本実装では `InvalidUsernameExtValue` で reject)。
fn decode_username_ext_value(input: &str) -> Result<String, AuthError> {
    let first_quote = input.find('\'').ok_or(AuthError::InvalidUsernameExtValue)?;
    let charset = &input[..first_quote];
    if !charset.eq_ignore_ascii_case("UTF-8") {
        // RFC 7616 §3.4 は UTF-8 charset を要求する。
        return Err(AuthError::InvalidUsernameExtValue);
    }

    let rest = &input[first_quote + 1..];
    let second_quote = rest.find('\'').ok_or(AuthError::InvalidUsernameExtValue)?;
    // language タグは無視する (RFC 8187 §3.2.1: 受信側は無視してよい)
    let value_chars = &rest[second_quote + 1..];

    // percent-decode する。RFC 8187 §3.2.1 の attr-char 範囲外は reject。
    let bytes = value_chars.as_bytes();
    let mut result = alloc::vec::Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(AuthError::InvalidUsernameExtValue);
            }
            let hi = (bytes[i + 1] as char)
                .to_digit(16)
                .ok_or(AuthError::InvalidUsernameExtValue)? as u8;
            let lo = (bytes[i + 2] as char)
                .to_digit(16)
                .ok_or(AuthError::InvalidUsernameExtValue)? as u8;
            result.push((hi << 4) | lo);
            i += 3;
        } else if is_attr_char(b) {
            result.push(b);
            i += 1;
        } else {
            return Err(AuthError::InvalidUsernameExtValue);
        }
    }

    String::from_utf8(result).map_err(|_| AuthError::InvalidUsernameExtValue)
}

/// RFC 8187 Section 3.2.1: attr-char
///
/// attr-char = ALPHA / DIGIT / "!" / "#" / "$" / "&" / "+" / "-" / "." /
///             "^" / "_" / "`" / "|" / "~"
fn is_attr_char(b: u8) -> bool {
    matches!(
        b,
        b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'!'
            | b'#'
            | b'$'
            | b'&'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_www_authenticate_parse_empty() {
        assert!(WwwAuthenticate::parse("").is_err());
    }

    #[test]
    fn test_www_authenticate_parse_not_basic() {
        assert!(WwwAuthenticate::parse("Digest realm=\"test\"").is_err());
    }

    #[test]
    fn test_digest_auth_missing_param() {
        let header = "Digest username=\"Mufasa\", realm=\"test\"";
        assert!(DigestAuth::parse(header).is_err());
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
    fn test_digest_auth_non_ascii_input() {
        let input = ")ϓ )ϓ";
        assert!(DigestAuth::parse(input).is_err());
    }

    #[test]
    fn test_token68_equals_only_at_end() {
        // 末尾の = は OK (Base64 パディング)
        assert!(is_token68("Zm8="));
        assert!(is_token68("Zg=="));
        assert!(is_token68("abc"));
        // 途中の = は NG
        assert!(!is_token68("a=b"));
        assert!(!is_token68("a=b="));
        // = のみは NG (1*(...) が必要)
        assert!(!is_token68("="));
        assert!(!is_token68("=="));
        // 空は NG
        assert!(!is_token68(""));
    }
}
