use crate::compression::{CompressionError, CompressionStatus, Compressor, NoCompression};
use crate::error::EncodeError;
use crate::host::Host;
use crate::request::Request;
use crate::request_target::RequestTargetForm;
use crate::response::Response;
use crate::validate::{
    is_valid_field_value, is_valid_header_name, is_valid_method, is_valid_reason_phrase,
    is_valid_request_target, is_valid_status_code,
};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// エンコード用のバージョン文字列バリデーション
///
/// VCHAR のみ (SP/CTL 禁止)。RTSP 等の非 HTTP プロトコルにも対応。
fn is_valid_version_for_encode(version: &str) -> bool {
    !version.is_empty() && version.bytes().all(|b| matches!(b, 0x21..=0x7E))
}

/// `usize` を 16 進数 ASCII (小文字) としてバッファに書き込む
///
/// `n == 0` を早期分岐で扱うのは、`leading_zeros` で桁数を計算する変種が
/// `n == 0` で誤桁数を返すのを避けるため。CRLF はヘルパー外で結合する。
fn write_hex_usize(buf: &mut Vec<u8>, n: usize) {
    if n == 0 {
        buf.push(b'0');
        return;
    }
    let mut tmp = [0u8; 16]; // 64bit usize の 16 進表記は最大 16 桁
    let mut i = tmp.len();
    let mut remaining = n;
    while remaining > 0 {
        i -= 1;
        let nibble = (remaining & 0xF) as u8;
        tmp[i] = if nibble < 10 {
            b'0' + nibble
        } else {
            b'a' + nibble - 10
        };
        remaining >>= 4;
    }
    buf.extend_from_slice(&tmp[i..]);
}

/// `usize` を 10 進数 ASCII としてバッファに書き込む
fn write_usize_decimal(buf: &mut Vec<u8>, n: usize) {
    if n == 0 {
        buf.push(b'0');
        return;
    }
    let mut tmp = [0u8; 20]; // 64bit usize の 10 進表記は最大 20 桁
    let mut i = tmp.len();
    let mut remaining = n;
    while remaining > 0 {
        i -= 1;
        tmp[i] = b'0' + (remaining % 10) as u8;
        remaining /= 10;
    }
    buf.extend_from_slice(&tmp[i..]);
}

/// `encode_request` / `encode_response` の事前確保サイズ上限 (64 MB)
///
/// 攻撃者制御のヘッダー値で見積もりが膨張した場合に、
/// `Vec::with_capacity` の OOM abort で DoS を引き起こさないための防御線。
/// 上限を超えた場合は `Vec::new()` にフォールバックして既存挙動と同等にする。
const ENCODE_CAPACITY_LIMIT: usize = 64 * 1024 * 1024;

/// 自動付与される Content-Length 行の容量見積もり (固定値)
///
/// `"Content-Length: " (16) + usize 最大 20 桁 + CRLF (2) = 38`
/// 桁数の厳密計算は二度走査回避のために行わず、最悪ケースで過剰確保する。
const AUTO_CONTENT_LENGTH_CAPACITY: usize = 38;

/// `encode_request` で Content-Length を自動付与するか判定
fn should_auto_emit_content_length_for_request(request: &Request) -> bool {
    request.body.is_some()
        && !request.has_header("Content-Length")
        && !request.has_header("Transfer-Encoding")
}

/// ステータスコードがボディを持ちうるか判定
///
/// RFC 9110 Section 6.4.1: 1xx / 204 / 304 はボディを含めてはならない
fn response_status_has_body(status_code: u16) -> bool {
    !((100..200).contains(&status_code) || status_code == 204 || status_code == 304)
}

/// `encode_response` で Content-Length を自動付与するか判定
fn should_auto_emit_content_length_for_response(response: &Response) -> bool {
    let status_has_body = response_status_has_body(response.status_code);
    let body_len = response.body.as_deref().map(<[u8]>::len);
    status_has_body
        && !response.has_header("Content-Length")
        && !response.has_header("Transfer-Encoding")
        && match (response.omit_body, body_len) {
            (_, None) => false,
            (true, Some(0)) => false,
            (_, Some(_)) => true,
        }
}

/// `encode_request` の出力容量を `checked_add` で見積もる
///
/// オーバーフロー時は `None` を返し、呼び出し側は `Vec::new()` にフォールバックする
fn estimate_request_capacity(request: &Request) -> Option<usize> {
    let mut total: usize = 0;
    // Request line: METHOD SP URI SP VERSION CRLF (固定 4: SP + SP + CRLF)
    total = total.checked_add(request.method.len())?;
    total = total.checked_add(request.uri.len())?;
    total = total.checked_add(request.version.len())?;
    total = total.checked_add(4)?;
    // 各ヘッダー: name + ": " + value + CRLF (固定 4)
    for (name, value) in &request.headers {
        total = total.checked_add(name.len())?;
        total = total.checked_add(value.len())?;
        total = total.checked_add(4)?;
    }
    if should_auto_emit_content_length_for_request(request) {
        total = total.checked_add(AUTO_CONTENT_LENGTH_CAPACITY)?;
    }
    // End-of-headers CRLF
    total = total.checked_add(2)?;
    if let Some(body) = request.body.as_deref() {
        total = total.checked_add(body.len())?;
    }
    Some(total)
}

/// `encode_response` の出力容量を `checked_add` で見積もる
///
/// オーバーフロー時は `None` を返し、呼び出し側は `Vec::new()` にフォールバックする
fn estimate_response_capacity(response: &Response) -> Option<usize> {
    let mut total: usize = 0;
    // Status line: VERSION SP STATUS-CODE SP REASON CRLF
    // (固定 4: SP + SP + CRLF, 加えて status code は 3 桁固定で見積もる)
    total = total.checked_add(response.version.len())?;
    total = total.checked_add(3)?; // status_code 最大桁数 (validate_response_fields で 100..=599 が保証)
    total = total.checked_add(response.reason_phrase.len())?;
    total = total.checked_add(4)?;
    for (name, value) in &response.headers {
        total = total.checked_add(name.len())?;
        total = total.checked_add(value.len())?;
        total = total.checked_add(4)?;
    }
    if should_auto_emit_content_length_for_response(response) {
        total = total.checked_add(AUTO_CONTENT_LENGTH_CAPACITY)?;
    }
    total = total.checked_add(2)?;
    let body_will_be_encoded =
        response_status_has_body(response.status_code) && !response.omit_body;
    if body_will_be_encoded && let Some(body) = response.body.as_deref() {
        total = total.checked_add(body.len())?;
    }
    Some(total)
}

