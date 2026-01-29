# tokio-http11

tokio と tokio-rustls を使用した非同期 HTTP/1.1 クライアント/サーバーライブラリ。

## 概要

`tokio_http11` は [shiguredo_http11](https://github.com/shiguredo/http11-rs) の Sans I/O API をベースに、tokio による非同期 I/O と tokio-rustls による TLS 対応を提供します。

## 特徴

- **shiguredo_http11 ベース**: Sans I/O ライブラリをベースにした設計
- **非同期 I/O**: tokio による完全非同期対応
- **TLS 対応**: tokio-rustls による HTTPS 対応
- **Keep-Alive**: HTTP/1.1 Keep-Alive 接続のサポート
- **HTTP/HTTPS 透過**: 同じ API で HTTP と HTTPS を扱える
- **OS 証明書ストア**: rustls-platform-verifier による OS のルート証明書自動使用
- **Host ヘッダー自動設定**: URL から Host ヘッダーを自動設定

## 依存関係

```toml
[dependencies]
tokio_http11 = "2026.0"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

## 使い方

### クライアント (HTTP)

```rust
use tokio_http11::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();

    // GET リクエスト (Host ヘッダーは自動設定)
    let response = client.get("http://httpbin.org/get").await?;
    println!("Status: {} {}", response.status_code, response.reason_phrase);

    // ヘッダーを追加
    let response = client.get("http://httpbin.org/get")
        .header("User-Agent", "tokio_http11")
        .await?;

    Ok(())
}
```

### クライアント (HTTPS)

HTTPS は OS のルート証明書ストアを自動的に使用します。

```rust
use tokio_http11::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();

    // HTTPS でも同じ API で使える
    let response = client.get("https://example.com").await?;
    println!("Status: {} {}", response.status_code, response.reason_phrase);

    Ok(())
}
```

### POST リクエスト

```rust
use tokio_http11::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();

    let response = client.post("https://httpbin.org/post")
        .header("Content-Type", "application/json")
        .body(b"{\"key\": \"value\"}")
        .await?;

    println!("Status: {} {}", response.status_code, response.reason_phrase);
    Ok(())
}
```

### カスタム TLS 設定

カスタムの TLS 設定を使用する場合は `tls_config()` を使用します。

```rust
use std::sync::Arc;
use tokio_http11::Client;
use rustls::ClientConfig;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // カスタム TLS 設定
    let tls_config = Arc::new(
        ClientConfig::builder()
            .with_root_certificates(custom_root_store)
            .with_no_client_auth()
    );

    let client = Client::new().tls_config(tls_config);
    let response = client.get("https://example.com").await?;

    Ok(())
}
```

### Keep-Alive 接続

```rust
use std::time::Duration;
use tokio_http11::Connection;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // URL から接続 (スキームに応じて HTTP/HTTPS を自動判別)
    // tls_config に None を渡すと OS のルート証明書を使用
    let mut conn = Connection::connect(
        "https://example.com",
        None,
        Duration::from_secs(30),
    ).await?;

    // 同じ接続で複数のリクエストを送信 (Host ヘッダーは自動設定)
    for path in ["/", "/about", "/contact"] {
        let response = conn.get(path).await?;
        println!("{}: {}", path, response.status_code);
    }

    // ヘッダーやボディを追加する場合
    let response = conn.post("/api")
        .header("Content-Type", "application/json")
        .body(b"{\"key\": \"value\"}")
        .await?;

    Ok(())
}
```

### サーバー (HTTP)

```rust
use tokio_http11::{Server, Request, Response};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    async fn handler(request: Request) -> Response {
        match request.uri.as_str() {
            "/" => Response::new(200, "OK")
                .header("Content-Type", "text/plain")
                .body(b"Hello, World!".to_vec()),
            _ => Response::new(404, "Not Found")
                .header("Content-Type", "text/plain")
                .body(b"Not Found".to_vec()),
        }
    }

    let server = Server::bind("0.0.0.0:8080").await?;
    println!("HTTP server listening on http://0.0.0.0:8080");
    server.serve(handler).await?;

    Ok(())
}
```

### サーバー (HTTPS)

```rust
use std::sync::Arc;
use tokio_http11::{Server, Request, Response};
use rustls::ServerConfig;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 証明書と秘密鍵を読み込み
    let certs: Vec<CertificateDer<'static>> =
        CertificateDer::pem_file_iter("cert.pem")?
            .collect::<Result<Vec<_>, _>>()?;
    let key = PrivateKeyDer::from_pem_file("key.pem")?;

    // TLS 設定
    let tls_config = Arc::new(
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)?
    );

    async fn handler(request: Request) -> Response {
        Response::new(200, "OK")
            .header("Content-Type", "text/plain")
            .body(b"Hello, TLS!".to_vec())
    }

    // tls() で HTTPS を有効化
    let server = Server::bind("0.0.0.0:8443").await?.tls(tls_config);
    println!("HTTPS server listening on https://0.0.0.0:8443");
    server.serve(handler).await?;

    Ok(())
}
```

## API

### Client

HTTP/HTTPS クライアント。

```rust
let client = Client::new()
    .tls_config(tls_config)      // カスタム TLS 設定 (省略時は OS 証明書ストアを使用)
    .connect_timeout(Duration::from_secs(30))
    .read_timeout(Duration::from_secs(60));

