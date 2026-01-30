use crate::compression::{CompressionError, CompressionStatus, Compressor, NoCompression};
use crate::error::EncodeError;
use crate::request::Request;
use crate::response::Response;

/// リクエストをエンコード
///
/// RFC 9112 Section 3.2: HTTP/1.1 リクエストには Host ヘッダーが必須
pub fn encode_request(request: &Request) -> Result<Vec<u8>, EncodeError> {
    // RFC 9112 Section 3.2: HTTP/1.1 には Host ヘッダーが必須
    if request.version == "HTTP/1.1" && !request.has_header("Host") {
        return Err(EncodeError::MissingHostHeader);
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
    // RFC 9112: Transfer-Encoding と Content-Length は同時に送信してはならない
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

    // Content-Length (if not already set and not chunked)
    // RFC 9112: keep-alive を維持するために Content-Length または Transfer-Encoding が必要
    // 1xx/204/304 はボディがないため Content-Length を追加しない
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

    buf
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
///
/// RFC 9112 Section 3.2: HTTP/1.1 リクエストには Host ヘッダーが必須
pub fn encode_request_headers(request: &Request) -> Result<Vec<u8>, EncodeError> {
    // RFC 9112 Section 3.2: HTTP/1.1 には Host ヘッダーが必須
    if request.version == "HTTP/1.1" && !request.has_header("Host") {
        return Err(EncodeError::MissingHostHeader);
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
    pub fn encode_headers(&self) -> Vec<u8> {
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
