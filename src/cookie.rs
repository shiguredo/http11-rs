//! Cookie ヘッダーパース (RFC 6265)
//!
//! ## 概要
//!
//! RFC 6265 に基づいた Cookie および Set-Cookie ヘッダーのパースを提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::cookie::{Cookie, SetCookie, SameSite};
//!
//! // Cookie ヘッダーパース
//! let cookies = Cookie::parse("session=abc123; user=john").unwrap();
//! assert_eq!(cookies[0].name(), "session");
//! assert_eq!(cookies[0].value(), "abc123");
//!
//! // Set-Cookie ヘッダーパース
//! let set_cookie = SetCookie::parse("session=abc123; Path=/; HttpOnly; Secure").unwrap();
//! assert_eq!(set_cookie.name(), "session");
//! assert_eq!(set_cookie.value(), "abc123");
//! assert_eq!(set_cookie.path(), Some("/"));
//! assert!(set_cookie.http_only());
//! assert!(set_cookie.secure());
//! ```

use crate::date::HttpDate;
use core::fmt;

/// Cookie パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CookieError {
    /// 空の Cookie
    Empty,
    /// 不正な形式
    InvalidFormat,
    /// 不正な名前
    InvalidName,
    /// 不正な値
    InvalidValue,
    /// 不正な属性
    InvalidAttribute,
    /// 不正な Expires
    InvalidExpires,
    /// 不正な Max-Age
    InvalidMaxAge,
    /// 不正な SameSite
    InvalidSameSite,
}

impl fmt::Display for CookieError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CookieError::Empty => write!(f, "empty cookie"),
            CookieError::InvalidFormat => write!(f, "invalid cookie format"),
            CookieError::InvalidName => write!(f, "invalid cookie name"),
            CookieError::InvalidValue => write!(f, "invalid cookie value"),
            CookieError::InvalidAttribute => write!(f, "invalid cookie attribute"),
            CookieError::InvalidExpires => write!(f, "invalid Expires attribute"),
            CookieError::InvalidMaxAge => write!(f, "invalid Max-Age attribute"),
            CookieError::InvalidSameSite => write!(f, "invalid SameSite attribute"),
        }
    }
}

impl std::error::Error for CookieError {}

/// Cookie (name=value ペア)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cookie {
    /// Cookie 名
    name: String,
    /// Cookie 値
    value: String,
}

impl Cookie {
    /// Cookie ヘッダー文字列をパース
    ///
    /// Cookie ヘッダーは複数の name=value ペアをセミコロンで区切って含みます。
    ///
    /// # 例
    ///
    /// ```rust
    /// use shiguredo_http11::cookie::Cookie;
    ///
    /// let cookies = Cookie::parse("session=abc123; user=john").unwrap();
    /// assert_eq!(cookies.len(), 2);
    /// assert_eq!(cookies[0].name(), "session");
    /// assert_eq!(cookies[1].name(), "user");
    /// ```
    pub fn parse(input: &str) -> Result<Vec<Cookie>, CookieError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(CookieError::Empty);
        }

        let mut cookies = Vec::new();

        for pair in input.split(';') {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }

            let (name, value) = parse_cookie_pair(pair)?;
            cookies.push(Cookie {
                name: name.to_string(),
                value: value.to_string(),
            });
        }

        if cookies.is_empty() {
            return Err(CookieError::Empty);
        }

        Ok(cookies)
    }

    /// 新しい Cookie を作成
    pub fn new(name: &str, value: &str) -> Result<Self, CookieError> {
        if name.is_empty() || !is_valid_cookie_name(name) {
            return Err(CookieError::InvalidName);
        }
        if !is_valid_cookie_value(value) {
            return Err(CookieError::InvalidValue);
        }

        Ok(Cookie {
            name: name.to_string(),
            value: value.to_string(),
        })
    }

    /// Cookie 名を取得
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Cookie 値を取得
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Display for Cookie {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", self.name, self.value)
    }
}

/// SameSite 属性
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SameSite {
    /// Strict: 同一サイトリクエストのみ送信
    Strict,
    /// Lax: トップレベルナビゲーションでは送信
    #[default]
    Lax,
    /// None: すべてのリクエストで送信 (Secure 必須)
    None,
}

