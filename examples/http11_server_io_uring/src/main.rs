//! io_uring + kTLS を使った HTTP/1.1 サーバーの例 (Linux 専用)
//!
//! 使い方:
//!   cargo run -p http11_server_io_uring -- --cert cert.pem --key key.pem
//!
//! テスト用の自己署名証明書の作成:
//!   openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes \
//!     -subj "/CN=localhost"
//!
//! 要件:
//!   - Linux カーネル 6.7 以上 (io_uring setsockopt サポート)
//!   - CONFIG_TLS=y または CONFIG_TLS=m
//!   - tls カーネルモジュールがロード済み (modprobe tls)
//!
//! 圧縮対応:
//!   クライアントの Accept-Encoding ヘッダーに基づいて gzip, br, zstd で圧縮します。
//!   優先順位: zstd > br > gzip

mod compressor;

use std::collections::VecDeque;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::sync::Arc;

use compressor::{compress_body, encoding_header, select_encoding};
use io_uring::opcode;
use io_uring::squeue::Flags;
use io_uring::types::Fd;
use io_uring::{IoUring, Probe};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ServerConfig, ServerConnection, SupportedCipherSuite};
use shiguredo_http11::{EncodeError, HttpHead, RequestDecoder, Response, StatusCode};
use slab::Slab;
use tracing::{error, info};

/// Keep-Alive タイムアウト (秒)
/// TODO: io_uring でタイムアウト処理を実装する際に使用
#[allow(dead_code)]
const DEFAULT_KEEP_ALIVE_TIMEOUT: u64 = 60;
/// 1 接続あたりの最大リクエスト数
const DEFAULT_MAX_REQUESTS: u32 = 1000;
/// 読み取りバッファサイズ
const READ_BUF_SIZE: usize = 8192;
/// 書き込みバッファサイズ
const WRITE_BUF_SIZE: usize = 65536;
/// io_uring のエントリ数
const RING_ENTRIES: u32 = 256;

// kTLS 関連の定数
const SOL_TCP: u32 = 6;
const SOL_TLS: u32 = 282;
const TCP_ULP: u32 = 31;
const TLS_TX: u32 = 1;
const TLS_RX: u32 = 2;

/// サーバーオプション
struct ServerOptions {
    port: u16,
    cert_path: String,
    key_path: String,
}

/// 接続の状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionState {
    /// TLS ハンドシェイク中 (読み取り待ち)
    HandshakeReading,
    /// TLS ハンドシェイク中 (書き込み待ち)
    HandshakeWriting,
    /// kTLS 有効化中
    EnablingKtls,
    /// HTTP リクエスト読み取り中
    Reading,
    /// HTTP レスポンス書き込み中
    Writing,
    /// 接続終了処理中
    Closing,
}

/// io_uring 操作の種類
#[derive(Debug, Clone, Copy)]
enum OpType {
    Accept,
    Read,
    Write,
    Close,
    SetSockOpt,
}

/// io_uring のユーザーデータ
#[derive(Clone, Copy)]
struct UserData {
    conn_id: usize,
    op_type: OpType,
}

impl UserData {
    fn encode(conn_id: usize, op_type: OpType) -> u64 {
        let op_bits = match op_type {
            OpType::Accept => 0,
            OpType::Read => 1,
            OpType::Write => 2,
            OpType::Close => 3,
            OpType::SetSockOpt => 4,
        };
        ((conn_id as u64) << 32) | op_bits
    }

    fn decode(value: u64) -> Self {
        let conn_id = (value >> 32) as usize;
        let op_type = match value & 0xFFFFFFFF {
            0 => OpType::Accept,
            1 => OpType::Read,
            2 => OpType::Write,
            3 => OpType::Close,
            4 => OpType::SetSockOpt,
            _ => OpType::Close,
        };
        Self { conn_id, op_type }
    }
}

