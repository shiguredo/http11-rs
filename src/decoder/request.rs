//! HTTP リクエストデコーダー
//!
//! # RFC 非準拠
//!
//! - RFC 9112 Section 2.2: HTTP/1.1 メッセージはオクテット列として解析すべき (SHOULD) だが、
//!   本実装では UTF-8 として強制的に解析している。非 UTF-8 バイト列を含むリクエストは
//!   エラーとして拒否される。
//!
//! - RFC 9112 Section 2.2: request-line の前に受信した空行 (CRLF) を少なくとも 1 行は
//!   無視すべき (SHOULD) だが、本実装では厳格にパースし、先頭の空行を不正なリクエスト行
//!   として拒否する。アプリケーション層で必要に応じて先頭の空行を除去すること。

use crate::compression::{CompressionStatus, Decompressor, NoCompression};
use crate::error::Error;
use crate::limits::DecoderLimits;
use crate::request::Request;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::validate::{is_valid_method, is_valid_protocol_version, is_valid_request_target};

use super::body::{
    BodyDecoder, BodyKind, BodyProgress, find_line, parse_header_line, parse_request_target_form,
    resolve_body_headers_for_request, validate_request_target_for_method,
};
use super::buffer;
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
    /// `mut_buf` で確保した未確定領域のバイト数
    pending: usize,
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
            pending: 0,
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
            pending: 0,
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
            pending: 0,
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
            pending: 0,
        }
    }

    /// 制限設定を取得
    pub fn limits(&self) -> &DecoderLimits {
        &self.limits
    }

    /// 既にメモリ上にあるバイト列を内部バッファに投入する
    ///
    /// `data` を `extend_from_slice` でコピーする (1 回の memcpy)。
    ///
    /// # 使い分け
    ///
    /// `feed` と [`mut_buf`](Self::mut_buf) / [`advance_buf`](Self::advance_buf)
    /// は入力経路の違う別の最適解として共存しており、用途で使い分ける:
    ///
    /// - **これから書き込む先のバッファが必要なケース** (OS の `read` でソケット
    ///   から受信する等): `mut_buf` / `advance_buf` を使う。OS が内部バッファに
    ///   直接書き込めるので、スタックバッファ → 内部 `Vec<u8>` のコピーが発生
    ///   しない。
    /// - **既にバイト列が `&[u8]` として手元にあるケース** (io_uring 等の完了通知
    ///   型 I/O で渡されるバイト列、テスト用バイトリテラル、別経路から受け取った
    ///   バイト列の中継等): `feed` を使う。`mut_buf(len) + copy_from_slice +
    ///   advance_buf(len)` だと「ゼロ初期化 + memcpy」の二段になるが、`feed`
    ///   は素直に 1 memcpy で済む。
    pub fn feed(&mut self, data: &[u8]) -> Result<(), Error> {
        buffer::feed(
            &mut self.buf,
            self.pending,
            self.limits.max_buffer_size,
            data,
        )
    }

    /// バッファにデータを追加 (制限チェックなし)
    ///
    /// 用途は [`feed`](Self::feed) と同じ「既にメモリ上にあるバイト列を投入する」
    /// で、`max_buffer_size` チェックをスキップする点だけが異なる。
    ///
    /// # 警告
    ///
    /// この関数は `DecoderLimits` による `max_buffer_size` チェックをスキップする。
    /// 未信頼入力に対して使用すると、メモリを無制限に消費して OOM を引き起こす可能性がある。
    /// 信頼済み入力またはテスト用途にのみ使用すること。
    pub fn feed_unchecked(&mut self, data: &[u8]) {
        buffer::feed_unchecked(&mut self.buf, self.pending, data);
    }

    /// 内部バッファ末尾に `len` バイトの書き込み枠を確保し、その可変スライスを返す
    ///
    /// 返るスライスは `Vec::resize(_, 0)` によりゼロ初期化済みなので、
    /// `std::io::Read::read` 等にそのまま渡せる。書き込み後は必ず
    /// [`advance_buf`](Self::advance_buf) で実書き込みバイト数を通知すること。
    ///
    /// 直前の `mut_buf` で確保された未確定領域は、関数の先頭で必ず破棄される
    /// (`advance_buf` 呼び忘れの回復)。よってエラー時 (`BufferOverflow`) も
    /// 「呼び出し前の状態に巻き戻る」のではなく、「pending 領域が破棄された
    /// 上で新規枠が確保されない」状態になる。
    ///
    /// pending 破棄後の `remaining().len() + len` が `max_buffer_size` を
    /// 超える場合は `Err(Error::BufferOverflow)` を返す。
    pub fn mut_buf(&mut self, len: usize) -> Result<&mut [u8], Error> {
        buffer::mut_buf(
            &mut self.buf,
            &mut self.pending,
            self.limits.max_buffer_size,
            len,
        )
    }

    /// 直前の [`mut_buf`](Self::mut_buf) で確保した枠のうち、
    /// 実際に書き込まれた `len` バイトを確定する
    ///
    /// 残り (`mut_buf` で確保した長さ - `len`) は破棄される。
    /// `len = 0` で呼ぶと枠全体を破棄 (EOF や read 失敗時のリセット用)。
    ///
    /// `len > pending` の場合、debug ビルドでは panic、release ビルドでは
    /// `pending` で飽和する。
    pub fn advance_buf(&mut self, len: usize) {
        buffer::advance_buf(&mut self.buf, &mut self.pending, len);
    }

    /// 書き込み可能な残り容量を返す
    ///
    /// `max_buffer_size` から現在のバッファ長 (確定済みデータ + 未確定 `pending`)
    /// を引いた値。`mut_buf(decoder.available_buf().min(N))` のようにチャンクサイズ
    /// を残容量に適応させる用途で使う。
    pub fn available_buf(&self) -> usize {
        buffer::available_buf(&self.buf, self.limits.max_buffer_size)
    }

    /// バッファの残りデータを取得
    pub fn remaining(&self) -> &[u8] {
        buffer::remaining(&self.buf, self.pending)
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
        self.pending = 0;
    }

    /// ボディモードを決定
    ///
    /// RFC 9112 Section 6: HTTP/1.0 では Transfer-Encoding は定義されていないため、
    /// HTTP/1.0 リクエストで Transfer-Encoding が指定されている場合はエラーとする
    ///
    /// RFC 9112 Section 6.1: リクエストでは chunked 以外の Transfer-Encoding は拒否
    fn determine_body_kind(&self, version: &str) -> Result<BodyKind, Error> {
        let (transfer_encoding_chunked, content_length) =
            resolve_body_headers_for_request(&self.headers)?;

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
            if len > self.limits.max_body_size as u64 {
                return Err(Error::BodyTooLarge {
                    size: usize::try_from(len).unwrap_or(usize::MAX),
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
        debug_assert!(
            self.pending == 0,
            "decode_headers called with pending mut_buf"
        );
        loop {
            match &self.phase {
                DecodePhase::StartLine => {
                    if let Some(pos) = find_line(&self.buf) {
                        let line = String::from_utf8(self.buf[..pos].to_vec()).map_err(|e| {
                            Error::InvalidData(alloc::format!("invalid UTF-8: {e}"))
                        })?;
                        self.buf.drain(..pos + 2);
                        if line.contains('\r') || line.contains('\n') {
                            return Err(Error::InvalidData(
                                "invalid request line: contains CR/LF".to_string(),
                            ));
                        }

                        // Parse: METHOD SP URI SP VERSION CRLF
                        let parts: Vec<&str> = line.splitn(3, ' ').collect();
                        if parts.len() != 3 {
                            return Err(Error::InvalidData(alloc::format!(
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

                        // request-target の形式判定と検証 (RFC 9112 Section 3.2)
                        let request_target_form = parse_request_target_form(parts[1])?;
                        validate_request_target_for_method(parts[0], &request_target_form)?;

                        // プロトコルバージョンの検証
                        if !is_valid_protocol_version(parts[2]) {
                            return Err(Error::InvalidData(
                                "invalid request line: invalid protocol version".to_string(),
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
                                // 空の Host ヘッダーは許可 (RFC 9112 Section 3.2)
                                let (_, host_value) = host_headers[0];
                                if !host_value.is_empty()
                                    && crate::host::Host::parse(host_value).is_err()
                                {
                                    return Err(Error::InvalidData(
                                        "HTTP/1.1 request contains invalid Host header value"
                                            .to_string(),
                                    ));
                                }
                            }

                            // RFC 9110 Section 9.3.6: "A CONNECT request message does not have content."
                            // CONNECT リクエストは content を持たないため、BodyKind::None として扱う。
                            // Content-Length / Transfer-Encoding が付いていても即エラーにはしないが、
                            // body として読むこともしない。ヘッダー終端でリクエスト完了とする。
                            // RFC は CONNECT リクエスト側に TE/CL を MUST NOT とはしていない
                            // (MUST NOT は 2xx レスポンス側の制約)。
                            let method = start_line_ref.split(' ').next().unwrap_or("");
                            let body_kind = if method == "CONNECT" {
                                BodyKind::None
                            } else {
                                self.determine_body_kind(version)?
                            };

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
                                BodyKind::Tunnel => {
                                    // リクエストではトンネルモードは発生しない
                                    unreachable!("Tunnel mode is only for CONNECT responses")
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
                                headers: core::mem::take(&mut self.headers),
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

                            let line =
                                String::from_utf8(self.buf[..pos].to_vec()).map_err(|e| {
                                    Error::InvalidData(alloc::format!("invalid UTF-8: {e}"))
                                })?;
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
        debug_assert!(self.pending == 0, "peek_body called with pending mut_buf");
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
        debug_assert!(
            self.pending == 0,
            "peek_body_decompressed called with pending mut_buf"
        );
        let input = match self.body_decoder.peek_body(&self.buf, &self.phase) {
            Some(data) if !data.is_empty() => data,
            _ => return Ok(None),
        };

        let status = self.decompressor.decompress(input, output)?;
        Ok(Some(status))
    }

    /// ボディデータを消費
    ///
    /// `peek_body()` で取得したデータを処理した後に呼ぶ
    /// `len` は消費するバイト数 (1 以上)
    pub fn consume_body(&mut self, len: usize) -> Result<BodyProgress, Error> {
        debug_assert!(
            self.pending == 0,
            "consume_body called with pending mut_buf"
        );
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
        debug_assert!(self.pending == 0, "progress called with pending mut_buf");
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
        debug_assert!(self.pending == 0, "decode called with pending mut_buf");
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
            BodyKind::Tunnel => {
                // リクエストではトンネルモードは発生しない
                unreachable!("Tunnel mode is only for CONNECT responses")
            }
            BodyKind::ContentLength(_) | BodyKind::Chunked => loop {
                // バッファからボディデータを直接消費。
                // body_decoder.peek_body() を直接呼ぶことで、返り値の lifetime が
                // &self.buf に紐付き、self.decoded_body の可変借用と並立できる。
                if let Some(data) = self.body_decoder.peek_body(&self.buf, &self.phase) {
                    let len = data.len();
                    self.decoded_body.extend_from_slice(data);
                    self.consume_body(len)?;
                    // consume_body の戻り値ではなく phase で完了判定する。
                    // BodyChunkedData → BodyChunkedDataCrlf → BodyChunkedSize の
                    // 多段遷移時は Advanced を返すため、phase を直接見るのが確実。
                    if matches!(self.phase, DecodePhase::Complete) {
                        break;
                    }
                    continue;
                }

                // ボディデータがない → 状態機械を進める
                match self.progress()? {
                    BodyProgress::Complete { .. } => break,
                    BodyProgress::Advanced => continue,
                    BodyProgress::NeedData => return Ok(None),
                }
            },
            BodyKind::CloseDelimited | BodyKind::None => {}
        }

        // Request を構築
        // BodyKind::None / Tunnel は「フレーミングがない」ため body = None。
        // それ以外 (ContentLength / Chunked / CloseDelimited) は明示的なボディなので body = Some。
        let head = self.decoded_head.take().unwrap();
        let body = match body_kind {
            BodyKind::None | BodyKind::Tunnel => None,
            BodyKind::ContentLength(_) | BodyKind::Chunked | BodyKind::CloseDelimited => {
                Some(core::mem::take(&mut self.decoded_body))
            }
        };

        // Keep-Alive 対応: 次のリクエストのために状態をリセット
        self.phase = DecodePhase::StartLine;
        self.decoded_body_kind = None;
        self.decoded_body.clear();
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
