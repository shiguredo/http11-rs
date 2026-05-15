//! RFC 9110 / RFC 3986 基本文字集合の共通検証（デコード・エンコード双方で使用）

use alloc::string::String;

/// トークン文字か確認 (RFC 9110 Section 5.6.2)
///
/// token = 1*tchar
/// tchar = "!" / "#" / "$" / "%" / "&" / "'" / "*" / "+" / "-" / "." /
///         DIGIT / ALPHA / "^" / "_" / "`" / "|" / "~"
pub(crate) fn is_token_char(b: u8) -> bool {
    matches!(
        b,
        b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.' |
        b'0'..=b'9' | b'A'..=b'Z' | b'^' | b'_' | b'`' | b'a'..=b'z' | b'|' | b'~'
    )
}

/// ヘッダー名が有効か確認
pub(crate) fn is_valid_header_name(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(is_token_char)
}

/// token が有効か確認 (RFC 9110 Section 5.6.2)
///
/// token = 1*tchar
///
/// `is_valid_header_name` と同じロジックだが、Transfer-Encoding の coding 名など
/// ヘッダー名以外の token 検証に使用する。意味的に区別するため別関数として提供する。
pub(crate) fn is_valid_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_token_char)
}

/// ヘッダー値に許可される文字か確認 (RFC 9110 Section 5.5)
///
/// field-value = *field-content
/// field-vchar = VCHAR / obs-text
/// VCHAR = %x21-7E (可視文字)
/// obs-text = %x80-FF
///
/// SP (0x20) と HTAB (0x09) も許可される (field-content の一部)
///
/// # RFC 非準拠
///
/// 現在の実装ではヘッダー値を UTF-8 として解釈しており、obs-text (0x80-0xFF) を
/// バイト列として扱っていない。RFC 9110 Section 5.5 では obs-text は任意のバイト列
/// として定義されているが、本実装では UTF-8 として解釈するため、不正な UTF-8
/// シーケンスを含むヘッダー値は拒否される。現時点ではこの制限を維持する。
pub(crate) fn is_valid_field_vchar(b: u8) -> bool {
    matches!(b, 0x09 | 0x20..=0x7E | 0x80..=0xFF)
}

/// ヘッダー値が有効か確認 (RFC 9110 Section 5.5)
///
/// 制御文字 (0x00-0x08, 0x0A-0x1F, 0x7F) を含む場合は無効
///
/// # RFC 非準拠
///
/// 現在の実装ではヘッダー値を UTF-8 として解釈しており、obs-text (0x80-0xFF) を
/// バイト列として扱っていない。RFC 9110 Section 5.5 では obs-text は任意のバイト列
/// として定義されているが、本実装では UTF-8 として解釈するため、不正な UTF-8
/// シーケンスを含むヘッダー値は拒否される。現時点ではこの制限を維持する。
pub(crate) fn is_valid_field_value(value: &str) -> bool {
    value.bytes().all(is_valid_field_vchar)
}

/// メソッド名が有効か確認
///
/// RFC 9110 Section 9.1: method = token
/// token = 1*tchar (RFC 9110 Section 5.6.2)
///
/// RTSP (RFC 7826) の GET_PARAMETER, SET_PARAMETER なども tchar で表現可能。
pub(crate) fn is_valid_method(method: &str) -> bool {
    !method.is_empty() && method.bytes().all(is_token_char)
}

