use crate::compression::{CompressionError, CompressionStatus, Compressor, NoCompression};
use crate::error::EncodeError;
use crate::host::Host;
use crate::request::Request;
use crate::response::Response;
use crate::validate::{
    is_valid_field_value, is_valid_header_name, is_valid_method, is_valid_reason_phrase,
    is_valid_request_target, is_valid_status_code, is_valid_version_for_encode,
};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

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

    // RFC 3986: URI は ASCII のみで構成される
    // obs-text (0x80-0xFF) は受信側では許容するが、送信側では拒否する
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

/// request-target の形式
enum RequestTargetForm {
    /// origin-form: "/" path ["?" query]
    Origin,
    /// absolute-form: absolute-URI
    Absolute,
    /// authority-form: uri-host ":" port
    Authority,
    /// asterisk-form: "*"
    Asterisk,
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
    if !request.has_header("Transfer-Encoding")
        && let Some(header_value) = validate_content_length_headers(&request.headers)?
    {
        let body_length = request.body.len() as u64;
        if header_value != body_length {
            return Err(EncodeError::ContentLengthMismatch {
                header_value,
                body_length,
            });
        }
    }

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

    // Content-Length (if body is present, not already set, and not chunked)
    if !request.body.is_empty()
        && !request.has_header("Content-Length")
        && !request.has_header("Transfer-Encoding")
    {
        buf.extend_from_slice(b"Content-Length: ");
        buf.extend_from_slice(request.body.len().to_string().as_bytes());
        buf.extend_from_slice(b"\r\n");
    }

    // End of headers
    buf.extend_from_slice(b"\r\n");

    // Body
    buf.extend_from_slice(&request.body);

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
    if response.status_code == 205 {
        if !response.body.is_empty() {
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

    let status_has_body = !((100..200).contains(&response.status_code)
        || response.status_code == 204
        || response.status_code == 304);
    let body_will_be_encoded = status_has_body && !response.omit_body;

    // Content-Length ヘッダーの ABNF 検証と body.len() との整合性を検証
    // - 通常レスポンス: 常に一致必須
    // - omit_body: true の場合は、body が空のときのみ検証をスキップ
    //   (HEAD レスポンスで Content-Length が表現長を示すケース)
    // - 1xx/204/304 は message body がないため、ここでは検証しない
    if status_has_body
        && !response.has_header("Transfer-Encoding")
        && let Some(header_value) = validate_content_length_headers(&response.headers)?
    {
        let body_length = response.body.len() as u64;
        let should_validate = body_will_be_encoded || body_length != 0;
        if should_validate && header_value != body_length {
            return Err(EncodeError::ContentLengthMismatch {
                header_value,
                body_length,
            });
        }
    }

    let mut buf = Vec::new();

    // Status line: VERSION SP STATUS-CODE SP REASON-PHRASE CRLF
    buf.extend_from_slice(response.version.as_bytes());
    buf.push(b' ');
    buf.extend_from_slice(response.status_code.to_string().as_bytes());
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

    // Content-Length (if not already set and not chunked)
    // RFC 9112: keep-alive を維持するために Content-Length または Transfer-Encoding が必要
    // 1xx/204/304 はボディがないため Content-Length を追加しない
    // 205 は RFC 9110 Section 15.3.6 でボディ生成禁止だが、受信者のメッセージ長決定規則のため
    // Content-Length: 0 を付与する (close-delimited にならないようにする)
    // omit_body: true かつ body が空の場合は自動付与しない
    // (HEAD レスポンスで表現長が不明なケースに配慮)
    if status_has_body
        && (!response.omit_body || !response.body.is_empty())
        && !response.has_header("Content-Length")
        && !response.has_header("Transfer-Encoding")
    {
        buf.extend_from_slice(b"Content-Length: ");
        buf.extend_from_slice(response.body.len().to_string().as_bytes());
        buf.extend_from_slice(b"\r\n");
    }

    // End of headers
    buf.extend_from_slice(b"\r\n");

    // Body
    // RFC 9110 Section 6.4.1: 1xx/204/304 はボディを含めてはならない
    // HEAD レスポンスでは omit_body: true としてボディ送信を抑止する
    if body_will_be_encoded {
        buf.extend_from_slice(&response.body);
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
    let mut buf = Vec::new();

    if data.is_empty() {
        // 終端チャンク
        buf.extend_from_slice(b"0\r\n\r\n");
    } else {
        // チャンクサイズ (16進数)
        buf.extend_from_slice(format!("{:x}\r\n", data.len()).as_bytes());
        // チャンクデータ
        buf.extend_from_slice(data);
        // CRLF
        buf.extend_from_slice(b"\r\n");
    }

    buf
}

/// 複数のデータを chunked 形式でエンコード
///
/// すべてのチャンクを結合し、終端チャンクも追加します。
pub fn encode_chunks(chunks: &[&[u8]]) -> Vec<u8> {
    let mut buf = Vec::new();

    for chunk in chunks {
        buf.extend_from_slice(format!("{:x}\r\n", chunk.len()).as_bytes());
        buf.extend_from_slice(chunk);
        buf.extend_from_slice(b"\r\n");
    }

    // 終端チャンク
    buf.extend_from_slice(b"0\r\n\r\n");

    buf
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
    buf.extend_from_slice(response.status_code.to_string().as_bytes());
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
