//! HTTP/1.1 リバースプロキシの例（ストリーミング対応）
//!
//! 使い方:
//!   cargo run -p http11_reverse_proxy -- --port 8888 --upstream https://example.com
//!   curl http://localhost:8888/

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, StreamOwned};
use rustls_platform_verifier::ConfigVerifierExt;
use shiguredo_http11::{
    BodyKind, BodyProgress, DecoderLimits, Request, RequestDecoder, Response, ResponseDecoder,
    encode_response_headers,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream as TokioTcpStream};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = noargs::raw_args();
    args.metadata_mut().app_name = "http11_reverse_proxy";

    // --help フラグ
    noargs::HELP_FLAG.take_help(&mut args);

    // --version フラグ
    let version_flag: bool = noargs::flag("version")
        .short('V')
        .doc("Show version")
        .take(&mut args)
        .is_present();
    if version_flag {
        println!("{}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    // --debug フラグ
    let debug: bool = noargs::flag("debug")
        .doc("Enable debug logging")
        .take(&mut args)
        .is_present();

    // --port オプション
    let port: u16 = noargs::opt("port")
        .short('p')
        .doc("Port to listen on (default: 8888)")
        .default("8888")
        .take(&mut args)
        .then(|o| o.value().parse())
        .map_err(|e| format!("{:?}", e))?;

    // --upstream オプション
    let upstream_url: String = noargs::opt("upstream")
        .short('u')
        .doc("Upstream URL (default: https://example.com)")
        .default("https://example.com")
        .take(&mut args)
        .then(|o| Ok::<_, &str>(o.value().to_string()))
        .map_err(|e| format!("{:?}", e))?;

    // 未知の引数があればエラー、ヘルプが返されたら表示
    if let Some(help) = args.finish().map_err(|e| format!("{:?}", e))? {
        print!("{}", help);
        std::process::exit(0);
    }

    // upstream URL からホスト名を抽出
    let upstream_host = parse_upstream_url(&upstream_url)?;

    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await?;

    println!("リバースプロキシをバインド: {}", addr);
    println!("  http://localhost:{}/ -> {}/", port, upstream_url);

    loop {
        let (socket, _) = listener.accept().await?;
        let upstream_host = upstream_host.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(socket, &upstream_host, debug).await {
                eprintln!("クライアント処理エラー: {}", e);
            }
        });
    }
}

async fn handle_client(
    mut socket: TokioTcpStream,
    upstream_host: &str,
    debug: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // クライアントからリクエストヘッダーを受信
    let mut decoder = RequestDecoder::new();
    let (req_head, req_body_kind) = loop {
        let mut buffer = vec![0u8; 4096];
        let n = socket.read(&mut buffer).await?;
        if n == 0 {
            log_debug(debug, "client input is empty");
            return Ok(());
        }

        buffer.truncate(n);
        log_debug(debug, &format!("received bytes: {}", n));

        decoder.feed(&buffer)?;
        if let Some(result) = decoder.decode_headers()? {
            break result;
        }
    };

    log_debug(
        debug,
        &format!(
            "request line: {} {} {}",
            req_head.method, req_head.uri, req_head.version
        ),
    );
    log_debug(
        debug,
        &format!("received headers: {}", req_head.headers.len()),
    );

    // リクエストボディを収集（ストリーミングも可能だが、ここではシンプルに全部読む）
    let mut request_body = Vec::new();
    if !matches!(req_body_kind, BodyKind::None) {
        loop {
            if let Some(data) = decoder.peek_body() {
                request_body.extend_from_slice(data);
                let len = data.len();
                match decoder.consume_body(len)? {
                    BodyProgress::Complete { .. } => break,
                    BodyProgress::Continue => {}
                }
            } else {
                // データ不足、追加読み込み
                let mut buffer = vec![0u8; 4096];
                let n = socket.read(&mut buffer).await?;
                if n == 0 {
                    break;
                }
                buffer.truncate(n);
                decoder.feed(&buffer)?;
            }
        }
    }

    // アップストリームへプロキシリクエストを作成
    let mut upstream_request = Request::new(&req_head.method, &req_head.uri);

    // ヘッダーをコピー (Host は除外)
    for (name, value) in &req_head.headers {
        if !name.eq_ignore_ascii_case("host") {
            upstream_request.add_header(name, value);
        }
    }

    // Host ヘッダーを アップストリームに設定
    upstream_request.add_header("Host", upstream_host);

    // Connection を close に設定
    upstream_request.add_header("Connection", "close");

    // ボディをコピー
    upstream_request.body = request_body;
    log_debug(
        debug,
        &format!(
            "upstream request body size: {}",
            upstream_request.body.len()
        ),
    );

    // アップストリームへリクエストを送信し、レスポンスをストリーミングで転送
    stream_upstream_response(&mut socket, &upstream_request, upstream_host, debug).await?;

    Ok(())
}