/// プロトコルバージョンが有効か確認
///
/// HTTP (RFC 9112 Section 2.3):
///   HTTP-version = HTTP-name "/" DIGIT "." DIGIT
///   HTTP-name = %s"HTTP"
///
/// RTSP (RFC 7826 Section 20.3):
///   RTSP-version = "RTSP" "/" 1*DIGIT "." 1*DIGIT
///
/// 両方をカバーするため、token "/" DIGIT+ "." DIGIT+ 形式で検証する。
/// token = 1*tchar (RFC 9110 Section 5.6.2)
pub(crate) fn is_valid_protocol_version(version: &str) -> bool {
    let bytes = version.as_bytes();

    // "/" を探す
    let slash_pos = match bytes.iter().position(|&b| b == b'/') {
        Some(pos) => pos,
        None => return false,
    };

    // token 部分: 1 文字以上の tchar
    if slash_pos == 0 {
        return false;
    }
    if !bytes[..slash_pos].iter().all(|&b| is_token_char(b)) {
        return false;
    }

    // "/" の後: DIGIT+ "." DIGIT+
    let after_slash = &bytes[slash_pos + 1..];

    // "." を探す
    let dot_pos = match after_slash.iter().position(|&b| b == b'.') {
        Some(pos) => pos,
        None => return false,
    };

    // "." の前: 1 文字以上の DIGIT
    if dot_pos == 0 {
        return false;
    }
    if !after_slash[..dot_pos].iter().all(|b| b.is_ascii_digit()) {
        return false;
    }

    // "." の後: 1 文字以上の DIGIT
    let after_dot = &after_slash[dot_pos + 1..];
    if after_dot.is_empty() {
        return false;
    }
    after_dot.iter().all(|b| b.is_ascii_digit())
}

/// ステータスコードが有効か確認 (RFC 9110 Section 15)
///
/// ステータスコードは 3 桁の数字で、100-599 の範囲
pub(crate) fn is_valid_status_code(code: u16) -> bool {
    (100..=599).contains(&code)
}

/// reason-phrase が有効か確認 (RFC 9112 Section 4)
///
/// reason-phrase = 1*( HTAB / SP / VCHAR / obs-text )
/// VCHAR = %x21-7E
/// obs-text = %x80-FF
///
/// 空文字列は RFC 9112 Section 4 の status-line ABNF で reason-phrase が
/// absent (未指定) の場合に発生するが、本関数は「reason-phrase が指定された場合」
/// の文字集合検証を意図している。空文字列の扱いは呼び出し側の責務とする。
///
/// # RFC 非準拠
///
/// 現在の実装では reason-phrase を UTF-8 として解釈しており、obs-text (0x80-0xFF) を
/// バイト列として扱っていない。RFC 9112 Section 4 では obs-text は任意のバイト列
/// として定義されているが、本実装では UTF-8 として解釈するため、不正な UTF-8
/// シーケンスを含む reason-phrase は拒否される。現時点ではこの制限を維持する。
pub(crate) fn is_valid_reason_phrase(phrase: &str) -> bool {
    !phrase.is_empty()
        && phrase
            .bytes()
            .all(|b| matches!(b, 0x09 | 0x20..=0x7E | 0x80..=0xFF))
}

/// RFC 3986 で除外されている文字および request-target で許可されない文字
///
/// RFC 9112 Section 3.2: request-target は origin-form / absolute-form / authority-form / asterisk-form のいずれか
/// RFC 3986: absolute-URI にはフラグメントが含まれない (absolute-URI = scheme ":" hier-part [ "?" query ])
/// したがって request-target では "#" (フラグメント区切り) は許可されない
pub(crate) const RFC3986_EXCLUDED: &[u8] = b"\"#<>\\^`{|}";

/// リクエストターゲット (URI) が有効か確認（受信側用）
///
/// RFC 9112 Section 3.2: request-target には制御文字を含めない
/// RFC 3986 Section 2: URI で許可されない文字を拒否
///
/// 本関数は受信側の寛容な検証として実装している。
/// obs-text (0x80-0xFF) は構文上含まれないが、歴史的互換性のため許容する。
/// 受信側は 0x80-0xFF を勝手に UTF-8/Latin-1 と断定してはならない。
/// 送信側で obs-text を拒否する必要がある場合は呼び出し側で別途チェックすること。
///
/// 拒否する文字:
/// - 制御文字 (0x00-0x20, 0x7F)
/// - RFC 3986 で除外されている文字: " < > \ ^ ` { | }
/// - 不正なパーセントエンコーディング (% の後に 2 桁の 16 進数がない)
/// - パーセントエンコーディングされた NUL バイト (%00)
pub(crate) fn is_valid_request_target(target: &str) -> bool {
    if target.is_empty() {
        return false;
    }

    let bytes = target.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];

        // 制御文字の拒否 (0x00-0x20, 0x7F)
        if b <= 0x20 || b == 0x7F {
            return false;
        }

        // RFC 3986 除外文字の拒否
        if RFC3986_EXCLUDED.contains(&b) {
            return false;
        }

        // パーセントエンコーディングの検証
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return false; // 不完全
            }
            let high = bytes[i + 1];
            let low = bytes[i + 2];

            if !high.is_ascii_hexdigit() || !low.is_ascii_hexdigit() {
                return false; // 不正な 16 進数
            }

            // %00 (NUL) の拒否
            if high == b'0' && low == b'0' {
                return false;
            }

            i += 3;
            continue;
        }

        i += 1;
    }

    true
}