/// 接続情報
struct Connection {
    fd: RawFd,
    state: ConnectionState,
    tls_conn: Option<ServerConnection>,
    cipher_suite: Option<SupportedCipherSuite>,
    ktls_tx: Option<ktls::CryptoInfo>,
    ktls_rx: Option<ktls::CryptoInfo>,
    decoder: RequestDecoder,
    read_buf: Vec<u8>,
    write_buf: Vec<u8>,
    write_offset: usize,
    request_count: u32,
    peer_addr: SocketAddr,
    // kTLS 有効化用の一時バッファ (setsockopt に渡すため生存期間を保証)
    ulp_name: Vec<u8>,
    ktls_pending_ops: u8,
}

impl Connection {
    fn new(fd: RawFd, peer_addr: SocketAddr, tls_config: Arc<ServerConfig>) -> Self {
        let tls_conn = ServerConnection::new(tls_config).expect("failed to create TLS connection");
        Self {
            fd,
            state: ConnectionState::HandshakeReading,
            tls_conn: Some(tls_conn),
            cipher_suite: None,
            ktls_tx: None,
            ktls_rx: None,
            decoder: RequestDecoder::new(),
            read_buf: vec![0u8; READ_BUF_SIZE],
            write_buf: Vec::with_capacity(WRITE_BUF_SIZE),
            write_offset: 0,
            request_count: 0,
            peer_addr,
            ulp_name: b"tls\0".to_vec(),
            ktls_pending_ops: 0,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let options = parse_args()?;

    // io_uring のサポートを確認
    check_io_uring_support()?;

    // TLS 設定を読み込み
    let tls_config = Arc::new(load_tls_config(&options.cert_path, &options.key_path)?);

    // TCP リスナーを作成
    let addr = format!("0.0.0.0:{}", options.port);
    let listener = TcpListener::bind(&addr)?;
    listener.set_nonblocking(true)?;

    info!(addr = %addr, "HTTPS server listening (io_uring + kTLS)");

    // io_uring を初期化
    let mut ring: IoUring = IoUring::builder()
        .setup_sqpoll(1000) // SQPOLL モード (1ms idle)
        .build(RING_ENTRIES)?;

    // 接続を管理する slab
    let mut connections: Slab<Connection> = Slab::with_capacity(1024);

    // リスナー fd を登録
    let listener_fd = listener.as_raw_fd();

    // 最初の accept を発行
    submit_accept(&mut ring, listener_fd)?;

    // メインループ
    loop {
        // 完了キューを処理
        ring.submit_and_wait(1)?;

        let cq = ring.completion();
        let cqes: Vec<_> = cq.collect();

        for cqe in cqes {
            let user_data = UserData::decode(cqe.user_data());
            let result = cqe.result();

            match user_data.op_type {
                OpType::Accept => {
                    // 次の accept を発行
                    submit_accept(&mut ring, listener_fd)?;

                    if result < 0 {
                        let err = std::io::Error::from_raw_os_error(-result);
                        if err.kind() != std::io::ErrorKind::WouldBlock {
                            error!(error = %err, "Accept error");
                        }
                        continue;
                    }

                    let client_fd = result;
                    let peer_addr = get_peer_addr(client_fd)
                        .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], 0)));

                    info!(peer_addr = %peer_addr, "Connection accepted");

                    // 新しい接続を登録
                    let conn = Connection::new(client_fd, peer_addr, tls_config.clone());
                    let conn_id = connections.insert(conn);

                    // ハンドシェイクのための読み取りを開始
                    submit_read(&mut ring, conn_id, client_fd, &mut connections)?;
                }
                OpType::Read => {
                    if !connections.contains(user_data.conn_id) {
                        continue;
                    }

                    if result <= 0 {
                        if result < 0 {
                            let err = std::io::Error::from_raw_os_error(-result);
                            if err.kind() != std::io::ErrorKind::WouldBlock {
                                error!(
                                    peer_addr = %connections[user_data.conn_id].peer_addr,
                                    error = %err,
                                    "Read error"
                                );
                            }
                        } else {
                            info!(
                                peer_addr = %connections[user_data.conn_id].peer_addr,
                                "Connection closed by client"
                            );
                        }
                        close_connection(&mut ring, &mut connections, user_data.conn_id)?;
                        continue;
                    }

                    let bytes_read = result as usize;
                    handle_read(&mut ring, &mut connections, user_data.conn_id, bytes_read)?;
                }
                OpType::Write => {
                    if !connections.contains(user_data.conn_id) {
                        continue;
                    }

                    if result < 0 {
                        let err = std::io::Error::from_raw_os_error(-result);
                        error!(
                            peer_addr = %connections[user_data.conn_id].peer_addr,
                            error = %err,
                            "Write error"
                        );
                        close_connection(&mut ring, &mut connections, user_data.conn_id)?;
                        continue;
                    }

                    let bytes_written = result as usize;
                    handle_write(
                        &mut ring,
                        &mut connections,
                        user_data.conn_id,
                        bytes_written,
                    )?;
                }
                OpType::SetSockOpt => {
                    if !connections.contains(user_data.conn_id) {
                        continue;
                    }

                    if result < 0 {
                        let err = std::io::Error::from_raw_os_error(-result);
                        error!(
                            peer_addr = %connections[user_data.conn_id].peer_addr,
                            error = %err,
                            "SetSockOpt error"
                        );
                        close_connection(&mut ring, &mut connections, user_data.conn_id)?;
                        continue;
                    }

                    handle_setsockopt_complete(&mut ring, &mut connections, user_data.conn_id)?;
                }
                OpType::Close => {
                    if connections.contains(user_data.conn_id) {
                        connections.remove(user_data.conn_id);
                    }
                }
            }
        }
    }
}

