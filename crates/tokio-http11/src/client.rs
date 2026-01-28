//! HTTP/HTTPS クライアント
//!
//! tokio と tokio-rustls を使用した非同期 HTTP クライアント。
//!
//! ## 使い方
//!
//! ```ignore
//! use tokio_http11::Client;
//!
//! // GET
//! let client = Client::new();
//! let response = client.get("http://example.com/path").await?;
//!
//! // ヘッダー追加
//! let response = client.get("http://example.com")
//!     .header("User-Agent", "my-app")
//!     .await?;
//!
//! // POST with body
//! let response = client.post("http://example.com/api")
//!     .header("Content-Type", "application/json")
//!     .body(b"{\"key\": \"value\"}")
//!     .await?;
//!
//! // カスタム TLS 設定を使用する場合
//! let client = Client::new().tls_config(custom_tls_config);
//! ```

use std::future::IntoFuture;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use rustls::ClientConfig;
use rustls_pki_types::ServerName;
use shiguredo_http11::uri::{Uri, percent_encode};
use shiguredo_http11::{Request, Response, ResponseDecoder};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::error::{Error, Result};

/// OS のルート証明書ストアを使用するデフォルトの TLS 設定を作成
fn default_tls_config() -> Arc<ClientConfig> {
    Arc::new(
        ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(rustls_platform_verifier::Verifier::new()))
            .with_no_client_auth(),
    )
}

/// HTTP クライアント
///
/// HTTP と HTTPS の両方に対応。HTTPS を使用する場合は `tls_config()` で TLS 設定を指定する。
#[derive(Clone)]
pub struct Client {
    connect_timeout: Duration,
    read_timeout: Duration,
    tls_config: Option<Arc<ClientConfig>>,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// 新しいクライアントを作成
    pub fn new() -> Self {
        Self {
            connect_timeout: Duration::from_secs(30),
            read_timeout: Duration::from_secs(60),
            tls_config: None,
        }
    }

    /// TLS 設定を指定 (HTTPS 用)
    pub fn tls_config(mut self, config: Arc<ClientConfig>) -> Self {
        self.tls_config = Some(config);
        self
    }

    /// 接続タイムアウトを設定
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// 読み取りタイムアウトを設定
    pub fn read_timeout(mut self, timeout: Duration) -> Self {
        self.read_timeout = timeout;
        self
    }

