//! HTTP レスポンスデコーダー
//!
//! # RFC 非準拠
//!
//! - RFC 9112 Section 2.2: HTTP/1.1 メッセージはオクテット列として解析すべき (SHOULD) だが、
//!   本実装では UTF-8 として強制的に解析している。非 UTF-8 バイト列を含むレスポンスは
//!   エラーとして拒否される。

use crate::compression::{CompressionStatus, Decompressor, NoCompression};
use crate::error::Error;
use crate::limits::DecoderLimits;
use crate::response::Response;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::validate::{is_valid_protocol_version, is_valid_reason_phrase, is_valid_status_code};

use super::body::{
    BodyDecoder, BodyKind, BodyProgress, TransferEncodingResult, find_line, parse_header_line,
    resolve_body_headers_for_response,
};
use super::buffer;
use super::head::ResponseHead;
use super::phase::DecodePhase;

/// HTTP レスポンスデコーダー (Sans I/O)
///
/// クライアント側でサーバーからのレスポンスをパースする際に使用
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
/// use shiguredo_http11::ResponseDecoder;
///
/// let mut decoder = ResponseDecoder::new();
/// ```
///
/// ## 展開あり
///
/// ```ignore
/// use shiguredo_http11::ResponseDecoder;
///
/// let mut decoder = ResponseDecoder::with_decompressor(GzipDecompressor::new());
/// ```
#[derive(Debug)]
pub struct ResponseDecoder<D: Decompressor = NoCompression> {
    buf: Vec<u8>,
    phase: DecodePhase,
    start_line: Option<String>,
    headers: Vec<(String, String)>,
    body_decoder: BodyDecoder,
    limits: DecoderLimits,
    /// HEAD リクエストへのレスポンスかどうか
    expect_no_body: bool,
    /// ステータスコード（ヘッダーデコード後に保持）
    status_code: u16,
    /// decode() 用: デコード済みヘッダー
    decoded_head: Option<ResponseHead>,
    /// decode() 用: ボディ種別
    decoded_body_kind: Option<BodyKind>,
    /// decode() 用: デコード済みボディ
    decoded_body: Vec<u8>,
    /// 展開器
    decompressor: D,
    /// リクエストメソッド (CONNECT トンネル判定用)
    request_method: Option<String>,
    /// `mut_buf` で確保した未確定領域のバイト数
    pending: usize,
}

impl Default for ResponseDecoder<NoCompression> {
    fn default() -> Self {
        Self::new()
    }
}

impl ResponseDecoder<NoCompression> {
    /// 新しいデコーダーを作成
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            phase: DecodePhase::StartLine,
            start_line: None,
            headers: Vec::new(),
            body_decoder: BodyDecoder::new(),
            limits: DecoderLimits::default(),
            expect_no_body: false,
            status_code: 0,
            decoded_head: None,
            decoded_body_kind: None,
            decoded_body: Vec::new(),
            decompressor: NoCompression::new(),
            request_method: None,
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
            expect_no_body: false,
            status_code: 0,
            decoded_head: None,
            decoded_body_kind: None,
            decoded_body: Vec::new(),
            decompressor: NoCompression::new(),
            request_method: None,
            pending: 0,
        }
    }
}

impl<D: Decompressor> ResponseDecoder<D> {
    /// 展開器付きでデコーダーを作成
    pub fn with_decompressor(decompressor: D) -> Self {
        Self {
            buf: Vec::new(),
            phase: DecodePhase::StartLine,
            start_line: None,
            headers: Vec::new(),
            body_decoder: BodyDecoder::new(),
            limits: DecoderLimits::default(),
            expect_no_body: false,
            status_code: 0,
            decoded_head: None,
            decoded_body_kind: None,
            decoded_body: Vec::new(),
            decompressor,
            request_method: None,
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
            expect_no_body: false,
            status_code: 0,
            decoded_head: None,
            decoded_body_kind: None,
            decoded_body: Vec::new(),
            decompressor,
            request_method: None,
            pending: 0,
        }
    }

    /// HEAD リクエストへのレスポンスとしてデコード (ボディなし)
    pub fn set_expect_no_body(&mut self, expect_no_body: bool) {
        self.expect_no_body = expect_no_body;
    }

