//! HTTP/1.1 リバースプロキシの例（接続プール対応）
//!
//! 使い方:
//!   cargo run -p http11_reverse_proxy -- --port 8888 --upstream https://example.com
//!   curl http://localhost:8888/

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustls::ClientConfig;
use rustls::pki_types::ServerName;
use rustls_platform_verifier::ConfigVerifierExt;
use shiguredo_http11::{
    BodyKind, BodyProgress, DecoderLimits, Request, RequestDecoder, Response, ResponseDecoder,
    encode_response_headers,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

/// 接続プールの設定
#[derive(Debug, Clone)]
struct PoolConfig {
    /// ホストあたりの最大接続数
    max_connections_per_host: usize,
    /// アイドル接続のタイムアウト（秒）
    idle_timeout_secs: u64,
    /// 接続の最大生存時間（秒）
    max_lifetime_secs: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections_per_host: 10,
            idle_timeout_secs: 60,
            max_lifetime_secs: 300,
        }
    }
}

/// プールされた接続
struct PooledConnection {
    stream: TlsStream<TcpStream>,
    created_at: Instant,
    last_used: Instant,
}

impl PooledConnection {
    fn new(stream: TlsStream<TcpStream>) -> Self {
        let now = Instant::now();
        Self {
            stream,
            created_at: now,
            last_used: now,
        }
    }

    /// 接続が有効かどうかを確認
    fn is_valid(&self, config: &PoolConfig) -> bool {
        let now = Instant::now();
        let idle_duration = now.duration_since(self.last_used);
        let lifetime = now.duration_since(self.created_at);

        idle_duration < Duration::from_secs(config.idle_timeout_secs)
            && lifetime < Duration::from_secs(config.max_lifetime_secs)
    }
}

/// 接続プール
struct ConnectionPool {
    /// ホストごとのアイドル接続
    idle_connections: HashMap<String, Vec<PooledConnection>>,
    config: PoolConfig,
    tls_connector: TlsConnector,
}

impl ConnectionPool {
    fn new(tls_connector: TlsConnector, config: PoolConfig) -> Self {
        Self {
            idle_connections: HashMap::new(),
            config,
            tls_connector,
        }
    }

    /// プールからアイドル接続を取得（ロック内で高速に実行）
    fn try_acquire(&mut self, host: &str) -> Option<PooledConnection> {
        if let Some(connections) = self.idle_connections.get_mut(host) {
            while let Some(mut conn) = connections.pop() {
                if conn.is_valid(&self.config) {
                    conn.last_used = Instant::now();
                    return Some(conn);
                }
                // 無効な接続は破棄
            }
        }
        None
    }

    /// TLS コネクタを取得（ロック外で接続を作成するため）
    fn tls_connector(&self) -> TlsConnector {
        self.tls_connector.clone()
    }

    /// 接続をプールに返却
    fn release(&mut self, host: &str, conn: PooledConnection) {
        if !conn.is_valid(&self.config) {
            return;
        }

        let connections = self.idle_connections.entry(host.to_string()).or_default();

        // 最大接続数を超えている場合は破棄
        if connections.len() >= self.config.max_connections_per_host {
            return;
        }

        connections.push(conn);
    }

    /// 期限切れの接続を削除
    fn cleanup_expired(&mut self) {
        for connections in self.idle_connections.values_mut() {
            connections.retain(|conn| conn.is_valid(&self.config));
        }
        self.idle_connections
            .retain(|_, connections| !connections.is_empty());
    }

    /// プールの統計情報を取得
    fn stats(&self) -> (usize, usize) {
        let hosts = self.idle_connections.len();
        let connections: usize = self.idle_connections.values().map(|v| v.len()).sum();
        (hosts, connections)
    }
}

/// 新規 TLS 接続を作成（ロック外で実行）
async fn create_connection(
    host: &str,
    tls_connector: &TlsConnector,
) -> Result<PooledConnection, Box<dyn std::error::Error + Send + Sync>> {
    let server_name = ServerName::try_from(host.to_string())?;
    let tcp_stream = TcpStream::connect((host, 443)).await?;
    let tls_stream = tls_connector.connect(server_name, tcp_stream).await?;
    Ok(PooledConnection::new(tls_stream))
}

