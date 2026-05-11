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
    BodyDecoder, BodyKind, BodyProgress, collect_declared_trailers, find_line, parse_header_line,
    parse_request_target_form, resolve_body_headers_for_request,
    validate_request_target_for_method,
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

    /// バッファの残りデータを取り出す (トンネルモード用)
    ///
    /// CONNECT リクエスト受信後にトンネルモードに切り替わった場合、
    /// このメソッドでヘッダー終端以降のデータを取り出してトンネルの相手側
    /// (バックエンドサーバ) に転送する。RFC 9110 Section 9.3.6 が要求する
    /// 「ヘッダー終端直後からの transparent な転送」を実現するための API。
    ///
    /// 呼び出し後、バッファは空になる。
    pub fn take_remaining(&mut self) -> Vec<u8> {
        debug_assert!(
            self.pending == 0,
            "take_remaining called with pending mut_buf"
        );
        // 不変条件 "buf 空 ↔ pending == 0" を維持する。
        // release ビルドで debug_assert! が消えた状態で契約違反 (pending > 0 で
        // 呼ばれた場合) でも、内部状態の整合性を保つために明示的にリセットする。
        self.pending = 0;
        core::mem::take(&mut self.buf)
    }

    /// トンネルモードかどうかを判定
    ///
    /// CONNECT リクエストのヘッダーを受信した直後はトンネルモードになる。
    /// サーバが 2xx で応答できる場合、ここから先のバイト列はトンネルデータ
    /// として扱う。サーバが 4xx/5xx を返す場合は接続をクローズするか、
    /// `reset()` でデコーダーをリセットして通常モードに戻す。
    pub fn is_tunnel(&self) -> bool {
        matches!(self.phase, DecodePhase::Tunnel)
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

                        // 送信側ポリシーとの一貫性のため、decoder 側でも obs-text (0x80-0xFF) を拒否する。
                        // is_valid_request_target は受信側互換性のため obs-text を許容するが、
                        // 構築された Request は送信されることを前提とするため、ここで早期に拒否する。
                        // 注: validate.rs 側の obs-text 許容撤去は別 issue で対応する暫定措置である。
                        if parts[1].bytes().any(|b| b >= 0x80) {
                            return Err(Error::InvalidData(
                                "invalid request-target: non-ASCII characters".to_string(),
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

                            // RFC 9110 Section 9.3.6:
                            // "A CONNECT request message does not have content."
                            // "When a server responds with a 2xx (Successful) status code to a
                            //  CONNECT request, the connection becomes a tunnel immediately
                            //  after the header section, with the connection used as-is to
                            //  convey the data of the tunnel."
                            //
                            // CONNECT 受信時はヘッダー終端直後の任意バイト列をトンネルデータと
                            // して扱う必要がある。`BodyKind::None` で Complete 遷移してしまうと
                            // 後続バイトが「次の HTTP リクエスト」として decode_headers で
                            // parse されはじめ、HTTP Request Smuggling 経路を生む。
                            // ResponseDecoder の 2xx 応答経路と対称に `BodyKind::Tunnel` に
                            // 遷移させ、`take_remaining()` で transparent に転送できるようにする。
                            //
                            // CONNECT 失敗時 (サーバが 4xx/5xx を返す等) は呼出側で `reset()`
                            // して通常のリクエスト処理に戻すか、接続をクローズする。
                            //
                            // RFC は CONNECT リクエスト側の Content-Length / Transfer-Encoding
                            // を MUST NOT としていない (MUST NOT は 2xx レスポンス側の制約)
                            // ため、それらヘッダーが付いていても即エラーにはしない。
                            let method = start_line_ref.split(' ').next().unwrap_or("");
                            let body_kind = if method == "CONNECT" {
                                BodyKind::Tunnel
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
                                    // CONNECT リクエスト: ヘッダー終端後はトンネルモード。
                                    // 後続バイトは `take_remaining()` で取り出す。
                                    self.phase = DecodePhase::Tunnel;
                                }
                            }

                            // RFC 9110 Section 6.5.1 のホワイトリスト方式 trailer 受理に
                            // 必要な「申告された trailer フィールド名リスト」を、
                            // ヘッダーから `Trailer:` を抽出して BodyDecoder に渡す。
                            // chunked 以外の本 body kind では trailer は来ないが、
                            // BodyDecoder は body kind を問わず参照するため常に設定する。
                            let declared_trailers = collect_declared_trailers(&self.headers);
                            self.body_decoder.set_declared_trailers(declared_trailers);

                            // RequestHead を構築
                            let start_line = self.start_line.take().ok_or_else(|| {
                                Error::InvalidData("missing request line".to_string())
                            })?;
                            let parts: Vec<&str> = start_line.splitn(3, ' ').collect();

                            let head = RequestHead::from_validated_parts(
                                parts[0].to_string(),
                                parts[1].to_string(),
                                parts[2].to_string(),
                                core::mem::take(&mut self.headers),
                            );

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
    /// ボディデータが枯渇しても展開器の内部 buffer に未 drain のバイトが
    /// 残っている場合があるため、その場合は空 input で展開器を駆動して
    /// 残りを drain する。
    ///
    /// # 引数
    /// - `output`: 展開データを書き込む出力バッファ
    ///
    /// # 戻り値
    /// - `Ok(Some(status))`: 展開成功。`status.produced()` バイトが output に書き込まれた。
    ///   `status.consumed()` バイトを `consume_body()` で消費する必要がある (0 のときは不要)。
    /// - `Ok(None)`: 利用可能なボディデータがなく、展開器の内部 buffer も空
    /// - `Err(e)`: 展開エラー
    ///
    /// # 使い方
    ///
    /// ```ignore
    /// let mut output = vec![0u8; 8192];
    /// while let Some(status) = decoder.peek_body_decompressed(&mut output)? {
    ///     // output[..status.produced()] に展開済みデータ
    ///     process(&output[..status.produced()]);
    ///     if status.consumed() > 0 {
    ///         decoder.consume_body(status.consumed())?;
    ///     }
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
        // ボディデータがあればそれを、なければ空 input を渡して展開器内部の
        // 未 drain バイトを取り出す。
        // 例: noflate::gzip::Decoder のように feed したバイトを内部 buffer に
        //     蓄積する型の Decompressor 実装では、ボディ末尾の chunk を feed
        //     した後でも内部 buffer に展開済みバイトが残ることがある。
        let input = self
            .body_decoder
            .peek_body(&self.buf, &self.phase)
            .unwrap_or(&[]);
        let status = self.decompressor.decompress(input, output)?;

        // 進展なし & 待機すべき状態でなければ None を返す。
        // - Continue { 0, 0 }: 入力も出力もない、より多くのボディデータが必要
        // - Complete { 0, 0 }: 終端到達後の重複呼び出し
        // どちらも呼び出し側がループを抜けるべきタイミング。
        // 一方 OutputFull { 0, 0 } は「output buffer が小さすぎる」back-pressure
        // のシグナルなので Some で返し、呼び出し側に通知する。
        match status {
            CompressionStatus::Continue {
                consumed: 0,
                produced: 0,
            }
            | CompressionStatus::Complete {
                consumed: 0,
                produced: 0,
            } => Ok(None),
            _ => Ok(Some(status)),
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
                return Err(Error::InvalidData(
                    "decode() cannot be used in tunnel mode, use take_remaining() instead"
                        .to_string(),
                ));
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

        Ok(Some(Request::from_raw_parts(
            head.method,
            head.uri,
            head.version,
            head.headers,
            body,
        )))
    }
}