// HTTP メソッド
let response = client.get("https://example.com").await?;
let response = client.post("https://example.com/api").await?;
let response = client.put("https://example.com/api").await?;
let response = client.delete("https://example.com/api").await?;
let response = client.head("https://example.com").await?;
let response = client.patch("https://example.com/api").await?;
let response = client.request("OPTIONS", "https://example.com").await?;

// ヘッダーとボディを追加
let response = client.post("https://example.com/api")
    .header("Content-Type", "application/json")
    .body(b"{\"key\": \"value\"}")
    .await?;
```

### Connection

Keep-Alive 対応の接続。

```rust
// URL から接続 (スキーム自動判別)
let mut conn = Connection::connect(url, tls_config, timeout).await?;

// ホスト/ポート指定で接続
let mut conn = Connection::connect_to(host, port, use_tls, tls_config, timeout).await?;

// HTTP メソッド
let response = conn.get("/path").await?;
let response = conn.post("/api").await?;

// ヘッダーとボディを追加
let response = conn.post("/api")
    .header("Content-Type", "application/json")
    .body(b"{\"key\": \"value\"}")
    .await?;
```

### Server

HTTP/HTTPS サーバー。

```rust
let server = Server::bind("0.0.0.0:8080").await?
    .tls(tls_config)                        // HTTPS 用 TLS 設定
    .keep_alive_timeout(Duration::from_secs(60))
    .max_requests_per_connection(1000);

server.serve(handler).await?;
```

### Handler

リクエストハンドラー。関数やクロージャから自動実装される。

```rust
async fn handler(request: Request) -> Response {
    Response::new(200, "OK").body(b"Hello".to_vec())
}
```

## ResponseExt

`ResponseExt` トレイトで Response に便利なメソッドを追加。

```rust
use tokio_http11::{Client, ResponseExt};

let client = Client::new();
let response = client.get("https://example.com").await?;

// テキストとして取得
let text = response.text()?;

// バイト列として取得
let bytes = response.bytes();
```

### JSON パース (json feature)

`json` feature を有効にすると `response.json()` が使用可能。

```toml
tokio_http11 = { version = "2026.0", features = ["json"] }
```

```rust
use tokio_http11::{Client, ResponseExt};

// TryFrom<RawJsonValue> を実装した型に変換
struct User {
    name: String,
    age: u32,
}

impl<'text, 'raw> TryFrom<nojson::RawJsonValue<'text, 'raw>> for User {
    type Error = nojson::JsonParseError;

    fn try_from(value: nojson::RawJsonValue<'text, 'raw>) -> Result<Self, Self::Error> {
        Ok(User {
            name: value.to_member("name")?.required()?.try_into()?,
            age: value.to_member("age")?.required()?.try_into()?,
        })
    }
}

let client = Client::new();
let response = client.get("https://api.example.com/user").await?;
let user: User = response.json()?;
```

## Features

- `client` - HTTP/HTTPS クライアント機能 (デフォルト有効)
- `server` - HTTP/HTTPS サーバー機能 (デフォルト有効)
- `json` - JSON パース機能 (`response.json()`)
- `full` - すべての機能 (client + server + json)

```toml
# すべての機能を使用
tokio_http11 = { version = "2026.0", features = ["full"] }

# client のみ使用
tokio_http11 = { version = "2026.0", default-features = false, features = ["client"] }

# server のみ使用
tokio_http11 = { version = "2026.0", default-features = false, features = ["server"] }

# json 機能を追加
tokio_http11 = { version = "2026.0", features = ["json"] }
```

## ライセンス

Apache-2.0