/// 共有可能な接続プール
type SharedPool = Arc<Mutex<ConnectionPool>>;

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

    // TLS 設定を事前に作成
    let tls_config = Arc::new(ClientConfig::with_platform_verifier()?);
    let tls_connector = TlsConnector::from(tls_config);

    // 接続プールを作成
    let pool_config = PoolConfig::default();
    let pool = Arc::new(Mutex::new(ConnectionPool::new(
        tls_connector,
        pool_config.clone(),
    )));

    // 定期的なクリーンアップタスク
    let cleanup_pool = pool.clone();
    let cleanup_debug = debug;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let mut pool = cleanup_pool.lock().await;
            pool.cleanup_expired();
            if cleanup_debug {
                let (hosts, conns) = pool.stats();
                eprintln!(
                    "[{}] POOL: cleanup done, {} hosts, {} connections",
                    now_timestamp(),
                    hosts,
                    conns
                );
            }
        }
    });

    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await?;

    println!("リバースプロキシをバインド: {}", addr);
    println!("  http://localhost:{}/ -> {}/", port, upstream_url);
    println!(
        "  接続プール有効（最大 {} 接続/ホスト、アイドル {}秒、最大生存 {}秒）",
        pool_config.max_connections_per_host,
        pool_config.idle_timeout_secs,
        pool_config.max_lifetime_secs
    );

    loop {
        let (socket, _) = listener.accept().await?;
        let upstream_host = upstream_host.clone();
        let pool = pool.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(socket, &upstream_host, pool, debug).await {
                eprintln!("クライアント処理エラー: {}", e);
            }
        });
    }
}

async fn handle_client(
    mut socket: TcpStream,
    upstream_host: &str,
    pool: SharedPool,
    debug: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

    // リクエストボディを収集
    let mut request_body = Vec::new();
    if !matches!(req_body_kind, BodyKind::None) {
        'outer: loop {
            loop {
                match decoder.peek_body() {
                    Some(data) => {
                        request_body.extend_from_slice(data);
                        let len = data.len();
                        match decoder.consume_body(len)? {
                            BodyProgress::Complete { .. } => break 'outer,
                            BodyProgress::Continue => {}
                        }
                    }
                    None => {
                        // peek_body() が None でも consume_body(0) で状態遷移を試みる
                        let remaining_before = decoder.remaining().len();
                        match decoder.consume_body(0)? {
                            BodyProgress::Complete { .. } => break 'outer,
                            BodyProgress::Continue => {
                                if decoder.remaining().len() == remaining_before {
                                    break; // 内側ループを抜けてデータ読み取り
                                }
                                // remaining が変化した場合は内側ループを継続
                            }
                        }
                    }
                }
            }

            let mut buffer = vec![0u8; 4096];
            let n = socket.read(&mut buffer).await?;
            if n == 0 {
                // クライアントが切断した - 不完全なボディを upstream に送信してはいけない
                return Err("client disconnected during request body".into());
            }
            buffer.truncate(n);
            decoder.feed(&buffer)?;
        }
    }

    // アップストリームへプロキシリクエストを作成
    let mut upstream_request = Request::new(&req_head.method, &req_head.uri);

    // Connection ヘッダーに列挙されたヘッダー名を収集
    let connection_headers: Vec<String> = req_head
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

    // ヘッダーをコピー (hop-by-hop ヘッダーと Host は除外)
    for (name, value) in &req_head.headers {
        if name.eq_ignore_ascii_case("host") {
            continue;
        }
        if is_hop_by_hop_header(name, &connection_headers) {
            continue;
        }
        upstream_request.add_header(name, value);
    }

    upstream_request.add_header("Host", upstream_host);
    // Keep-Alive を使用して接続を再利用
    upstream_request.add_header("Connection", "keep-alive");
    upstream_request.body = request_body;

    log_debug(
        debug,
        &format!(
            "upstream request body size: {}",
            upstream_request.body.len()
        ),
    );

    // 接続プールから接続を取得してリクエストを送信
    let result = stream_upstream_response_pooled(
        &mut socket,
        &upstream_request,
        upstream_host,
        pool.clone(),
        debug,
    )
    .await;

    // エラーの場合はログに出力
    if let Err(ref e) = result {
        log_debug(debug, &format!("upstream error: {}", e));
    }

    result
}

