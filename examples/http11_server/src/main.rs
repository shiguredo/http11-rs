//! HTTP/HTTPS サーバーの例 (tokio + tokio-rustls)
//!
//! 使い方:
//!   # HTTP サーバー (ポート 8080)
//!   cargo run -p http11_server
//!
//!   # HTTPS サーバー (ポート 8443)
//!   cargo run -p http11_server -- --tls --cert cert.pem --key key.pem
//!
//! テスト用の自己署名証明書の作成:
//!   openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes \
//!     -subj "/CN=localhost"
//!
//! 圧縮対応:
//!   クライアントの Accept-Encoding ヘッダーに基づいて gzip, br, zstd で圧縮します。
//!   優先順位: zstd > br > gzip
//!
//! ストリーミング API:
//!   このサンプルは decode() 一括 API ではなく、
//!   decode_headers() + peek_body() / consume_body() / progress() を
//!   使用したストリーミング API の実装例です。
//!   詳細は本ソースコードを参照してください。

mod compressor;

use std::sync::Arc;
use std::time::Duration;

use rustls::ServerConfig;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use shiguredo_http11::{
    BodyKind, BodyProgress, EncodeError, Request, RequestDecoder, RequestHead, Response,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

use compressor::{compress_body, encoding_header, select_encoding};
use tracing::{error, info};

/// Keep-Alive タイムアウト (秒)
const DEFAULT_KEEP_ALIVE_TIMEOUT: u64 = 60;
/// 1 接続あたりの最大リクエスト数
const DEFAULT_MAX_REQUESTS: u32 = 1000;

struct ServerOptions {
    port: u16,
    tls: bool,
    cert_path: Option<String>,
    key_path: Option<String>,
}

/// Keep-Alive 接続の状態管理
struct ConnectionState {
    request_count: u32,
    max_requests: u32,
    keep_alive_timeout: Duration,
}

/// ストリーミングデコードの状態
struct StreamingState {
    head: Option<RequestHead>,
    body_kind: Option<BodyKind>,
    body: Option<Vec<u8>>,
}

impl StreamingState {
    fn new() -> Self {
        Self {
            head: None,
            body_kind: None,
            body: None,
        }
    }

    #[allow(dead_code)]
    fn reset(&mut self) {
        self.head = None;
        self.body_kind = None;
        self.body = None;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let options = parse_args()?;

    let addr = format!("0.0.0.0:{}", options.port);
    let listener = TcpListener::bind(&addr).await?;

    if options.tls {
        let cert_path = options
            .cert_path
            .as_ref()
            .ok_or("--cert is required for TLS")?;
        let key_path = options
            .key_path
            .as_ref()
            .ok_or("--key is required for TLS")?;

        let config = load_tls_config(cert_path, key_path)?;
        let acceptor = TlsAcceptor::from(Arc::new(config));

        info!(addr = %addr, "HTTPS server listening");

        loop {
            let (stream, peer_addr) = listener.accept().await?;
            let acceptor = acceptor.clone();

            tokio::spawn(async move {
                match acceptor.accept(stream).await {
                    Ok(tls_stream) => {
                        if let Err(e) = handle_tls_client(tls_stream, peer_addr).await {
                            error!(peer_addr = %peer_addr, error = %e, "TLS client error");
                        }
                    }
                    Err(e) => error!(peer_addr = %peer_addr, error = %e, "TLS handshake error"),
                }
            });
        }
    } else {
        info!(addr = %addr, "HTTP server listening");

        loop {
            let (stream, peer_addr) = listener.accept().await?;

            tokio::spawn(async move {
                if let Err(e) = handle_client(stream, peer_addr).await {
                    error!(peer_addr = %peer_addr, error = %e, "Client error");
                }
            });
        }
    }
}

fn parse_args() -> Result<ServerOptions, Box<dyn std::error::Error>> {
    let mut args = noargs::raw_args();
    args.metadata_mut().app_name = "http11_server";

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

    // --tls フラグ
    let tls: bool = noargs::flag("tls")
        .doc("Enable HTTPS")
        .take(&mut args)
        .is_present();

    // --port オプション (デフォルトは TLS の有無で変わる)
    let default_port = if tls { "8443" } else { "8080" };
    let port: u16 = noargs::opt("port")
        .short('p')
        .doc("Port to listen on (default: 8080, or 8443 with --tls)")
        .default(default_port)
        .take(&mut args)
        .then(|o| o.value().parse())
        .map_err(|e| format!("{:?}", e))?;

    // --cert オプション
    let cert_path: Option<String> = noargs::opt("cert")
        .doc("Path to certificate file (PEM)")
        .take(&mut args)
        .present_and_then(|o| Ok::<_, &str>(o.value().to_string()))
        .map_err(|e| format!("{:?}", e))?;

    // --key オプション
    let key_path: Option<String> = noargs::opt("key")
        .doc("Path to private key file (PEM)")
        .take(&mut args)
        .present_and_then(|o| Ok::<_, &str>(o.value().to_string()))
        .map_err(|e| format!("{:?}", e))?;

    // 未知の引数があればエラー、ヘルプが返されたら表示
    if let Some(help) = args.finish().map_err(|e| format!("{:?}", e))? {
        print!("{}", help);
        std::process::exit(0);
    }

    Ok(ServerOptions {
        port,
        tls,
        cert_path,
        key_path,
    })
}

fn load_tls_config(
    cert_path: &str,
    key_path: &str,
) -> Result<ServerConfig, Box<dyn std::error::Error>> {
    let certs: Vec<CertificateDer<'static>> =
        CertificateDer::pem_file_iter(cert_path)?.collect::<Result<Vec<_>, _>>()?;

    if certs.is_empty() {
        return Err("No certificates found in cert file".into());
    }

    let key = PrivateKeyDer::from_pem_file(key_path)?;

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    Ok(config)
}

/// ボディをストリーミングで受信する
///
/// true を返す: ボディ受信完了
/// false を返す: 追加データが必要
fn stream_body(
    decoder: &mut RequestDecoder,
    body_kind: &BodyKind,
    buf: &mut Option<Vec<u8>>,
) -> Result<bool, shiguredo_http11::Error> {
    match body_kind {
        BodyKind::None | BodyKind::CloseDelimited | BodyKind::Tunnel => {
            *buf = None;
            Ok(true)
        }
        BodyKind::ContentLength(_) | BodyKind::Chunked => {
            let mut acc = buf.take().unwrap_or_default();
            loop {
                if let Some(data) = decoder.peek_body() {
                    acc.extend_from_slice(data);
                    let len = data.len();
                    match decoder.consume_body(len)? {
                        BodyProgress::Complete { .. } => {
                            *buf = Some(acc);
                            return Ok(true);
                        }
                        // NeedData (chunked CRLF 不足) でも内側ループ継続。
                        // 直後の peek_body() は None を返すため progress 分岐に fall through する。
                        BodyProgress::Advanced | BodyProgress::NeedData => continue,
                    }
                }
                match decoder.progress()? {
                    BodyProgress::Complete { .. } => {
                        *buf = Some(acc);
                        return Ok(true);
                    }
                    BodyProgress::Advanced => continue,
                    BodyProgress::NeedData => {
                        *buf = Some(acc);
                        return Ok(false);
                    }
                }
            }
        }
    }
}

/// 1 リクエストを処理し、Keep-Alive を継続するかどうかを返す
///
/// true を返す: 次のリクエストを処理可能
/// false を返す: 接続をクローズすべき
async fn serve_request(
    state: &mut StreamingState,
    conn_state: &mut ConnectionState,
    writer: &mut (impl AsyncWriteExt + Unpin),
    peer_addr: std::net::SocketAddr,
    tls: bool,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let h = state.head.take().unwrap();
    let _ = state.body_kind.take();
    let request = Request {
        method: h.method,
        uri: h.uri,
        version: h.version,
        headers: h.headers,
        body: state.body.take(),
    };

    conn_state.request_count += 1;

    info!(
        method = %request.method,
        uri = %request.uri,
        version = %request.version,
        peer_addr = %peer_addr,
        tls = tls,
        request_count = conn_state.request_count,
        "Request received"
    );

    let should_keep_alive =
        request.is_keep_alive() && conn_state.request_count < conn_state.max_requests;

    let response = build_response(&request, should_keep_alive)?;
    let response_bytes = response.encode();
    writer.write_all(&response_bytes).await?;
    writer.flush().await?;

    if !should_keep_alive {
        if conn_state.request_count >= conn_state.max_requests {
            info!(
                max_requests = conn_state.max_requests,
                peer_addr = %peer_addr,
                tls = tls,
                "Max requests reached"
            );
        } else {
            info!(peer_addr = %peer_addr, tls = tls, "Connection close");
        }
        return Ok(false);
    }

    Ok(true)
}

async fn handle_client(
    stream: TcpStream,
    peer_addr: std::net::SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!(peer_addr = %peer_addr, "Connection accepted");

    let (reader, writer) = stream.into_split();
    let mut reader = tokio::io::BufReader::with_capacity(8192, reader);
    let mut writer = BufWriter::with_capacity(65536, writer);

    let mut decoder = RequestDecoder::new();
    const READ_CHUNK: usize = 8192;
    let mut conn_state = ConnectionState {
        request_count: 0,
        max_requests: DEFAULT_MAX_REQUESTS,
        keep_alive_timeout: Duration::from_secs(DEFAULT_KEEP_ALIVE_TIMEOUT),
    };
    let mut state = StreamingState::new();

    loop {
        let want = decoder.available_buf().min(READ_CHUNK);
        if want == 0 {
            error!(peer_addr = %peer_addr, "Decoder buffer full");
            break;
        }
        let buf = decoder.mut_buf(want)?;
        let read_result =
            tokio::time::timeout(conn_state.keep_alive_timeout, reader.read(buf)).await;

        let n = match read_result {
            Ok(Ok(n)) => n,
            Ok(Err(e)) => {
                decoder.advance_buf(0);
                error!(peer_addr = %peer_addr, error = %e, "Read error");
                break;
            }
            Err(_) => {
                decoder.advance_buf(0);
                info!(peer_addr = %peer_addr, "Keep-Alive timeout");
                break;
            }
        };

        if n == 0 {
            decoder.advance_buf(0);
            info!(peer_addr = %peer_addr, "Connection closed by client");
            break;
        }

        decoder.advance_buf(n);

        loop {
            if state.head.is_none() {
                match decoder.decode_headers()? {
                    Some((h, k)) => {
                        state.head = Some(h);
                        state.body_kind = Some(k);
                    }
                    None => break,
                }
            }

            let body_complete = stream_body(
                &mut decoder,
                state.body_kind.as_ref().unwrap(),
                &mut state.body,
            )?;

            if !body_complete {
                break;
            }

            let keep_alive =
                serve_request(&mut state, &mut conn_state, &mut writer, peer_addr, false).await?;
            if !keep_alive {
                return Ok(());
            }
        }
    }

    Ok(())
}

async fn handle_tls_client(
    stream: tokio_rustls::server::TlsStream<TcpStream>,
    peer_addr: std::net::SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!(peer_addr = %peer_addr, "TLS connection accepted");

    let (reader, writer) = tokio::io::split(stream);
    let mut reader = tokio::io::BufReader::with_capacity(8192, reader);
    let mut writer = BufWriter::with_capacity(65536, writer);

    let mut decoder = RequestDecoder::new();
    const READ_CHUNK: usize = 8192;
    let mut conn_state = ConnectionState {
        request_count: 0,
        max_requests: DEFAULT_MAX_REQUESTS,
        keep_alive_timeout: Duration::from_secs(DEFAULT_KEEP_ALIVE_TIMEOUT),
    };
    let mut state = StreamingState::new();

    loop {
        let want = decoder.available_buf().min(READ_CHUNK);
        if want == 0 {
            error!(peer_addr = %peer_addr, "TLS decoder buffer full");
            break;
        }
        let buf = decoder.mut_buf(want)?;
        let read_result =
            tokio::time::timeout(conn_state.keep_alive_timeout, reader.read(buf)).await;

        let n = match read_result {
            Ok(Ok(n)) => n,
            Ok(Err(e)) => {
                decoder.advance_buf(0);
                error!(peer_addr = %peer_addr, error = %e, "TLS read error");
                break;
            }
            Err(_) => {
                decoder.advance_buf(0);
                info!(peer_addr = %peer_addr, "TLS Keep-Alive timeout");
                break;
            }
        };

        if n == 0 {
            decoder.advance_buf(0);
            info!(peer_addr = %peer_addr, "TLS connection closed by client");
            break;
        }

        decoder.advance_buf(n);

        loop {
            if state.head.is_none() {
                match decoder.decode_headers()? {
                    Some((h, k)) => {
                        state.head = Some(h);
                        state.body_kind = Some(k);
                    }
                    None => break,
                }
            }

            let body_complete = stream_body(
                &mut decoder,
                state.body_kind.as_ref().unwrap(),
                &mut state.body,
            )?;

            if !body_complete {
                break;
            }

            let keep_alive =
                serve_request(&mut state, &mut conn_state, &mut writer, peer_addr, true).await?;
            if !keep_alive {
                return Ok(());
            }
        }
    }

    Ok(())
}

fn build_response(request: &Request, should_keep_alive: bool) -> Result<Response, EncodeError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // RFC 9110 準拠の Date ヘッダー (IMF-fixdate 形式)
    let date = format_http_date(now);

    // RFC 9110 Section 9.3.2: HEAD レスポンスは GET と同じヘッダーを返すがボディは送信しない
    let is_head = request.method.eq_ignore_ascii_case("HEAD");

    // Accept-Encoding ヘッダーから圧縮方式を選択
    let accept_encoding = request
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("Accept-Encoding"))
        .map(|(_, value)| value.as_str());

    let encoding = accept_encoding.and_then(select_encoding);

    let response = match request.uri.as_str() {
        "/" => {
            let body_content = r#"<!DOCTYPE html>
<html>
<head><title>shiguredo_http11 Server</title></head>
<body>
<h1>Welcome to shiguredo_http11 Server</h1>
<p>This server is powered by shiguredo_http11 library.</p>
<ul>
<li><a href="/info">/info</a> - Server information</li>
<li><a href="/echo">/echo</a> - Echo request details</li>
</ul>
</body>
</html>
"#;
            build_compressed_response(
                200,
                "OK",
                "text/html; charset=utf-8",
                body_content.as_bytes(),
                &date,
                is_head,
                encoding,
            )?
        }
        "/info" => {
            let body_content = format!(
                r#"{{"server":"shiguredo_http11","version":"0.1.0","timestamp":{}}}"#,
                now
            );
            build_compressed_response(
                200,
                "OK",
                "application/json",
                body_content.as_bytes(),
                &date,
                is_head,
                encoding,
            )?
        }
        "/echo" => {
            // HEAD リクエストの /echo は空のボディで Content-Length: 0 を返す
            // (実際の GET レスポンスはリクエストに依存するため)
            if is_head {
                let head_response = Response::new(200, "OK")?
                    .header("Date", &date)?
                    .header("Content-Type", "text/plain; charset=utf-8")?
                    .header("Content-Length", "0")?
                    .header("Server", "shiguredo_http11/0.1.0")?
                    .omit_body(true);
                return add_connection_headers(head_response, should_keep_alive);
            }

            let mut body = format!(
                "Method: {}\nURI: {}\nVersion: {}\n\nHeaders:\n",
                request.method, request.uri, request.version
            );

            for (name, value) in &request.headers {
                body.push_str(&format!("  {}: {}\n", name, value));
            }

            if let Some(req_body) = request.body.as_deref()
                && !req_body.is_empty()
            {
                body.push_str(&format!("\nBody ({} bytes):\n", req_body.len()));
                if let Ok(text) = std::str::from_utf8(req_body) {
                    body.push_str(text);
                } else {
                    body.push_str("[binary data]");
                }
            }

            build_compressed_response(
                200,
                "OK",
                "text/plain; charset=utf-8",
                body.as_bytes(),
                &date,
                false,
                encoding,
            )?
        }
        _ => {
            let body_content = "404 Not Found\n";
            build_compressed_response(
                404,
                "Not Found",
                "text/plain",
                body_content.as_bytes(),
                &date,
                is_head,
                encoding,
            )?
        }
    };

    add_connection_headers(response, should_keep_alive)
}