/// 容量見積もりを `ENCODE_CAPACITY_LIMIT` で頭打ちにし、`Vec` を確保する
///
/// オーバーフロー (`None`) または上限超過のときは `Vec::new()` を返す。
/// `Vec::with_capacity` の内部 `alloc` は fallible でないため、
/// 巨大値の見積もりが abort/panic につながらないように防御する。
fn allocate_encode_buffer(estimated: Option<usize>) -> Vec<u8> {
    match estimated {
        Some(c) if c <= ENCODE_CAPACITY_LIMIT => Vec::with_capacity(c),
        _ => Vec::new(),
    }
}

/// リクエストフィールドのバリデーション
fn validate_request_fields(request: &Request) -> Result<(), EncodeError> {
    // メソッドの検証
    if !is_valid_method(&request.method) {
        return Err(EncodeError::InvalidMethod {
            method: request.method.clone(),
        });
    }

    // リクエストターゲットの検証
    if !is_valid_request_target(&request.uri) {
        return Err(EncodeError::InvalidRequestTarget {
            uri: request.uri.clone(),
        });
    }

    // is_valid_request_target() は受信側の寛容な検証で obs-text を許容する。
    // 送信側では新規に obs-text を生成してはならないため、ここで拒否する。
    if request.uri.bytes().any(|b| b > 0x7E) {
        return Err(EncodeError::InvalidRequestTarget {
            uri: request.uri.clone(),
        });
    }

    // RFC 9112 Section 3.2: メソッドと request-target 形式の整合性を検証
    validate_request_target_form(&request.method, &request.uri)?;

    // バージョンの検証
    if !is_valid_version_for_encode(&request.version) {
        return Err(EncodeError::InvalidVersion {
            version: request.version.clone(),
        });
    }

    // ヘッダーの検証
    validate_headers(&request.headers)?;

    Ok(())
}

/// request-target の形式を判定
///
/// RFC 9112 Section 3.2:
/// - origin-form: "/" で始まる (例: /path?query)
/// - absolute-form: absolute-URI (例: http://host/path, urn:isbn:0451450523)
/// - authority-form: uri-host ":" port (例: host:port)
/// - asterisk-form: "*" のみ
///
/// authority-form と "://" なし absolute-form は文法的に曖昧なため、
/// デコーダー (decoder/body.rs) と同じ順序で判定する:
/// 1. port が数値の host:port → authority-form
/// 2. 有効なスキームが検出 → absolute-form
fn detect_request_target_form(uri: &str) -> Result<RequestTargetForm, EncodeError> {
    if uri == "*" {
        Ok(RequestTargetForm::Asterisk)
    } else if uri.contains("://") {
        Ok(RequestTargetForm::Absolute)
    } else if uri.starts_with('/') {
        Ok(RequestTargetForm::Origin)
    } else if looks_like_authority_form(uri) {
        Ok(RequestTargetForm::Authority)
    } else if detect_scheme(uri).is_some() {
        // "://" を含まない absolute-URI (例: urn:isbn:0451450523)
        Ok(RequestTargetForm::Absolute)
    } else {
        Err(EncodeError::InvalidRequestTarget {
            uri: uri.to_string(),
        })
    }
}

/// authority-form かどうかの簡易判定
///
/// authority-form = uri-host ":" port (RFC 9112 Section 3.2.3)
/// uri-host = host (RFC 3986 Section 3.2.2)
///
/// ホスト部分に userinfo 区切りの "@" が含まれる場合は authority-form ではない。
/// RFC 9110 Section 9.3.6: "consisting of only the host and port number"
fn looks_like_authority_form(uri: &str) -> bool {
    // userinfo は authority-form に含まれない
    if uri.contains('@') {
        return false;
    }
    if let Some(colon_pos) = uri.rfind(':') {
        let port_str = &uri[colon_pos + 1..];
        let host = &uri[..colon_pos];
        !port_str.is_empty()
            && port_str.bytes().all(|b| b.is_ascii_digit())
            && port_str.parse::<u16>().is_ok()
            && !host.is_empty()
    } else {
        false
    }
}

/// authority-form の host を検証 (エンコーダー用)
///
/// RFC 9112 Section 3.2.3: authority-form = uri-host ":" port
/// デコーダー (decoder/body.rs) と同等の Host::parse による host 検証を行う
fn validate_encoder_authority_form(uri: &str) -> Result<(), EncodeError> {
    if let Some(colon_pos) = uri.rfind(':') {
        let host = &uri[..colon_pos];
        Host::parse(host).map_err(|_| EncodeError::InvalidRequestTarget {
            uri: uri.to_string(),
        })?;
    }
    Ok(())
}

/// スキームを検出する (RFC 3986 Section 3.1)
///
/// scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )
/// 先頭が有効なスキーム + ":" であればスキームの長さを返す
fn detect_scheme(target: &str) -> Option<usize> {
    let bytes = target.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return None;
    }
    let colon_pos = bytes.iter().position(|&b| b == b':')?;
    if colon_pos == 0 {
        return None;
    }
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

/// RFC 9112 Section 3.2: メソッドと request-target 形式の整合性を検証
///
/// - CONNECT は authority-form のみ (Section 3.2.3)
/// - asterisk-form ("*") は OPTIONS のみ (Section 3.2.4)
/// - その他のメソッドは origin-form または absolute-form (Section 3.2.1, 3.2.2)
fn validate_request_target_form(method: &str, uri: &str) -> Result<(), EncodeError> {
    let form = detect_request_target_form(uri)?;

    // RFC 9110 Section 9.1: メソッドトークンは case-sensitive
    match (method, &form) {
        // CONNECT は authority-form のみ (RFC 9112 Section 3.2.3)
        ("CONNECT", RequestTargetForm::Authority) => {
            // RFC 3986: authority-form の host を検証
            validate_encoder_authority_form(uri)?;
            Ok(())
        }
        ("CONNECT", _) => Err(EncodeError::InvalidRequestTargetForm {
            method: method.to_string(),
            uri: uri.to_string(),
        }),
        // asterisk-form は OPTIONS のみ (RFC 9112 Section 3.2.4)
        (_, RequestTargetForm::Asterisk) => {
            if method == "OPTIONS" {
                Ok(())
            } else {
                Err(EncodeError::InvalidRequestTargetForm {
                    method: method.to_string(),
                    uri: uri.to_string(),
                })
            }
        }
        // CONNECT 以外が authority-form を使うのは不正
        (_, RequestTargetForm::Authority) => Err(EncodeError::InvalidRequestTargetForm {
            method: method.to_string(),
            uri: uri.to_string(),
        }),
        // origin-form: path と query の文字を検証
        (_, RequestTargetForm::Origin) => {
            // RFC 3986 Section 3.3/3.4: origin-form では "[" "]" は pchar に含まれない
            // (authority 内の IP-literal でのみ合法)
            if uri.bytes().any(|b| b == b'[' || b == b']') {
                return Err(EncodeError::InvalidRequestTarget {
                    uri: uri.to_string(),
                });
            }
            Ok(())
        }
        // absolute-form: http/https は "://" 必須 (RFC 9110 Section 4.2)
        (_, RequestTargetForm::Absolute) => {
            reject_http_without_authority_prefix(uri)?;
            Ok(())
        }
    }
}