async fn stream_upstream_response_pooled(
    downstream: &mut TcpStream,
    request: &Request,
    upstream_host: &str,
    pool: SharedPool,
    debug: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let start = Instant::now();

    // まずプールからアイドル接続を取得（ロックは短時間のみ保持）
    let (mut conn, from_pool) = {
        let mut pool_guard = pool.lock().await;
        if let Some(conn) = pool_guard.try_acquire(upstream_host) {
            (conn, true)
        } else {
            // プールにない場合は TLS コネクタを取得してロックを解放
            let tls_connector = pool_guard.tls_connector();
            drop(pool_guard); // 明示的にロックを解放

            // ロック外で新規接続を作成（時間がかかる処理）
            let conn = create_connection(upstream_host, &tls_connector).await?;
            (conn, false)
        }
    };

    let acquire_time = start.elapsed();
    log_debug(
        debug,
        &format!(
            "acquired connection for: {} ({}ms, {})",
            upstream_host,
            acquire_time.as_millis(),
            if from_pool { "from pool" } else { "new" }
        ),
    );

    // リクエスト送信とレスポンス受信
    let result = stream_response_on_connection(downstream, request, &mut conn.stream, debug).await;

    // 接続を再利用するかどうかを判定
    let should_reuse = match &result {
        Ok(reuse) => *reuse,
        Err(_) => false,
    };

    if should_reuse {
        conn.last_used = Instant::now();
        pool.lock().await.release(upstream_host, conn);
        log_debug(debug, "connection returned to pool");
    } else {
        log_debug(debug, "connection closed (not reusable)");
    }

    result.map(|_| ())
}

