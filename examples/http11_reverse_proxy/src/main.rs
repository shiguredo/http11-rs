//! HTTP/1.1 リバースプロキシの例（接続プール対応）
//!
//! 使い方:
//!   cargo run -p http11_reverse_proxy -- --port 8888 --upstream https://example.com
//!   curl http://localhost:8888/

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use rustls::ClientConfig;
use rustls::pki_types::ServerName;
use rustls_platform_verifier::ConfigVerifierExt;
use shiguredo_http11::{
    BodyKind, BodyProgress, DecoderLimits, HttpHead, Request, RequestDecoder, Response,
    ResponseDecoder, StatusCode, encode_chunk, encode_response_headers,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufWriter, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tracing::{debug, error, info};

/// upstream の scheme (issue 0050)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Scheme {
    Http,
    Https,
}

impl Scheme {
    /// RFC 9110 Section 4.2.1 / 4.2.2 のデフォルトポート
    fn default_port(self) -> u16 {
        match self {
            Scheme::Http => 80,
            Scheme::Https => 443,
        }
    }
}

/// upstream URL から抽出した接続情報 (issue 0050)
#[derive(Debug, Clone)]
struct UpstreamUrl {
    scheme: Scheme,
    /// IPv6 リテラルは brackets を **含まない** 裸の host (例: "::1")
    host: String,
    port: u16,
}

impl UpstreamUrl {
    /// 接続プールキー (issue 0050)
    fn key(&self) -> UpstreamKey {
        (self.scheme, self.host.clone(), self.port)
    }
}

/// 接続プールキー: scheme / host / port のタプル (issue 0050)
type UpstreamKey = (Scheme, String, u16);

/// upstream URL をパースして scheme / host / port を抽出する (issue 0050)
///
/// 受理する形式:
/// - `http://host[:port][/path][?query]`
/// - `https://host[:port][/path][?query]`
/// - IPv6 リテラルは `[host]` 形式 (例: `https://[::1]:8443/`)
///
/// path / query 部は無視する (本サンプルではクライアントの URL をそのまま転送する)。
fn parse_upstream_url(url: &str) -> Result<UpstreamUrl, Box<dyn std::error::Error>> {
    let (scheme, rest) = if let Some(rest) = url.strip_prefix("https://") {
        (Scheme::Https, rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        (Scheme::Http, rest)
    } else {
        return Err(format!("upstream URL must start with http:// or https://: {}", url).into());
    };

    // authority 部 (host[:port]) を path / query 区切りより前で取り出す
    let authority_end = rest.find(['/', '?']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() {
        return Err(format!("upstream URL has empty host: {}", url).into());
    }

    // IPv6 リテラルの判定
    let (host, port_opt) = if let Some(rest) = authority.strip_prefix('[') {
        // IPv6 リテラル: ']' まで host、その後 ':port' があれば port
        let close = rest
            .find(']')
            .ok_or_else(|| format!("unterminated IPv6 literal in upstream URL: {}", url))?;
        let host = &rest[..close];
        let after = &rest[close + 1..];
        if after.is_empty() {
            (host, None)
        } else if let Some(port_str) = after.strip_prefix(':') {
            (host, Some(port_str))
        } else {
            return Err(format!(
                "unexpected characters after IPv6 literal in upstream URL: {}",
                url
            )
            .into());
        }
    } else if let Some(colon) = authority.rfind(':') {
        // IPv4 / reg-name: 末尾の ':' で port を切り出す
        (&authority[..colon], Some(&authority[colon + 1..]))
    } else {
        (authority, None)
    };

    if host.is_empty() {
        return Err(format!("upstream URL has empty host: {}", url).into());
    }

    let port = match port_opt {
        Some("") => scheme.default_port(),
        Some(s) => s
            .parse::<u16>()
            .map_err(|e| format!("invalid port in upstream URL: {} ({})", url, e))?,
        None => scheme.default_port(),
    };

    Ok(UpstreamUrl {
        scheme,
        host: host.to_string(),
        port,
    })
}

/// Host ヘッダー値を組み立てる (RFC 9110 Section 7.2、issue 0050)
///
/// - デフォルトポートは省略する (正書法)
/// - IPv6 リテラルはブラケット表記で構築する
fn format_host_header(scheme: Scheme, host: &str, port: u16) -> String {
    let host_part = if host.contains(':') {
        format!("[{}]", host)
    } else {
        host.to_string()
    };
    if port == scheme.default_port() {
        host_part
    } else {
        format!("{}:{}", host_part, port)
    }
}

/// upstream への接続。plaintext / TLS を保持する (issue 0050)
///
/// `BufWriter` で wrap することで書き込みのシステムコール回数を抑える。
/// TLS バリアント (`rustls::ClientConnection` 内蔵) は plaintext の十数倍のサイズが
/// あるため `Box` でヒープに退避し、enum バリアント間のサイズ差を抑える。
enum UpstreamStream {
    Plain(BufWriter<TcpStream>),
    Tls(Box<BufWriter<TlsStream<TcpStream>>>),
}

impl AsyncRead for UpstreamStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            UpstreamStream::Plain(s) => Pin::new(s).poll_read(cx, buf),
            UpstreamStream::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for UpstreamStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            UpstreamStream::Plain(s) => Pin::new(s).poll_write(cx, buf),
            UpstreamStream::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            UpstreamStream::Plain(s) => Pin::new(s).poll_flush(cx),
            UpstreamStream::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            UpstreamStream::Plain(s) => Pin::new(s).poll_shutdown(cx),
            UpstreamStream::Tls(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
        }
    }
}

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
    stream: UpstreamStream,
    created_at: Instant,
    last_used: Instant,
}

