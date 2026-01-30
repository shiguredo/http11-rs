//! URI パースとパーセントエンコーディング (RFC 3986)
//!
//! ## 概要
//!
//! RFC 3986 に基づいた URI のパースとパーセントエンコーディング/デコーディングを提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::uri::{Uri, percent_encode, percent_decode};
//!
//! // URI パース
//! let uri = Uri::parse("https://example.com:8080/path?query=value#fragment").unwrap();
//! assert_eq!(uri.scheme(), Some("https"));
//! assert_eq!(uri.host(), Some("example.com"));
//! assert_eq!(uri.port(), Some(8080));
//! assert_eq!(uri.path(), "/path");
//! assert_eq!(uri.query(), Some("query=value"));
//! assert_eq!(uri.fragment(), Some("fragment"));
//!
//! // パーセントエンコーディング
//! let encoded = percent_encode("hello world");
//! assert_eq!(encoded, "hello%20world");
//!
//! // パーセントデコーディング
//! let decoded = percent_decode("hello%20world").unwrap();
//! assert_eq!(decoded, "hello world");
//! ```

use core::fmt;

/// URI パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UriError {
    /// 空の URI
    Empty,
    /// 不正なパーセントエンコーディング
    InvalidPercentEncoding,
    /// 不正なポート番号
    InvalidPort,
    /// 不正な文字
    InvalidCharacter(char),
    /// 不正なスキーム
    InvalidScheme,
    /// 不正なホスト
    InvalidHost,
    /// 不正な UTF-8 シーケンス
    InvalidUtf8,
}

impl fmt::Display for UriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UriError::Empty => write!(f, "empty URI"),
            UriError::InvalidPercentEncoding => write!(f, "invalid percent encoding"),
            UriError::InvalidPort => write!(f, "invalid port"),
            UriError::InvalidCharacter(c) => write!(f, "invalid character: {:?}", c),
            UriError::InvalidScheme => write!(f, "invalid scheme"),
            UriError::InvalidHost => write!(f, "invalid host"),
            UriError::InvalidUtf8 => write!(f, "invalid UTF-8 sequence"),
        }
    }
}

impl std::error::Error for UriError {}

/// パーセントエンコーディング対象外の文字 (unreserved characters)
/// RFC 3986 Section 2.3
fn is_unreserved(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-' || c == b'.' || c == b'_' || c == b'~'
}

/// パーセントエンコーディング
///
/// RFC 3986 Section 2.1 に基づき、unreserved 文字以外をパーセントエンコードします。
///
/// # 例
///
/// ```rust
/// use shiguredo_http11::uri::percent_encode;
///
/// assert_eq!(percent_encode("hello world"), "hello%20world");
/// assert_eq!(percent_encode("foo=bar&baz=qux"), "foo%3Dbar%26baz%3Dqux");
/// assert_eq!(percent_encode("日本語"), "%E6%97%A5%E6%9C%AC%E8%AA%9E");
/// ```
pub fn percent_encode(input: &str) -> String {
    let mut result = String::with_capacity(input.len() * 3);
    for byte in input.bytes() {
        if is_unreserved(byte) {
            result.push(byte as char);
        } else {
            result.push('%');
            result.push(to_hex_char(byte >> 4));
            result.push(to_hex_char(byte & 0x0F));
        }
    }
    result
}

/// パーセントエンコーディング (パス用)
///
/// パス区切り文字 `/` はエンコードしません。
pub fn percent_encode_path(input: &str) -> String {
    let mut result = String::with_capacity(input.len() * 3);
    for byte in input.bytes() {
        if is_unreserved(byte) || byte == b'/' {
            result.push(byte as char);
        } else {
            result.push('%');
            result.push(to_hex_char(byte >> 4));
            result.push(to_hex_char(byte & 0x0F));
        }
    }
    result
}

/// パーセントエンコーディング (クエリ用)
///
/// `=` と `&` はエンコードしません。
pub fn percent_encode_query(input: &str) -> String {
    let mut result = String::with_capacity(input.len() * 3);
    for byte in input.bytes() {
        if is_unreserved(byte) || byte == b'=' || byte == b'&' {
            result.push(byte as char);
        } else {
            result.push('%');
            result.push(to_hex_char(byte >> 4));
            result.push(to_hex_char(byte & 0x0F));
        }
    }
    result
}

fn to_hex_char(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'A' + nibble - 10) as char,
        _ => unreachable!(),
    }
}