    /// リクエストメソッドを設定 (CONNECT トンネル判定用)
    ///
    /// CONNECT メソッドへの 2xx レスポンスはトンネルモードに切り替わる。
    /// この場合、ボディは存在せず、バッファ残りデータはトンネルデータとなる。
    pub fn set_request_method(&mut self, method: &str) {
        self.request_method = Some(method.to_string());
    }

    /// バッファの残りデータを取り出す (トンネルモード用)
    ///
    /// CONNECT 2xx レスポンス後にトンネルモードに切り替わった場合、
    /// このメソッドでヘッダー後のデータを取り出してトンネルに転送する。
    ///
    /// 呼び出し後、バッファは空になる。
    pub fn take_remaining(&mut self) -> Vec<u8> {
        debug_assert!(
            self.pending == 0,
            "take_remaining called with pending mut_buf"
        );
        core::mem::take(&mut self.buf)
    }

    /// トンネルモードかどうかを判定
    ///
    /// CONNECT 2xx レスポンスの場合、トンネルモードになる。
    pub fn is_tunnel(&self) -> bool {
        matches!(self.phase, DecodePhase::Tunnel)
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
        self.expect_no_body = false;
        self.status_code = 0;
        self.decoded_head = None;
        self.decoded_body_kind = None;
        self.decoded_body.clear();
        self.decompressor.reset();
        self.request_method = None;
        self.pending = 0;
    }

    /// 接続終了を通知 (close-delimited ボディ用)
    ///
    /// close-delimited ボディを読み取り中に接続が閉じられた場合に呼び出す。
    /// これにより、バッファ内の残りデータがボディとして確定し、Complete に遷移する。
    ///
    /// close-delimited 以外の状態で呼び出した場合は何もしない。
    pub fn mark_eof(&mut self) {
        debug_assert!(self.pending == 0, "mark_eof called with pending mut_buf");
        if matches!(self.phase, DecodePhase::BodyCloseDelimited) {
            self.phase = DecodePhase::Complete;
        }
    }

    /// close-delimited ボディを読み取り中かどうかを判定
    pub fn is_close_delimited(&self) -> bool {
        matches!(self.phase, DecodePhase::BodyCloseDelimited)
    }

    /// ステータスコードからボディがあるかどうかを判定
    fn status_has_body(status_code: u16) -> bool {
        // RFC 9112 Section 6.3: 1xx, 204, 304 はボディなし
        !((100..200).contains(&status_code) || status_code == 204 || status_code == 304)
    }

    /// ボディモードを決定
    ///
    /// RFC 9112 Section 6.3 の優先順位に従う:
    /// 1. CONNECT 2xx はトンネルモード (Transfer-Encoding/Content-Length は無視)
    /// 2. HEAD レスポンス、1xx/204/304 はボディなし (Transfer-Encoding/Content-Length を解析しない)
    ///    注: 205 は送信者制約 (RFC 9110) だが、受信者はメッセージ長決定規則に従う
    /// 3. Transfer-Encoding がある場合:
    ///    - chunked が最後 → chunked
    ///    - chunked がないか最後でない → close-delimited
    /// 4. Content-Length がある場合は固定長
    /// 5. それ以外は close-delimited (接続が閉じるまでがボディ)
    fn determine_body_kind(&self, status_code: u16) -> Result<BodyKind, Error> {
        // RFC 9112 Section 6.1: HTTP/1.0 + Transfer-Encoding は framing fault
        let version = self
            .start_line
            .as_ref()
            .and_then(|sl| sl.split(' ').next())
            .unwrap_or("");
        if version == "HTTP/1.0"
            && self
                .headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("Transfer-Encoding"))
        {
            return Err(Error::InvalidData(
                "Transfer-Encoding is not defined in HTTP/1.0".to_string(),
            ));
        }

        // RFC 9112 Section 6.3: CONNECT メソッドへの 2xx レスポンスは
        // トンネルモードに切り替わる。Transfer-Encoding と Content-Length は無視される。
        // RFC 9110 Section 9.1: メソッドトークンは case-sensitive
        if let Some(ref method) = self.request_method
            && method == "CONNECT"
            && (200..300).contains(&status_code)
        {
            return Ok(BodyKind::Tunnel);
        }

