//! HTTP/HTTPS サーバー
//!
//! tokio と tokio-rustls を使用した非同期 HTTP サーバー。
//!
//! ## 使い方
//!
//! ```ignore
//! use tokio_http11::{Server, Request, Response};
//!
//! // HTTP サーバー
//! let server = Server::bind("0.0.0.0:8080").await?;
//! server.serve(handler).await?;
//!
//! // HTTPS サーバー
//! let server = Server::bind("0.0.0.0:8443").await?
//!     .tls(tls_config);
//! server.serve(handler).await?;
//! ```

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use rustls::ServerConfig;
use shiguredo_http11::{Request, RequestDecoder, Response};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

use crate::error::{Error, Result};

/// HTTP リクエストハンドラー
pub trait Handler: Send + Sync + 'static {
    /// リクエストを処理してレスポンスを返す
    fn handle(&self, request: Request) -> impl Future<Output = Response> + Send;
}

/// 関数からハンドラーを作成
impl<F, Fut> Handler for F
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send,
{
    fn handle(&self, request: Request) -> impl Future<Output = Response> + Send {
        (self)(request)
    }
}

/// HTTP サーバー
///
/// HTTP と HTTPS の両方に対応。HTTPS を使用する場合は `tls()` で TLS 設定を指定する。
pub struct Server {
    listener: TcpListener,
    keep_alive_timeout: Duration,
    max_requests_per_connection: u32,
    read_buffer_size: usize,
    write_buffer_size: usize,
    tls_acceptor: Option<TlsAcceptor>,
}

impl Server {
    /// 指定アドレスにバインド
    pub async fn bind(addr: &str) -> Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Self {
            listener,
            keep_alive_timeout: Duration::from_secs(60),
            max_requests_per_connection: 1000,
            read_buffer_size: 8192,
            write_buffer_size: 65536,
            tls_acceptor: None,
        })
    }

    /// TLS 設定を指定 (HTTPS 用)
    pub fn tls(mut self, config: Arc<ServerConfig>) -> Self {
        self.tls_acceptor = Some(TlsAcceptor::from(config));
        self
    }

    /// Keep-Alive タイムアウトを設定
    pub fn keep_alive_timeout(mut self, timeout: Duration) -> Self {
        self.keep_alive_timeout = timeout;
        self
    }

    /// 1 接続あたりの最大リクエスト数を設定
    pub fn max_requests_per_connection(mut self, max: u32) -> Self {
        self.max_requests_per_connection = max;
        self
    }

    /// 読み取りバッファサイズを設定
    pub fn read_buffer_size(mut self, size: usize) -> Self {
        self.read_buffer_size = size;
        self
    }

    /// 書き込みバッファサイズを設定
    pub fn write_buffer_size(mut self, size: usize) -> Self {
        self.write_buffer_size = size;
        self
    }

    /// ローカルアドレスを取得
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.listener.local_addr()?)
    }

    /// TLS が有効かどうかを返す
    pub fn is_tls(&self) -> bool {
        self.tls_acceptor.is_some()
    }

    /// サーバーを起動
    pub async fn serve<H: Handler>(self, handler: H) -> Result<()> {
        let config = Arc::new(ConnectionConfig {
            keep_alive_timeout: self.keep_alive_timeout,
            max_requests_per_connection: self.max_requests_per_connection,
            read_buffer_size: self.read_buffer_size,
            write_buffer_size: self.write_buffer_size,
        });
        let handler = Arc::new(handler);

        loop {
            let (stream, peer_addr) = self.listener.accept().await?;
            let config = config.clone();
            let handler = handler.clone();
            let tls_acceptor = self.tls_acceptor.clone();

            tokio::spawn(async move {
                let result = if let Some(acceptor) = tls_acceptor {
                    match acceptor.accept(stream).await {
                        Ok(tls_stream) => {
                            handle_tls_connection(tls_stream, peer_addr, config, handler).await
                        }
                        Err(e) => Err(Error::Tls(e.to_string())),
                    }
                } else {
                    handle_connection(stream, peer_addr, config, handler).await
                };

                if let Err(e) = result {
                    eprintln!("Connection error from {}: {}", peer_addr, e);
                }
            });
        }
    }

    /// 単一の接続を処理 (テスト用)
    pub async fn handle_one<H: Handler>(self, handler: H) -> Result<()> {
        let (stream, peer_addr) = self.listener.accept().await?;
        let config = Arc::new(ConnectionConfig {
            keep_alive_timeout: self.keep_alive_timeout,
            max_requests_per_connection: self.max_requests_per_connection,
            read_buffer_size: self.read_buffer_size,
            write_buffer_size: self.write_buffer_size,
        });
        let handler = Arc::new(handler);

        if let Some(ref acceptor) = self.tls_acceptor {
            let tls_stream = acceptor
                .accept(stream)
                .await
                .map_err(|e| Error::Tls(e.to_string()))?;
            handle_tls_connection(tls_stream, peer_addr, config, handler).await
        } else {
            handle_connection(stream, peer_addr, config, handler).await
        }
    }
}