/// パーセントデコーディング
///
/// RFC 3986 Section 2.1 に基づき、パーセントエンコードされた文字列をデコードします。
///
/// # 例
///
/// ```rust
/// use shiguredo_http11::uri::percent_decode;
///
/// assert_eq!(percent_decode("hello%20world").unwrap(), "hello world");
/// assert_eq!(percent_decode("%E6%97%A5%E6%9C%AC%E8%AA%9E").unwrap(), "日本語");
/// ```
pub fn percent_decode(input: &str) -> Result<String, UriError> {
    let bytes = percent_decode_bytes(input)?;
    String::from_utf8(bytes).map_err(|_| UriError::InvalidUtf8)
}

/// パーセントデコーディング (バイト列として)
pub fn percent_decode_bytes(input: &str) -> Result<Vec<u8>, UriError> {
    let mut result = Vec::with_capacity(input.len());
    let mut bytes = input.bytes();

    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let high = bytes.next().ok_or(UriError::InvalidPercentEncoding)?;
            let low = bytes.next().ok_or(UriError::InvalidPercentEncoding)?;
            let high = from_hex_char(high).ok_or(UriError::InvalidPercentEncoding)?;
            let low = from_hex_char(low).ok_or(UriError::InvalidPercentEncoding)?;
            result.push((high << 4) | low);
        } else {
            result.push(byte);
        }
    }

    Ok(result)
}