    /// GET リクエストを作成
    pub fn get(&self, url: &str) -> RequestBuilder<'_> {
        self.request("GET", url)
    }

    /// POST リクエストを作成
    pub fn post(&self, url: &str) -> RequestBuilder<'_> {
        self.request("POST", url)
    }

    /// PUT リクエストを作成
    pub fn put(&self, url: &str) -> RequestBuilder<'_> {
        self.request("PUT", url)
    }

    /// DELETE リクエストを作成
    pub fn delete(&self, url: &str) -> RequestBuilder<'_> {
        self.request("DELETE", url)
    }

    /// HEAD リクエストを作成
    pub fn head(&self, url: &str) -> RequestBuilder<'_> {
        self.request("HEAD", url)
    }

    /// PATCH リクエストを作成
    pub fn patch(&self, url: &str) -> RequestBuilder<'_> {
        self.request("PATCH", url)
    }

    /// 任意のメソッドでリクエストを作成
    pub fn request(&self, method: &str, url: &str) -> RequestBuilder<'_> {
        RequestBuilder {
            client: self,
            method: method.to_string(),
            url: url.to_string(),
            headers: Vec::new(),
            body: None,
            query_params: Vec::new(),
        }
    }

    async fn send_request(&self, request: Request, url: &str) -> Result<Response> {
        let (scheme, host, port, _path) = parse_url(url)?;

        let addr = format!("{}:{}", host, port);
        let stream =
            tokio::time::timeout(self.connect_timeout, TcpStream::connect(&addr)).await??;

        let request_bytes = request.encode();

        if scheme == "https" {
            self.send_https(stream, &host, &request_bytes).await
        } else {
            self.send_http(stream, &request_bytes).await
        }
    }

    async fn send_http(&self, mut stream: TcpStream, request_bytes: &[u8]) -> Result<Response> {
        stream.write_all(request_bytes).await?;

        let mut decoder = ResponseDecoder::new();
        let mut buf = [0u8; 8192];

        loop {
            let read_result =
                tokio::time::timeout(self.read_timeout, stream.read(&mut buf)).await?;

            let n = read_result?;
            if n == 0 {
                decoder.mark_eof();
                if let Some(response) = decoder.decode()? {
                    return Ok(response);
                }
                return Err(Error::ConnectionClosed);
            }

            decoder.feed(&buf[..n])?;

            if let Some(response) = decoder.decode()? {
                return Ok(response);
            }
        }
    }

    async fn send_https(
        &self,
        stream: TcpStream,
        host: &str,
        request_bytes: &[u8],
    ) -> Result<Response> {
        let tls_config = self.tls_config.clone().unwrap_or_else(default_tls_config);

        let connector = TlsConnector::from(tls_config);
        let server_name = ServerName::try_from(host.to_string())?;
        let mut tls_stream = connector
            .connect(server_name, stream)
            .await
            .map_err(|e| Error::Tls(e.to_string()))?;

        tls_stream.write_all(request_bytes).await?;

        let mut decoder = ResponseDecoder::new();
        let mut buf = [0u8; 8192];

        loop {
            let read_result =
                tokio::time::timeout(self.read_timeout, tls_stream.read(&mut buf)).await?;

            let n = read_result?;
            if n == 0 {
                decoder.mark_eof();
                if let Some(response) = decoder.decode()? {
                    return Ok(response);
                }
                return Err(Error::ConnectionClosed);
            }

            decoder.feed(&buf[..n])?;

            if let Some(response) = decoder.decode()? {
                return Ok(response);
            }
        }
    }
}

/// リクエストビルダー
///
/// Client のメソッド (get, post など) から取得し、ヘッダーやボディを追加してから
/// `.await` でリクエストを送信する。
pub struct RequestBuilder<'a> {
    client: &'a Client,
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
    query_params: Vec<(String, String)>,
}

impl<'a> RequestBuilder<'a> {
    /// ヘッダーを追加
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    /// クエリパラメータを追加
    ///
    /// 複数回呼び出すと追加される。URL に既存のクエリパラメータがある場合は
    /// それに追加される。
    ///
    /// ```ignore
    /// let response = client.get("https://api.example.com/users")
    ///     .query([("page", "1"), ("limit", "10")])
    ///     .await?;
    /// // -> GET /users?page=1&limit=10
    /// ```
    pub fn query<I, K, V>(mut self, params: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        for (key, value) in params {
            self.query_params
                .push((key.as_ref().to_string(), value.as_ref().to_string()));
        }
        self
    }

    /// ボディを設定
    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// リクエストを送信
    async fn send(self) -> Result<Response> {
        let (_, host, port, path) = parse_url(&self.url)?;
        let path_with_query = build_path_with_query(path, &self.query_params);

        // Host ヘッダーを自動設定
        let host_value = if port == 80 || port == 443 {
            host.clone()
        } else {
            format!("{}:{}", host, port)
        };

        let mut request = Request::new(&self.method, &path_with_query);

        // Host ヘッダーを最初に設定（ユーザーが上書き可能）
        let has_host = self
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("Host"));
        if !has_host {
            request = request.header("Host", &host_value);
        }

        // ユーザー指定のヘッダーを追加
        for (name, value) in &self.headers {
            request = request.header(name, value);
        }

        // ボディを設定
        if let Some(body) = self.body {
            request = request.body(body);
        }

        self.client.send_request(request, &self.url).await
    }
}