impl SameSite {
    fn from_str(s: &str) -> Result<Self, CookieError> {
        match s.to_ascii_lowercase().as_str() {
            "strict" => Ok(SameSite::Strict),
            "lax" => Ok(SameSite::Lax),
            "none" => Ok(SameSite::None),
            _ => Err(CookieError::InvalidSameSite),
        }
    }
}

impl fmt::Display for SameSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SameSite::Strict => write!(f, "Strict"),
            SameSite::Lax => write!(f, "Lax"),
            SameSite::None => write!(f, "None"),
        }
    }
}

/// Set-Cookie ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetCookie {
    /// Cookie 名
    name: String,
    /// Cookie 値
    value: String,
    /// Expires 属性
    expires: Option<HttpDate>,
    /// Max-Age 属性 (秒)
    max_age: Option<i64>,
    /// Domain 属性
    domain: Option<String>,
    /// Path 属性
    path: Option<String>,
    /// Secure 属性
    secure: bool,
    /// HttpOnly 属性
    http_only: bool,
    /// SameSite 属性
    same_site: Option<SameSite>,
}

impl SetCookie {
    /// Set-Cookie ヘッダー文字列をパース
    ///
    /// # 例
    ///
    /// ```rust
    /// use shiguredo_http11::cookie::SetCookie;
    ///
    /// let cookie = SetCookie::parse("session=abc123; Path=/; HttpOnly; Secure").unwrap();
    /// assert_eq!(cookie.name(), "session");
    /// assert_eq!(cookie.value(), "abc123");
    /// assert_eq!(cookie.path(), Some("/"));
    /// assert!(cookie.http_only());
    /// assert!(cookie.secure());
    /// ```
    pub fn parse(input: &str) -> Result<Self, CookieError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(CookieError::Empty);
        }

        let mut parts = input.split(';');

        // 最初の部分は name=value
        let first = parts.next().ok_or(CookieError::InvalidFormat)?;
        let (name, value) = parse_cookie_pair(first.trim())?;

        let mut set_cookie = SetCookie {
            name: name.to_string(),
            value: value.to_string(),
            expires: None,
            max_age: None,
            domain: None,
            path: None,
            secure: false,
            http_only: false,
            same_site: None,
        };

        // 属性をパース
        for part in parts {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            if let Some(eq_pos) = part.find('=') {
                let attr_name = part[..eq_pos].trim();
                let attr_value = part[eq_pos + 1..].trim();

                match attr_name.to_ascii_lowercase().as_str() {
                    "expires" => {
                        set_cookie.expires = Some(
                            HttpDate::parse(attr_value).map_err(|_| CookieError::InvalidExpires)?,
                        );
                    }
                    "max-age" => {
                        set_cookie.max_age = Some(
                            attr_value
                                .parse::<i64>()
                                .map_err(|_| CookieError::InvalidMaxAge)?,
                        );
                    }
                    "domain" => {
                        set_cookie.domain = Some(attr_value.to_string());
                    }
                    "path" => {
                        set_cookie.path = Some(attr_value.to_string());
                    }
                    "samesite" => {
                        set_cookie.same_site = Some(SameSite::from_str(attr_value)?);
                    }
                    _ => {
                        // 未知の属性は無視 (RFC 6265 の推奨)
                    }
                }
            } else {
                // 値なし属性
                match part.to_ascii_lowercase().as_str() {
                    "secure" => set_cookie.secure = true,
                    "httponly" => set_cookie.http_only = true,
                    _ => {
                        // 未知の属性は無視
                    }
                }
            }
        }

        Ok(set_cookie)
    }

    /// 新しい SetCookie を作成
    pub fn new(name: &str, value: &str) -> Result<Self, CookieError> {
        if name.is_empty() || !is_valid_cookie_name(name) {
            return Err(CookieError::InvalidName);
        }
        if !is_valid_cookie_value(value) {
            return Err(CookieError::InvalidValue);
        }

        Ok(SetCookie {
            name: name.to_string(),
            value: value.to_string(),
            expires: None,
            max_age: None,
            domain: None,
            path: None,
            secure: false,
            http_only: false,
            same_site: None,
        })
    }

    /// Cookie 名を取得
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Cookie 値を取得
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Expires 属性を取得
    pub fn expires(&self) -> Option<&HttpDate> {
        self.expires.as_ref()
    }

    /// Max-Age 属性を取得 (秒)
    pub fn max_age(&self) -> Option<i64> {
        self.max_age
    }

    /// Domain 属性を取得
    pub fn domain(&self) -> Option<&str> {
        self.domain.as_deref()
    }

    /// Path 属性を取得
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    /// Secure 属性を取得
    pub fn secure(&self) -> bool {
        self.secure
    }

    /// HttpOnly 属性を取得
    pub fn http_only(&self) -> bool {
        self.http_only
    }

    /// SameSite 属性を取得
    pub fn same_site(&self) -> Option<SameSite> {
        self.same_site
    }

    /// Expires を設定
    pub fn with_expires(mut self, expires: HttpDate) -> Self {
        self.expires = Some(expires);
        self
    }

    /// Max-Age を設定
    pub fn with_max_age(mut self, max_age: i64) -> Self {
        self.max_age = Some(max_age);
        self
    }

    /// Domain を設定
    pub fn with_domain(mut self, domain: &str) -> Self {
        self.domain = Some(domain.to_string());
        self
    }

    /// Path を設定
    pub fn with_path(mut self, path: &str) -> Self {
        self.path = Some(path.to_string());
        self
    }

    /// Secure を設定
    pub fn with_secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    /// HttpOnly を設定
    pub fn with_http_only(mut self, http_only: bool) -> Self {
        self.http_only = http_only;
        self
    }

    /// SameSite を設定
    pub fn with_same_site(mut self, same_site: SameSite) -> Self {
        self.same_site = Some(same_site);
        self
    }
}