/// レスポンスフィールドのバリデーション
fn validate_response_fields(response: &Response) -> Result<(), EncodeError> {
    // バージョンの検証
    if !is_valid_version_for_encode(&response.version) {
        return Err(EncodeError::InvalidVersion {
            version: response.version.clone(),
        });
    }

    // ステータスコードの検証
    if !is_valid_status_code(response.status_code) {
        return Err(EncodeError::InvalidStatusCode {
            code: response.status_code,
        });
    }

    // reason-phrase の検証
    if !is_valid_reason_phrase(&response.reason_phrase) {
        return Err(EncodeError::InvalidReasonPhrase {
            phrase: response.reason_phrase.clone(),
        });
    }

    // ヘッダーの検証
    validate_headers(&response.headers)?;

    Ok(())
}

/// ヘッダー名と値のバリデーション
fn validate_headers(headers: &[(String, String)]) -> Result<(), EncodeError> {
    for (name, value) in headers {
        if !is_valid_header_name(name) {
            return Err(EncodeError::InvalidHeaderName { name: name.clone() });
        }
        if !is_valid_field_value(value) {
            return Err(EncodeError::InvalidHeaderValue {
                name: name.clone(),
                value: value.clone(),
            });
        }
    }
    Ok(())
}

/// Host ヘッダーの詳細バリデーション (リクエスト用)
///
/// RFC 9112 Section 3.2:
/// - HTTP/1.1 リクエストには Host ヘッダーが必須
/// - Host ヘッダーは重複してはならない
/// - Host ヘッダーの値は有効な authority でなければならない
fn validate_host_header(request: &Request) -> Result<(), EncodeError> {
    if request.version != "HTTP/1.1" {
        return Ok(());
    }

    let host_headers: Vec<&str> = request
        .headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("Host"))
        .map(|(_, value)| value.as_str())
        .collect();

    if host_headers.is_empty() {
        return Err(EncodeError::MissingHostHeader);
    }

    if host_headers.len() > 1 {
        return Err(EncodeError::DuplicateHostHeader);
    }

    let host_value = host_headers[0];
    // 空の Host ヘッダーは許可 (RFC 9112 Section 3.2: 空の field-value は許可)
    if !host_value.is_empty() && Host::parse(host_value).is_err() {
        return Err(EncodeError::InvalidHostHeader {
            value: host_value.to_string(),
        });
    }

    // RFC 9112 Section 3.2: absolute-form の場合、Host は authority と同一でなければならない (MUST)
    // authority が非空なら Host も非空かつ一致していなければならない
    if request.uri.contains("://")
        && let Some(authority) = extract_authority_from_uri(&request.uri)
        && !authority.is_empty()
        && !authority.eq_ignore_ascii_case(host_value)
    {
        return Err(EncodeError::HostAuthorityMismatch {
            host: host_value.to_string(),
            authority: authority.to_string(),
        });
    }

    // RFC 9112 Section 3.2: CONNECT (authority-form) の場合、
    // Host は request-target の authority と意味的に一致しなければならない (MUST)
    // RFC 9110 Section 7.2: Host = uri-host [ ":" port ] (port は省略可能)
    // RFC 9112 Section 3.2.3 / RFC 9110 Section 9.3.6 の例:
    //   CONNECT www.example.com:80 HTTP/1.1
    //   Host: www.example.com
    if request.method == "CONNECT"
        && let Some(colon_pos) = request.uri.rfind(':')
    {
        let target_host = &request.uri[..colon_pos];
        let target_port_str = &request.uri[colon_pos + 1..];

        // CONNECT の authority は常に存在するため、Host は非空でなければならない
        if host_value.is_empty() {
            return Err(EncodeError::HostAuthorityMismatch {
                host: host_value.to_string(),
                authority: request.uri.clone(),
            });
        }

        if let Ok(parsed_host) = Host::parse(host_value) {
            // host 部分の比較 (case-insensitive)
            if !parsed_host.host().eq_ignore_ascii_case(target_host) {
                return Err(EncodeError::HostAuthorityMismatch {
                    host: host_value.to_string(),
                    authority: request.uri.clone(),
                });
            }

            // Host にポートが指定されている場合、request-target のポートと一致するか確認
            if let Some(host_port) = parsed_host.port()
                && let Ok(target_port) = target_port_str.parse::<u16>()
                && host_port != target_port
            {
                return Err(EncodeError::HostAuthorityMismatch {
                    host: host_value.to_string(),
                    authority: request.uri.clone(),
                });
            }
        }
    }

    // RFC 9112 Section 3.2: authority がない target URI では Host を空にしなければならない (MUST)
    // "://" を含まない absolute-form (例: urn:isbn:xxx) が該当する
    if let Ok(RequestTargetForm::Absolute) = detect_request_target_form(&request.uri)
        && !request.uri.contains("://")
        && !host_value.is_empty()
    {
        return Err(EncodeError::NonEmptyHostWithoutAuthority {
            host: host_value.to_string(),
            uri: request.uri.clone(),
        });
    }

    Ok(())
}

/// RFC 9110 Section 4.2.4: http/https URI の userinfo を検出して拒否する
///
/// 送信者は http/https URI に userinfo を生成してはならない (MUST NOT)
/// 他のスキームには適用しない
fn reject_http_userinfo(uri: &str) -> Result<(), EncodeError> {
    let lower = uri.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Ok(());
    }
    let after_scheme = match uri.find("://") {
        Some(i) => &uri[i + 3..],
        None => return Ok(()),
    };
    let end = after_scheme.find(['/', '?']).unwrap_or(after_scheme.len());
    let authority = &after_scheme[..end];
    if authority.contains('@') {
        return Err(EncodeError::UserinfoInHttpUri {
            uri: uri.to_string(),
        });
    }
    Ok(())
}