fn parse_args() -> Result<ServerOptions, Box<dyn std::error::Error>> {
    let mut args = noargs::raw_args();
    args.metadata_mut().app_name = "http11_server_io_uring";

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

    // --port オプション
    let port: u16 = noargs::opt("port")
        .short('p')
        .doc("Port to listen on (default: 8443)")
        .default("8443")
        .take(&mut args)
        .then(|o| o.value().parse())
        .map_err(|e| format!("{:?}", e))?;

    // --cert オプション (必須)
    let cert_path: String = noargs::opt("cert")
        .doc("Path to certificate file (PEM) [required]")
        .take(&mut args)
        .then(|o| Ok::<_, &str>(o.value().to_string()))
        .map_err(|e| format!("{:?}", e))?;

    // --key オプション (必須)
    let key_path: String = noargs::opt("key")
        .doc("Path to private key file (PEM) [required]")
        .take(&mut args)
        .then(|o| Ok::<_, &str>(o.value().to_string()))
        .map_err(|e| format!("{:?}", e))?;

    // 未知の引数があればエラー、ヘルプが返されたら表示
    if let Some(help) = args.finish().map_err(|e| format!("{:?}", e))? {
        print!("{}", help);
        std::process::exit(0);
    }

    Ok(ServerOptions {
        port,
        cert_path,
        key_path,
    })
}

fn check_io_uring_support() -> Result<(), Box<dyn std::error::Error>> {
    let mut probe = Probe::new();
    if IoUring::new(8)?
        .submitter()
        .register_probe(&mut probe)
        .is_err()
    {
        return Err("io_uring probe failed".into());
    }

    // 必要な操作がサポートされているか確認
    let required_ops = [
        (opcode::Accept::CODE, "ACCEPT"),
        (opcode::Read::CODE, "READ"),
        (opcode::Write::CODE, "WRITE"),
        (opcode::Close::CODE, "CLOSE"),
        (opcode::SetSockOpt::CODE, "SETSOCKOPT"),
    ];

    for (code, name) in required_ops {
        if !probe.is_supported(code) {
            return Err(format!("io_uring operation {} is not supported", name).into());
        }
    }

    Ok(())
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

    // kTLS 互換の暗号スイートのみを有効化
    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    // kTLS のために秘密鍵抽出を有効化
    config.enable_secret_extraction = true;

    Ok(config)
}