fn from_hex_char(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'A'..=b'F' => Some(c - b'A' + 10),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

/// パース済み URI
///
/// RFC 3986 Section 3 に基づいた URI 構造:
/// ```text
///   foo://example.com:8042/over/there?name=ferret#nose
///   \_/   \______________/\_________/ \_________/ \__/
///    |           |            |            |        |
/// scheme     authority       path        query   fragment
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uri {
    /// 元の URI 文字列
    source: String,
    /// スキームの終了位置 (`:` の位置)
    scheme_end: Option<usize>,
    /// authority の開始位置 (`//` の後)
    authority_start: Option<usize>,
    /// authority の終了位置
    authority_end: Option<usize>,
    /// ホストの終了位置
    host_end: Option<usize>,
    /// ポート番号
    port: Option<u16>,
    /// パスの開始位置
    path_start: usize,
    /// パスの終了位置
    path_end: usize,
    /// クエリの開始位置 (`?` の後)
    query_start: Option<usize>,
    /// クエリの終了位置
    query_end: Option<usize>,
    /// フラグメントの開始位置 (`#` の後)
    fragment_start: Option<usize>,
}

impl Uri {
    /// URI 文字列をパース
    ///
    /// # 例
    ///
    /// ```rust
    /// use shiguredo_http11::uri::Uri;
    ///
    /// let uri = Uri::parse("https://example.com/path?query#fragment").unwrap();
    /// assert_eq!(uri.scheme(), Some("https"));
    /// assert_eq!(uri.host(), Some("example.com"));
    /// assert_eq!(uri.path(), "/path");
    /// ```
    pub fn parse(input: &str) -> Result<Self, UriError> {
        if input.is_empty() {
            return Err(UriError::Empty);
        }

        let source = input.to_string();
        let bytes = input.as_bytes();
        let len = bytes.len();

        let mut pos = 0;

        // スキームのパース (RFC 3986 Section 3.1)
        // scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )
        let scheme_end = if let Some(colon_pos) = find_scheme_end(bytes) {
            // スキームの検証
            if !bytes[0].is_ascii_alphabetic() {
                return Err(UriError::InvalidScheme);
            }
            for &b in &bytes[1..colon_pos] {
                if !b.is_ascii_alphanumeric() && b != b'+' && b != b'-' && b != b'.' {
                    return Err(UriError::InvalidScheme);
                }
            }
            pos = colon_pos + 1;
            Some(colon_pos)
        } else {
            None
        };

        // authority のパース (RFC 3986 Section 3.2)
        let (authority_start, authority_end, host_end, port) =
            if pos + 1 < len && bytes[pos] == b'/' && bytes[pos + 1] == b'/' {
                pos += 2;
                let auth_start = pos;

                // authority の終端を探す
                let auth_end = bytes[pos..]
                    .iter()
                    .position(|&b| b == b'/' || b == b'?' || b == b'#')
                    .map(|p| pos + p)
                    .unwrap_or(len);

                let authority = &input[auth_start..auth_end];
                let (h_end, p) = parse_authority(authority)?;

                pos = auth_end;
                (
                    Some(auth_start),
                    Some(auth_end),
                    Some(auth_start + h_end),
                    p,
                )
            } else {
                (None, None, None, None)
            };

        // パスのパース (RFC 3986 Section 3.3)
        let path_start = pos;
        let path_end = bytes[pos..]
            .iter()
            .position(|&b| b == b'?' || b == b'#')
            .map(|p| pos + p)
            .unwrap_or(len);
        pos = path_end;

        // クエリのパース (RFC 3986 Section 3.4)
        let (query_start, query_end) = if pos < len && bytes[pos] == b'?' {
            pos += 1;
            let start = pos;
            let end = bytes[pos..]
                .iter()
                .position(|&b| b == b'#')
                .map(|p| pos + p)
                .unwrap_or(len);
            pos = end;
            (Some(start), Some(end))
        } else {
            (None, None)
        };

        // フラグメントのパース (RFC 3986 Section 3.5)
        let fragment_start = if pos < len && bytes[pos] == b'#' {
            Some(pos + 1)
        } else {
            None
        };

        Ok(Uri {
            source,
            scheme_end,
            authority_start,
            authority_end,
            host_end,
            port,
            path_start,
            path_end,
            query_start,
            query_end,
            fragment_start,
        })
    }

    /// スキームを取得
    pub fn scheme(&self) -> Option<&str> {
        self.scheme_end.map(|end| &self.source[..end])
    }

    /// authority 全体を取得
    pub fn authority(&self) -> Option<&str> {
        match (self.authority_start, self.authority_end) {
            (Some(start), Some(end)) => Some(&self.source[start..end]),
            _ => None,
        }
    }

    /// ホストを取得
    pub fn host(&self) -> Option<&str> {
        match (self.authority_start, self.host_end) {
            (Some(start), Some(end)) => {
                let auth = &self.source[start..end];
                // userinfo を除去
                if let Some(at_pos) = auth.rfind('@') {
                    Some(&auth[at_pos + 1..])
                } else {
                    Some(auth)
                }
            }
            _ => None,
        }
    }

    /// ポート番号を取得
    pub fn port(&self) -> Option<u16> {
        self.port
    }

    /// パスを取得
    pub fn path(&self) -> &str {
        &self.source[self.path_start..self.path_end]
    }

    /// クエリを取得
    pub fn query(&self) -> Option<&str> {
        match (self.query_start, self.query_end) {
            (Some(start), Some(end)) => Some(&self.source[start..end]),
            _ => None,
        }
    }

    /// フラグメントを取得
    pub fn fragment(&self) -> Option<&str> {
        self.fragment_start.map(|start| &self.source[start..])
    }

    /// 元の URI 文字列を取得
    pub fn as_str(&self) -> &str {
        &self.source
    }

    /// origin-form を取得 (path + query)
    ///
    /// HTTP リクエストの request-target として使用
    pub fn origin_form(&self) -> String {
        let path = self.path();
        let path = if path.is_empty() { "/" } else { path };

        if let Some(query) = self.query() {
            format!("{}?{}", path, query)
        } else {
            path.to_string()
        }
    }

    /// 絶対 URI かどうか
    pub fn is_absolute(&self) -> bool {
        self.scheme_end.is_some()
    }

    /// 相対参照かどうか
    pub fn is_relative(&self) -> bool {
        self.scheme_end.is_none()
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.source)
    }
}

/// スキームの終端位置を探す
fn find_scheme_end(bytes: &[u8]) -> Option<usize> {
    for (i, &b) in bytes.iter().enumerate() {
        if b == b':' {
            // `:` の前にスキーム文字以外があれば、これはスキームではない
            if i > 0 {
                return Some(i);
            }
            return None;
        }
        // スキームに使えない文字が出たら終了
        if !b.is_ascii_alphanumeric() && b != b'+' && b != b'-' && b != b'.' {
            return None;
        }
    }
    None
}