/// RFC 9110 Section 4.2.1/4.2.2: http/https URI の空 host を検出して拒否する
///
/// 送信者は空 host 識別子を持つ http/https URI を生成してはならない (MUST NOT)
fn reject_http_empty_host(uri: &str) -> Result<(), EncodeError> {
    let lower = uri.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Ok(());
    }
    let after_scheme = match uri.find("://") {
        Some(i) => &uri[i + 3..],
        None => return Ok(()),
    };
    let end = after_scheme.find(['/', '?']).unwrap_or(after_scheme.len());
    let authority = &after_scheme[..end];
    // userinfo を除外して host 部分を取得
    let host_port = if let Some(at_pos) = authority.rfind('@') {
        &authority[at_pos + 1..]
    } else {
        authority
    };
    // host が空 (authority 自体が空、またはポートのみ)
    if host_port.is_empty() || host_port.starts_with(':') {
        return Err(EncodeError::EmptyHostInHttpUri {
            uri: uri.to_string(),
        });
    }
    Ok(())
}

/// RFC 9110 Section 4.2.1/4.2.2: http/https URI は "://" を含まなければならない
///
/// http-URI  = "http"  "://" authority path-abempty [ "?" query ]
/// https-URI = "https" "://" authority path-abempty [ "?" query ]
fn reject_http_without_authority_prefix(uri: &str) -> Result<(), EncodeError> {
    if let Some(colon_pos) = uri.find(':') {
        let scheme = &uri[..colon_pos];
        if (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
            && !uri[colon_pos..].starts_with("://")
        {
            return Err(EncodeError::InvalidRequestTarget {
                uri: uri.to_string(),
            });
        }
    }
    Ok(())
}

/// Content-Length ヘッダーの ABNF 検証 (RFC 9110 Section 8.6)
///
/// 全 Content-Length ヘッダーを走査し:
/// 1. 各値が 1*DIGIT であることを検証 → 違反なら InvalidContentLengthValue
/// 2. 複数ヘッダーの値が一致することを検証 → 不一致なら DuplicateContentLength
/// 3. 検証済みの値を Option<u64> で返す
fn validate_content_length_headers(
    headers: &[(String, String)],
) -> Result<Option<u64>, EncodeError> {
    let mut result: Option<u64> = None;

    for (name, value) in headers {
        if !name.eq_ignore_ascii_case("Content-Length") {
            continue;
        }
        let trimmed = value.trim();
        // RFC 9110 Section 8.6: Content-Length = 1*DIGIT
        if trimmed.is_empty() || !trimmed.bytes().all(|b| b.is_ascii_digit()) {
            return Err(EncodeError::InvalidContentLengthValue {
                value: value.clone(),
            });
        }
        let parsed =
            trimmed
                .parse::<u64>()
                .map_err(|_| EncodeError::InvalidContentLengthValue {
                    value: value.clone(),
                })?;

        match result {
            None => result = Some(parsed),
            Some(prev) if prev != parsed => {
                return Err(EncodeError::DuplicateContentLength);
            }
            Some(_) => {} // 同じ値なので OK
        }
    }

    Ok(result)
}

/// URI から authority 部分を抽出 (userinfo を除外)
///
/// scheme "://" authority ["/" path]
/// RFC 9112 Section 3.2: Host ヘッダーとの比較では userinfo を除外する
fn extract_authority_from_uri(uri: &str) -> Option<String> {
    let after_scheme = uri.find("://").map(|i| &uri[i + 3..])?;
    // authority は次の "/" または "?" または末尾まで
    let end = after_scheme.find(['/', '?']).unwrap_or(after_scheme.len());
    let authority = &after_scheme[..end];
    // userinfo を除外して host:port のみを返す
    let host_port = if let Some(at_pos) = authority.rfind('@') {
        &authority[at_pos + 1..]
    } else {
        authority
    };
    Some(host_port.to_string())
}

/// リクエストをエンコード
///
/// RFC 9112 Section 3.2: HTTP/1.1 リクエストには Host ヘッダーが必須
/// RFC 9112 Section 6.2: Transfer-Encoding と Content-Length は同時に送信してはならない
pub fn encode_request(request: &Request) -> Result<Vec<u8>, EncodeError> {
    // フィールドバリデーション
    validate_request_fields(request)?;

    // RFC 9110 Section 4.2.4: http/https URI の userinfo を拒否する
    reject_http_userinfo(&request.uri)?;

    // RFC 9110 Section 4.2.1/4.2.2: http/https URI の空 host を拒否する
    reject_http_empty_host(&request.uri)?;

    // Host ヘッダーの詳細バリデーション
    validate_host_header(request)?;

    // RFC 9112 Section 6.2: Transfer-Encoding と Content-Length の同時送信は禁止
    if request.has_header("Transfer-Encoding") && request.has_header("Content-Length") {
        return Err(EncodeError::ConflictingTransferEncodingAndContentLength);
    }

    // RFC 9110 Section 9.3.6: "A CONNECT request message does not have content."
    // ただし RFC は CONNECT リクエスト側に MUST NOT の制約を課していない。
    // CONNECT の意味論的制約の判断はアプリケーション層の責務とする。

    // Content-Length ヘッダーの ABNF 検証と body.len() との整合性を検証
    // body == None の場合は body 長 0 として扱う
    if !request.has_header("Transfer-Encoding")
        && let Some(header_value) = validate_content_length_headers(&request.headers)?
    {
        let body_length = request.body.as_deref().map(<[u8]>::len).unwrap_or(0) as u64;
        if header_value != body_length {
            return Err(EncodeError::ContentLengthMismatch {
                header_value,
                body_length,
            });
        }
    }

    let mut buf = allocate_encode_buffer(estimate_request_capacity(request));

    // Request line: METHOD SP URI SP VERSION CRLF
    buf.extend_from_slice(request.method.as_bytes());
    buf.push(b' ');
    buf.extend_from_slice(request.uri.as_bytes());
    buf.push(b' ');
    buf.extend_from_slice(request.version.as_bytes());
    buf.extend_from_slice(b"\r\n");

    // Headers
    for (name, value) in &request.headers {
        buf.extend_from_slice(name.as_bytes());
        buf.extend_from_slice(b": ");
        buf.extend_from_slice(value.as_bytes());
        buf.extend_from_slice(b"\r\n");
    }

    // Content-Length (body == Some の場合、Content-Length / Transfer-Encoding 未指定なら自動付与)
    // RFC 9110 Section 8.6: メソッド意味論で content が想定されるかは呼び出し側の判断とする。
    // body == Some(vec![]) なら Content-Length: 0、body == None なら自動付与しない。
    if let Some(body) = request.body.as_deref()
        && !request.has_header("Content-Length")
        && !request.has_header("Transfer-Encoding")
    {
        buf.extend_from_slice(b"Content-Length: ");
        write_usize_decimal(&mut buf, body.len());
        buf.extend_from_slice(b"\r\n");
    }

    // End of headers
    buf.extend_from_slice(b"\r\n");

    // Body
    if let Some(body) = request.body.as_deref() {
        buf.extend_from_slice(body);
    }

    Ok(buf)
}

/// レスポンスをエンコード
///
/// RFC 9112 Section 6.1: 1xx / 204 レスポンスに Transfer-Encoding を含めてはならない
/// RFC 9112 Section 6.2: Transfer-Encoding と Content-Length は同時に送信してはならない
///
/// # CONNECT 2xx レスポンスについて
///
/// RFC 9112 Section 6.1 / RFC 9110 Section 8.6 により、CONNECT リクエストへの
/// 2xx レスポンスには Transfer-Encoding / Content-Length を含めてはならない (MUST NOT)。
/// しかし、エンコーダーはリクエストメソッドの情報を持たないため、この制約は
/// 呼び出し側アプリケーションの責務とする。
pub fn encode_response(response: &Response) -> Result<Vec<u8>, EncodeError> {
    // フィールドバリデーション
    validate_response_fields(response)?;

    // RFC 9112 Section 6.2: Transfer-Encoding と Content-Length の同時送信は禁止
    if response.has_header("Transfer-Encoding") && response.has_header("Content-Length") {
        return Err(EncodeError::ConflictingTransferEncodingAndContentLength);
    }

    // RFC 9112 Section 6.1: 1xx / 204 レスポンスに Transfer-Encoding は禁止
    let is_1xx_or_204 = (100..200).contains(&response.status_code) || response.status_code == 204;
    if is_1xx_or_204 && response.has_header("Transfer-Encoding") {
        return Err(EncodeError::ForbiddenTransferEncoding {
            status_code: response.status_code,
        });
    }

    // RFC 9110 Section 8.6: 1xx / 204 レスポンスに Content-Length は禁止
    if is_1xx_or_204 && response.has_header("Content-Length") {
        return Err(EncodeError::ForbiddenContentLength {
            status_code: response.status_code,
        });
    }

    // RFC 9110 Section 15.3.6: 205 Reset Content はボディを生成してはならない
    // body == Some(non-empty) のときのみ違反。Some(vec![]) と None は許容する。
    if response.status_code == 205 {
        if response.body.as_deref().is_some_and(|b| !b.is_empty()) {
            return Err(EncodeError::ForbiddenBodyFor205);
        }
        if response.has_header("Transfer-Encoding") {
            return Err(EncodeError::ForbiddenTransferEncoding { status_code: 205 });
        }
        // RFC 9110 Section 8.6: 205 の Content-Length は 0 のみ許可
        if let Some(cl) = response.get_header("Content-Length")
            && cl.trim() != "0"
        {
            return Err(EncodeError::ForbiddenContentLength { status_code: 205 });
        }
    }

    let body_will_be_encoded =
        response_status_has_body(response.status_code) && !response.omit_body;

    // Content-Length ヘッダーの ABNF 検証と body.len() との整合性を検証
    // - 通常レスポンス: 常に一致必須
    // - omit_body: true の場合は、body が空 (None または Some(vec![])) のときのみ検証をスキップ
    //   (HEAD レスポンスで Content-Length が表現長を示すケース)
    // - 1xx/204/304 は message body がないため、ここでは検証しない
    // body == None は body 長 0 として扱う
    if response_status_has_body(response.status_code)
        && !response.has_header("Transfer-Encoding")
        && let Some(header_value) = validate_content_length_headers(&response.headers)?
    {
        let body_length = response.body.as_deref().map(<[u8]>::len).unwrap_or(0) as u64;
        let should_validate = body_will_be_encoded || body_length != 0;
        if should_validate && header_value != body_length {
            return Err(EncodeError::ContentLengthMismatch {
                header_value,
                body_length,
            });
        }
    }

    let mut buf = allocate_encode_buffer(estimate_response_capacity(response));

    // Status line: VERSION SP STATUS-CODE SP REASON-PHRASE CRLF
    buf.extend_from_slice(response.version.as_bytes());
    buf.push(b' ');
    write_usize_decimal(&mut buf, response.status_code as usize);
    buf.push(b' ');
    buf.extend_from_slice(response.reason_phrase.as_bytes());
    buf.extend_from_slice(b"\r\n");

    // Headers
    for (name, value) in &response.headers {
        buf.extend_from_slice(name.as_bytes());
        buf.extend_from_slice(b": ");
        buf.extend_from_slice(value.as_bytes());
        buf.extend_from_slice(b"\r\n");
    }

    // Content-Length 自動付与
    // RFC 9112: keep-alive を維持するために Content-Length または Transfer-Encoding が必要
    // 1xx/204/304 はボディがないため Content-Length を追加しない
    // 205 は RFC 9110 Section 15.3.6 でボディ生成禁止だが、受信者のメッセージ長決定規則のため
    // body が Some(vec![]) なら Content-Length: 0 を付与する (close-delimited にならないようにする)
    // body == None ならボディ長を表現できないため自動付与しない
    // omit_body: true かつ body が空 (None または Some(vec![])) の場合も自動付与しない
    // (HEAD レスポンスで表現長が不明なケースに配慮)
    // 容量見積もりと判定ロジックを統一するため、`should_auto_emit_content_length_for_response`
    // を介して判定する (条件がずれると過小確保で再確保が発生する)
    if should_auto_emit_content_length_for_response(response) {
        let len = response.body.as_deref().map(<[u8]>::len).unwrap_or(0);
        buf.extend_from_slice(b"Content-Length: ");
        write_usize_decimal(&mut buf, len);
        buf.extend_from_slice(b"\r\n");
    }

    // End of headers
    buf.extend_from_slice(b"\r\n");

    // Body
    // RFC 9110 Section 6.4.1: 1xx/204/304 はボディを含めてはならない
    // HEAD レスポンスでは omit_body: true としてボディ送信を抑止する
    if body_will_be_encoded && let Some(body) = response.body.as_deref() {
        buf.extend_from_slice(body);
    }

    Ok(buf)
}

impl Request {
    /// リクエストをバイト列にエンコード
    ///
    /// HTTP/1.1 リクエストで Host ヘッダーがない場合はパニックする。
    /// エラーハンドリングが必要な場合は `try_encode()` を使用する。
    pub fn encode(&self) -> Vec<u8> {
        encode_request(self).expect("HTTP/1.1 request requires Host header")
    }

    /// リクエストをバイト列にエンコード (Result 版)
    ///
    /// RFC 9112 Section 3.2: HTTP/1.1 リクエストには Host ヘッダーが必須
    pub fn try_encode(&self) -> Result<Vec<u8>, EncodeError> {
        encode_request(self)
    }
}

impl Response {
    /// レスポンスをバイト列にエンコード
    ///
    /// RFC 違反のヘッダー組み合わせがある場合はパニックする。
    /// エラーハンドリングが必要な場合は `try_encode()` を使用する。
    pub fn encode(&self) -> Vec<u8> {
        encode_response(self).expect("invalid header combination")
    }

    /// レスポンスをバイト列にエンコード (Result 版)
    ///
    /// RFC 9112 Section 6.1: 1xx / 204 レスポンスに Transfer-Encoding を含めてはならない
    /// RFC 9112 Section 6.2: Transfer-Encoding と Content-Length は同時に送信してはならない
    pub fn try_encode(&self) -> Result<Vec<u8>, EncodeError> {
        encode_response(self)
    }
}

/// Chunked Transfer Encoding 用のチャンクをエンコード
///
/// データを HTTP chunked 形式にエンコードします。
/// 空のデータを渡すと終端チャンク (0\r\n\r\n) を生成します。
pub fn encode_chunk(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        // 終端チャンク
        let mut buf = Vec::with_capacity(5);
        buf.extend_from_slice(b"0\r\n\r\n");
        return buf;
    }

    // 1 チャンクは `hex(最大 16) + CRLF(2) + data + CRLF(2) = data.len() + 20` バイト
    // 攻撃者制御の `data.len()` で wraparound を起こさないよう checked_add でフォールバック
    let cap = data.len().checked_add(20);
    let mut buf = match cap {
        Some(c) => Vec::with_capacity(c),
        None => Vec::new(),
    };

    // チャンクサイズ (16進数)
    write_hex_usize(&mut buf, data.len());
    buf.extend_from_slice(b"\r\n");
    // チャンクデータ
    buf.extend_from_slice(data);
    // CRLF
    buf.extend_from_slice(b"\r\n");

    buf
}