/// 圧縮対応のレスポンスを構築
fn build_compressed_response(
    status_code: u16,
    reason_phrase: &str,
    content_type: &str,
    body: &[u8],
    date: &str,
    is_head: bool,
    encoding: Option<&str>,
) -> Result<Response, EncodeError> {
    // 圧縮を試みる
    let (final_body, content_encoding) = if let Some(enc) = encoding {
        match compress_body(body, enc) {
            Ok(compressed) => {
                // 圧縮後のサイズが元より小さい場合のみ圧縮を使用
                if compressed.len() < body.len() {
                    (compressed, Some(encoding_header(enc)))
                } else {
                    (body.to_vec(), None)
                }
            }
            Err(_) => (body.to_vec(), None),
        }
    } else {
        (body.to_vec(), None)
    };

    let mut response = Response::new(status_code, reason_phrase)?
        .header("Date", date)?
        .header("Content-Type", content_type)?
        .header("Content-Length", &final_body.len().to_string())?
        .header("Server", "shiguredo_http11/0.1.0")?
        .header("Vary", "Accept-Encoding")?;

    if let Some(enc) = content_encoding {
        response = response.header("Content-Encoding", enc)?;
    }

    Ok(response.body(final_body).omit_body(is_head))
}

/// RFC 9112 準拠で Connection ヘッダーを設定する
///
/// HTTP/1.1 では keep-alive がデフォルトのため:
/// - keep-alive 継続: ヘッダー不要
/// - 接続終了: Connection: close を追加
fn add_connection_headers(
    response: Response,
    should_keep_alive: bool,
) -> Result<Response, EncodeError> {
    if should_keep_alive {
        Ok(response)
    } else {
        response.header("Connection", "close")
    }
}

/// RFC 9110 準拠の IMF-fixdate 形式で日付を生成
fn format_http_date(timestamp: u64) -> String {
    // 日曜日始まりの曜日配列
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    // Unix epoch (1970-01-01) は木曜日 (= 4)
    let days_since_epoch = timestamp / 86400;
    let day_of_week = ((days_since_epoch + 4) % 7) as usize;

    // 年月日を計算
    let mut remaining_days = days_since_epoch as i64;
    let mut year = 1970i32;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let mut month = 0usize;
    let days_in_months = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    for (i, &days) in days_in_months.iter().enumerate() {
        if remaining_days < days as i64 {
            month = i;
            break;
        }
        remaining_days -= days as i64;
    }

    let day = remaining_days + 1;

    // 時分秒を計算
    let time_of_day = timestamp % 86400;
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;

    format!(
        "{}, {:02} {} {} {:02}:{:02}:{:02} GMT",
        DAYS[day_of_week], day, MONTHS[month], year, hour, minute, second
    )
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}
