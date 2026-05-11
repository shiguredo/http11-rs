//! HTTP / HTTPS リクエストの送受信処理
//!
//! `ResponseDecoder` のストリーミング API
//! (`decode_headers` + `peek_body` / `consume_body` / `progress`) と
//! `Decompressor` トレイト実装 (`AnyDecompressor`) を組み合わせ、
//! `BodyKind::None` / `Length` / `Chunked` / `CloseDelimited` / `Tunnel` の
//! 各経路を網羅しつつ、Content-Encoding に応じた展開もストリーミングで行う実装例。
//!
//! 本ファイルは `peek_body()` で raw な圧縮バイト列を取得し、
//! `AnyDecompressor::decompress` で 8 KiB 単位の出力バッファに段階的に展開する
//! 経路を示す (peek_body と Decompressor を手動で連携させる典型パターン)。
//! 一方 `ResponseDecoder::peek_body_decompressed` を使う経路は integration test
//! 側 (`tests/nginx_streaming.rs::peek_body_decompressed_streams_gzip`) で示す。

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Instant;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, StreamOwned};
use rustls_platform_verifier::ConfigVerifierExt;
use shiguredo_http11::compression::{CompressionStatus, Decompressor, NoCompression};
use shiguredo_http11::{BodyKind, BodyProgress, HttpHead, Response, ResponseDecoder, ResponseHead};
use tracing::info;

use crate::decompressor::AnyDecompressor;

/// 1 回の `read` で要求する最大バイト数
const READ_CHUNK: usize = 8192;
/// 展開出力を 1 回で受ける作業バッファのサイズ
///
/// 8 KiB に固定して大きな body も小さな出力バッファでストリーミング展開できることを
/// サンプルとして示す。これより大きい展開結果は OutputFull で複数回に分けて受ける。
const DECOMPRESS_OUTPUT_CAP: usize = 8192;

/// 平文 HTTP で 1 リクエスト送信して 1 レスポンスを受信する (Keep-Alive 不使用)
///
/// `request_method` は decoder に伝えるために必要。HEAD / CONNECT のように
/// レスポンスボディが抑止されるケース (RFC 9110 §9.3.2 / §9.3.6) を
/// `BodyKind::None` / `BodyKind::Tunnel` として正しく扱うため。
pub fn http_request(
    host: &str,
    port: u16,
    request_method: &str,
    request_bytes: &[u8],
) -> Result<Response, Box<dyn std::error::Error + Send + Sync>> {
    let connect_at = Instant::now();
    let mut stream = TcpStream::connect((host, port))?;
    stream.write_all(request_bytes)?;
    let request_sent_at = Instant::now();

    let mut session = ResponseSession::new(request_method);
    let mut output_buf = vec![0u8; DECOMPRESS_OUTPUT_CAP];

    'outer: loop {
        let want = session.decoder.available_buf().min(READ_CHUNK);
        if want == 0 {
            return Err("decoder buffer full".into());
        }
        let buf = session.decoder.mut_buf(want)?;
        let n = stream.read(buf)?;
        if n == 0 {
            session.decoder.advance_buf(0);
            session.decoder.mark_eof();
        } else {
            session.decoder.advance_buf(n);
        }

        if session.try_decode_headers()? {
            // ヘッダーが揃った
        } else if n == 0 {
            return Err("Connection closed before headers complete".into());
        } else {
            continue;
        }

        if session.body_done() {
            break 'outer;
        }
        if session.pump_body(&mut output_buf)? {
            break 'outer;
        }

        if n == 0 {
            if matches!(session.body_kind, Some(BodyKind::CloseDelimited)) {
                continue;
            }
            return Err("Connection closed before response complete".into());
        }
    }

    session.finish(connect_at, request_sent_at)
}