/// pchar または "/" か確認 (RFC 3986)
pub(crate) fn is_pchar_or_slash(b: u8) -> bool {
    is_pchar_byte(b) || b == b'/'
}

/// pchar か確認 (RFC 3986 Section 3.3)
/// pchar = unreserved / pct-encoded / sub-delims / ":" / "@"
pub(crate) fn is_pchar_byte(b: u8) -> bool {
    is_unreserved_byte(b) || is_sub_delim_byte(b) || b == b':' || b == b'@'
}

/// query で許可される文字か確認 (RFC 3986 Section 3.4)
/// query = *( pchar / "/" / "?" )
pub(crate) fn is_query_char(b: u8) -> bool {
    is_pchar_byte(b) || b == b'/' || b == b'?'
}

/// unreserved か確認 (RFC 3986 Section 2.3)
pub(crate) fn is_unreserved_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'_' || b == b'~'
}

/// sub-delims か確認 (RFC 3986 Section 2.2)
pub(crate) fn is_sub_delim_byte(b: u8) -> bool {
    matches!(
        b,
        b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

/// qdtext char か確認 (RFC 9110 Section 5.6.4)
///
/// ABNF (bytes): qdtext = HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text
///                obs-text = %x80-FF (RFC 9110 Section 5.5)
///
/// 本実装は valid UTF-8 `&str` を char 単位で走査するため、
/// ABNF のオクテット表現を Unicode scalar に拡張解釈し、obs-text の
/// オクテット範囲 (`U+0080..=U+00FF`) を超える Unicode scalar
/// (`U+0100..=U+10FFFF`、surrogate `U+D800..=U+DFFF` は char 型で構築不能)
/// も opaque char としてそのまま受理する。RFC 9110 Section 5.5 の
/// 「recipient SHOULD treat ... obs-text ... as opaque data」を
/// char 単位に拡張解釈したもの。
///
/// DQUOTE (`"`) と backslash (`\`) は除く。
/// CR / LF / NUL / 他の CTL (`U+0001..=U+001F` のうち HTAB 以外、`U+007F`) は不許可。
pub(crate) fn is_qdtext_char(c: char) -> bool {
    matches!(c, '\t' | ' ' | '!' | '#'..='[' | ']'..='~') || c as u32 >= 0x80
}

/// quoted-pair の右辺 char か確認 (RFC 9110 Section 5.6.4)
///
/// ABNF (bytes): quoted-pair = "\" ( HTAB / SP / VCHAR / obs-text )
///                VCHAR = %x21-7E, obs-text = %x80-FF (RFC 9110 Section 5.5)
///
/// `is_qdtext_char` と同じく Unicode scalar 単位に拡張解釈する。
/// NUL (`U+0000`) / CR (`U+000D`) / LF (`U+000A`) / 他の CTL は不許可。
/// 受信側でも CR / LF を含む quoted-pair を素通りさせると、上位アプリでの再エンコード経路で
/// response splitting / log injection に至る経路を生むため厳格に reject する。
pub(crate) fn is_quoted_pair_char(c: char) -> bool {
    matches!(c, '\t' | ' '..='~') || c as u32 >= 0x80
}

/// quoted-string パースのエラー種別 (RFC 9110 Section 5.6.4)
///
/// `parse_quoted_string` から返り、各ヘッダーモジュールが自身のエラー型に
/// マッピングする。文字種違反と構造違反を区別することで、`Content-Type` の
/// `UnterminatedQuote` のような既存の細粒度エラーを保てる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum QuotedStringError {
    /// qdtext 経路で `is_qdtext_char` が false を返した
    InvalidQdtext,
    /// quoted-pair 経路で `is_quoted_pair_char` が false を返した
    InvalidQuotedPair,
    /// 閉じ DQUOTE が見つからずに入力を使い切った
    /// (バックスラッシュエスケープ未完了で入力が尽きた場合も含む)
    Unterminated,
}

/// 引用符付き文字列をパース (RFC 9110 Section 5.6.4)
///
/// ABNF (`refs/rfc9110.txt:1786-1794`):
/// ```text
/// quoted-string = DQUOTE *( qdtext / quoted-pair ) DQUOTE
/// qdtext        = HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text
/// quoted-pair   = "\" ( HTAB / SP / VCHAR / obs-text )
/// ```
///
/// 入力は開く DQUOTE を消費した残り。閉じ DQUOTE までを `qdtext / quoted-pair`
/// として走査し、検証済み中身と閉じ DQUOTE 以降の残り `&str` を返す。
///
/// CR / LF / NUL は RFC 9110 Section 5.5 (`refs/rfc9110.txt:1606-1615`) で MUST reject。
/// 他の CTL (%x01-08, %x0B-0C, %x0E-1F, %x7F DEL) は同節で MAY retain (safe context 限定)
/// だが、本ヘルパーを使うヘッダ群は HTTP インターミディアリが解釈・書換する
/// 標準ヘッダ (Accept / Content-Type / Expect 等) であり safe context に該当しないため
/// 保守的に reject する。素通りさせると上位アプリの再エンコード経路で
/// response splitting (CWE-113) / log injection に至る経路を生む。
///
/// obs-text (RFC 上は %x80-FF) は `is_qdtext_char` / `is_quoted_pair_char` の
/// Unicode scalar 拡張解釈 (`U+0080..=U+10FFFF`、surrogate 除く) で受理する。
///
/// 本関数は RFC 9110 (本リリース時点) の規定に基づく。将来の改訂や erratum で
/// ABNF が変更される可能性がある。
pub(crate) fn parse_quoted_string(input: &str) -> Result<(String, &str), QuotedStringError> {
    let mut result = String::new();
    let mut escaped = false;

    for (i, c) in input.char_indices() {
        if escaped {
            if !is_quoted_pair_char(c) {
                return Err(QuotedStringError::InvalidQuotedPair);
            }
            result.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            return Ok((result, &input[i + 1..]));
        } else {
            if !is_qdtext_char(c) {
                return Err(QuotedStringError::InvalidQdtext);
            }
            result.push(c);
        }
    }

    Err(QuotedStringError::Unterminated)
}

/// quoted-string の値文字列をエスケープ (送信側、RFC 9110 Section 5.6.4)
///
/// ABNF (`refs/rfc9110.txt:1786-1794`):
/// ```text
/// quoted-string = DQUOTE *( qdtext / quoted-pair ) DQUOTE
/// qdtext        = HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text
/// quoted-pair   = "\" ( HTAB / SP / VCHAR / obs-text )
/// ```
///
/// quoted-pair が必要な `"` と `\` のみエスケープし、それ以外はそのまま出力する。
///
/// CR / LF / NUL (RFC 9110 Section 5.5 `refs/rfc9110.txt:1606-1611`) および
/// 他の CTL (%x01-08, %x0B-0C, %x0E-1F, %x7F DEL) は SP に置換する。
/// RFC 9110 Section 5.5 は CR / LF / NUL に対し "MUST either reject the message
/// or replace each of those characters with SP" と規定しており、SP 置換は RFC 準拠。
/// 他の CTL については "recipients MAY retain such characters ... within a safe
/// context" (`refs/rfc9110.txt:1611-1615`) とされ、本関数の出力先 (WWW-Authenticate /
/// Accept / Content-Type / Expect 等の HTTP 標準ヘッダ) は safe context に該当しない
/// ため retain せず SP 置換する。
///
/// 本関数は RFC 9110 (本リリース時点) の規定に基づく。将来の改訂や erratum で
/// ABNF が変更される可能性がある。
pub(crate) fn escape_quotes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        if !is_quoted_pair_char(c) {
            result.push(' '); // CTL を SP に置換 (RFC 9110 Section 5.5)
            continue;
        }
        if c == '"' || c == '\\' {
            result.push('\\');
        }
        result.push(c);
    }
    result
}

