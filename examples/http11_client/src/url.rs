//! URL のパース処理
//!
//! `http://` / `https://` スキームのみをサポートする。
//! RFC 3986 の URI パーサを利用し、IPv6 リテラル、userinfo、query、
//! 空ポートなどの境界ケースを一貫して扱う。

use shiguredo_http11::uri::Uri;

/// URL を (scheme, host, port, path) に分解する
///
/// scheme 省略不可。port 省略時は `https` → 443 / `http` → 80 を補う。
/// path 省略時は `/` を返す。fragment は request-target に含めない。
pub fn parse_url(
    url: &str,
) -> Result<(String, String, u16, String), Box<dyn std::error::Error + Send + Sync>> {
    let uri = Uri::parse(url)?;
    let scheme = uri
        .scheme()
        .ok_or("URL must have an http:// or https:// scheme")?
        .to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err("URL scheme must be http or https".into());
    }

    let host = uri.host().ok_or("URL has no host")?;
    if host.is_empty() {
        return Err("URL has an empty host".into());
    }
    // RFC 3986 の IP-literal はブラケット付きで保持されるため、
    // TcpStream::connect に渡す裸のホストへ変換する。
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);

    let port = uri
        .port()
        .unwrap_or(if scheme == "https" { 443 } else { 80 });
    let path = uri.origin_form();

    Ok((scheme, host.to_string(), port, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_url_default_port_and_path() {
        let (scheme, host, port, path) =
            parse_url("https://example.com").expect("URL のパースは成功するはず (実装バグ)");
        assert_eq!(scheme, "https");
        assert_eq!(host, "example.com");
        assert_eq!(port, 443);
        assert_eq!(path, "/");
    }

    #[test]
    fn test_parse_url_ipv6() {
        let (scheme, host, port, path) = parse_url("http://[::1]:8080/path?query")
            .expect("URL のパースは成功するはず (実装バグ)");
        assert_eq!(scheme, "http");
        assert_eq!(host, "::1");
        assert_eq!(port, 8080);
        assert_eq!(path, "/path?query");
    }

    #[test]
    fn test_parse_url_query_only() {
        let (scheme, host, port, path) =
            parse_url("http://example.com?query=1").expect("URL のパースは成功するはず (実装バグ)");
        assert_eq!(scheme, "http");
        assert_eq!(host, "example.com");
        assert_eq!(port, 80);
        assert_eq!(path, "/?query=1");
    }

    #[test]
    fn test_parse_url_empty_port() {
        let (_, host, port, _) =
            parse_url("http://example.com:/path").expect("URL のパースは成功するはず (実装バグ)");
        assert_eq!(host, "example.com");
        assert_eq!(port, 80);
    }

    #[test]
    fn test_parse_url_invalid() {
        assert!(parse_url("ftp://example.com/").is_err());
        assert!(parse_url("http:///path").is_err());
        assert!(parse_url("http://example.com:abc/").is_err());
    }
}