/// authority をパース
/// 戻り値: (host_end, port)
fn parse_authority(authority: &str) -> Result<(usize, Option<u16>), UriError> {
    if authority.is_empty() {
        return Ok((0, None));
    }

    // userinfo を除去
    let host_part = if let Some(at_pos) = authority.rfind('@') {
        &authority[at_pos + 1..]
    } else {
        authority
    };

    // IPv6 アドレス
    if host_part.starts_with('[') {
        if let Some(bracket_end) = host_part.find(']') {
            let after_bracket = &host_part[bracket_end + 1..];
            if after_bracket.is_empty() {
                return Ok((authority.len(), None));
            } else if let Some(port_str) = after_bracket.strip_prefix(':') {
                let port = port_str.parse::<u16>().map_err(|_| UriError::InvalidPort)?;
                return Ok((authority.len() - after_bracket.len(), Some(port)));
            } else {
                return Err(UriError::InvalidHost);
            }
        } else {
            return Err(UriError::InvalidHost);
        }
    }

    // 通常のホスト:ポート
    if let Some(colon_pos) = host_part.rfind(':') {
        let port_str = &host_part[colon_pos + 1..];
        if !port_str.is_empty() {
            let port = port_str.parse::<u16>().map_err(|_| UriError::InvalidPort)?;
            let host_end = if let Some(at_pos) = authority.rfind('@') {
                at_pos + 1 + colon_pos
            } else {
                colon_pos
            };
            return Ok((host_end, Some(port)));
        }
    }

    Ok((authority.len(), None))
}

/// 相対 URI を基底 URI に対して解決
///
/// RFC 3986 Section 5 に基づいて相対参照を解決します。
///
/// # 例
///
/// ```rust
/// use shiguredo_http11::uri::{Uri, resolve};
///
/// let base = Uri::parse("http://example.com/a/b/c").unwrap();
/// let relative = Uri::parse("../d").unwrap();
/// let resolved = resolve(&base, &relative).unwrap();
/// assert_eq!(resolved.as_str(), "http://example.com/a/d");
/// ```
pub fn resolve(base: &Uri, reference: &Uri) -> Result<Uri, UriError> {
    // RFC 3986 Section 5.3
    if reference.is_absolute() {
        // 参照が絶対 URI なら、そのまま返す (パスの正規化のみ)
        let path = remove_dot_segments(reference.path());
        return Uri::parse(&build_uri(
            reference.scheme(),
            reference.authority(),
            &path,
            reference.query(),
            reference.fragment(),
        ));
    }

    if reference.authority().is_some() {
        // authority があれば、base のスキームのみ使用
        let path = remove_dot_segments(reference.path());
        return Uri::parse(&build_uri(
            base.scheme(),
            reference.authority(),
            &path,
            reference.query(),
            reference.fragment(),
        ));
    }

    if reference.path().is_empty() {
        // パスが空
        let query = reference.query().or(base.query());
        return Uri::parse(&build_uri(
            base.scheme(),
            base.authority(),
            base.path(),
            query,
            reference.fragment(),
        ));
    }

    let path = if reference.path().starts_with('/') {
        remove_dot_segments(reference.path())
    } else {
        let merged = merge_paths(base, reference.path());
        remove_dot_segments(&merged)
    };

    Uri::parse(&build_uri(
        base.scheme(),
        base.authority(),
        &path,
        reference.query(),
        reference.fragment(),
    ))
}

/// パスをマージ
fn merge_paths(base: &Uri, reference_path: &str) -> String {
    if base.authority().is_some() && base.path().is_empty() {
        format!("/{}", reference_path)
    } else {
        // base パスの最後のセグメントを除去して reference パスを追加
        let base_path = base.path();
        if let Some(last_slash) = base_path.rfind('/') {
            format!("{}{}", &base_path[..=last_slash], reference_path)
        } else {
            reference_path.to_string()
        }
    }
}

/// `.` と `..` セグメントを除去
///
/// RFC 3986 Section 5.2.4 のアルゴリズムに基づく
fn remove_dot_segments(path: &str) -> String {
    let mut output: Vec<&str> = Vec::new();
    let mut i = 0;
    let bytes = path.as_bytes();
    let len = bytes.len();

    while i < len {
        // A: `../` または `./` で始まる場合、除去
        if path[i..].starts_with("../") {
            i += 3;
            continue;
        }
        if path[i..].starts_with("./") {
            i += 2;
            continue;
        }

        // B: `/./` で始まる場合、`/` に置き換え
        if path[i..].starts_with("/./") {
            i += 2; // `/.` を飛ばし `/` を残す
            continue;
        }
        // `/.` で終わる場合
        if &path[i..] == "/." {
            output.push("/");
            break;
        }

        // C: `/../` で始まる場合、`/` に置き換え、出力から最後のセグメントを除去
        if path[i..].starts_with("/../") {
            i += 3; // `/..` を飛ばし `/` を残す
            output.pop();
            continue;
        }
        // `/..` で終わる場合
        if &path[i..] == "/.." {
            output.pop();
            output.push("/");
            break;
        }

        // D: `.` または `..` のみ
        if &path[i..] == "." || &path[i..] == ".." {
            break;
        }

        // E: 最初のパスセグメントを出力に移動
        let start = i;
        if bytes[i] == b'/' {
            i += 1;
        }
        while i < len && bytes[i] != b'/' {
            i += 1;
        }
        output.push(&path[start..i]);
    }

    output.concat()
}