/// OWS (Optional Whitespace) を前後から除去 (RFC 9110 Section 5.6.3)
///
/// OWS = *( SP / HTAB )
///
/// Rust の `str::trim()` は `char::is_whitespace` に基づき U+00A0 (NBSP) や
/// U+2000-200A 等の Unicode 空白も除去する。`is_valid_field_value` は obs-text
/// (0x80-0xFF) を許容するためヘッダー値にこれらのバイトが含まれ得るが、
/// OWS として扱ってよいのは SP / HTAB のみ。本関数を使うことで encoder と
/// decoder の Content-Length 等の解釈を一致させ HTTP Request Smuggling
/// (CWE-444) 経路を塞ぐ。
pub(crate) fn trim_ows(s: &str) -> &str {
    let bytes = s.as_bytes();
    let start = bytes
        .iter()
        .position(|&b| b != b' ' && b != b'\t')
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|&b| b != b' ' && b != b'\t')
        .map(|p| p + 1)
        .unwrap_or(start);
    // start..end は全て ASCII 文字 (SP/HTAB) の境界なので UTF-8 として安全
    &s[start..end]
}

// validate モジュールは `pub(crate)` で外部 integration test (tests/) から参照不可。
// このためインラインテストとして配置する。CLAUDE.md:93 の「単体テストは tests/test_<module>.rs」
// 規約は public モジュールが対象であり、本モジュールは対象外。
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_quotes_passes_through_safe_chars() {
        assert_eq!(escape_quotes(""), "");
        assert_eq!(escape_quotes("hello"), "hello");
        assert_eq!(escape_quotes("日本語"), "日本語");
        // obs-text のオクテット範囲 (U+0080..=U+00FF) と Unicode scalar 拡張解釈の範囲 (U+0100..=U+10FFFF)
        assert_eq!(escape_quotes("\u{0080}\u{00FF}"), "\u{0080}\u{00FF}");
        assert_eq!(escape_quotes("\u{0100}\u{10FFFF}"), "\u{0100}\u{10FFFF}");
    }

    #[test]
    fn escape_quotes_escapes_dquote_and_backslash() {
        assert_eq!(escape_quotes("a\"b"), "a\\\"b");
        assert_eq!(escape_quotes("a\\b"), "a\\\\b");
    }

    #[test]
    fn escape_quotes_replaces_ctl_with_space() {
        // CR / LF / NUL は MUST replace with SP (RFC 9110 Section 5.5)
        assert_eq!(escape_quotes("\r"), " ");
        assert_eq!(escape_quotes("\n"), " ");
        assert_eq!(escape_quotes("\0"), " ");
        // 他の CTL も SP 置換
        assert_eq!(escape_quotes("\x01"), " ");
        assert_eq!(escape_quotes("\x1F"), " ");
        // DEL
        assert_eq!(escape_quotes("\x7F"), " ");
        // 複数 CTL の連続
        assert_eq!(escape_quotes("\r\n\0"), "   ");
        // CTL とエスケープ対象の相互作用
        assert_eq!(escape_quotes("\0\""), " \\\"");
        assert_eq!(escape_quotes("\0\\"), " \\\\");
        // CTL と正常文字の混在
        assert_eq!(escape_quotes("a\rb\nc"), "a b c");
    }
}