impl fmt::Display for SetCookie {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", self.name, self.value)?;

        if let Some(expires) = &self.expires {
            write!(f, "; Expires={}", expires)?;
        }

        if let Some(max_age) = self.max_age {
            write!(f, "; Max-Age={}", max_age)?;
        }

        if let Some(domain) = &self.domain {
            write!(f, "; Domain={}", domain)?;
        }

        if let Some(path) = &self.path {
            write!(f, "; Path={}", path)?;
        }

        if self.secure {
            write!(f, "; Secure")?;
        }

        if self.http_only {
            write!(f, "; HttpOnly")?;
        }

        if let Some(same_site) = self.same_site {
            write!(f, "; SameSite={}", same_site)?;
        }

        Ok(())
    }
}

/// Cookie name=value ペアをパース
fn parse_cookie_pair(pair: &str) -> Result<(&str, &str), CookieError> {
    let eq_pos = pair.find('=').ok_or(CookieError::InvalidFormat)?;

    let name = pair[..eq_pos].trim();
    let value = pair[eq_pos + 1..].trim();

    if name.is_empty() {
        return Err(CookieError::InvalidName);
    }

    if !is_valid_cookie_name(name) {
        return Err(CookieError::InvalidName);
    }

    // 値の引用符を除去
    let value = if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        &value[1..value.len() - 1]
    } else {
        value
    };

    Ok((name, value))
}

/// 有効な Cookie 名かどうか
/// RFC 6265 Section 4.1.1: cookie-name = token
fn is_valid_cookie_name(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_token_char)
}

/// 有効な Cookie 値かどうか
/// RFC 6265 Section 4.1.1
fn is_valid_cookie_value(s: &str) -> bool {
    s.bytes().all(is_cookie_octet)
}

/// トークン文字 (RFC 7230)
fn is_token_char(b: u8) -> bool {
    matches!(b,
        b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.' |
        b'0'..=b'9' | b'A'..=b'Z' | b'^' | b'_' | b'`' | b'a'..=b'z' | b'|' | b'~'
    )
}