/// 複数のデータを chunked 形式でエンコード
///
/// すべてのチャンクを結合し、終端チャンクも追加します。
pub fn encode_chunks(chunks: &[&[u8]]) -> Vec<u8> {
    // 容量見積もり: 各チャンク `data.len() + 20` の総和 + 終端チャンク 5 バイト
    // 攻撃者制御のサイズで wraparound しないよう checked_add でフォールバックする
    let cap = encode_chunks_capacity(chunks);
    let mut buf = match cap {
        Some(c) => Vec::with_capacity(c),
        None => Vec::new(),
    };

    for chunk in chunks {
        write_hex_usize(&mut buf, chunk.len());
        buf.extend_from_slice(b"\r\n");
        buf.extend_from_slice(chunk);
        buf.extend_from_slice(b"\r\n");
    }

    // 終端チャンク
    buf.extend_from_slice(b"0\r\n\r\n");

    buf
}

/// `encode_chunks` の出力容量を `checked_add` で見積もる
/// オーバーフロー時は `None` を返し、呼び出し側は `Vec::new()` にフォールバックする
fn encode_chunks_capacity(chunks: &[&[u8]]) -> Option<usize> {
    let mut total: usize = 0;
    for chunk in chunks {
        let per = chunk.len().checked_add(20)?;
        total = total.checked_add(per)?;
    }
    total.checked_add(5)
}

