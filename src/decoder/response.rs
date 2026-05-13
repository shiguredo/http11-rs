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
    BodyDecoder, BodyKind, BodyProgress, TransferEncodingResult, collect_declared_trailers,
    find_line, parse_header_line, resolve_body_headers_for_response,
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
    /// リクエストメソッド (HEAD/CONNECT 判定用)
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
            status_code: 0,
            decoded_head: None,
            decoded_body_kind: None,
            decoded_body: Vec::new(),
            decompressor,
            request_method: None,
            pending: 0,
        }
    }

    /// リクエストメソッドを設定 (HEAD/CONNECT 判定用)
    ///
    /// レスポンスの元となったリクエストのメソッドを通知する。
    ///
    /// - `"HEAD"` を渡すと、Content-Length / Transfer-Encoding の値に関わらず
    ///   ボディなしとして扱う (RFC 9112 Section 6.3 item 1)。
    /// - `"CONNECT"` を渡すと、2xx レスポンスはトンネルモードに切り替わる
    ///   (RFC 9112 Section 6.3 item 2)。この場合、ボディは存在せず、
    ///   バッファ残りデータはトンネルデータとなる。
    ///
    /// RFC 9110 Section 9.1: メソッドトークンは case-sensitive。
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
        // 不変条件 "buf 空 ↔ pending == 0" を維持する。
        // release ビルドで debug_assert! が消えた状態で契約違反 (pending > 0 で
        // 呼ばれた場合) でも、内部状態の整合性を保つために明示的にリセットする。
        self.pending = 0;
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
    /// RFC 9112 Section 6.3 の "in order of precedence" に従う。
    /// 将来 RFC が改訂された場合は、優先順位の見直しが必要になる可能性がある。
    ///
    /// 1. item 1: HEAD レスポンス / 1xx / 204 / 304 はボディなし
    ///    (CONNECT + 204 もここで吸収される)
    /// 2. item 2: CONNECT への 2xx (204 を除く) はトンネルモード
    /// 3. RFC 9112 Section 6.1: HTTP/1.0 + Transfer-Encoding は framing fault
    /// 4. item 3〜8: Transfer-Encoding / Content-Length 解析
    fn determine_body_kind(&mut self, status_code: u16) -> Result<BodyKind, Error> {
        // RFC 9112 Section 6.3 item 1: HEAD レスポンス、1xx/204/304 はヘッダー
        // フィールドの内容に関わらずヘッダー終了で終わる。
        // "in order of precedence" により item 1 が最優先で評価される。
        // RFC 9110 Section 9.1: メソッドトークンは case-sensitive。
        // 205 は status_has_body が true を返すためここではマッチせず、後続の
        // TE/CL 解析に進む (RFC 9110 Section 15.3.6: 送信者制約のみ)。
        // 将来 RFC が改訂されてステータスコードの分類が変わる可能性がある。
        if self.request_method.as_deref().is_some_and(|m| m == "HEAD")
            || !Self::status_has_body(status_code)
        {
            return Ok(BodyKind::None);
        }

        // RFC 9112 Section 6.3 item 2: CONNECT メソッドへの 2xx レスポンスは
        // トンネルモードに切り替わる。Transfer-Encoding と Content-Length は無視される。
        // item 1 で 1xx/204/304 は既に返っているため、ここに到達するのは
        // status が 200-203, 205-299 でかつ request_method == "CONNECT" の場合のみ。
        // RFC 9110 Section 9.3.6: CONNECT への 2xx はヘッダー終了直後にトンネル
        // モードへ切り替わる。
        // RFC 9110 Section 9.1: メソッドトークンは case-sensitive。
        if self
            .request_method
            .as_deref()
            .is_some_and(|m| m == "CONNECT")
            && (200..300).contains(&status_code)
        {
            // RFC 9110 Section 9.3.6: "A client MUST ignore any Content-Length or
            // Transfer-Encoding header fields received in a successful response to CONNECT"
            // MUST ignore を物理消去で実装する。`ResponseHead.headers` から両ヘッダーを
            // 除去することで、上位アプリ (reverse proxy 等) が `head.get_header(...)` /
            // `head.content_length()` / `head.is_chunked()` 経由で値を観測して
            // 下流に再生成し HTTP Response Smuggling の足場とすることを防ぐ。
            // 将来 RFC が改訂されて CONNECT 2xx の framing が変更される可能性がある。
            self.headers.retain(|(name, _)| {
                !name.eq_ignore_ascii_case("Transfer-Encoding")
                    && !name.eq_ignore_ascii_case("Content-Length")
            });
            return Ok(BodyKind::Tunnel);
        }

        // RFC 9112 Section 6.1 (Transfer-Encoding は HTTP/1.1 のみで定義) および
        // RFC 2326 Section 5 (RTSP では Transfer-Encoding は未定義) に従い、
        // HTTP/1.1 完全一致以外で Transfer-Encoding が出現した場合は framing fault
        // として reject する。HRS (CWE-444) の足場となる version 偽装
        // (HTTP/0.9 / 2.0 / 3.0 / RTSP/x / FOO/1.0 等) を遮断する。
        // HTTP/1.2 が将来定義された場合は別途検討する (将来変更される可能性がある)。
        // item 1 で HEAD/1xx/204/304 は既に返っているため、このチェックに到達
        // するのはボディが存在しうるレスポンスのみ。
        let version = self
            .start_line
            .as_ref()
            .and_then(|sl| sl.split(' ').next())
            .unwrap_or("");
        if version != "HTTP/1.1"
            && self
                .headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("Transfer-Encoding"))
        {
            return Err(Error::InvalidData(
                "Transfer-Encoding is only defined for HTTP/1.1".to_string(),
            ));
        }

        // RFC 9112 Section 6.3 item 3〜8: TE/CL 解析。
        // chunked → Chunked (item 4)、非 chunked → CloseDelimited (item 4)、
        // CL → ContentLength (item 6)、どちらもなし → CloseDelimited (item 8)。
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

        // RFC 9112 Section 6.3 item 8: TE も CL もない場合は close-delimited
        // (接続が閉じられるまでをボディとして扱う)
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
                        // 空文字列は status-line ABNF における reason-phrase absent
                        // (`HTTP/1.1 200 \r\n`) として許容する。
                        if let Some(reason) = parts.get(2)
                            && !reason.is_empty()
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
                            // 空行 — ヘッダーセクション終端
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

                            // RFC 9110 Section 6.5.1 ホワイトリスト方式 trailer 受理用に、
                            // ヘッダーから `Trailer:` フィールドで申告された名前を抽出して
                            // BodyDecoder に渡す。chunked 以外の body kind では trailer は
                            // 来ないが、BodyDecoder は body kind を問わず参照する。
                            let declared_trailers = collect_declared_trailers(&self.headers);
                            self.body_decoder.set_declared_trailers(declared_trailers);

                            // ResponseHead を構築
                            let start_line = self.start_line.take().unwrap();
                            let parts: Vec<&str> = start_line.splitn(3, ' ').collect();

                            let head = ResponseHead::from_validated_parts(
                                parts[0].to_string(),
                                status_code,
                                parts.get(2).unwrap_or(&"").to_string(),
                                core::mem::take(&mut self.headers),
                            );

                            return Ok(Some((head, body_kind)));
                        } else {
                            // ヘッダー行サイズ上限の検査
                            if pos > self.limits.max_header_line_size {
                                return Err(Error::HeaderLineTooLong {
                                    size: pos,
                                    limit: self.limits.max_header_line_size,
                                });
                            }

                            // ヘッダー数上限の検査
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
                    self.status_code = 0;
                    // request_method は元のリクエストごとに設定し直す前提で
                    // ここでクリアする。クリアしないと Keep-Alive 接続で前回の
                    // CONNECT などが残り、次のレスポンスを誤ってトンネル判定
                    // してしまう状態漏れバグになる。
                    self.request_method = None;
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
    ///
    /// # 多段階遷移の注意
    ///
    /// 単一呼出で複数のデコードフェーズを跨ぐ場合があるため、戻り値が
    /// `Advanced` であっても `BodyProgress::Complete` に達している可能性がある。
    /// 完了判定には戻り値だけでなく `self.phase` (内部状態) も併せて確認すること。
    /// ストリーミング API の実装例では `matches!(self.phase, DecodePhase::Complete)`
    /// で直接 phase を確認している。
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
            BodyKind::CloseDelimited => {
                // close-delimited: バッファにあるデータを読み込み、mark_eof() を待つ。
                // max_body_size チェックは consume_body 内でも行うが、ここでは
                // decoded_body に書き込む前にも事前チェックする (既存コードと同様)。
                if let Some(data) = self.body_decoder.peek_body(&self.buf, &self.phase) {
                    let len = data.len();
                    let new_size =
                        self.decoded_body
                            .len()
                            .checked_add(len)
                            .ok_or(Error::BodyTooLarge {
                                size: usize::MAX,
                                limit: self.limits.max_body_size,
                            })?;
                    if new_size > self.limits.max_body_size {
                        return Err(Error::BodyTooLarge {
                            size: new_size,
                            limit: self.limits.max_body_size,
                        });
                    }
                    self.decoded_body.extend_from_slice(data);
                    self.consume_body(len)?;
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
        self.status_code = 0;
        // request_method は元のリクエストごとに設定し直す前提でクリアする。
        // クリアしないと Keep-Alive 接続で前回の CONNECT などが残り、次のレス
        // ポンスを誤ってトンネル判定してしまう状態漏れバグになる。
        self.request_method = None;

        Ok(Some(Response::from_raw_parts(
            head.version,
            head.status_code,
            head.reason_phrase,
            head.headers,
            body,
        )))
    }
}
