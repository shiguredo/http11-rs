//! HTTP/1.1 リバースプロキシの例
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
    DecoderLimits, Request, RequestDecoder, Response, ResponseDecoder, encode_chunk, encode_chunks,
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
    // クライアントからリクエストを受信
    let mut decoder = RequestDecoder::new();
    let request = loop {
        let mut buffer = vec![0u8; 4096];
        let n = socket.read(&mut buffer).await?;
        if n == 0 {
            log_debug(debug, "client input is empty");
            return Ok(());
        }

        buffer.truncate(n);
        log_debug(debug, &format!("received bytes: {}", n));

        decoder.feed(&buffer)?;
        if let Some(req) = decoder.decode()? {
            break req;
        }
    };

    log_debug(
        debug,
        &format!(
            "request line: {} {} {}",
            request.method, request.uri, request.version
        ),
    );
    log_debug(
        debug,
        &format!("received headers: {}", request.headers.len()),
    );

    // アップストリームへプロキシリクエストを作成
    let mut upstream_request = Request::new(&request.method, &request.uri);

    // ヘッダーをコピー (Host は除外)
    for (name, value) in &request.headers {
        if name.to_lowercase() != "host" {
            upstream_request.add_header(name, value);
        }
    }

    // Host ヘッダーを アップストリームに設定
    upstream_request.add_header("Host", upstream_host);

    // Connection を close に設定
    upstream_request.add_header("Connection", "close");

    // ボディをコピー
    upstream_request.body = request.body.clone();
    log_debug(
        debug,
        &format!(
            "upstream request body size: {}",
            upstream_request.body.len()
        ),
    );

    // アップストリームへリクエストを送信
    let mut upstream_response =
        send_upstream_request(&upstream_request, upstream_host, debug).await?;
    normalize_upstream_response(&mut upstream_response);

    log_debug(
        debug,
        &format!(
            "upstream response: {} {} {}",
            upstream_response.version,
            upstream_response.status_code,
            upstream_response.reason_phrase
        ),
    );
    log_debug(
        debug,
        &format!(
            "upstream response body size: {}",
            upstream_response.body.len()
        ),
    );

    // クライアントへレスポンスを送信
    let response_bytes = upstream_response.encode();
    log_debug(
        debug,
        &format!("response bytes to client: {}", response_bytes.len()),
    );
    socket.write_all(&response_bytes).await?;

    Ok(())
}

async fn send_upstream_request(
    request: &Request,
    upstream_host: &str,
    debug: bool,
) -> Result<Response, Box<dyn std::error::Error>> {
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

    // レスポンス受信
    let mut decoder = ResponseDecoder::with_limits(DecoderLimits {
        max_buffer_size: 256 * 1024,
        max_body_size: 5 * 1024 * 1024,
        ..Default::default()
    });
    let mut buf = [0u8; 4096];

    loop {
        let n = match tls.read(&mut buf) {
            Ok(0) => return Err("接続が閉じられました".into()),
            Ok(n) => n,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(e) => return Err(e.into()),
        };

        log_debug(debug, &format!("upstream received bytes: {}", n));
        decoder.feed(&buf[..n])?;

        if let Some(response) = decoder.decode()? {
            log_debug(
                debug,
                &format!("upstream response headers: {}", response.headers.len()),
            );
            log_debug(
                debug,
                &format!("upstream response body size: {}", response.body.len()),
            );
            return Ok(response);
        }
    }
}

fn normalize_upstream_response(response: &mut Response) {
    if response.is_chunked() {
        let chunked_body = if response.body.is_empty() {
            encode_chunk(&[])
        } else {
            encode_chunks(&[response.body.as_slice()])
        };
        response.body = chunked_body;
        response
            .headers
            .retain(|(name, _)| !name.eq_ignore_ascii_case("Content-Length"));
        response
            .headers
            .retain(|(name, _)| !name.eq_ignore_ascii_case("Connection"));
        response.add_header("Connection", "close");
        return;
    }

    // chunked 以外 は デコード 済み なので、ヘッダー を 正規化
    response.headers.retain(|(name, _)| {
        !name.eq_ignore_ascii_case("Transfer-Encoding")
            && !name.eq_ignore_ascii_case("Content-Length")
            && !name.eq_ignore_ascii_case("Connection")
    });
    response.add_header("Content-Length", &response.body.len().to_string());
    response.add_header("Connection", "close");
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