/// リクエストヘッダーのみをエンコード (ボディなし)
///
/// Chunked Transfer Encoding を使う場合に便利です。
/// ヘッダー送信後に `encode_chunk` でボディを送信できます。
///
/// RFC 9112 Section 3.2: HTTP/1.1 リクエストには Host ヘッダーが必須
/// RFC 9112 Section 6.2: Transfer-Encoding と Content-Length は同時に送信してはならない
pub fn encode_request_headers(request: &Request) -> Result<Vec<u8>, EncodeError> {
    // フィールドバリデーション
    validate_request_fields(request)?;

    // RFC 9110 Section 4.2.4: http/https URI の userinfo を拒否する
    reject_http_userinfo(&request.uri)?;

    // RFC 9110 Section 4.2.1/4.2.2: http/https URI の空 host を拒否する
    reject_http_empty_host(&request.uri)?;

    // Host ヘッダーの詳細バリデーション
    validate_host_header(request)?;

    // RFC 9112 Section 6.2: Transfer-Encoding と Content-Length の同時送信は禁止
    if request.has_header("Transfer-Encoding") && request.has_header("Content-Length") {
        return Err(EncodeError::ConflictingTransferEncodingAndContentLength);
    }

    // RFC 9110 Section 9.3.6: "A CONNECT request message does not have content."
    // RFC は CONNECT リクエスト側に Content-Length / Transfer-Encoding を MUST NOT とはしていない。
    // encode_request_headers はボディを扱わないため、CONNECT 専用チェックは不要。
    // ヘッダーの有無による制約はアプリケーション層の責務とする。

    let mut buf = Vec::new();

    // Request line: METHOD SP URI SP VERSION CRLF
    buf.extend_from_slice(request.method.as_bytes());
    buf.push(b' ');
    buf.extend_from_slice(request.uri.as_bytes());
    buf.push(b' ');
    buf.extend_from_slice(request.version.as_bytes());
    buf.extend_from_slice(b"\r\n");

    // Headers
    for (name, value) in &request.headers {
        buf.extend_from_slice(name.as_bytes());
        buf.extend_from_slice(b": ");
        buf.extend_from_slice(value.as_bytes());
        buf.extend_from_slice(b"\r\n");
    }

    // End of headers
    buf.extend_from_slice(b"\r\n");

    Ok(buf)
}

