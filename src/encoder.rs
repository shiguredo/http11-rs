use crate::compression::{CompressionError, CompressionStatus, Compressor, NoCompression};
use crate::error::EncodeError;
use crate::host::Host;
use crate::request::Request;
use crate::response::Response;
use crate::validate::{
    is_valid_field_value, is_valid_header_name, is_valid_method, is_valid_reason_phrase,
    is_valid_request_target, is_valid_status_code, is_valid_version_for_encode,
};

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
/// - absolute-form: "://" を含む (例: http://host/path)
/// - authority-form: 上記以外で ":" を含む (例: host:port)
/// - asterisk-form: "*" のみ
fn detect_request_target_form(uri: &str) -> RequestTargetForm {
    if uri == "*" {
        RequestTargetForm::Asterisk
    } else if uri.contains("://") {
        RequestTargetForm::Absolute
    } else if uri.starts_with('/') {
        RequestTargetForm::Origin
    } else {
        RequestTargetForm::Authority
    }
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
    let form = detect_request_target_form(uri);

    match (method.to_ascii_uppercase().as_str(), &form) {
        // CONNECT は authority-form のみ
        ("CONNECT", RequestTargetForm::Authority) => Ok(()),
        ("CONNECT", _) => Err(EncodeError::InvalidRequestTargetForm {
            method: method.to_string(),
            uri: uri.to_string(),
        }),
        // asterisk-form は OPTIONS のみ
        (_, RequestTargetForm::Asterisk) => {
            if method.eq_ignore_ascii_case("OPTIONS") {
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
        // origin-form / absolute-form は OK
        (_, RequestTargetForm::Origin | RequestTargetForm::Absolute) => Ok(()),
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

    // absolute-form の場合、Host と authority の一致検証
    if request.uri.contains("://")
        && let Some(authority) = extract_authority_from_uri(&request.uri)
        && !authority.is_empty()
        && !host_value.is_empty()
        && !authority.eq_ignore_ascii_case(host_value)
    {
        return Err(EncodeError::HostAuthorityMismatch {
            host: host_value.to_string(),
            authority: authority.to_string(),
        });
    }

    Ok(())
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

    // Host ヘッダーの詳細バリデーション
    validate_host_header(request)?;

    // RFC 9112 Section 6.2: Transfer-Encoding と Content-Length の同時送信は禁止
    if request.has_header("Transfer-Encoding") && request.has_header("Content-Length") {
        return Err(EncodeError::ConflictingTransferEncodingAndContentLength);
    }

    // Content-Length ヘッダーが手動設定されている場合、body.len() との整合性を検証
    if !request.has_header("Transfer-Encoding")
        && let Some(cl_value) = request.get_header("Content-Length")
        && let Ok(header_value) = cl_value.trim().parse::<u64>()
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

    // Content-Length ヘッダーが手動設定されている場合、body.len() との整合性を検証
    // omit_content_length: true の場合はスキップ (HEAD レスポンス用: body は空だが
    // Content-Length は GET と同じ値を返す)
    if !response.omit_content_length
        && !response.has_header("Transfer-Encoding")
        && let Some(cl_value) = response.get_header("Content-Length")
        && let Ok(header_value) = cl_value.trim().parse::<u64>()
    {
        let body_length = response.body.len() as u64;
        if header_value != body_length {
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
    // omit_content_length が true の場合は自動付与しない (HEAD レスポンス用)
    let status_has_body = !((100..200).contains(&response.status_code)
        || response.status_code == 204
        || response.status_code == 304);
    if status_has_body
        && !response.omit_content_length
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
    if status_has_body {
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

    // Host ヘッダーの詳細バリデーション
    validate_host_header(request)?;

    // RFC 9112 Section 6.2: Transfer-Encoding と Content-Length の同時送信は禁止
    if request.has_header("Transfer-Encoding") && request.has_header("Content-Length") {
        return Err(EncodeError::ConflictingTransferEncodingAndContentLength);
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