impl PooledConnection {
    fn new(stream: UpstreamStream) -> Self {
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
    /// scheme / host / port ごとのアイドル接続 (issue 0050)
    ///
    /// 旧実装は host 文字列のみをキーにしていたため、`http://a:8080/` と
    /// `https://a:443/` のプールエントリが混在する経路があった。
    idle_connections: HashMap<UpstreamKey, Vec<PooledConnection>>,
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
    fn try_acquire(&mut self, key: &UpstreamKey) -> Option<PooledConnection> {
        if let Some(connections) = self.idle_connections.get_mut(key) {
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
    fn release(&mut self, key: UpstreamKey, conn: PooledConnection) {
        if !conn.is_valid(&self.config) {
            return;
        }

        let connections = self.idle_connections.entry(key).or_default();

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
        let endpoints = self.idle_connections.len();
        let connections: usize = self.idle_connections.values().map(|v| v.len()).sum();
        (endpoints, connections)
    }
}

/// 新規接続を作成（ロック外で実行）。scheme で plaintext / TLS を分岐する (issue 0050)。
async fn create_connection(
    scheme: Scheme,
    host: &str,
    port: u16,
    tls_connector: &TlsConnector,
) -> Result<PooledConnection, Box<dyn std::error::Error + Send + Sync>> {
    let tcp_stream = TcpStream::connect((host, port)).await?;
    let stream = match scheme {
        Scheme::Http => UpstreamStream::Plain(BufWriter::with_capacity(65536, tcp_stream)),
        Scheme::Https => {
            // RFC 6066 Section 3: SNI / TLS Server Name は裸の host (ブラケット無し) を使う
            let server_name = ServerName::try_from(host.to_string())?;
            let tls_stream = tls_connector.connect(server_name, tcp_stream).await?;
            UpstreamStream::Tls(Box::new(BufWriter::with_capacity(65536, tls_stream)))
        }
    };
    Ok(PooledConnection::new(stream))
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
        .doc(concat!(
            "Upstream URL with scheme (http or https) and optional port\n",
            "Default: https://example.com\n",
            "Examples: http://internal:8080, https://[::1]:8443"
        ))
        .default("https://example.com")
        .take(&mut args)
        .then(|o| Ok::<_, &str>(o.value().to_string()))
        .map_err(|e| format!("{:?}", e))?;

    // 未知の引数があればエラー、ヘルプが返されたら表示
    if let Some(help) = args.finish().map_err(|e| format!("{:?}", e))? {
        print!("{}", help);
        std::process::exit(0);
    }

    // tracing の初期化
    tracing_subscriber::fmt()
        .with_max_level(if debug {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        })
        .init();

    // upstream URL から scheme / host / port を抽出 (issue 0050)
    let upstream = parse_upstream_url(&upstream_url)?;
    let upstream_host_header = format_host_header(upstream.scheme, &upstream.host, upstream.port);
    let upstream = Arc::new(upstream);

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
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let mut pool = cleanup_pool.lock().await;
            pool.cleanup_expired();
            let (hosts, conns) = pool.stats();
            debug!(hosts, connections = conns, "Pool cleanup done");
        }
    });

    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await?;

