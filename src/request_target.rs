//! request-target の形式 (RFC 9112 Section 3.2)
//!
//! encoder と decoder で共有される概念。

/// RFC 9112 Section 3.2 request-target の形式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestTargetForm {
    /// origin-form: absolute-path [ "?" query ]
    /// 例: /path/to/resource?query=value
    Origin,
    /// absolute-form: absolute-URI
    /// 例: http://example.com/path
    Absolute,
    /// authority-form: uri-host ":" port (CONNECT のみ)
    /// 例: example.com:443
    Authority,
    /// asterisk-form: "*" (OPTIONS のみ)
    Asterisk,
}

/// スキームを検出する (RFC 3986 Section 3.1)
///
/// scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )
/// absolute-URI = scheme ":" hier-part [ "?" query ]
///
/// target の先頭が有効なスキーム + ":" であればスキームの長さを返す。
/// "://" を含む URI だけでなく、":" の後に "//" がない absolute-URI にも対応する。
///
/// encoder / decoder 両方から共有される共通ロジック。
pub(crate) fn detect_scheme(target: &str) -> Option<usize> {
    let bytes = target.as_bytes();

    // 最初の文字が ALPHA でなければスキームではない
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return None;
    }

    // ":" を探す
    let colon_pos = bytes.iter().position(|&b| b == b':')?;

    // スキーム部分が空でないこと (最低 1 文字)
    if colon_pos == 0 {
        return None;
    }

    // スキーム文字の検証
    for &b in &bytes[1..colon_pos] {
        if !b.is_ascii_alphanumeric() && b != b'+' && b != b'-' && b != b'.' {
            return None;
        }
    }

    // 意図的な RFC 非準拠: path-empty (scheme ":" のみ) を拒否する。
    // RFC 3986 の ABNF では path-empty は合法だが、HTTP request-target として
    // path-empty が単独で出現する実用的なケースはないため、不正な入力として扱う。
    if colon_pos + 1 >= bytes.len() {
        return None;
    }

    Some(colon_pos)
}