/// 接続上でリクエストを送信しレスポンスを転送
/// 戻り値: 接続を再利用可能かどうか
async fn stream_response_on_connection(
    downstream: &mut TcpStream,
    request: &Request,
    upstream: &mut TlsStream<TcpStream>,
    debug: bool,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    // ダウンストリームをバッファリング（64KB バッファ）
    let mut downstream = BufWriter::with_capacity(65536, downstream);

    // リクエスト送信
    let request_bytes = request.encode();
    log_debug(
        debug,
        &format!("upstream request bytes: {}", request_bytes.len()),
    );
    upstream.write_all(&request_bytes).await?;

    // レスポンスヘッダーを受信
    let mut decoder = ResponseDecoder::with_limits(DecoderLimits {
        max_buffer_size: 256 * 1024,
        max_body_size: 50 * 1024 * 1024,
        ..Default::default()
    });
    let mut buf = [0u8; 8192];

    let (resp_head, body_kind) = loop {
        let n = upstream.read(&mut buf).await?;
        if n == 0 {
            return Err("接続が閉じられました".into());
        }

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

    // Connection ヘッダーを確認して再利用可能性を判定
    let connection_close = resp_head.headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("Connection")
            && value
                .split(',')
                .any(|v| v.trim().eq_ignore_ascii_case("close"))
    });
    let can_reuse = !connection_close && resp_head.version.ends_with("/1.1");

    // クライアントへレスポンスヘッダーを送信
    let mut response_for_headers = Response::new(resp_head.status_code, &resp_head.reason_phrase);

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

    let use_chunked = matches!(body_kind, BodyKind::Chunked);
    let content_length = match body_kind {
        BodyKind::ContentLength(len) => Some(len),
        _ => None,
    };

    for (name, value) in &resp_head.headers {
        if is_hop_by_hop_header(name, &connection_headers) {
            continue;
        }
        // Content-Length と Transfer-Encoding は body_kind に基づいて後で設定する
        if name.eq_ignore_ascii_case("content-length")
            || name.eq_ignore_ascii_case("transfer-encoding")
        {
            continue;
        }
        response_for_headers.add_header(name, value);
    }

    if let Some(len) = content_length {
        response_for_headers.add_header("Content-Length", &len.to_string());
        log_debug(debug, &format!("using Content-Length: {}", len));
    } else if use_chunked {
        response_for_headers.add_header("Transfer-Encoding", "chunked");
        log_debug(debug, "using Transfer-Encoding: chunked");
    } else if !matches!(body_kind, BodyKind::None) {
        response_for_headers.add_header("Connection", "close");
        log_debug(debug, "using Connection: close");
    }

    let header_bytes = encode_response_headers(&response_for_headers);
    downstream.write_all(&header_bytes).await?;
    downstream.flush().await?;

    // ボディをストリーミング転送
    let mut total_body_bytes = 0usize;
    if !matches!(body_kind, BodyKind::None) {
        'outer: loop {
            loop {
                match decoder.peek_body() {
                    Some(data) => {
                        let len = data.len();
                        if use_chunked {
                            let mut chunk = format!("{:x}\r\n", len).into_bytes();
                            chunk.extend_from_slice(data);
                            chunk.extend_from_slice(b"\r\n");
                            downstream.write_all(&chunk).await?;
                        } else {
                            downstream.write_all(data).await?;
                        }
                        total_body_bytes += len;

                        match decoder.consume_body(len)? {
                            BodyProgress::Complete { trailers } => {
                                if use_chunked {
                                    let mut end_chunk = b"0\r\n".to_vec();
                                    for (name, value) in &trailers {
                                        end_chunk.extend_from_slice(
                                            format!("{}: {}\r\n", name, value).as_bytes(),
                                        );
                                    }
                                    end_chunk.extend_from_slice(b"\r\n");
                                    downstream.write_all(&end_chunk).await?;
                                }
                                log_debug(
                                    debug,
                                    &format!("body complete, total: {} bytes", total_body_bytes),
                                );
                                break 'outer;
                            }
                            BodyProgress::Continue => {}
                        }
                    }
                    None => {
                        let remaining_before = decoder.remaining().len();
                        match decoder.consume_body(0)? {
                            BodyProgress::Complete { trailers } => {
                                if use_chunked {
                                    let mut end_chunk = b"0\r\n".to_vec();
                                    for (name, value) in &trailers {
                                        end_chunk.extend_from_slice(
                                            format!("{}: {}\r\n", name, value).as_bytes(),
                                        );
                                    }
                                    end_chunk.extend_from_slice(b"\r\n");
                                    downstream.write_all(&end_chunk).await?;
                                }
                                log_debug(
                                    debug,
                                    &format!("body complete, total: {} bytes", total_body_bytes),
                                );
                                break 'outer;
                            }
                            BodyProgress::Continue => {
                                if decoder.remaining().len() == remaining_before {
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            let n = upstream.read(&mut buf).await?;
            if n == 0 {
                // upstream が予期せず切断 - 終端チャンクを送らずにエラーを返す
                // 不完全なレスポンスを完了扱いにしてはいけない
                log_debug(debug, "upstream disconnected during response body");
                return Err("upstream disconnected during response body".into());
            }

            log_debug(debug, &format!("upstream body bytes: {}", n));
            decoder.feed(&buf[..n])?;
        }
    }

    // バッファをフラッシュ
    downstream.flush().await?;

    log_debug(
        debug,
        &format!("response body streamed: {} bytes", total_body_bytes),
    );

    Ok(can_reuse)
}

fn is_hop_by_hop_header(name: &str, connection_headers: &[String]) -> bool {
    const HOP_BY_HOP_HEADERS: &[&str] = &[
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "proxy-connection",
        "te",
        "transfer-encoding",
        "upgrade",
    ];

    let name_lower = name.to_ascii_lowercase();
    if HOP_BY_HOP_HEADERS.contains(&name_lower.as_str()) {
        return true;
    }

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