impl<'a> IntoFuture for RequestBuilder<'a> {
    type Output = Result<Response>;
    type IntoFuture = Pin<Box<dyn std::future::Future<Output = Self::Output> + Send + 'a>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.send())
    }
}

/// HTTP 接続 (Keep-Alive 対応)
///
/// 同じ接続で複数のリクエストを送信する場合に使用する。
pub struct Connection {
    stream: ConnectionStream,
    read_timeout: Duration,
    host: String,
    port: u16,
}

enum ConnectionStream {
    Plain(TcpStream),
    Tls(Box<tokio_rustls::client::TlsStream<TcpStream>>),
}

impl Connection {
    /// URL から接続を確立
    ///
    /// URL のスキームに応じて HTTP または HTTPS で接続する。
    /// HTTPS の場合は `tls_config` を指定する必要がある。
    pub async fn connect(
        url: &str,
        tls_config: Option<Arc<ClientConfig>>,
        timeout: Duration,
    ) -> Result<Self> {
        let (scheme, host, port, _path) = parse_url(url)?;
        Self::connect_to(&host, port, scheme == "https", tls_config, timeout).await
    }

    /// ホストとポートを指定して接続を確立
    ///
    /// `use_tls` が true の場合は HTTPS で接続する。
    pub async fn connect_to(
        host: &str,
        port: u16,
        use_tls: bool,
        tls_config: Option<Arc<ClientConfig>>,
        timeout: Duration,
    ) -> Result<Self> {
        let addr = format!("{}:{}", host, port);
        let stream = tokio::time::timeout(timeout, TcpStream::connect(&addr)).await??;

        let connection_stream = if use_tls {
            let tls_config = tls_config.unwrap_or_else(default_tls_config);
            let connector = TlsConnector::from(tls_config);
            let server_name = ServerName::try_from(host.to_string())?;
            let tls_stream = connector
                .connect(server_name, stream)
                .await
                .map_err(|e| Error::Tls(e.to_string()))?;
            ConnectionStream::Tls(Box::new(tls_stream))
        } else {
            ConnectionStream::Plain(stream)
        };

        Ok(Self {
            stream: connection_stream,
            read_timeout: Duration::from_secs(60),
            host: host.to_string(),
            port,
        })
    }

    /// 読み取りタイムアウトを設定
    pub fn set_read_timeout(&mut self, timeout: Duration) {
        self.read_timeout = timeout;
    }

