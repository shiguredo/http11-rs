use crate::request::Request;
use crate::response::Response;

/// リクエストをエンコード
pub fn encode_request(request: &Request) -> Vec<u8> {
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

    // Content-Length (if body is present and not already set)
    if !request.body.is_empty() && !request.has_header("Content-Length") {
        buf.extend_from_slice(b"Content-Length: ");
        buf.extend_from_slice(request.body.len().to_string().as_bytes());
        buf.extend_from_slice(b"\r\n");
    }

    // End of headers
    buf.extend_from_slice(b"\r\n");

    // Body
    buf.extend_from_slice(&request.body);

    buf
}

/// レスポンスをエンコード
pub fn encode_response(response: &Response) -> Vec<u8> {
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

    // Content-Length (if body is present, not already set, and not chunked)
    if !response.body.is_empty()
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
    buf.extend_from_slice(&response.body);

    buf
}

impl Request {
    /// リクエストをバイト列にエンコード
    pub fn encode(&self) -> Vec<u8> {
        encode_request(self)
    }
}

impl Response {
    /// レスポンスをバイト列にエンコード
    pub fn encode(&self) -> Vec<u8> {
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
pub fn encode_request_headers(request: &Request) -> Vec<u8> {
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

    buf
}

/// レスポンスヘッダーのみをエンコード (ボディなし)
///
/// Chunked Transfer Encoding を使う場合に便利です。
/// ヘッダー送信後に `encode_chunk` でボディを送信できます。
pub fn encode_response_headers(response: &Response) -> Vec<u8> {
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

    buf
}

impl Request {
    /// ヘッダーのみをエンコード (Chunked Transfer Encoding 用)
    pub fn encode_headers(&self) -> Vec<u8> {
        encode_request_headers(self)
    }
}

impl Response {
    /// ヘッダーのみをエンコード (Chunked Transfer Encoding 用)
    pub fn encode_headers(&self) -> Vec<u8> {
        encode_response_headers(self)
    }
}