/// URI を構築
fn build_uri(
    scheme: Option<&str>,
    authority: Option<&str>,
    path: &str,
    query: Option<&str>,
    fragment: Option<&str>,
) -> String {
    let mut result = String::new();

    if let Some(s) = scheme {
        result.push_str(s);
        result.push(':');
    }

    if let Some(a) = authority {
        result.push_str("//");
        result.push_str(a);
    }

    result.push_str(path);

    if let Some(q) = query {
        result.push('?');
        result.push_str(q);
    }

    if let Some(f) = fragment {
        result.push('#');
        result.push_str(f);
    }

    result
}

/// URI を正規化
///
/// RFC 3986 Section 6 に基づいて URI を正規化します。
pub fn normalize(uri: &Uri) -> Result<Uri, UriError> {
    let scheme = uri.scheme().map(|s| s.to_ascii_lowercase());
    // RFC 3986: host のみ case-insensitive、userinfo は case-sensitive
    let authority = uri.authority().map(normalize_authority);
    let path = remove_dot_segments(uri.path());

    // パーセントエンコーディングの正規化
    let path = normalize_percent_encoding(&path)?;

    let query = uri.query().map(normalize_percent_encoding).transpose()?;
    let fragment = uri.fragment().map(normalize_percent_encoding).transpose()?;

    Uri::parse(&build_uri(
        scheme.as_deref(),
        authority.as_deref(),
        &path,
        query.as_deref(),
        fragment.as_deref(),
    ))
}

/// authority を正規化 (userinfo は case-sensitive、host は case-insensitive)
fn normalize_authority(authority: &str) -> String {
    if let Some(at_pos) = authority.rfind('@') {
        // userinfo あり: userinfo はそのまま、host:port は小文字化
        let userinfo = &authority[..at_pos];
        let host_port = &authority[at_pos + 1..];
        format!("{}@{}", userinfo, normalize_host_port(host_port))
    } else {
        // userinfo なし: 全体が host:port
        normalize_host_port(authority)
    }
}

/// host:port を正規化 (host は小文字化、port はそのまま)
fn normalize_host_port(host_port: &str) -> String {
    // IPv6 アドレス
    if let Some(bracket_end) = host_port.strip_prefix('[').and_then(|s| s.find(']')) {
        let host = &host_port[..=bracket_end + 1];
        let after = &host_port[bracket_end + 2..];
        return format!("{}{}", host.to_ascii_lowercase(), after);
    }

    // 通常の host:port
    if let Some(colon_pos) = host_port.rfind(':') {
        let host = &host_port[..colon_pos];
        let port = &host_port[colon_pos..];
        format!("{}{}", host.to_ascii_lowercase(), port)
    } else {
        host_port.to_ascii_lowercase()
    }
}