    /// GET リクエストを作成
    pub fn get(&mut self, path: &str) -> ConnectionRequestBuilder<'_> {
        self.request("GET", path)
    }

    /// POST リクエストを作成
    pub fn post(&mut self, path: &str) -> ConnectionRequestBuilder<'_> {
        self.request("POST", path)
    }

    /// PUT リクエストを作成
    pub fn put(&mut self, path: &str) -> ConnectionRequestBuilder<'_> {
        self.request("PUT", path)
    }

    /// DELETE リクエストを作成
    pub fn delete(&mut self, path: &str) -> ConnectionRequestBuilder<'_> {
        self.request("DELETE", path)
    }

    /// HEAD リクエストを作成
    pub fn head(&mut self, path: &str) -> ConnectionRequestBuilder<'_> {
        self.request("HEAD", path)
    }

    /// PATCH リクエストを作成
    pub fn patch(&mut self, path: &str) -> ConnectionRequestBuilder<'_> {
        self.request("PATCH", path)
    }

    /// 任意のメソッドでリクエストを作成
    pub fn request(&mut self, method: &str, path: &str) -> ConnectionRequestBuilder<'_> {
        ConnectionRequestBuilder {
            connection: self,
            method: method.to_string(),
            path: path.to_string(),
            headers: Vec::new(),
            body: None,
            query_params: Vec::new(),
        }
    }

    async fn send_request(&mut self, request: Request) -> Result<Response> {
        let request_bytes = request.encode();
        let read_timeout = self.read_timeout;

        match &mut self.stream {
            ConnectionStream::Plain(stream) => {
                stream.write_all(&request_bytes).await?;
                receive_response_plain(stream, read_timeout).await
            }
            ConnectionStream::Tls(stream) => {
                stream.write_all(&request_bytes).await?;
                receive_response_tls(stream, read_timeout).await
            }
        }
    }

    /// TLS 接続かどうかを返す
    pub fn is_tls(&self) -> bool {
        matches!(self.stream, ConnectionStream::Tls(_))
    }

    /// Host ヘッダー用の値を取得
    fn host_header_value(&self) -> String {
        if self.port == 80 || self.port == 443 {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

/// Connection 用リクエストビルダー
pub struct ConnectionRequestBuilder<'a> {
    connection: &'a mut Connection,
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
    query_params: Vec<(String, String)>,
}

impl<'a> ConnectionRequestBuilder<'a> {
    /// ヘッダーを追加
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    /// クエリパラメータを追加
    ///
    /// 複数回呼び出すと追加される。パスに既存のクエリパラメータがある場合は
    /// それに追加される。
    ///
    /// ```ignore
    /// let response = conn.get("/users")
    ///     .query([("page", "1"), ("limit", "10")])
    ///     .await?;
    /// // -> GET /users?page=1&limit=10
    /// ```
    pub fn query<I, K, V>(mut self, params: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        for (key, value) in params {
            self.query_params
                .push((key.as_ref().to_string(), value.as_ref().to_string()));
        }
        self
    }

    /// ボディを設定
    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// リクエストを送信
    async fn send(self) -> Result<Response> {
        let host_value = self.connection.host_header_value();
        let path_with_query = build_path_with_query(self.path, &self.query_params);

        let mut request = Request::new(&self.method, &path_with_query);

        // Host ヘッダーを最初に設定（ユーザーが上書き可能）
        let has_host = self
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("Host"));
        if !has_host {
            request = request.header("Host", &host_value);
        }

        // ユーザー指定のヘッダーを追加
        for (name, value) in &self.headers {
            request = request.header(name, value);
        }

        // ボディを設定
        if let Some(body) = self.body {
            request = request.body(body);
        }

        self.connection.send_request(request).await
    }
}

impl<'a> IntoFuture for ConnectionRequestBuilder<'a> {
    type Output = Result<Response>;
    type IntoFuture = Pin<Box<dyn std::future::Future<Output = Self::Output> + Send + 'a>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.send())
    }
}

async fn receive_response_plain(
    stream: &mut TcpStream,
    read_timeout: Duration,
) -> Result<Response> {
    let mut decoder = ResponseDecoder::new();
    let mut buf = [0u8; 8192];

    loop {
        let read_result = tokio::time::timeout(read_timeout, stream.read(&mut buf)).await?;

        let n = read_result?;
        if n == 0 {
            decoder.mark_eof();
            if let Some(response) = decoder.decode()? {
                return Ok(response);
            }
            return Err(Error::ConnectionClosed);
        }

        decoder.feed(&buf[..n])?;

        if let Some(response) = decoder.decode()? {
            return Ok(response);
        }
    }
}

async fn receive_response_tls(
    stream: &mut tokio_rustls::client::TlsStream<TcpStream>,
    read_timeout: Duration,
) -> Result<Response> {
    let mut decoder = ResponseDecoder::new();
    let mut buf = [0u8; 8192];

    loop {
        let read_result = tokio::time::timeout(read_timeout, stream.read(&mut buf)).await?;

        let n = read_result?;
        if n == 0 {
            decoder.mark_eof();
            if let Some(response) = decoder.decode()? {
                return Ok(response);
            }
            return Err(Error::ConnectionClosed);
        }

        decoder.feed(&buf[..n])?;

        if let Some(response) = decoder.decode()? {
            return Ok(response);
        }
    }
}