        // RFC 9112 Section 6.3: HEAD/1xx/204/304 はボディなし
        // 205 は送信者がボディを生成してはならない (RFC 9110 Section 15.3.6) が、
        // 受信者はメッセージ長決定規則に従って処理する必要がある
        // これらのステータスでは Transfer-Encoding/Content-Length を解析しない
        // (不正な TE/CL があってもエラーにしない)
        if self.expect_no_body || !Self::status_has_body(status_code) {
            return Ok(BodyKind::None);
        }

        // ボディがある場合のみ TE/CL を解析
        let (te_result, content_length) = resolve_body_headers_for_response(&self.headers)?;

        match te_result {
            TransferEncodingResult::Chunked => return Ok(BodyKind::Chunked),
            TransferEncodingResult::Other => return Ok(BodyKind::CloseDelimited),
            TransferEncodingResult::None => {}
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

        // RFC 9112: TE も CL もない場合は close-delimited
        // 接続が閉じられるまでをボディとして扱う
        Ok(BodyKind::CloseDelimited)
    }

    /// ヘッダーをデコード
    ///
    /// ヘッダーが完了したら `Some((ResponseHead, BodyKind))` を返す
    /// データ不足の場合は `None` を返す
    /// 既にヘッダーデコード済みの場合はエラー
    pub fn decode_headers(&mut self) -> Result<Option<(ResponseHead, BodyKind)>, Error> {
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

                        // CR/LF チェック (埋め込まれた改行を拒否)
                        if line.contains('\r') || line.contains('\n') {
                            return Err(Error::InvalidData(
                                "invalid status line: contains CR/LF".to_string(),
                            ));
                        }

                        // Parse: VERSION SP STATUS-CODE SP REASON-PHRASE CRLF
                        let parts: Vec<&str> = line.splitn(3, ' ').collect();
                        if parts.len() < 2 {
                            return Err(Error::InvalidData(alloc::format!(
                                "invalid status line: {}",
                                line
                            )));
                        }

                        // プロトコルバージョンの検証
                        if !is_valid_protocol_version(parts[0]) {
                            return Err(Error::InvalidData(
                                "invalid status line: invalid protocol version".to_string(),
                            ));
                        }

                        // ステータスコードの検証 (RFC 9110 Section 15)
                        let status_code: u16 = parts[1].parse().map_err(|_| {
                            Error::InvalidData(alloc::format!(
                                "invalid status line: invalid status code: {}",
                                parts[1]
                            ))
                        })?;
                        if !is_valid_status_code(status_code) {
                            return Err(Error::InvalidData(alloc::format!(
                                "invalid status line: status code out of range: {}",
                                status_code
                            )));
                        }

                        // reason-phrase の検証 (RFC 9112 Section 4)
                        if let Some(reason) = parts.get(2)
                            && !is_valid_reason_phrase(reason)
                        {
                            return Err(Error::InvalidData(
                                "invalid status line: invalid reason-phrase".to_string(),
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

                            // ステータスコードを取得
                            let start_line = self.start_line.as_ref().ok_or_else(|| {
                                Error::InvalidData("missing status line".to_string())
                            })?;
                            let parts: Vec<&str> = start_line.splitn(3, ' ').collect();
                            let status_code: u16 = parts[1].parse().map_err(|_| {
                                Error::InvalidData(alloc::format!(
                                    "invalid status code: {}",
                                    parts[1]
                                ))
                            })?;

                            self.status_code = status_code;
                            let body_kind = self.determine_body_kind(status_code)?;

                            // ヘッダー完了、ボディフェーズに遷移
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
                                BodyKind::CloseDelimited => {
                                    self.phase = DecodePhase::BodyCloseDelimited;
                                }
                                BodyKind::None => {
                                    self.phase = DecodePhase::Complete;
                                }
                                BodyKind::Tunnel => {
                                    self.phase = DecodePhase::Tunnel;
                                }
                            }

                            // ResponseHead を構築
                            let start_line = self.start_line.take().unwrap();
                            let parts: Vec<&str> = start_line.splitn(3, ' ').collect();

                            let head = ResponseHead {
                                version: parts[0].to_string(),
                                status_code,
                                reason_phrase: parts.get(2).unwrap_or(&"").to_string(),
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
                    self.expect_no_body = false;
                    self.status_code = 0;
                    continue;
                }
                DecodePhase::Tunnel => {
                    return Err(Error::InvalidData(
                        "decode_headers cannot be used in tunnel mode".to_string(),
                    ));
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

    /// 利用可能なボディデータのバイト数を取得
    fn available_body_len(&self) -> usize {
        match &self.phase {
            DecodePhase::BodyContentLength { remaining } => {
                if *remaining >= self.buf.len() as u64 {
                    self.buf.len()
                } else {
                    *remaining as usize
                }
            }
            DecodePhase::BodyChunkedData { remaining } => self.buf.len().min(*remaining),
            DecodePhase::BodyCloseDelimited => self.buf.len(),
            _ => 0,
        }
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

    /// レスポンス全体を一括でデコード
    ///
    /// ストリーミング API (`decode_headers()` / `peek_body()` / `consume_body()`) を
    /// 内部で使用して、レスポンス全体をデコードする。
    ///
    /// データ不足の場合は `None` を返す。
    /// ストリーミング API と混在使用するとエラーを返す。
    ///
    /// ## close-delimited ボディの場合
    ///
    /// `BodyKind::CloseDelimited` の場合、接続が閉じられるまでがボディとなる。
    /// `decode()` を使う場合は、接続終了後に `mark_eof()` を呼んでから
    /// 再度 `decode()` を呼ぶ必要がある。
    pub fn decode(&mut self) -> Result<Option<Response>, Error> {
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
        let body_kind = *self.decoded_body_kind.as_ref().unwrap();
        match body_kind {
            BodyKind::Tunnel => {
                return Err(Error::InvalidData(
                    "decode() cannot be used in tunnel mode, use take_remaining() instead"
                        .to_string(),
                ));
            }
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
            BodyKind::CloseDelimited => {
                // close-delimited: バッファにあるデータを読み込み、mark_eof() を待つ
                let available = self.available_body_len();
                if available > 0 {
                    // max_body_size チェック (コピー前に行う)
                    // checked_add でオーバーフローを検出し、オーバーフロー時も BodyTooLarge を返す
                    let new_size = self.decoded_body.len().checked_add(available).ok_or(
                        Error::BodyTooLarge {
                            size: usize::MAX,
                            limit: self.limits.max_body_size,
                        },
                    )?;
                    if new_size > self.limits.max_body_size {
                        return Err(Error::BodyTooLarge {
                            size: new_size,
                            limit: self.limits.max_body_size,
                        });
                    }
                    self.decoded_body.extend_from_slice(&self.buf[..available]);
                    self.consume_body(available)?;
                }

                // mark_eof() が呼ばれて Complete になったか確認
                if !matches!(self.phase, DecodePhase::Complete) {
                    // まだ EOF でないのでデータ不足
                    return Ok(None);
                }
            }
            BodyKind::None => {}
        }

        // Response を構築
        // BodyKind::None / Tunnel は「フレーミングがない」ため body = None。
        // それ以外 (ContentLength / Chunked / CloseDelimited) は明示的なボディなので body = Some。
        let head = self.decoded_head.take().unwrap();
        let body = match body_kind {
            BodyKind::None | BodyKind::Tunnel => None,
            BodyKind::ContentLength(_) | BodyKind::Chunked | BodyKind::CloseDelimited => {
                Some(core::mem::take(&mut self.decoded_body))
            }
        };

        // Keep-Alive 対応: 次のレスポンスのために状態をリセット
        self.phase = DecodePhase::StartLine;
        self.decoded_body_kind = None;
        self.decoded_body.clear();
        self.body_decoder.reset();
        self.expect_no_body = false;
        self.status_code = 0;

        Ok(Some(Response {
            version: head.version,
            status_code: head.status_code,
            reason_phrase: head.reason_phrase,
            headers: head.headers,
            body,
            omit_body: false,
        }))
    }
}
