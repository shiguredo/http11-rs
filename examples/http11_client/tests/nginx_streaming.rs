//! 実 nginx 相手のストリーミング系 integration test
//!
//! `BodyKind::Chunked` / `BodyKind::Length` (大きなボディ) / close 終端の各経路を網羅する:
//!   - `Transfer-Encoding: chunked` 受信 + gzip 展開 (chunked decoder + decompressor)
//!   - 1 MiB のボディを Content-Length 経由で完全受信する (BodyKind::Length)
//!   - `keepalive_timeout 0` で nginx が `Connection: close` を返す (close 終端の挙動確認)

mod helpers;

use http11_client::decompressor::decompress_body;
use http11_client::{http_request, parse_url};
use shiguredo_http11::Request;

/// gzip 圧縮を強制してレスポンスを `Transfer-Encoding: chunked` で返させる nginx 設定
///
/// `gzip on; gzip_min_length 0;` で短文も圧縮対象にし、`gzip_types` を text/plain に絞る。
/// nginx は gzip filter 経路に入ると Content-Length を破棄し chunked transfer に切り替えるため、
/// chunked decoder のテスト経路として機能する。
const CONF_GZIP: &str = r#"
server {
    listen 80;
    server_name _;
    gzip on;
    gzip_min_length 0;
    gzip_types text/plain;
    location / {
        root /usr/share/nginx/html;
    }
}
"#;

/// keepalive を完全に無効化して全レスポンスに `Connection: close` を付けさせる nginx 設定
const CONF_CLOSE: &str = r#"
server {
    listen 80;
    server_name _;
    keepalive_timeout 0;
    location / {
        root /usr/share/nginx/html;
    }
}
"#;

/// デフォルト相当の nginx 設定 (静的ファイルを `/usr/share/nginx/html/` から配信する)
const CONF_STATIC: &str = r#"
server {
    listen 80;
    server_name _;
    location / {
        root /usr/share/nginx/html;
    }
}
"#;

/// 任意のヘッダーを乗せて 1 リクエスト送り、Response を返す
async fn fetch_with_headers(
    nginx: &helpers::NginxHandle,
    method: &str,
    path: &str,
    extra_headers: &[(&str, &str)],
) -> shiguredo_http11::Response {
    let url = nginx.http_url(path);
    let (_scheme, host, port, request_path) = parse_url(&url).expect("parse_url");
    let mut request = Request::new(method, &request_path)
        .expect("Request::new")
        .header("Host", &host)
        .expect("Host header")
        .header("User-Agent", "http11_client-test")
        .expect("User-Agent header");
    for (name, value) in extra_headers {
        request = request.header(name, value).expect("extra header");
    }
    let request_method = request.method().to_string();
    let request_bytes = request.try_encode().expect("try_encode");

    tokio::task::spawn_blocking(move || http_request(&host, port, &request_method, &request_bytes))
        .await
        .expect("spawn_blocking task")
        .expect("http_request")
}

#[tokio::test]
async fn chunked_response_decoded_properly() {
    helpers::ensure_docker();
    // gzip filter を確実に通すために十分なサイズのテキストを用意する
    let body_text = "lorem ipsum dolor sit amet ".repeat(512);
    let nginx = helpers::spawn_nginx_with_files(
        CONF_GZIP,
        &[(
            "/usr/share/nginx/html/chunked.txt",
            body_text.as_bytes().to_vec(),
        )],
    )
    .await;

    let response = fetch_with_headers(
        &nginx,
        "GET",
        "/chunked.txt",
        &[("Accept-Encoding", "gzip"), ("Connection", "close")],
    )
    .await;

    assert_eq!(response.status_code(), 200);
    assert_eq!(
        response.get_header("Content-Encoding"),
        Some("gzip"),
        "Content-Encoding must be gzip"
    );
    assert_eq!(
        response.get_header("Transfer-Encoding"),
        Some("chunked"),
        "Transfer-Encoding must be chunked (gzip filter discards Content-Length)"
    );

    // gzip 展開して元の本文と一致することを確認する
    let raw_body = response.body_bytes().expect("body bytes present");
    let decompressed = decompress_body(raw_body, "gzip").expect("gzip decompression succeeds");
    let recovered = String::from_utf8(decompressed).expect("decompressed body is UTF-8");
    assert_eq!(recovered, body_text);
}

#[tokio::test]
async fn large_body_received_completely() {
    helpers::ensure_docker();
    // 1 MiB の決定論的バイト列 (パターン検証可能)
    let body: Vec<u8> = (0..1024 * 1024).map(|i| (i % 251) as u8).collect();
    let nginx = helpers::spawn_nginx_with_files(
        CONF_STATIC,
        &[("/usr/share/nginx/html/large.bin", body.clone())],
    )
    .await;

    let response =
        fetch_with_headers(&nginx, "GET", "/large.bin", &[("Connection", "close")]).await;

    assert_eq!(response.status_code(), 200);
    let received = response.body_bytes().expect("body bytes present");
    assert_eq!(
        received.len(),
        body.len(),
        "received body length mismatch: got {} expected {}",
        received.len(),
        body.len()
    );
    assert!(
        received == body.as_slice(),
        "received body content mismatch"
    );
}

#[tokio::test]
async fn connection_close_terminates_request() {
    helpers::ensure_docker();
    let nginx = helpers::spawn_nginx_with_files(
        CONF_CLOSE,
        &[(
            "/usr/share/nginx/html/index.html",
            b"<html><body>hello</body></html>".to_vec(),
        )],
    )
    .await;

    // リクエスト側ではあえて keep-alive を要求する。サーバー側 keepalive_timeout 0 で
    // close を強制されることを検証するため。
    let response = fetch_with_headers(
        &nginx,
        "GET",
        "/index.html",
        &[("Connection", "keep-alive")],
    )
    .await;

    assert_eq!(response.status_code(), 200);
    assert_eq!(
        response.get_header("Connection"),
        Some("close"),
        "server must override to Connection: close due to keepalive_timeout 0"
    );
    let body = response.body_bytes().expect("body bytes present");
    assert_eq!(body, b"<html><body>hello</body></html>");
}
