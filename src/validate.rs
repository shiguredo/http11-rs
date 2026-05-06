//! RFC 9110 / RFC 3986 基本文字集合の共通検証（デコード・エンコード双方で使用）

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
/// RFC 9110 Section 9: method = token
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
/// RFC 9112 Section 3: request-target には制御文字を含めない
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