/// クエリパラメータを含むパスを構築
fn build_path_with_query(path: String, query_params: &[(String, String)]) -> String {
    if query_params.is_empty() {
        return path;
    }

    let query_string = query_params
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    if path.contains('?') {
        format!("{}&{}", path, query_string)
    } else {
        format!("{}?{}", path, query_string)
    }
}

/// URL をパース
///
/// shiguredo_http11::uri::Uri を使用して URL をパースし、
/// (scheme, host, port, path) のタプルを返す。
pub fn parse_url(url: &str) -> Result<(String, String, u16, String)> {
    let uri = Uri::parse(url).map_err(|e| Error::InvalidUrl(e.to_string()))?;

    let scheme = uri
        .scheme()
        .ok_or_else(|| Error::InvalidUrl("URL must have a scheme".to_string()))?;

    if scheme != "http" && scheme != "https" {
        return Err(Error::InvalidUrl(
            "URL must start with http:// or https://".to_string(),
        ));
    }

    let host = uri
        .host()
        .ok_or_else(|| Error::InvalidUrl("URL must have a host".to_string()))?;

    let port = uri
        .port()
        .unwrap_or(if scheme == "https" { 443 } else { 80 });

    let path = if uri.path().is_empty() {
        "/".to_string()
    } else {
        uri.origin_form()
    };

    Ok((scheme.to_string(), host.to_string(), port, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_url() {
        let (scheme, host, port, path) = parse_url("https://example.com/path").unwrap();
        assert_eq!(scheme, "https");
        assert_eq!(host, "example.com");
        assert_eq!(port, 443);
        assert_eq!(path, "/path");

        let (scheme, host, port, path) = parse_url("http://localhost:8080/api").unwrap();
        assert_eq!(scheme, "http");
        assert_eq!(host, "localhost");
        assert_eq!(port, 8080);
        assert_eq!(path, "/api");

        let (scheme, host, port, path) = parse_url("http://example.com").unwrap();
        assert_eq!(scheme, "http");
        assert_eq!(host, "example.com");
        assert_eq!(port, 80);
        assert_eq!(path, "/");
    }

    #[test]
    fn test_parse_url_invalid() {
        assert!(parse_url("ftp://example.com").is_err());
        assert!(parse_url("example.com").is_err());
    }

    #[test]
    fn test_build_path_with_query_empty() {
        let path = "/users".to_string();
        let params: Vec<(String, String)> = vec![];
        assert_eq!(build_path_with_query(path, &params), "/users");
    }

    #[test]
    fn test_build_path_with_query_single() {
        let path = "/users".to_string();
        let params = vec![("page".to_string(), "1".to_string())];
        assert_eq!(build_path_with_query(path, &params), "/users?page=1");
    }

    #[test]
    fn test_build_path_with_query_multiple() {
        let path = "/users".to_string();
        let params = vec![
            ("page".to_string(), "1".to_string()),
            ("limit".to_string(), "10".to_string()),
        ];
        assert_eq!(
            build_path_with_query(path, &params),
            "/users?page=1&limit=10"
        );
    }

    #[test]
    fn test_build_path_with_query_existing_query() {
        let path = "/search?q=rust".to_string();
        let params = vec![("page".to_string(), "1".to_string())];
        assert_eq!(
            build_path_with_query(path, &params),
            "/search?q=rust&page=1"
        );
    }

    #[test]
    fn test_build_path_with_query_encoding() {
        let path = "/search".to_string();
        let params = vec![
            ("q".to_string(), "hello world".to_string()),
            ("tag".to_string(), "rust&go".to_string()),
        ];
        assert_eq!(
            build_path_with_query(path, &params),
            "/search?q=hello%20world&tag=rust%26go"
        );
    }

    #[test]
    fn test_build_path_with_query_special_chars() {
        let path = "/api".to_string();
        let params = vec![("key".to_string(), "a=b".to_string())];
        assert_eq!(build_path_with_query(path, &params), "/api?key=a%3Db");
    }
}