/// Cookie 値に使える文字
fn is_cookie_octet(b: u8) -> bool {
    b == 0x21
        || (0x23..=0x2B).contains(&b)
        || (0x2D..=0x3A).contains(&b)
        || (0x3C..=0x5B).contains(&b)
        || (0x5D..=0x7E).contains(&b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cookie_parse_single() {
        let cookies = Cookie::parse("session=abc123").unwrap();
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].name(), "session");
        assert_eq!(cookies[0].value(), "abc123");
    }

    #[test]
    fn test_cookie_parse_multiple() {
        let cookies = Cookie::parse("session=abc123; user=john").unwrap();
        assert_eq!(cookies.len(), 2);
        assert_eq!(cookies[0].name(), "session");
        assert_eq!(cookies[0].value(), "abc123");
        assert_eq!(cookies[1].name(), "user");
        assert_eq!(cookies[1].value(), "john");
    }

    #[test]
    fn test_cookie_parse_with_spaces() {
        let cookies = Cookie::parse("  session = abc123 ; user = john  ").unwrap();
        assert_eq!(cookies.len(), 2);
        assert_eq!(cookies[0].name(), "session");
        assert_eq!(cookies[0].value(), "abc123");
    }

    #[test]
    fn test_cookie_parse_empty() {
        assert!(Cookie::parse("").is_err());
    }

    #[test]
    fn test_cookie_display() {
        let cookie = Cookie::new("session", "abc123").unwrap();
        assert_eq!(cookie.to_string(), "session=abc123");
    }

    #[test]
    fn test_set_cookie_parse_simple() {
        let cookie = SetCookie::parse("session=abc123").unwrap();
        assert_eq!(cookie.name(), "session");
        assert_eq!(cookie.value(), "abc123");
        assert!(!cookie.secure());
        assert!(!cookie.http_only());
    }

    #[test]
    fn test_set_cookie_parse_with_attributes() {
        let cookie = SetCookie::parse("session=abc123; Path=/; HttpOnly; Secure").unwrap();
        assert_eq!(cookie.name(), "session");
        assert_eq!(cookie.value(), "abc123");
        assert_eq!(cookie.path(), Some("/"));
        assert!(cookie.http_only());
        assert!(cookie.secure());
    }

    #[test]
    fn test_set_cookie_parse_with_domain() {
        let cookie = SetCookie::parse("session=abc123; Domain=example.com").unwrap();
        assert_eq!(cookie.domain(), Some("example.com"));
    }

    #[test]
    fn test_set_cookie_parse_with_max_age() {
        let cookie = SetCookie::parse("session=abc123; Max-Age=3600").unwrap();
        assert_eq!(cookie.max_age(), Some(3600));
    }

    #[test]
    fn test_set_cookie_parse_with_expires() {
        let cookie =
            SetCookie::parse("session=abc123; Expires=Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
        assert!(cookie.expires().is_some());
    }

    #[test]
    fn test_set_cookie_parse_with_samesite() {
        let cookie = SetCookie::parse("session=abc123; SameSite=Strict").unwrap();
        assert_eq!(cookie.same_site(), Some(SameSite::Strict));

        let cookie = SetCookie::parse("session=abc123; SameSite=Lax").unwrap();
        assert_eq!(cookie.same_site(), Some(SameSite::Lax));

        let cookie = SetCookie::parse("session=abc123; SameSite=None").unwrap();
        assert_eq!(cookie.same_site(), Some(SameSite::None));
    }

    #[test]
    fn test_set_cookie_display() {
        let cookie = SetCookie::new("session", "abc123")
            .unwrap()
            .with_path("/")
            .with_secure(true)
            .with_http_only(true);
        let s = cookie.to_string();
        assert!(s.contains("session=abc123"));
        assert!(s.contains("Path=/"));
        assert!(s.contains("Secure"));
        assert!(s.contains("HttpOnly"));
    }

    #[test]
    fn test_set_cookie_builder() {
        let cookie = SetCookie::new("session", "abc123")
            .unwrap()
            .with_domain("example.com")
            .with_path("/app")
            .with_max_age(3600)
            .with_secure(true)
            .with_http_only(true)
            .with_same_site(SameSite::Strict);

        assert_eq!(cookie.name(), "session");
        assert_eq!(cookie.value(), "abc123");
        assert_eq!(cookie.domain(), Some("example.com"));
        assert_eq!(cookie.path(), Some("/app"));
        assert_eq!(cookie.max_age(), Some(3600));
        assert!(cookie.secure());
        assert!(cookie.http_only());
        assert_eq!(cookie.same_site(), Some(SameSite::Strict));
    }

    #[test]
    fn test_cookie_parse_quoted_value() {
        let cookies = Cookie::parse("session=\"abc123\"").unwrap();
        assert_eq!(cookies[0].value(), "abc123");
    }

    #[test]
    fn test_same_site_default() {
        assert_eq!(SameSite::default(), SameSite::Lax);
    }
}
