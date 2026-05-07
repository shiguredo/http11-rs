//! HTTP / HTTPS リクエストの送受信処理
//!
//! `ResponseDecoder` のストリーミング API
//! (`decode_headers` + `peek_body` / `consume_body` / `progress`) を使い、
//! `BodyKind::None` / `Length` / `Chunked` / `CloseDelimited` / `Tunnel` の
//! 各経路を網羅して受信する実装例。

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Instant;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, StreamOwned};
use rustls_platform_verifier::ConfigVerifierExt;
use shiguredo_http11::{BodyKind, BodyProgress, Response, ResponseDecoder, ResponseHead};
use tracing::info;

/// 1 回の `read` で要求する最大バイト数
const READ_CHUNK: usize = 8192;

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

    let mut decoder = ResponseDecoder::new();
    decoder.set_request_method(request_method);
    let mut head: Option<ResponseHead> = None;
    let mut body_kind: Option<BodyKind> = None;
    let mut body = Vec::new();
    let mut headers_at: Option<Instant> = None;
    let mut first_body_at: Option<Instant> = None;

    'outer: loop {
        let want = decoder.available_buf().min(READ_CHUNK);
        if want == 0 {
            return Err("decoder buffer full".into());
        }
        let buf = decoder.mut_buf(want)?;
        let n = stream.read(buf)?;
        if n == 0 {
            decoder.advance_buf(0);
            decoder.mark_eof();
        } else {
            decoder.advance_buf(n);
        }

        if head.is_none() {
            if let Some((h, k)) = decoder.decode_headers()? {
                headers_at = Some(Instant::now());
                head = Some(h);
                body_kind = Some(k);
            } else if n == 0 {
                return Err("Connection closed before headers complete".into());
            } else {
                continue;
            }
        }

        match body_kind.as_ref().unwrap() {
            BodyKind::None | BodyKind::Tunnel => break 'outer,
            _ => {}
        }
        loop {
            if let Some(data) = decoder.peek_body() {
                if first_body_at.is_none() {
                    first_body_at = Some(Instant::now());
                }
                body.extend_from_slice(data);
                let len = data.len();
                match decoder.consume_body(len)? {
                    BodyProgress::Complete { .. } => break 'outer,
                    // NeedData (chunked CRLF 不足) でも内側ループ継続。
                    // 直後の peek_body() は None を返すため progress 分岐に fall through する。
                    BodyProgress::Advanced | BodyProgress::NeedData => continue,
                }
            }
            match decoder.progress()? {
                BodyProgress::Complete { .. } => break 'outer,
                BodyProgress::Advanced => continue,
                // バッファ不足: 内側ループを抜けて外側の I/O ループに戻る
                BodyProgress::NeedData => break,
            }
        }

        if n == 0 {
            if matches!(body_kind, Some(BodyKind::CloseDelimited)) {
                continue;
            }
            return Err("Connection closed before response complete".into());
        }
    }

    let complete_at = Instant::now();
    let h = head.unwrap();
    let k = body_kind.unwrap();

    info!(
        connect_ms = request_sent_at.duration_since(connect_at).as_millis() as u64,
        ttfb_ms = headers_at
            .unwrap()
            .duration_since(request_sent_at)
            .as_millis() as u64,
        first_body_ms = first_body_at.map(|t| t.duration_since(request_sent_at).as_millis() as u64),
        total_ms = complete_at.duration_since(request_sent_at).as_millis() as u64,
        "Timing"
    );

    let body_field = match k {
        BodyKind::None | BodyKind::Tunnel => None,
        _ => Some(body),
    };
    let mut response = Response::with_version(&h.version, h.status_code, &h.reason_phrase)?;
    for (name, value) in h.headers {
        response.add_header(&name, &value)?;
    }
    if let Some(b) = body_field {
        response = response.body(b);
    }
    Ok(response)
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

    let mut decoder = ResponseDecoder::new();
    decoder.set_request_method(request_method);
    let mut head: Option<ResponseHead> = None;
    let mut body_kind: Option<BodyKind> = None;
    let mut body = Vec::new();
    let mut headers_at: Option<Instant> = None;
    let mut first_body_at: Option<Instant> = None;

    'outer: loop {
        let want = decoder.available_buf().min(READ_CHUNK);
        if want == 0 {
            return Err("decoder buffer full".into());
        }
        let buf = decoder.mut_buf(want)?;
        let n = match tls.read(buf) {
            Ok(0) => {
                decoder.advance_buf(0);
                decoder.mark_eof();
                0
            }
            Ok(n) => {
                decoder.advance_buf(n);
                n
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                decoder.advance_buf(0);
                continue;
            }
            Err(e) => {
                decoder.advance_buf(0);
                return Err(e.into());
            }
        };

        if head.is_none() {
            if let Some((h, k)) = decoder.decode_headers()? {
                headers_at = Some(Instant::now());
                head = Some(h);
                body_kind = Some(k);
            } else if n == 0 {
                return Err("Connection closed before headers complete".into());
            } else {
                continue;
            }
        }

        match body_kind.as_ref().unwrap() {
            BodyKind::None | BodyKind::Tunnel => break 'outer,
            _ => {}
        }
        loop {
            if let Some(data) = decoder.peek_body() {
                if first_body_at.is_none() {
                    first_body_at = Some(Instant::now());
                }
                body.extend_from_slice(data);
                let len = data.len();
                match decoder.consume_body(len)? {
                    BodyProgress::Complete { .. } => break 'outer,
                    // NeedData (chunked CRLF 不足) でも内側ループ継続。
                    // 直後の peek_body() は None を返すため progress 分岐に fall through する。
                    BodyProgress::Advanced | BodyProgress::NeedData => continue,
                }
            }
            match decoder.progress()? {
                BodyProgress::Complete { .. } => break 'outer,
                BodyProgress::Advanced => continue,
                // バッファ不足: 内側ループを抜けて外側の I/O ループに戻る
                BodyProgress::NeedData => break,
            }
        }

        if n == 0 {
            if matches!(body_kind, Some(BodyKind::CloseDelimited)) {
                continue;
            }
            return Err("Connection closed before response complete".into());
        }
    }

    let complete_at = Instant::now();
    let h = head.unwrap();
    let k = body_kind.unwrap();

    info!(
        connect_ms = request_sent_at.duration_since(connect_at).as_millis() as u64,
        ttfb_ms = headers_at
            .unwrap()
            .duration_since(request_sent_at)
            .as_millis() as u64,
        first_body_ms = first_body_at.map(|t| t.duration_since(request_sent_at).as_millis() as u64),
        total_ms = complete_at.duration_since(request_sent_at).as_millis() as u64,
        "Timing"
    );

    let body_field = match k {
        BodyKind::None | BodyKind::Tunnel => None,
        _ => Some(body),
    };
    let mut response = Response::with_version(&h.version, h.status_code, &h.reason_phrase)?;
    for (name, value) in h.headers {
        response.add_header(&name, &value)?;
    }
    if let Some(b) = body_field {
        response = response.body(b);
    }
    Ok(response)
}
