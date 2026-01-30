//! HTTP リクエストデコーダー

use crate::compression::{CompressionStatus, Decompressor, NoCompression};
use crate::error::Error;
use crate::limits::DecoderLimits;
use crate::request::Request;

use super::body::{
    BodyDecoder, BodyKind, BodyProgress, find_line, is_valid_http_version, is_valid_method,
    is_valid_request_target, parse_header_line, resolve_body_headers,
};
use super::head::RequestHead;
use super::phase::DecodePhase;

/// HTTP リクエストデコーダー (Sans I/O)
///
/// サーバー側でクライアントからのリクエストをパースする際に使用
///
/// # 型パラメータ
///
/// - `D`: 展開器の型。デフォルトは `NoCompression`（展開なし）。
///
/// # 使い方
///
/// ## 展開なし（既存 API 互換）
///
/// ```rust
/// use shiguredo_http11::RequestDecoder;
///
/// let mut decoder = RequestDecoder::new();
/// ```
///
/// ## 展開あり
///
/// ```ignore
/// use shiguredo_http11::RequestDecoder;
///
/// let mut decoder = RequestDecoder::with_decompressor(GzipDecompressor::new());
/// ```
#[derive(Debug)]
pub struct RequestDecoder<D: Decompressor = NoCompression> {
    buf: Vec<u8>,
    phase: DecodePhase,
    start_line: Option<String>,
    headers: Vec<(String, String)>,
    body_decoder: BodyDecoder,
    limits: DecoderLimits,
    /// decode() 用: デコード済みヘッダー
    decoded_head: Option<RequestHead>,
    /// decode() 用: ボディ種別
    decoded_body_kind: Option<BodyKind>,
    /// decode() 用: デコード済みボディ
    decoded_body: Vec<u8>,
    /// 展開器
    decompressor: D,
}

impl Default for RequestDecoder<NoCompression> {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestDecoder<NoCompression> {
    /// 新しいデコーダーを作成
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            phase: DecodePhase::StartLine,
            start_line: None,
            headers: Vec::new(),
            body_decoder: BodyDecoder::new(),
            limits: DecoderLimits::default(),
            decoded_head: None,
            decoded_body_kind: None,
            decoded_body: Vec::new(),
            decompressor: NoCompression::new(),
        }
    }

    /// 制限付きでデコーダーを作成
    pub fn with_limits(limits: DecoderLimits) -> Self {
        Self {
            buf: Vec::new(),
            phase: DecodePhase::StartLine,
            start_line: None,
            headers: Vec::new(),
            body_decoder: BodyDecoder::new(),
            limits,
            decoded_head: None,
            decoded_body_kind: None,
            decoded_body: Vec::new(),
            decompressor: NoCompression::new(),
        }
    }
}

impl<D: Decompressor> RequestDecoder<D> {
    /// 展開器付きでデコーダーを作成
    pub fn with_decompressor(decompressor: D) -> Self {
        Self {
            buf: Vec::new(),
            phase: DecodePhase::StartLine,
            start_line: None,
            headers: Vec::new(),
            body_decoder: BodyDecoder::new(),
            limits: DecoderLimits::default(),
            decoded_head: None,
            decoded_body_kind: None,
            decoded_body: Vec::new(),
            decompressor,
        }
    }

    /// 展開器と制限付きでデコーダーを作成
    pub fn with_decompressor_and_limits(decompressor: D, limits: DecoderLimits) -> Self {
        Self {
            buf: Vec::new(),
            phase: DecodePhase::StartLine,
            start_line: None,
            headers: Vec::new(),
            body_decoder: BodyDecoder::new(),
            limits,
            decoded_head: None,
            decoded_body_kind: None,
            decoded_body: Vec::new(),
            decompressor,
        }
    }

    /// 制限設定を取得
    pub fn limits(&self) -> &DecoderLimits {
        &self.limits
    }

    /// バッファにデータを追加
    pub fn feed(&mut self, data: &[u8]) -> Result<(), Error> {
        let new_size = self.buf.len() + data.len();
        if new_size > self.limits.max_buffer_size {
            return Err(Error::BufferOverflow {
                size: new_size,
                limit: self.limits.max_buffer_size,
            });
        }
        self.buf.extend_from_slice(data);
        Ok(())
    }