    info!(addr = %addr, upstream = %upstream_url, "Reverse proxy listening");
    info!(
        max_connections_per_host = pool_config.max_connections_per_host,
        idle_timeout_secs = pool_config.idle_timeout_secs,
        max_lifetime_secs = pool_config.max_lifetime_secs,
        "Connection pool enabled"
    );

    loop {
        let (socket, _) = listener.accept().await?;
        let upstream = upstream.clone();
        let upstream_host_header = upstream_host_header.clone();
        let pool = pool.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(socket, &upstream, &upstream_host_header, pool).await {
                error!(error = %e, "Client handler error");
            }
        });
    }
}

async fn handle_client(
    mut socket: TcpStream,
    upstream: &UpstreamUrl,
    upstream_host_header: &str,
    pool: SharedPool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // クライアントからリクエストヘッダーを受信
    let mut decoder = RequestDecoder::new();
    const READ_CHUNK: usize = 4096;
    let (req_head, req_body_kind) = loop {
        let want = decoder.available_buf().min(READ_CHUNK);
        if want == 0 {
            return Err("decoder buffer full".into());
        }
        let buf = decoder.mut_buf(want)?;
        let n = socket.read(buf).await?;
        if n == 0 {
            decoder.advance_buf(0);
            debug!("client input is empty");
            return Ok(());
        }
        decoder.advance_buf(n);
        debug!(bytes = n, "Received bytes from client");

        if let Some(result) = decoder.decode_headers()? {
            break result;
        }
    };

    debug!(
        method = %req_head.method(),
        uri = %req_head.uri(),
        version = %req_head.version(),
        "Request line"
    );
    debug!(count = req_head.headers().len(), "Received headers");

    // CONNECT (RFC 9110 Section 9.3.6) は reverse proxy の責務外 (任意宛先への
    // トンネル化は forward proxy 機能) として 405 Method Not Allowed で拒否する。
    // RFC 9110 Section 15.5.6 + Section 10.2.1: 405 では `Allow` ヘッダーで
    // 許可メソッド一覧を返す MUST。
    // decoder は CONNECT 受信で Tunnel phase に遷移しているため後続の
    // decode_headers() / decode() / peek_body() は使えず、本判定を抜けると
    // ハンドラがハングする経路を持つ (issue 0051)。
    if matches!(req_body_kind, BodyKind::Tunnel) {
        info!(method = %req_head.method(), "CONNECT rejected with 405 Method Not Allowed");
        let response = Response::with_status(StatusCode::METHOD_NOT_ALLOWED)
            .header("Allow", "GET, HEAD, POST, PUT, DELETE, OPTIONS, PATCH")?
            .header("Content-Length", "0")?
            .header("Connection", "close")?;
        socket.write_all(&response.encode()?).await?;
        return Ok(());
    }

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
                            // NeedData (chunked CRLF 不足) でも内側ループ継続。
                            // 直後の peek_body() は None を返すため progress 分岐に進む。
                            BodyProgress::Advanced | BodyProgress::NeedData => {}
                        }
                    }
                    None => {
                        // peek_body() が None でも progress() で状態遷移を試みる
                        match decoder.progress()? {
                            BodyProgress::Complete { .. } => break 'outer,
                            BodyProgress::Advanced => continue,
                            // バッファ不足: 内側ループを抜けて I/O 読み取りに戻る
                            BodyProgress::NeedData => break,
                        }
                    }
                }
            }

            let want = decoder.available_buf().min(READ_CHUNK);
            if want == 0 {
                return Err("decoder buffer full".into());
            }
            let buf = decoder.mut_buf(want)?;
            let n = socket.read(buf).await?;
            if n == 0 {
                decoder.advance_buf(0);
                // クライアントが切断した - 不完全なボディを upstream に送信してはいけない
                return Err("client disconnected during request body".into());
            }
            decoder.advance_buf(n);
        }
    }

    // アップストリームへプロキシリクエストを作成
    let mut upstream_request = Request::new(req_head.method(), req_head.uri())?;

    // Connection ヘッダーに列挙されたヘッダー名を収集
    let connection_headers: Vec<String> = req_head
        .headers()
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

    // ヘッダーをコピー (hop-by-hop ヘッダー、Host、Content-Length は除外)
    // Content-Length は Transfer-Encoding 除外後に不整合が生じる可能性があるため除外し、
    // encoder の自動設定に任せる (RFC 9112 Section 6.3 対応)
    for (name, value) in req_head.headers() {
        if name.eq_ignore_ascii_case("host") {
            continue;
        }
        if name.eq_ignore_ascii_case("content-length") {
            continue;
        }
        if is_hop_by_hop_header(name, &connection_headers) {
            continue;
        }
        upstream_request.add_header(name, value)?;
    }

    upstream_request.add_header("Host", upstream_host_header)?;
    // Keep-Alive を使用して接続を再利用
    upstream_request.add_header("Connection", "keep-alive")?;
    // 元リクエストにフレーミングがあった場合のみボディを引き継ぐ。
    // BodyKind::None なら upstream にもボディなしで送る (Content-Length 自動付与もしない)。
    let upstream_request = if matches!(req_body_kind, BodyKind::None) {
        upstream_request
    } else {
        upstream_request.body(request_body)
    };

    debug!(
        body_size = upstream_request.body_bytes().map(<[u8]>::len).unwrap_or(0),
        "Upstream request body"
    );

    // 接続プールから接続を取得してリクエストを送信
    let result =
        stream_upstream_response_pooled(&mut socket, &upstream_request, upstream, pool.clone())
            .await;

    // エラーの場合はログに出力
    if let Err(ref e) = result {
        debug!(error = %e, "Upstream error");
    }

    result
}