fn submit_accept(ring: &mut IoUring, listener_fd: RawFd) -> std::io::Result<()> {
    let accept_e = opcode::Accept::new(Fd(listener_fd), std::ptr::null_mut(), std::ptr::null_mut())
        .build()
        .user_data(UserData::encode(usize::MAX, OpType::Accept));

    unsafe {
        ring.submission()
            .push(&accept_e)
            .map_err(|_| std::io::Error::other("submission queue full"))?;
    }
    Ok(())
}

fn submit_read(
    ring: &mut IoUring,
    conn_id: usize,
    fd: RawFd,
    connections: &mut Slab<Connection>,
) -> std::io::Result<()> {
    let conn = &mut connections[conn_id];
    let read_e = opcode::Read::new(
        Fd(fd),
        conn.read_buf.as_mut_ptr(),
        conn.read_buf.len() as u32,
    )
    .build()
    .user_data(UserData::encode(conn_id, OpType::Read));

    unsafe {
        ring.submission()
            .push(&read_e)
            .map_err(|_| std::io::Error::other("submission queue full"))?;
    }
    Ok(())
}

fn submit_write(
    ring: &mut IoUring,
    conn_id: usize,
    fd: RawFd,
    connections: &mut Slab<Connection>,
) -> std::io::Result<()> {
    let conn = &connections[conn_id];
    let remaining = &conn.write_buf[conn.write_offset..];
    let write_e = opcode::Write::new(Fd(fd), remaining.as_ptr(), remaining.len() as u32)
        .build()
        .user_data(UserData::encode(conn_id, OpType::Write));

    unsafe {
        ring.submission()
            .push(&write_e)
            .map_err(|_| std::io::Error::other("submission queue full"))?;
    }
    Ok(())
}

fn submit_close(ring: &mut IoUring, conn_id: usize, fd: RawFd) -> std::io::Result<()> {
    let close_e = opcode::Close::new(Fd(fd))
        .build()
        .user_data(UserData::encode(conn_id, OpType::Close));

    unsafe {
        ring.submission()
            .push(&close_e)
            .map_err(|_| std::io::Error::other("submission queue full"))?;
    }
    Ok(())
}

/// kTLS を有効化するための setsockopt を io_uring でサブミット
fn submit_enable_ktls(
    ring: &mut IoUring,
    conn_id: usize,
    connections: &mut Slab<Connection>,
) -> std::io::Result<()> {
    let conn = &connections[conn_id];
    let fd = conn.fd;

    // 1. TCP_ULP を "tls" に設定
    let ulp_op = opcode::SetSockOpt::new(
        Fd(fd),
        SOL_TCP,
        TCP_ULP,
        conn.ulp_name.as_ptr() as *const libc::c_void,
        conn.ulp_name.len() as u32,
    )
    .build()
    .flags(Flags::IO_LINK) // 次の操作とリンク
    .user_data(UserData::encode(conn_id, OpType::SetSockOpt));

    // 2. TLS_TX を設定
    let ktls_tx = conn.ktls_tx.as_ref().unwrap();
    let tx_op = opcode::SetSockOpt::new(
        Fd(fd),
        SOL_TLS,
        TLS_TX,
        ktls_tx.as_ptr(),
        ktls_tx.size() as u32,
    )
    .build()
    .flags(Flags::IO_LINK) // 次の操作とリンク
    .user_data(UserData::encode(conn_id, OpType::SetSockOpt));

    // 3. TLS_RX を設定
    let ktls_rx = conn.ktls_rx.as_ref().unwrap();
    let rx_op = opcode::SetSockOpt::new(
        Fd(fd),
        SOL_TLS,
        TLS_RX,
        ktls_rx.as_ptr(),
        ktls_rx.size() as u32,
    )
    .build()
    .user_data(UserData::encode(conn_id, OpType::SetSockOpt));

    unsafe {
        let mut sq = ring.submission();
        sq.push(&ulp_op)
            .map_err(|_| std::io::Error::other("submission queue full"))?;
        sq.push(&tx_op)
            .map_err(|_| std::io::Error::other("submission queue full"))?;
        sq.push(&rx_op)
            .map_err(|_| std::io::Error::other("submission queue full"))?;
    }

    // 3 つの操作を待機
    let conn = &mut connections[conn_id];
    conn.ktls_pending_ops = 3;

    Ok(())
}