/// レスポンスヘッダーのみをエンコード (ボディなし)
///
/// Chunked Transfer Encoding を使う場合に便利です。
/// ヘッダー送信後に `encode_chunk` でボディを送信できます。
///
/// RFC 9112 Section 6.1: 1xx / 204 レスポンスに Transfer-Encoding を含めてはならない
/// RFC 9112 Section 6.2: Transfer-Encoding と Content-Length は同時に送信してはならない
///
/// # CONNECT 2xx レスポンスについて
///
/// RFC 9112 Section 6.1 / RFC 9110 Section 8.6 により、CONNECT リクエストへの
/// 2xx レスポンスには Transfer-Encoding / Content-Length を含めてはならない (MUST NOT)。
/// しかし、エンコーダーはリクエストメソッドの情報を持たないため、この制約は
/// 呼び出し側アプリケーションの責務とする。
pub fn encode_response_headers(response: &Response) -> Result<Vec<u8>, EncodeError> {
    // フィールドバリデーション
    validate_response_fields(response)?;

    // RFC 9112 Section 6.2: Transfer-Encoding と Content-Length の同時送信は禁止
    if response.has_header("Transfer-Encoding") && response.has_header("Content-Length") {
        return Err(EncodeError::ConflictingTransferEncodingAndContentLength);
    }

    // RFC 9112 Section 6.1: 1xx / 204 レスポンスに Transfer-Encoding は禁止
    let is_1xx_or_204 = (100..200).contains(&response.status_code) || response.status_code == 204;
    if is_1xx_or_204 && response.has_header("Transfer-Encoding") {
        return Err(EncodeError::ForbiddenTransferEncoding {
            status_code: response.status_code,
        });
    }

    // RFC 9110 Section 8.6: 1xx / 204 レスポンスに Content-Length は禁止
    if is_1xx_or_204 && response.has_header("Content-Length") {
        return Err(EncodeError::ForbiddenContentLength {
            status_code: response.status_code,
        });
    }

    // RFC 9110 Section 15.3.6: 205 Reset Content の Transfer-Encoding 禁止
    if response.status_code == 205 && response.has_header("Transfer-Encoding") {
        return Err(EncodeError::ForbiddenTransferEncoding { status_code: 205 });
    }

    // RFC 9110 Section 8.6: 205 の Content-Length は 0 のみ許可
    if response.status_code == 205
        && let Some(cl) = response.get_header("Content-Length")
        && cl.trim() != "0"
    {
        return Err(EncodeError::ForbiddenContentLength { status_code: 205 });
    }

    let mut buf = Vec::new();

    // Status line: VERSION SP STATUS-CODE SP REASON-PHRASE CRLF
    buf.extend_from_slice(response.version.as_bytes());
    buf.push(b' ');
    write_usize_decimal(&mut buf, response.status_code as usize);
    buf.push(b' ');
    buf.extend_from_slice(response.reason_phrase.as_bytes());
    buf.extend_from_slice(b"\r\n");

    // Headers
    for (name, value) in &response.headers {
        buf.extend_from_slice(name.as_bytes());
        buf.extend_from_slice(b": ");
        buf.extend_from_slice(value.as_bytes());
        buf.extend_from_slice(b"\r\n");
    }

    // End of headers
    buf.extend_from_slice(b"\r\n");

    Ok(buf)
}

impl Request {
    /// ヘッダーのみをエンコード (Chunked Transfer Encoding 用)
    ///
    /// HTTP/1.1 リクエストで Host ヘッダーがない場合はパニックする。
    /// エラーハンドリングが必要な場合は `try_encode_headers()` を使用する。
    pub fn encode_headers(&self) -> Vec<u8> {
        encode_request_headers(self).expect("HTTP/1.1 request requires Host header")
    }

    /// ヘッダーのみをエンコード (Result 版)
    ///
    /// RFC 9112 Section 3.2: HTTP/1.1 リクエストには Host ヘッダーが必須
    pub fn try_encode_headers(&self) -> Result<Vec<u8>, EncodeError> {
        encode_request_headers(self)
    }
}

impl Response {
    /// ヘッダーのみをエンコード (Chunked Transfer Encoding 用)
    ///
    /// RFC 違反のヘッダー組み合わせがある場合はパニックする。
    /// エラーハンドリングが必要な場合は `try_encode_headers()` を使用する。
    pub fn encode_headers(&self) -> Vec<u8> {
        encode_response_headers(self).expect("invalid header combination")
    }

    /// ヘッダーのみをエンコード (Result 版)
    ///
    /// RFC 9112 Section 6.1: 1xx / 204 レスポンスに Transfer-Encoding を含めてはならない
    /// RFC 9112 Section 6.2: Transfer-Encoding と Content-Length は同時に送信してはならない
    pub fn try_encode_headers(&self) -> Result<Vec<u8>, EncodeError> {
        encode_response_headers(self)
    }
}

/// レスポンスエンコーダー (圧縮対応)
///
/// # 型パラメータ
///
/// - `C`: 圧縮器の型。デフォルトは `NoCompression`（圧縮なし）。
///
/// # 使い方
///
/// ## 圧縮なし（既存 API 互換）
///
/// ```rust
/// use shiguredo_http11::ResponseEncoder;
///
/// let encoder = ResponseEncoder::new();
/// ```
///
/// ## 圧縮あり
///
/// ```ignore
/// use shiguredo_http11::ResponseEncoder;
///
/// let mut encoder = ResponseEncoder::with_compressor(GzipCompressor::new());
/// let mut output = vec![0u8; 8192];
/// let status = encoder.compress_body(body_data, &mut output)?;
/// // output[..status.produced()] に圧縮データ
/// ```
#[derive(Debug)]
pub struct ResponseEncoder<C: Compressor = NoCompression> {
    compressor: C,
}

impl Default for ResponseEncoder<NoCompression> {
    fn default() -> Self {
        Self::new()
    }
}

impl ResponseEncoder<NoCompression> {
    /// 新しいエンコーダーを作成
    pub fn new() -> Self {
        Self {
            compressor: NoCompression::new(),
        }
    }
}

impl<C: Compressor> ResponseEncoder<C> {
    /// 圧縮器付きでエンコーダーを作成
    pub fn with_compressor(compressor: C) -> Self {
        Self { compressor }
    }

    /// ボディを圧縮（ストリーミング）
    ///
    /// # 引数
    /// - `input`: 圧縮する入力データ
    /// - `output`: 圧縮データを書き込む出力バッファ
    ///
    /// # 戻り値
    /// - `Continue`: 処理継続中、入力がすべて消費された
    /// - `OutputFull`: 出力バッファが満杯、再度呼び出す必要あり
    pub fn compress_body(
        &mut self,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<CompressionStatus, CompressionError> {
        self.compressor.compress(input, output)
    }

    /// 圧縮を終了
    ///
    /// 残りの圧縮データをフラッシュする。
    ///
    /// # 引数
    /// - `output`: 残りの圧縮データを書き込む出力バッファ
    ///
    /// # 戻り値
    /// - `Complete`: 圧縮完了
    /// - `OutputFull`: 出力バッファが満杯、再度呼び出す必要あり
    pub fn finish(&mut self, output: &mut [u8]) -> Result<CompressionStatus, CompressionError> {
        self.compressor.finish(output)
    }

    /// 圧縮器をリセット
    pub fn reset(&mut self) {
        self.compressor.reset();
    }
}

/// リクエストエンコーダー (圧縮対応)
///
/// # 型パラメータ
///
/// - `C`: 圧縮器の型。デフォルトは `NoCompression`（圧縮なし）。
///
/// # 使い方
///
/// ## 圧縮なし（既存 API 互換）
///
/// ```rust
/// use shiguredo_http11::RequestEncoder;
///
/// let encoder = RequestEncoder::new();
/// ```
///
/// ## 圧縮あり
///
/// ```ignore
/// use shiguredo_http11::RequestEncoder;
///
/// let mut encoder = RequestEncoder::with_compressor(GzipCompressor::new());
/// let mut output = vec![0u8; 8192];
/// let status = encoder.compress_body(body_data, &mut output)?;
/// // output[..status.produced()] に圧縮データ
/// ```
#[derive(Debug)]
pub struct RequestEncoder<C: Compressor = NoCompression> {
    compressor: C,
}

impl Default for RequestEncoder<NoCompression> {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestEncoder<NoCompression> {
    /// 新しいエンコーダーを作成
    pub fn new() -> Self {
        Self {
            compressor: NoCompression::new(),
        }
    }
}

impl<C: Compressor> RequestEncoder<C> {
    /// 圧縮器付きでエンコーダーを作成
    pub fn with_compressor(compressor: C) -> Self {
        Self { compressor }
    }