async fn stream_upstream_response_pooled(
    downstream: &mut TcpStream,
    request: &Request,
    upstream: &UpstreamUrl,
    pool: SharedPool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let start = Instant::now();
    let key = upstream.key();

    // まずプールからアイドル接続を取得（ロックは短時間のみ保持）
    let (mut conn, from_pool) = {
        let mut pool_guard = pool.lock().await;
        if let Some(conn) = pool_guard.try_acquire(&key) {
            (conn, true)
        } else {
            // プールにない場合は TLS コネクタを取得してロックを解放
            let tls_connector = pool_guard.tls_connector();
            drop(pool_guard); // 明示的にロックを解放

            // ロック外で新規接続を作成（時間がかかる処理）
            let conn = create_connection(
                upstream.scheme,
                &upstream.host,
                upstream.port,
                &tls_connector,
            )
            .await?;
            (conn, false)
        }
    };

    let acquire_time = start.elapsed();
    debug!(
        upstream_scheme = ?upstream.scheme,
        upstream_host = %upstream.host,
        upstream_port = upstream.port,
        acquire_time_ms = acquire_time.as_millis() as u64,
        source = if from_pool { "pool" } else { "new" },
        "Acquired connection"
    );

    // リクエスト送信とレスポンス受信
    let request_method = request.method().to_string();
    let result =
        stream_response_on_connection(downstream, request, &request_method, &mut conn.stream).await;

    // 接続を再利用するかどうかを判定
    let should_reuse = match &result {
        Ok(reuse) => *reuse,
        Err(_) => false,
    };

    if should_reuse {
        conn.last_used = Instant::now();
        pool.lock().await.release(key, conn);
        debug!("connection returned to pool");
    } else {
        debug!("connection closed (not reusable)");
    }

    result.map(|_| ())
}