/// パーセントエンコーディングを正規化
fn normalize_percent_encoding(input: &str) -> Result<String, UriError> {
    let mut result = String::with_capacity(input.len());
    let mut bytes = input.bytes().peekable();

    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let high = bytes.next().ok_or(UriError::InvalidPercentEncoding)?;
            let low = bytes.next().ok_or(UriError::InvalidPercentEncoding)?;
            let high_val = from_hex_char(high).ok_or(UriError::InvalidPercentEncoding)?;
            let low_val = from_hex_char(low).ok_or(UriError::InvalidPercentEncoding)?;
            let decoded = (high_val << 4) | low_val;

            // unreserved 文字はデコード、それ以外は大文字でエンコード
            if is_unreserved(decoded) {
                result.push(decoded as char);
            } else {
                result.push('%');
                result.push(to_hex_char(decoded >> 4));
                result.push(to_hex_char(decoded & 0x0F));
            }
        } else {
            result.push(byte as char);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percent_encode() {
        assert_eq!(percent_encode("hello"), "hello");
        assert_eq!(percent_encode("hello world"), "hello%20world");
        assert_eq!(percent_encode("foo=bar"), "foo%3Dbar");
        assert_eq!(percent_encode("日本語"), "%E6%97%A5%E6%9C%AC%E8%AA%9E");
    }

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("hello").unwrap(), "hello");
        assert_eq!(percent_decode("hello%20world").unwrap(), "hello world");
        assert_eq!(
            percent_decode("%E6%97%A5%E6%9C%AC%E8%AA%9E").unwrap(),
            "日本語"
        );
    }

    #[test]
    fn test_percent_decode_invalid() {
        assert!(percent_decode("%").is_err());
        assert!(percent_decode("%2").is_err());
        assert!(percent_decode("%GG").is_err());
    }

    #[test]
    fn test_uri_parse_full() {
        let uri =
            Uri::parse("https://user:pass@example.com:8080/path/to/resource?query=value#fragment")
                .unwrap();
        assert_eq!(uri.scheme(), Some("https"));
        assert_eq!(uri.authority(), Some("user:pass@example.com:8080"));
        assert_eq!(uri.host(), Some("example.com"));
        assert_eq!(uri.port(), Some(8080));
        assert_eq!(uri.path(), "/path/to/resource");
        assert_eq!(uri.query(), Some("query=value"));
        assert_eq!(uri.fragment(), Some("fragment"));
    }

    #[test]
    fn test_uri_parse_simple() {
        let uri = Uri::parse("http://example.com").unwrap();
        assert_eq!(uri.scheme(), Some("http"));
        assert_eq!(uri.host(), Some("example.com"));
        assert_eq!(uri.port(), None);
        assert_eq!(uri.path(), "");
        assert_eq!(uri.query(), None);
        assert_eq!(uri.fragment(), None);
    }

    #[test]
    fn test_uri_parse_path_only() {
        let uri = Uri::parse("/path/to/resource").unwrap();
        assert_eq!(uri.scheme(), None);
        assert_eq!(uri.host(), None);
        assert_eq!(uri.path(), "/path/to/resource");
    }

    #[test]
    fn test_uri_parse_relative() {
        let uri = Uri::parse("../other/path").unwrap();
        assert_eq!(uri.scheme(), None);
        assert!(uri.is_relative());
        assert_eq!(uri.path(), "../other/path");
    }

    #[test]
    fn test_uri_parse_ipv6() {
        let uri = Uri::parse("http://[::1]:8080/path").unwrap();
        assert_eq!(uri.host(), Some("[::1]"));
        assert_eq!(uri.port(), Some(8080));
    }

    #[test]
    fn test_origin_form() {
        let uri = Uri::parse("http://example.com/path?query").unwrap();
        assert_eq!(uri.origin_form(), "/path?query");

        let uri = Uri::parse("http://example.com").unwrap();
        assert_eq!(uri.origin_form(), "/");
    }

    #[test]
    fn test_resolve() {
        let base = Uri::parse("http://example.com/a/b/c").unwrap();

        let resolved = resolve(&base, &Uri::parse("../d").unwrap()).unwrap();
        assert_eq!(resolved.path(), "/a/d");

        let resolved = resolve(&base, &Uri::parse("/absolute").unwrap()).unwrap();
        assert_eq!(resolved.path(), "/absolute");

        let resolved = resolve(&base, &Uri::parse("relative").unwrap()).unwrap();
        assert_eq!(resolved.path(), "/a/b/relative");
    }

    #[test]
    fn test_remove_dot_segments() {
        assert_eq!(remove_dot_segments("/a/b/c/./../../g"), "/a/g");
        assert_eq!(remove_dot_segments("mid/content=5/../6"), "mid/6");
        assert_eq!(remove_dot_segments("/../a"), "/a");
        assert_eq!(remove_dot_segments("./a"), "a");
    }

    #[test]
    fn test_normalize_authority_userinfo_preserved() {
        // RFC 3986: userinfo は case-sensitive、host は case-insensitive
        let uri = Uri::parse("http://UserName:PassWord@EXAMPLE.COM/path").unwrap();
        let normalized = normalize(&uri).unwrap();
        // userinfo は大文字のまま、host は小文字化
        assert_eq!(
            normalized.authority(),
            Some("UserName:PassWord@example.com")
        );
    }

    #[test]
    fn test_normalize_authority_without_userinfo() {
        let uri = Uri::parse("http://EXAMPLE.COM:8080/path").unwrap();
        let normalized = normalize(&uri).unwrap();
        assert_eq!(normalized.authority(), Some("example.com:8080"));
    }
}