    /// ボディを圧縮（ストリーミング）
    ///
    /// # 引数
    /// - `input`: 圧縮する入力データ
    /// - `output`: 圧縮データを書き込む出力バッファ
    ///
    /// # 戻り値
    /// - `Continue`: 処理継続中、入力がすべて消費された
    /// - `OutputFull`: 出力バッファが満杯、再度呼び出す必要あり
    pub fn compress_body(
        &mut self,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<CompressionStatus, CompressionError> {
        self.compressor.compress(input, output)
    }

    /// 圧縮を終了
    ///
    /// 残りの圧縮データをフラッシュする。
    ///
    /// # 引数
    /// - `output`: 残りの圧縮データを書き込む出力バッファ
    ///
    /// # 戻り値
    /// - `Complete`: 圧縮完了
    /// - `OutputFull`: 出力バッファが満杯、再度呼び出す必要あり
    pub fn finish(&mut self, output: &mut [u8]) -> Result<CompressionStatus, CompressionError> {
        self.compressor.finish(output)
    }

    /// 圧縮器をリセット
    pub fn reset(&mut self) {
        self.compressor.reset();
    }
}

/// 容量見積もりが実際の出力長以上であることを検証する内部テスト
///
/// `estimate_request_capacity` / `estimate_response_capacity` はプライベート関数なので
/// 外部テストから直接呼べない。このモジュールに置くことで容量見積もりの正しさを
/// 確実に担保する (過小確保で再確保が起きると最適化の意味を失うため)。
#[cfg(test)]
mod capacity_tests {
    use super::*;
    use crate::request::Request;
    use crate::response::Response;

    fn assert_request_capacity_sufficient(req: &Request) {
        let est = estimate_request_capacity(req).expect("estimate overflow");
        let out = encode_request(req).expect("encode failed");
        assert!(
            est >= out.len(),
            "estimate {} < output {}: req={req:?}",
            est,
            out.len(),
        );
    }

    fn assert_response_capacity_sufficient(res: &Response) {
        let est = estimate_response_capacity(res).expect("estimate overflow");
        let out = encode_response(res).expect("encode failed");
        assert!(
            est >= out.len(),
            "estimate {} < output {}: res={res:?}",
            est,
            out.len(),
        );
    }

    #[test]
    fn test_request_capacity_simple_get() {
        let req = Request::new("GET", "/").header("Host", "example.com");
        assert_request_capacity_sufficient(&req);
    }

    #[test]
    fn test_request_capacity_post_with_body_auto_content_length() {
        let req = Request::new("POST", "/api")
            .header("Host", "example.com")
            .body(b"hello world".to_vec());
        assert_request_capacity_sufficient(&req);
    }

    #[test]
    fn test_request_capacity_post_with_explicit_content_length() {
        let req = Request::new("POST", "/api")
            .header("Host", "example.com")
            .header("Content-Length", "11")
            .body(b"hello world".to_vec());
        assert_request_capacity_sufficient(&req);
    }

    #[test]
    fn test_request_capacity_post_with_transfer_encoding_no_auto() {
        let req = Request::new("POST", "/api")
            .header("Host", "example.com")
            .header("Transfer-Encoding", "chunked")
            .body(b"hello".to_vec());
        assert_request_capacity_sufficient(&req);
    }

    #[test]
    fn test_request_capacity_many_headers() {
        let mut req = Request::new("GET", "/").header("Host", "example.com");
        for i in 0..50 {
            req = req.header(
                &alloc::format!("X-Custom-{i}"),
                &alloc::format!("value-{i}-with-some-padding"),
            );
        }
        assert_request_capacity_sufficient(&req);
    }

    #[test]
    fn test_request_capacity_empty_body_auto_content_length_zero() {
        let req = Request::new("POST", "/")
            .header("Host", "example.com")
            .body(Vec::new());
        assert_request_capacity_sufficient(&req);
    }

    #[test]
    fn test_request_capacity_no_body() {
        let req = Request::new("GET", "/path/to/resource?q=1").header("Host", "example.com");
        assert_request_capacity_sufficient(&req);
    }

    #[test]
    fn test_response_capacity_simple_ok() {
        let res = Response::new(200, "OK").body(b"hello".to_vec());
        assert_response_capacity_sufficient(&res);
    }

    #[test]
    fn test_response_capacity_no_body_status() {
        // 1xx / 204 / 304 は body を含めない
        for &code in &[100u16, 204, 304] {
            let res = Response::new(code, "Reason");
            assert_response_capacity_sufficient(&res);
        }
    }

    #[test]
    fn test_response_capacity_omit_body_with_content_length() {
        let res = Response::new(200, "OK")
            .header("Content-Length", "100")
            .omit_body(true);
        assert_response_capacity_sufficient(&res);
    }

    #[test]
    fn test_response_capacity_with_transfer_encoding() {
        let res = Response::new(200, "OK").header("Transfer-Encoding", "chunked");
        assert_response_capacity_sufficient(&res);
    }

    #[test]
    fn test_response_capacity_many_headers() {
        let mut res = Response::new(200, "OK").body(vec![b'X'; 1024]);
        for i in 0..50 {
            res = res.header(
                &alloc::format!("X-Custom-{i}"),
                &alloc::format!("value-{i}-with-some-padding"),
            );
        }
        assert_response_capacity_sufficient(&res);
    }

    #[test]
    fn test_response_capacity_status_code_3_digit_boundary() {
        // status_code は 100..=599、見積もりは 3 桁固定なので過小確保にならない
        for &code in &[100u16, 200, 599] {
            let res = Response::new(code, "Phrase").body(b"body".to_vec());
            assert_response_capacity_sufficient(&res);
        }
    }

    #[test]
    fn test_request_capacity_normal_input_does_not_panic() {
        // 巨大なヘッダー長をシミュレートするのは現実不可能なので、
        // ここではオーバーフロー時のフォールバックパス (`Vec::new()`) が
        // パニックしないことを通常入力で確認する。
        // 実際のオーバーフロー検出は fuzz_encode_request で網羅する。
        let req = Request::new("GET", "/").header("Host", "example.com");
        let _ = encode_request(&req).unwrap();
    }
}