    /// バッファにデータを追加 (制限チェックなし)
    pub fn feed_unchecked(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// バッファの残りデータを取得
    pub fn remaining(&self) -> &[u8] {
        &self.buf
    }

    /// デコーダーをリセット
    pub fn reset(&mut self) {
        self.buf.clear();
        self.phase = DecodePhase::StartLine;
        self.start_line = None;
        self.headers.clear();
        self.body_decoder.reset();
        self.decoded_head = None;
        self.decoded_body_kind = None;
        self.decoded_body.clear();
        self.decompressor.reset();
    }

    /// ボディモードを決定
    ///
    /// RFC 9112 Section 6: HTTP/1.0 では Transfer-Encoding は定義されていないため、
    /// HTTP/1.0 リクエストで Transfer-Encoding が指定されている場合はエラーとする
    fn determine_body_kind(&self, version: &str) -> Result<BodyKind, Error> {
        let (transfer_encoding_chunked, content_length) = resolve_body_headers(&self.headers)?;

        if transfer_encoding_chunked {
            // RFC 9112 Section 6: HTTP/1.0 では Transfer-Encoding は定義されていない
            if version == "HTTP/1.0" {
                return Err(Error::InvalidData(
                    "Transfer-Encoding is not defined in HTTP/1.0".to_string(),
                ));
            }
            return Ok(BodyKind::Chunked);
        }

        if let Some(len) = content_length {
            if len > self.limits.max_body_size {
                return Err(Error::BodyTooLarge {
                    size: len,
                    limit: self.limits.max_body_size,
                });
            }
            return Ok(BodyKind::ContentLength(len));
        }

        Ok(BodyKind::None)
    }

    /// ヘッダーをデコード
    ///
    /// ヘッダーが完了したら `Some((RequestHead, BodyKind))` を返す
    /// データ不足の場合は `None` を返す
    /// 既にヘッダーデコード済みの場合はエラー
    pub fn decode_headers(&mut self) -> Result<Option<(RequestHead, BodyKind)>, Error> {
        loop {
            match &self.phase {
                DecodePhase::StartLine => {
                    if let Some(pos) = find_line(&self.buf) {
                        let line = String::from_utf8(self.buf[..pos].to_vec())
                            .map_err(|e| Error::InvalidData(format!("invalid UTF-8: {e}")))?;
                        self.buf.drain(..pos + 2);
                        if line.contains('\r') || line.contains('\n') {
                            return Err(Error::InvalidData(
                                "invalid request line: contains CR/LF".to_string(),
                            ));
                        }

                        // Parse: METHOD SP URI SP VERSION CRLF
                        let parts: Vec<&str> = line.splitn(3, ' ').collect();
                        if parts.len() != 3 {
                            return Err(Error::InvalidData(format!(
                                "invalid request line: {}",
                                line
                            )));
                        }

                        // メソッド名の検証 (RFC 9110 Section 9)
                        if !is_valid_method(parts[0]) {
                            return Err(Error::InvalidData(
                                "invalid request line: invalid method".to_string(),
                            ));
                        }

                        // リクエストターゲットの検証 (RFC 9112 Section 3)
                        if !is_valid_request_target(parts[1]) {
                            return Err(Error::InvalidData(
                                "invalid request line: invalid request-target".to_string(),
                            ));
                        }

                        // HTTP バージョンの検証 (RFC 9112 Section 2.3)
                        if !is_valid_http_version(parts[2]) {
                            return Err(Error::InvalidData(
                                "invalid request line: invalid HTTP version".to_string(),
                            ));
                        }

                        self.start_line = Some(line);
                        self.phase = DecodePhase::Headers;
                    } else {
                        return Ok(None);
                    }
                }
                DecodePhase::Headers => {
                    if let Some(pos) = find_line(&self.buf) {
                        if pos == 0 {
                            // Empty line - end of headers
                            self.buf.drain(..2);

                            // RFC 9112 Section 3.2: HTTP/1.1 リクエストでは Host ヘッダーが必須
                            let start_line_ref = self.start_line.as_ref().ok_or_else(|| {
                                Error::InvalidData("missing request line".to_string())
                            })?;
                            let version = start_line_ref.split(' ').nth(2).unwrap_or("");
                            if version == "HTTP/1.1" {
                                let host_headers: Vec<_> = self
                                    .headers
                                    .iter()
                                    .filter(|(name, _)| name.eq_ignore_ascii_case("Host"))
                                    .collect();
                                if host_headers.is_empty() {
                                    return Err(Error::InvalidData(
                                        "HTTP/1.1 request missing Host header".to_string(),
                                    ));
                                }
                                if host_headers.len() > 1 {
                                    return Err(Error::InvalidData(
                                        "HTTP/1.1 request contains multiple Host headers"
                                            .to_string(),
                                    ));
                                }
                                // Host ヘッダー値検証
                                let (_, host_value) = host_headers[0];
                                if crate::host::Host::parse(host_value).is_err() {
                                    return Err(Error::InvalidData(
                                        "HTTP/1.1 request contains invalid Host header value"
                                            .to_string(),
                                    ));
                                }
                            }

                            let body_kind = self.determine_body_kind(version)?;

                            // ヘッダー完了、ボディフェーズに遷移
                            // RFC 9112: リクエストは close-delimited を使わない
                            match body_kind {
                                BodyKind::ContentLength(len) => {
                                    if len > 0 {
                                        self.phase =
                                            DecodePhase::BodyContentLength { remaining: len };
                                    } else {
                                        self.phase = DecodePhase::Complete;
                                    }
                                }
                                BodyKind::Chunked => {
                                    self.phase = DecodePhase::BodyChunkedSize;
                                }
                                BodyKind::CloseDelimited | BodyKind::None => {
                                    self.phase = DecodePhase::Complete;
                                }
                            }

                            // RequestHead を構築
                            let start_line = self.start_line.take().ok_or_else(|| {
                                Error::InvalidData("missing request line".to_string())
                            })?;
                            let parts: Vec<&str> = start_line.splitn(3, ' ').collect();

                            let head = RequestHead {
                                method: parts[0].to_string(),
                                uri: parts[1].to_string(),
                                version: parts[2].to_string(),
                                headers: std::mem::take(&mut self.headers),
                            };

                            return Ok(Some((head, body_kind)));
                        } else {
                            // Check header line size limit
                            if pos > self.limits.max_header_line_size {
                                return Err(Error::HeaderLineTooLong {
                                    size: pos,
                                    limit: self.limits.max_header_line_size,
                                });
                            }

                            // Check header count limit
                            if self.headers.len() >= self.limits.max_headers_count {
                                return Err(Error::TooManyHeaders {
                                    count: self.headers.len() + 1,
                                    limit: self.limits.max_headers_count,
                                });
                            }

                            let line = String::from_utf8(self.buf[..pos].to_vec())
                                .map_err(|e| Error::InvalidData(format!("invalid UTF-8: {e}")))?;
                            self.buf.drain(..pos + 2);

                            let (name, value) = parse_header_line(&line)?;
                            self.headers.push((name, value));
                        }
                    } else {
                        return Ok(None);
                    }
                }
                DecodePhase::Complete => {
                    // 完了状態から次のメッセージへ遷移
                    self.phase = DecodePhase::StartLine;
                    self.start_line = None;
                    self.headers.clear();
                    self.body_decoder.reset();
                    continue;
                }
                _ => {
                    return Err(Error::InvalidData(
                        "decode_headers called during body decoding".to_string(),
                    ));
                }
            }
        }
    }

    /// 利用可能なボディデータを覗く（ゼロコピー）
    ///
    /// `decode_headers()` 成功後に呼ぶ
    /// データがある場合はスライスを返す
    /// ボディがない場合や完了済みの場合は `None` を返す
    pub fn peek_body(&self) -> Option<&[u8]> {
        self.body_decoder.peek_body(&self.buf, &self.phase)
    }

    /// ボディデータを展開して取得
    ///
    /// `decode_headers()` 成功後に呼ぶ。
    /// 利用可能なボディデータを展開して output に書き込む。
    ///
    /// # 引数
    /// - `output`: 展開データを書き込む出力バッファ
    ///
    /// # 戻り値
    /// - `Ok(Some(status))`: 展開成功。`status.produced()` バイトが output に書き込まれた。
    ///   `status.consumed()` バイトを `consume_body()` で消費する必要がある。
    /// - `Ok(None)`: 利用可能なボディデータがない
    /// - `Err(e)`: 展開エラー
    ///
    /// # 使い方
    ///
    /// ```ignore
    /// let mut output = vec![0u8; 8192];
    /// while let Some(status) = decoder.peek_body_decompressed(&mut output)? {
    ///     // output[..status.produced()] に展開済みデータ
    ///     process(&output[..status.produced()]);
    ///     decoder.consume_body(status.consumed())?;
    /// }
    /// ```
    pub fn peek_body_decompressed(
        &mut self,
        output: &mut [u8],
    ) -> Result<Option<CompressionStatus>, Error> {
        let input = match self.body_decoder.peek_body(&self.buf, &self.phase) {
            Some(data) if !data.is_empty() => data,
            _ => return Ok(None),
        };

        let status = self.decompressor.decompress(input, output)?;
        Ok(Some(status))
    }

    /// 利用可能なボディデータのバイト数を取得
    fn available_body_len(&self) -> usize {
        match &self.phase {
            DecodePhase::BodyContentLength { remaining } => self.buf.len().min(*remaining),
            DecodePhase::BodyChunkedData { remaining } => self.buf.len().min(*remaining),
            _ => 0,
        }
    }

    /// ボディデータを消費
    ///
    /// `peek_body()` で取得したデータを処理した後に呼ぶ
    /// `len` は消費するバイト数 (1 以上)
    pub fn consume_body(&mut self, len: usize) -> Result<BodyProgress, Error> {
        if len == 0 {
            return Err(Error::InvalidData(
                "consume_body(0) is not allowed, use progress() instead".to_string(),
            ));
        }
        self.body_decoder
            .consume_body(&mut self.buf, &mut self.phase, len, &self.limits)
    }

    /// 状態機械を進める (ボディデータは消費しない)
    ///
    /// Chunked エンコーディングの場合、チャンクサイズ行のパースや
    /// 終端チャンクの処理を行う。
    pub fn progress(&mut self) -> Result<BodyProgress, Error> {
        self.body_decoder
            .consume_body(&mut self.buf, &mut self.phase, 0, &self.limits)
    }

    /// リクエスト全体を一括でデコード
    ///
    /// ストリーミング API (`decode_headers()` / `peek_body()` / `consume_body()`) を
    /// 内部で使用して、リクエスト全体をデコードする。
    ///
    /// データ不足の場合は `None` を返す。
    /// ストリーミング API と混在使用するとエラーを返す。
    pub fn decode(&mut self) -> Result<Option<Request>, Error> {
        // ヘッダーがまだデコードされていない場合はデコード
        if self.decoded_head.is_none() {
            match self.phase {
                DecodePhase::StartLine | DecodePhase::Headers => match self.decode_headers()? {
                    Some((head, body_kind)) => {
                        self.decoded_head = Some(head);
                        self.decoded_body_kind = Some(body_kind);
                    }
                    None => return Ok(None),
                },
                _ => {
                    return Err(Error::InvalidData(
                        "decode cannot be mixed with streaming API".to_string(),
                    ));
                }
            }
        }

        // ボディを読む
        // RFC 9112: リクエストは close-delimited を使わないため、CloseDelimited は None と同じ
        let body_kind = *self.decoded_body_kind.as_ref().unwrap();
        match body_kind {
            BodyKind::ContentLength(_) | BodyKind::Chunked => loop {
                // 直接バッファから利用可能なデータ長を取得（コピーなし）
                let available = self.available_body_len();
                if available > 0 {
                    // バッファから直接コピー
                    self.decoded_body.extend_from_slice(&self.buf[..available]);
                    match self.consume_body(available)? {
                        BodyProgress::Complete { .. } => break,
                        BodyProgress::Continue => continue,
                    }
                }

                // データがない場合、状態機械を進める
                match self.progress()? {
                    BodyProgress::Complete { .. } => break,
                    BodyProgress::Continue => {
                        // 状態遷移後にデータが利用可能になったか確認
                        if self.available_body_len() > 0 {
                            continue;
                        }
                        // データ不足
                        return Ok(None);
                    }
                }
            },
            BodyKind::CloseDelimited | BodyKind::None => {}
        }

        // Request を構築
        let head = self.decoded_head.take().unwrap();
        let body = std::mem::take(&mut self.decoded_body);

        // Keep-Alive 対応: 次のリクエストのために状態をリセット
        self.phase = DecodePhase::StartLine;
        self.decoded_body_kind = None;
        self.body_decoder.reset();

        Ok(Some(Request {
            method: head.method,
            uri: head.uri,
            version: head.version,
            headers: head.headers,
            body,
        }))
    }
}