/// 接続上でリクエストを送信しレスポンスを転送
/// 戻り値: 接続を再利用可能かどうか
async fn stream_response_on_connection(
    downstream: &mut TcpStream,
    request: &Request,
    method: &str,
    upstream: &mut UpstreamStream,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    // ダウンストリームをバッファリング（64KB バッファ）
    let mut downstream = BufWriter::with_capacity(65536, downstream);

    // リクエスト送信
    let request_bytes = request.encode()?;
    debug!(bytes = request_bytes.len(), "Upstream request bytes");
    upstream.write_all(&request_bytes).await?;
    // UpstreamStream は内部で BufWriter を保持しているため明示的に flush する必要がある。
    upstream.flush().await?;

    // レスポンスヘッダーを受信
    let mut decoder = ResponseDecoder::with_limits(DecoderLimits {
        max_buffer_size: 256 * 1024,
        max_body_size: 50 * 1024 * 1024,
        ..Default::default()
    });

    // RFC 9112 Section 6.3: メッセージ長は元リクエストのメソッドに依存する
    // (HEAD はボディなし、CONNECT 2xx はトンネルモード)。
    // RFC 9110 Section 9.1: メソッドトークンは case-sensitive のため、
    // 小文字化等の変換をせずそのまま渡す。
    decoder.set_request_method(method);

    const READ_CHUNK: usize = 8192;

    let (resp_head, body_kind) = loop {
        let want = decoder.available_buf().min(READ_CHUNK);
        if want == 0 {
            return Err("decoder buffer full".into());
        }
        let dst = decoder.mut_buf(want)?;
        let n = upstream.read(dst).await?;
        if n == 0 {
            decoder.advance_buf(0);
            return Err("接続が閉じられました".into());
        }
        decoder.advance_buf(n);

        debug!(bytes = n, "Upstream received bytes");

        if let Some(result) = decoder.decode_headers()? {
            break result;
        }
    };

    debug!(
        version = %resp_head.version(),
        status_code = resp_head.status_code(),
        reason_phrase = %resp_head.reason_phrase(),
        "Upstream response"
    );

    // Keep-Alive かどうかで再利用可能性を判定
    let mut can_reuse = resp_head.is_keep_alive();

    // RFC 9112 Section 6.3: Content-Length も Transfer-Encoding: chunked もない場合、
    // 接続が閉じるまでをボディとする (close-delimited body)
    let is_close_delimited = matches!(body_kind, BodyKind::CloseDelimited);

    if is_close_delimited {
        can_reuse = false;
        debug!("Close-delimited body detected, connection will be closed");
    }

    // クライアントへレスポンスヘッダーを送信
    // 注: upstream の reason_phrase が空文字列の場合 (RFC 9112 Section 4 の reason-phrase absent)、
    // Response::new は Err を返すため from_raw_parts 経路に切り替える必要があるが、
    // 本サンプルでは upstream が常に reason_phrase を送る前提で `Response::new` を使う。
    // 任意の upstream を受け入れる本格的な proxy では、decoder 経由で得た raw_parts を
    // そのまま再構築する経路 (本 issue では公開されていない) を将来検討する。
    let mut response_for_headers =
        Response::new(resp_head.status_code(), resp_head.reason_phrase())?;

    let connection_headers: Vec<String> = resp_head
        .headers()
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
    let is_head = method.eq_ignore_ascii_case("HEAD");
    // HEAD の場合は元のヘッダーから Content-Length を取得 (RFC 9110 Section 9.3.2)
    // resp_head.content_length() は smuggling 検知 (mismatched 複数行 CL 等) で
    // Err を返すため、ここで伝播する。
    let content_length = match body_kind {
        BodyKind::ContentLength(len) => Some(len),
        BodyKind::None if is_head => resp_head.content_length()?,
        _ => None,
    };

    for (name, value) in resp_head.headers() {
        if is_hop_by_hop_header(name, &connection_headers) {
            continue;
        }
        // Content-Length と Transfer-Encoding は body_kind に基づいて後で設定する
        if name.eq_ignore_ascii_case("content-length")
            || name.eq_ignore_ascii_case("transfer-encoding")
        {
            continue;
        }
        response_for_headers.add_header(name, value)?;
    }

    if let Some(len) = content_length {
        response_for_headers.add_header("Content-Length", len.to_string())?;
        debug!(content_length = len, "Using Content-Length");
    } else if use_chunked {
        response_for_headers.add_header("Transfer-Encoding", "chunked")?;
        debug!("using Transfer-Encoding: chunked");
    } else if is_close_delimited {
        // close-delimited body: 接続が閉じるまでがボディ
        response_for_headers.add_header("Connection", "close")?;
        debug!("using Connection: close (close-delimited body)");
    }

    let header_bytes = encode_response_headers(&response_for_headers)?;
    downstream.write_all(&header_bytes).await?;
    downstream.flush().await?;

    // close-delimited body の場合: upstream が閉じるまでデータを転送
    // 注: ResponseDecoder の mark_eof() API を使わずに直接ストリーミング転送する理由:
    // - ボディをメモリに蓄積せずにリアルタイムで downstream に転送するため
    // - 大容量レスポンスでもメモリ効率が良い
    if is_close_delimited {
        debug!("Streaming close-delimited body until connection closes");
        // close-delimited body はデコーダーを介さず、upstream から downstream へ
        // そのまま転送するためスタックバッファを使う
        let mut buf = [0u8; READ_CHUNK];
        let mut close_delimited_bytes = 0usize;
        loop {
            let n = upstream.read(&mut buf).await?;
            if n == 0 {
                // upstream が閉じた = ボディ終了
                debug!(
                    total_bytes = close_delimited_bytes,
                    "Close-delimited body complete"
                );
                break;
            }
            downstream.write_all(&buf[..n]).await?;
            close_delimited_bytes += n;
            debug!(bytes = n, "Close-delimited body chunk");
        }
        downstream.flush().await?;
        return Ok(can_reuse);
    }

    // ボディをストリーミング転送
    let mut total_body_bytes = 0usize;
    if !matches!(body_kind, BodyKind::None) {
        'outer: loop {
            loop {
                match decoder.peek_body() {
                    Some(data) => {
                        let len = data.len();
                        if use_chunked {
                            // 単一チャンクのフレーミングはライブラリの encode_chunk に委譲する
                            // (`size\r\n<data>\r\n` 形式、RFC 9112 Section 7.1)
                            downstream.write_all(&encode_chunk(data)).await?;
                        } else {
                            downstream.write_all(data).await?;
                        }
                        total_body_bytes += len;

                        match decoder.consume_body(len)? {
                            BodyProgress::Complete { trailers } => {
                                if use_chunked {
                                    write_last_chunk(&mut downstream, &trailers).await?;
                                }
                                debug!(total_bytes = total_body_bytes, "Body complete");
                                break 'outer;
                            }
                            // NeedData (chunked CRLF 不足) でも内側ループ継続。
                            // 直後の peek_body() は None を返すため progress 分岐に進む。
                            BodyProgress::Advanced | BodyProgress::NeedData => {}
                        }
                    }
                    None => {
                        match decoder.progress()? {
                            BodyProgress::Complete { trailers } => {
                                if use_chunked {
                                    write_last_chunk(&mut downstream, &trailers).await?;
                                }
                                debug!(total_bytes = total_body_bytes, "Body complete");
                                break 'outer;
                            }
                            BodyProgress::Advanced => continue,
                            // バッファ不足: 内側ループを抜けて I/O 読み取りに戻る
                            BodyProgress::NeedData => break,
                        }
                    }
                }
            }

            let want = decoder.available_buf().min(READ_CHUNK);
            if want == 0 {
                return Err("decoder buffer full".into());
            }
            let dst = decoder.mut_buf(want)?;
            let n = upstream.read(dst).await?;
            if n == 0 {
                decoder.advance_buf(0);
                // upstream が予期せず切断 - 終端チャンクを送らずにエラーを返す
                // 不完全なレスポンスを完了扱いにしてはいけない
                debug!("upstream disconnected during response body");
                return Err("upstream disconnected during response body".into());
            }
            decoder.advance_buf(n);

            debug!(bytes = n, "Upstream body bytes");
        }
    }

    // バッファをフラッシュ
    downstream.flush().await?;

    debug!(total_bytes = total_body_bytes, "Response body streamed");

    Ok(can_reuse)
}

/// chunked 転送の終端 (`0\r\n<trailers>\r\n`) を downstream へ書き出す
///
/// trailers が空なら `encode_chunk(b"")` (= `0\r\n\r\n`) を流用する。
/// trailers がある場合は RFC 9112 Section 7.1.2 の trailer-section を手書きする
/// (ライブラリ側は encode_chunk に trailer を載せる API を提供していない)。
async fn write_last_chunk(
    downstream: &mut BufWriter<&mut TcpStream>,
    trailers: &[(String, String)],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if trailers.is_empty() {
        downstream.write_all(&encode_chunk(b"")).await?;
        return Ok(());
    }
    let mut end_chunk = b"0\r\n".to_vec();
    for (name, value) in trailers {
        end_chunk.extend_from_slice(format!("{}: {}\r\n", name, value).as_bytes());
    }
    end_chunk.extend_from_slice(b"\r\n");
    downstream.write_all(&end_chunk).await?;
    Ok(())
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