/// HTTPS で 1 リクエスト送信して 1 レスポンスを受信する (Keep-Alive 不使用)
///
/// `request_method` は decoder に伝えるために必要。HEAD / CONNECT のように
/// レスポンスボディが抑止されるケース (RFC 9110 §9.3.2 / §9.3.6) を
/// `BodyKind::None` / `BodyKind::Tunnel` として正しく扱うため。
pub fn https_request(
    host: &str,
    port: u16,
    request_method: &str,
    request_bytes: &[u8],
) -> Result<Response, Box<dyn std::error::Error + Send + Sync>> {
    let connect_at = Instant::now();
    let config = ClientConfig::with_platform_verifier()?;
    let server_name = ServerName::try_from(host.to_string())?;
    let conn = ClientConnection::new(Arc::new(config), server_name)?;
    let sock = TcpStream::connect((host, port))?;
    let mut tls = StreamOwned::new(conn, sock);

    tls.write_all(request_bytes)?;
    let request_sent_at = Instant::now();

    let mut session = ResponseSession::new(request_method);
    let mut output_buf = vec![0u8; DECOMPRESS_OUTPUT_CAP];

    'outer: loop {
        let want = session.decoder.available_buf().min(READ_CHUNK);
        if want == 0 {
            return Err("decoder buffer full".into());
        }
        let buf = session.decoder.mut_buf(want)?;
        let n = match tls.read(buf) {
            Ok(0) => {
                session.decoder.advance_buf(0);
                session.decoder.mark_eof();
                0
            }
            Ok(n) => {
                session.decoder.advance_buf(n);
                n
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                session.decoder.advance_buf(0);
                continue;
            }
            Err(e) => {
                session.decoder.advance_buf(0);
                return Err(e.into());
            }
        };

        if session.try_decode_headers()? {
            // ヘッダーが揃った
        } else if n == 0 {
            return Err("Connection closed before headers complete".into());
        } else {
            continue;
        }

        if session.body_done() {
            break 'outer;
        }
        if session.pump_body(&mut output_buf)? {
            break 'outer;
        }

        if n == 0 {
            if matches!(session.body_kind, Some(BodyKind::CloseDelimited)) {
                continue;
            }
            return Err("Connection closed before response complete".into());
        }
    }

    session.finish(connect_at, request_sent_at)
}

/// 1 レスポンスのデコード状態を集約するヘルパー
///
/// `decoder` が raw 受信を担当し、`decompressor` が Content-Encoding に応じて
/// 展開を担当する。`body` には展開済みのバイト列が蓄積される。
struct ResponseSession {
    decoder: ResponseDecoder,
    decompressor: AnyDecompressor,
    head: Option<ResponseHead>,
    body_kind: Option<BodyKind>,
    body: Vec<u8>,
    headers_at: Option<Instant>,
    first_body_at: Option<Instant>,
    encoding_label: String,
}

impl ResponseSession {
    fn new(request_method: &str) -> Self {
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method(request_method);
        Self {
            decoder,
            decompressor: AnyDecompressor::None(NoCompression::new()),
            head: None,
            body_kind: None,
            body: Vec::new(),
            headers_at: None,
            first_body_at: None,
            encoding_label: String::new(),
        }
    }

    /// 今回の I/O ループでヘッダーが解析できれば `decompressor` も確定させ true を返す
    fn try_decode_headers(&mut self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        if self.head.is_some() {
            return Ok(true);
        }
        let Some((head, body_kind)) = self.decoder.decode_headers()? else {
            return Ok(false);
        };

        // RFC 9110 Section 8.4 では Content-Encoding に複数値 (chained encoding,
        // 例: `gzip, br`) や複数ヘッダーが許される。本サンプルでは
        // 1) 最初の Content-Encoding ヘッダーのみを参照する
        // 2) chained encoding の値 (カンマ区切り) は AnyDecompressor::for_encoding
        //    で「未知のエンコーディング」として弾く
        // という単純化を行っている。実用上ほぼ単一 encoding なので問題は少ないが、
        // chained encoding に対応する場合は値をカンマで分割し各 encoding を順に
        // 適用するパイプラインを組む必要がある (本サンプルのスコープ外)。
        let encoding = head
            .headers()
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("Content-Encoding"))
            .map(|(_, v)| v.as_str())
            .unwrap_or("");
        self.decompressor = AnyDecompressor::for_encoding(encoding)?;
        self.encoding_label = encoding.to_string();

