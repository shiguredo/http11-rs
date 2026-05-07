//! URL のパース処理
//!
//! `http://` / `https://` スキームのみをサポートする最小実装。
//! 本サンプル / integration test 用途であり、汎用 URL parser を目指していない。

/// URL を (scheme, host, port, path) に分解する
///
/// scheme 省略不可。port 省略時は `https` → 443 / `http` → 80 を補う。
/// path 省略時は `/` を返す。
pub fn parse_url(
    url: &str,
) -> Result<(String, String, u16, String), Box<dyn std::error::Error + Send + Sync>> {
    let (scheme, rest) = if let Some(rest) = url.strip_prefix("https://") {
        ("https".to_string(), rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        ("http".to_string(), rest)
    } else {
        return Err("URL must start with http:// or https://".into());
    };

    let (host_port, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };

    let (host, port) = match host_port.find(':') {
        Some(i) => {
            let port: u16 = host_port[i + 1..].parse()?;
            (&host_port[..i], port)
        }
        None => {
            let port = if scheme == "https" { 443 } else { 80 };
            (host_port, port)
        }
    };

    Ok((scheme, host.to_string(), port, path.to_string()))
}