fn handle_read(
    ring: &mut IoUring,
    connections: &mut Slab<Connection>,
    conn_id: usize,
    bytes_read: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = &mut connections[conn_id];
    let fd = conn.fd;
    let peer_addr = conn.peer_addr;

    match conn.state {
        ConnectionState::HandshakeReading | ConnectionState::HandshakeWriting => {
            // TLS ハンドシェイク処理
            let data = conn.read_buf[..bytes_read].to_vec();
            let tls_conn = conn.tls_conn.as_mut().unwrap();

            // 受信データを TLS 接続に渡す
            let mut rd = std::io::Cursor::new(&data);
            match tls_conn.read_tls(&mut rd) {
                Ok(_) => {}
                Err(e) => {
                    error!(peer_addr = %peer_addr, error = %e, "TLS read error");
                    close_connection(ring, connections, conn_id)?;
                    return Ok(());
                }
            }

            // TLS 状態を処理
            match tls_conn.process_new_packets() {
                Ok(_) => {}
                Err(e) => {
                    error!(peer_addr = %peer_addr, error = %e, "TLS handshake error");
                    close_connection(ring, connections, conn_id)?;
                    return Ok(());
                }
            }

            // TLS が書き込みを要求しているか確認
            if tls_conn.wants_write() {
                let conn = &mut connections[conn_id];
                conn.write_buf.clear();
                conn.write_offset = 0;
                let tls_conn = conn.tls_conn.as_mut().unwrap();
                tls_conn.write_tls(&mut conn.write_buf)?;
                conn.state = ConnectionState::HandshakeWriting;
                submit_write(ring, conn_id, fd, connections)?;
                return Ok(());
            }

            // ハンドシェイク完了チェック
            if !tls_conn.is_handshaking() {
                info!(peer_addr = %peer_addr, "TLS handshake completed");

                // 暗号スイートを取得
                let cipher_suite = tls_conn.negotiated_cipher_suite().unwrap();

                // tls_conn を取り出して秘密鍵を抽出
                let conn = &mut connections[conn_id];
                let tls_conn = conn.tls_conn.take().unwrap();
                let secrets = tls_conn
                    .dangerous_extract_secrets()
                    .map_err(|e| format!("failed to extract TLS secrets: {:?}", e))?;

                // ktls::CryptoInfo に変換
                let ktls_tx = ktls::CryptoInfo::from_rustls(cipher_suite, secrets.tx)
                    .map_err(|e| format!("failed to create ktls TX info: {:?}", e))?;
                let ktls_rx = ktls::CryptoInfo::from_rustls(cipher_suite, secrets.rx)
                    .map_err(|e| format!("failed to create ktls RX info: {:?}", e))?;

                // 接続情報を更新
                conn.cipher_suite = Some(cipher_suite);
                conn.ktls_tx = Some(ktls_tx);
                conn.ktls_rx = Some(ktls_rx);
                conn.state = ConnectionState::EnablingKtls;

                // kTLS を有効化
                submit_enable_ktls(ring, conn_id, connections)?;
            } else {
                // さらにデータが必要
                let conn = &mut connections[conn_id];
                conn.state = ConnectionState::HandshakeReading;
                submit_read(ring, conn_id, fd, connections)?;
            }
        }
        ConnectionState::Reading => {
            // HTTP リクエスト処理 (kTLS 有効時は平文)
            let data = conn.read_buf[..bytes_read].to_vec();
            conn.decoder.feed(&data)?;

            let mut responses = VecDeque::new();
            let peer_addr = conn.peer_addr;
            let mut request_count = conn.request_count;

            while let Some(request) = conn.decoder.decode()? {
                request_count += 1;

                info!(
                    method = %request.method(),
                    uri = %request.uri(),
                    version = %request.version(),
                    peer_addr = %peer_addr,
                    request_count,
                    "Request received (kTLS)"
                );

                // Keep-Alive 継続判定
                let should_keep_alive =
                    request.is_keep_alive() && request_count < DEFAULT_MAX_REQUESTS;

                let response = build_response(&request, should_keep_alive)?;
                responses.push_back((response.encode(), should_keep_alive));
            }

            conn.request_count = request_count;

            if let Some((response_bytes, should_keep_alive)) = responses.pop_front() {
                let conn = &mut connections[conn_id];
                conn.write_buf = response_bytes;
                conn.write_offset = 0;
                conn.state = if should_keep_alive {
                    ConnectionState::Writing
                } else {
                    ConnectionState::Closing
                };
                submit_write(ring, conn_id, fd, connections)?;
            } else {
                // リクエストがまだ完全ではない
                submit_read(ring, conn_id, fd, connections)?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn handle_write(
    ring: &mut IoUring,
    connections: &mut Slab<Connection>,
    conn_id: usize,
    bytes_written: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = &mut connections[conn_id];
    let fd = conn.fd;
    let peer_addr = conn.peer_addr;

    conn.write_offset += bytes_written;

    if conn.write_offset < conn.write_buf.len() {
        // まだ書き込みが残っている
        submit_write(ring, conn_id, fd, connections)?;
        return Ok(());
    }

    // 書き込み完了
    conn.write_buf.clear();
    conn.write_offset = 0;

    match conn.state {
        ConnectionState::HandshakeWriting => {
            let tls_conn = conn.tls_conn.as_ref().unwrap();
            if !tls_conn.is_handshaking() {
                info!(peer_addr = %peer_addr, "TLS handshake completed");

                // 暗号スイートを取得
                let cipher_suite = tls_conn.negotiated_cipher_suite().unwrap();

                // tls_conn を取り出して秘密鍵を抽出
                let tls_conn = conn.tls_conn.take().unwrap();
                let secrets = tls_conn
                    .dangerous_extract_secrets()
                    .map_err(|e| format!("failed to extract TLS secrets: {:?}", e))?;

                // ktls::CryptoInfo に変換
                let ktls_tx = ktls::CryptoInfo::from_rustls(cipher_suite, secrets.tx)
                    .map_err(|e| format!("failed to create ktls TX info: {:?}", e))?;
                let ktls_rx = ktls::CryptoInfo::from_rustls(cipher_suite, secrets.rx)
                    .map_err(|e| format!("failed to create ktls RX info: {:?}", e))?;

                // 接続情報を更新
                conn.cipher_suite = Some(cipher_suite);
                conn.ktls_tx = Some(ktls_tx);
                conn.ktls_rx = Some(ktls_rx);
                conn.state = ConnectionState::EnablingKtls;

                // kTLS を有効化
                submit_enable_ktls(ring, conn_id, connections)?;
            } else {
                // さらにハンドシェイクが必要
                conn.state = ConnectionState::HandshakeReading;
                submit_read(ring, conn_id, fd, connections)?;
            }
        }
        ConnectionState::Writing => {
            // Keep-Alive: 次のリクエストを待つ
            conn.state = ConnectionState::Reading;
            submit_read(ring, conn_id, fd, connections)?;
        }
        ConnectionState::Closing => {
            // 接続を閉じる
            close_connection(ring, connections, conn_id)?;
        }
        _ => {}
    }

    Ok(())
}

fn handle_setsockopt_complete(
    ring: &mut IoUring,
    connections: &mut Slab<Connection>,
    conn_id: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = &mut connections[conn_id];

    conn.ktls_pending_ops -= 1;

    if conn.ktls_pending_ops == 0 {
        // すべての setsockopt が完了
        info!(peer_addr = %conn.peer_addr, "kTLS enabled");

        // kTLS 情報をクリア (もう不要)
        conn.ktls_tx = None;
        conn.ktls_rx = None;

        // HTTP リクエストの読み取りを開始
        let fd = conn.fd;
        conn.state = ConnectionState::Reading;
        submit_read(ring, conn_id, fd, connections)?;
    }

    Ok(())
}

fn close_connection(
    ring: &mut IoUring,
    connections: &mut Slab<Connection>,
    conn_id: usize,
) -> std::io::Result<()> {
    if !connections.contains(conn_id) {
        return Ok(());
    }

    let conn = &connections[conn_id];
    let fd = conn.fd;
    info!(peer_addr = %conn.peer_addr, "Closing connection");
    submit_close(ring, conn_id, fd)?;
    Ok(())
}

fn get_peer_addr(fd: RawFd) -> std::io::Result<SocketAddr> {
    let stream = unsafe { TcpStream::from_raw_fd(fd) };
    let addr = stream.peer_addr()?;
    // fd の所有権を戻す (drop させない)
    std::mem::forget(stream);
    Ok(addr)
}

fn build_response(
    request: &shiguredo_http11::Request,
    should_keep_alive: bool,
) -> Result<Response, EncodeError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // RFC 9110 準拠の Date ヘッダー (IMF-fixdate 形式)
    let date = format_http_date(now);

    // RFC 9110 Section 9.3.2: HEAD レスポンスは GET と同じヘッダーを返すがボディは送信しない
    let is_head = request.method().eq_ignore_ascii_case("HEAD");

    // Accept-Encoding ヘッダーから圧縮方式を選択
    let accept_encoding = HttpHead::headers(request)
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("Accept-Encoding"))
        .map(|(_, value)| value.as_str());

    let encoding = accept_encoding.and_then(select_encoding);

    let response = match request.uri() {
        "/" => {
            let body_content = r#"<!DOCTYPE html>
<html>
<head><title>shiguredo_http11 Server (io_uring + kTLS)</title></head>
<body>
<h1>Welcome to shiguredo_http11 Server</h1>
<p>This server is powered by shiguredo_http11 library with io_uring and kTLS.</p>
<ul>
<li><a href="/info">/info</a> - Server information</li>
<li><a href="/echo">/echo</a> - Echo request details</li>
</ul>
</body>
</html>
"#;
            build_compressed_response(
                StatusCode::OK,
                "text/html; charset=utf-8",
                body_content.as_bytes(),
                &date,
                is_head,
                encoding,
            )?
        }
        "/info" => {
            let body_content = format!(
                r#"{{"server":"shiguredo_http11","version":"0.1.0","backend":"io_uring+kTLS","timestamp":{}}}"#,
                now
            );
            build_compressed_response(
                StatusCode::OK,
                "application/json",
                body_content.as_bytes(),
                &date,
                is_head,
                encoding,
            )?
        }
        "/echo" => {
            // HEAD リクエストの /echo は空のボディで Content-Length: 0 を返す
            if is_head {
                let head_response = Response::with_status(StatusCode::OK)
                    .header("Date", &date)?
                    .header("Content-Type", "text/plain; charset=utf-8")?
                    .header("Content-Length", "0")?
                    .header("Server", "shiguredo_http11/0.1.0 (io_uring+kTLS)")?
                    .omit_body(true);
                return add_connection_headers(head_response, should_keep_alive);
            }

            let mut body = format!(
                "Method: {}\nURI: {}\nVersion: {}\n\nHeaders:\n",
                request.method(),
                request.uri(),
                request.version()
            );

            for (name, value) in HttpHead::headers(request) {
                body.push_str(&format!("  {}: {}\n", name, value));
            }

            if let Some(req_body) = request.body_bytes()
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
                StatusCode::OK,
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
                StatusCode::NOT_FOUND,
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
    status: StatusCode,
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

    let mut response = Response::with_status(status)
        .header("Date", date)?
        .header("Content-Type", content_type)?
        .header("Content-Length", final_body.len().to_string())?
        .header("Server", "shiguredo_http11/0.1.0 (io_uring+kTLS)")?
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