        self.headers_at = Some(Instant::now());
        self.head = Some(head);
        self.body_kind = Some(body_kind);
        Ok(true)
    }

    /// `BodyKind::None` / `BodyKind::Tunnel` 等でボディ受信不要のケースを判定
    fn body_done(&self) -> bool {
        matches!(
            self.body_kind.as_ref().unwrap(),
            BodyKind::None | BodyKind::Tunnel
        )
    }

    /// peek_body / progress を回してボディを 1 バッチ分処理する
    ///
    /// 戻り値: `true` ならレスポンス完了 (外側ループを抜けてよい)
    fn pump_body(
        &mut self,
        output_buf: &mut [u8],
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        loop {
            if let Some(data) = self.decoder.peek_body() {
                if self.first_body_at.is_none() {
                    self.first_body_at = Some(Instant::now());
                }

                let total_consumed =
                    decompress_chunk(&mut self.decompressor, data, output_buf, &mut self.body)?;
                if total_consumed > 0 {
                    match self.decoder.consume_body(total_consumed)? {
                        BodyProgress::Complete { .. } => {
                            drain_decompressor(&mut self.decompressor, output_buf, &mut self.body)?;
                            return Ok(true);
                        }
                        BodyProgress::Advanced | BodyProgress::NeedData => continue,
                    }
                }
                // total_consumed == 0 (decompressor が input を消費しなかった) は通常起きない。
                // 安全側として progress() に流して状態を進める。
            }
            match self.decoder.progress()? {
                BodyProgress::Complete { .. } => {
                    drain_decompressor(&mut self.decompressor, output_buf, &mut self.body)?;
                    return Ok(true);
                }
                BodyProgress::Advanced => continue,
                // バッファ不足: 内側ループを抜けて外側の I/O ループに戻る
                BodyProgress::NeedData => return Ok(false),
            }
        }
    }

    fn finish(
        self,
        connect_at: Instant,
        request_sent_at: Instant,
    ) -> Result<Response, Box<dyn std::error::Error + Send + Sync>> {
        let complete_at = Instant::now();
        let head = self.head.ok_or("no response head")?;
        let body_kind = self.body_kind.ok_or("no body kind")?;

        info!(
            connect_ms = request_sent_at.duration_since(connect_at).as_millis() as u64,
            ttfb_ms = self
                .headers_at
                .ok_or("no headers timing")?
                .duration_since(request_sent_at)
                .as_millis() as u64,
            first_body_ms = self
                .first_body_at
                .map(|t| t.duration_since(request_sent_at).as_millis() as u64),
            total_ms = complete_at.duration_since(request_sent_at).as_millis() as u64,
            "Timing"
        );
        if !self.encoding_label.is_empty() {
            info!(
                encoding = self.encoding_label.as_str(),
                decompressed_size = self.body.len(),
                "Decompressed (streaming)"
            );
        }

        let body_field = match body_kind {
            BodyKind::None | BodyKind::Tunnel => None,
            _ => Some(self.body),
        };
        let mut response =
            Response::with_version(head.version(), head.status_code(), head.reason_phrase())?;
        for (name, value) in head.headers() {
            response.add_header(name, value)?;
        }
        if let Some(b) = body_field {
            response = response.body(b);
        }
        Ok(response)
    }
}

/// 1 回 peek_body() で得た raw bytes を展開して `body` に書き込み、
/// 消費したバイト数 (raw 側) を返す
///
/// 内部で OutputFull が起きる間は同じ input を進めながらループし、
/// Continue / Complete に到達するか input を使い切ったら戻る。
/// `consumed = 0 && produced = 0` (進展なし) を観測したら、再呼び出しが
/// 同じ結果を返すしかない (decompressor が input/output 不足を訴えている)
/// ため無限ループ防止のために return する。
fn decompress_chunk(
    decompressor: &mut AnyDecompressor,
    input: &[u8],
    output_buf: &mut [u8],
    body: &mut Vec<u8>,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let mut remaining = input;
    let mut total_consumed = 0usize;

    loop {
        let status = decompressor.decompress(remaining, output_buf)?;
        let produced = status.produced();
        let consumed = status.consumed();
        if produced > 0 {
            body.extend_from_slice(&output_buf[..produced]);
        }
        remaining = &remaining[consumed..];
        total_consumed += consumed;

        if matches!(status, CompressionStatus::Complete { .. }) {
            return Ok(total_consumed);
        }

        // 進展なし: OutputFull / Continue いずれの場合も同じ呼び出しを繰り返すと
        // 無限ループに陥るので呼び出し側にコントロールを返す
        if consumed == 0 && produced == 0 {
            return Ok(total_consumed);
        }

        // Continue で input を使い切ったら戻る (より多くの入力データが必要)
        if matches!(status, CompressionStatus::Continue { .. }) && remaining.is_empty() {
            return Ok(total_consumed);
        }

        // OutputFull もしくは remaining 残りありの Continue: drain / feed 続行
    }
}

/// ボディ受信完了後、展開器の内部バッファに残ったバイトを全て drain する
///
/// `produced = 0` (input 空なので consumed も自明に 0) を観測したら drain 完了。
/// `OutputFull { 0, 0 }` 等の degenerate ケースでも無限ループしない。
fn drain_decompressor(
    decompressor: &mut AnyDecompressor,
    output_buf: &mut [u8],
    body: &mut Vec<u8>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    loop {
        let status = decompressor.decompress(&[], output_buf)?;
        let produced = status.produced();
        if produced > 0 {
            body.extend_from_slice(&output_buf[..produced]);
        }

        if matches!(status, CompressionStatus::Complete { .. }) {
            return Ok(());
        }

        // 進展なし: 内部 buffer に未 drain のバイトはなく、無限ループ防止のため戻る
        if produced == 0 {
            return Ok(());
        }

        // OutputFull / Continue で produced > 0: drain 続行
    }
}