async fn stream_upstream_response(
    downstream: &mut TokioTcpStream,
    request: &Request,
    upstream_host: &str,
    debug: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // TLS 設定 (システムのプラットフォーム証明書ストアを使用)
    let config = ClientConfig::with_platform_verifier()?;

    let server_name = ServerName::try_from(upstream_host.to_string())?;

    let conn = ClientConnection::new(Arc::new(config), server_name)?;
    let sock = TcpStream::connect((upstream_host, 443))?;
    let mut tls = StreamOwned::new(conn, sock);
    log_debug(debug, &format!("TLS connected: {}", upstream_host));

    // リクエスト送信
    let request_bytes = request.encode();
    log_debug(
        debug,
        &format!("upstream request bytes: {}", request_bytes.len()),
    );
    tls.write_all(&request_bytes)?;

    // レスポンスヘッダーを受信
    let mut decoder = ResponseDecoder::with_limits(DecoderLimits {
        max_buffer_size: 256 * 1024,
        max_body_size: 50 * 1024 * 1024, // ストリーミングなので大きめに
        ..Default::default()
    });
    let mut buf = [0u8; 4096];

    let (resp_head, body_kind) = loop {
        let n = match tls.read(&mut buf) {
            Ok(0) => return Err("接続が閉じられました".into()),
            Ok(n) => n,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(e) => return Err(e.into()),
        };

        log_debug(debug, &format!("upstream received bytes: {}", n));
        decoder.feed(&buf[..n])?;

        if let Some(result) = decoder.decode_headers()? {
            break result;
        }
    };

    log_debug(
        debug,
        &format!(
            "upstream response: {} {} {}",
            resp_head.version, resp_head.status_code, resp_head.reason_phrase
        ),
    );
    log_debug(
        debug,
        &format!("upstream response headers: {}", resp_head.headers.len()),
    );

    // クライアントへレスポンスヘッダーを送信
    let mut response_for_headers = Response::new(resp_head.status_code, &resp_head.reason_phrase);

    // Connection ヘッダーに列挙されたヘッダー名を収集
    let connection_headers: Vec<String> = resp_head
        .headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("Connection"))
        .flat_map(|(_, value)| {
            value
                .split(',')
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .collect();

    for (name, value) in &resp_head.headers {
        // RFC 9110 Section 7.6.1: hop-by-hop ヘッダーを除外
        // RFC 9112 Section 6.3: intermediary は Content-Length を削除すべき (MUST)
        if is_hop_by_hop_header(name, &connection_headers) {
            continue;
        }
        response_for_headers.add_header(name, value);
    }
    response_for_headers.add_header("Connection", "close");

    let header_bytes = encode_response_headers(&response_for_headers);
    downstream.write_all(&header_bytes).await?;
    log_debug(
        debug,
        &format!(
            "sent response headers to client: {} bytes",
            header_bytes.len()
        ),
    );

    // ボディをストリーミング転送
    let mut total_body_bytes = 0usize;
    if !matches!(body_kind, BodyKind::None) {
        loop {
            // バッファにあるデータを転送
            while let Some(data) = decoder.peek_body() {
                downstream.write_all(data).await?;
                total_body_bytes += data.len();
                let len = data.len();
                match decoder.consume_body(len)? {
                    BodyProgress::Complete { trailers } => {
                        log_debug(
                            debug,
                            &format!(
                                "body complete, total: {} bytes, trailers: {}",
                                total_body_bytes,
                                trailers.len()
                            ),
                        );
                        return Ok(());
                    }
                    BodyProgress::Continue => {}
                }
            }

            // データ不足、アップストリームから追加読み込み
            let n = match tls.read(&mut buf) {
                Ok(0) => {
                    // 接続が閉じられた
                    log_debug(debug, "upstream connection closed");
                    break;
                }
                Ok(n) => n,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(e) => return Err(e.into()),
            };

            log_debug(debug, &format!("upstream body bytes: {}", n));
            decoder.feed(&buf[..n])?;
        }
    }

    log_debug(
        debug,
        &format!("response body streamed: {} bytes", total_body_bytes),
    );

    Ok(())
}

/// RFC 9110 Section 7.6.1 で定義された hop-by-hop ヘッダーかどうかを判定
///
/// hop-by-hop ヘッダーは intermediary が転送してはならない
fn is_hop_by_hop_header(name: &str, connection_headers: &[String]) -> bool {
    // RFC 9110 Section 7.6.1: 固定の hop-by-hop ヘッダー
    // RFC 9112 Appendix C.2.2: Proxy-Connection も除外
    const HOP_BY_HOP_HEADERS: &[&str] = &[
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "proxy-connection", // RFC 9112 Appendix C.2.2
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    ];

    // RFC 9112 Section 6.3: Content-Length も除外すべき (intermediary が処理するため)
    if name.eq_ignore_ascii_case("content-length") {
        return true;
    }

    // 固定の hop-by-hop ヘッダーをチェック
    let name_lower = name.to_ascii_lowercase();
    if HOP_BY_HOP_HEADERS.contains(&name_lower.as_str()) {
        return true;
    }

    // Connection ヘッダーに列挙されたヘッダーをチェック
    connection_headers.contains(&name_lower)
}

fn log_debug(enabled: bool, message: &str) {
    if enabled {
        eprintln!("[{}] DEBUG: {}", now_timestamp(), message);
    }
}

fn now_timestamp() -> String {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0));
    let secs = duration.as_secs();
    let millis = duration.subsec_millis();
    format!("{}.{:03}", secs, millis)
}

fn parse_upstream_url(url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let url_str = if let Some(rest) = url.strip_prefix("https://") {
        rest
    } else if let Some(rest) = url.strip_prefix("http://") {
        rest
    } else {
        url
    };

    // ホスト名部分を取得 (パス、クエリ、ポートを除外)
    let host = url_str
        .split('/')
        .next()
        .ok_or("Invalid URL: no host")?
        .split('?')
        .next()
        .ok_or("Invalid URL: no host")?
        .split(':')
        .next()
        .ok_or("Invalid URL: no host")?;

    Ok(host.to_string())
}