struct ConnectionConfig {
    keep_alive_timeout: Duration,
    max_requests_per_connection: u32,
    read_buffer_size: usize,
    write_buffer_size: usize,
}

/// HTTP 接続を処理
async fn handle_connection<H: Handler>(
    stream: TcpStream,
    _peer_addr: SocketAddr,
    config: Arc<ConnectionConfig>,
    handler: Arc<H>,
) -> Result<()> {
    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::with_capacity(config.read_buffer_size, reader);
    let mut writer = BufWriter::with_capacity(config.write_buffer_size, writer);

    let mut decoder = RequestDecoder::new();
    let mut buf = vec![0u8; config.read_buffer_size];
    let mut request_count = 0u32;

    loop {
        let read_result =
            tokio::time::timeout(config.keep_alive_timeout, reader.read(&mut buf)).await;

        let n = match read_result {
            Ok(Ok(n)) => n,
            Ok(Err(e)) => return Err(Error::Io(e)),
            Err(_) => return Ok(()), // タイムアウト
        };

        if n == 0 {
            return Ok(()); // 接続が閉じられた
        }

        decoder.feed(&buf[..n])?;

        while let Some(request) = decoder.decode()? {
            request_count += 1;

            let should_keep_alive =
                request.is_keep_alive() && request_count < config.max_requests_per_connection;

            let mut response = handler.handle(request).await;

            // Connection ヘッダーを設定
            if !should_keep_alive && !response.has_header("Connection") {
                response.add_header("Connection", "close");
            }

            let response_bytes = response.encode();
            writer.write_all(&response_bytes).await?;
            writer.flush().await?;

            if !should_keep_alive {
                return Ok(());
            }
        }
    }
}

/// TLS 接続を処理
async fn handle_tls_connection<H: Handler>(
    stream: tokio_rustls::server::TlsStream<TcpStream>,
    _peer_addr: SocketAddr,
    config: Arc<ConnectionConfig>,
    handler: Arc<H>,
) -> Result<()> {
    let (reader, writer) = tokio::io::split(stream);
    let mut reader = BufReader::with_capacity(config.read_buffer_size, reader);
    let mut writer = BufWriter::with_capacity(config.write_buffer_size, writer);

    let mut decoder = RequestDecoder::new();
    let mut buf = vec![0u8; config.read_buffer_size];
    let mut request_count = 0u32;

    loop {
        let read_result =
            tokio::time::timeout(config.keep_alive_timeout, reader.read(&mut buf)).await;

        let n = match read_result {
            Ok(Ok(n)) => n,
            Ok(Err(e)) => return Err(Error::Io(e)),
            Err(_) => return Ok(()), // タイムアウト
        };

        if n == 0 {
            return Ok(()); // 接続が閉じられた
        }

        decoder.feed(&buf[..n])?;

        while let Some(request) = decoder.decode()? {
            request_count += 1;

            let should_keep_alive =
                request.is_keep_alive() && request_count < config.max_requests_per_connection;

            let mut response = handler.handle(request).await;

            // Connection ヘッダーを設定
            if !should_keep_alive && !response.has_header("Connection") {
                response.add_header("Connection", "close");
            }

            let response_bytes = response.encode();
            writer.write_all(&response_bytes).await?;
            writer.flush().await?;

            if !should_keep_alive {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_bind() {
        let server = Server::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();
        assert!(addr.port() > 0);
        assert!(!server.is_tls());
    }
}
