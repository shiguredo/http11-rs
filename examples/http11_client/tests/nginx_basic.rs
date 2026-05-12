//! 実 nginx 相手の基本動作 integration test
//!
//! `nginx:1.27-alpine` をデフォルト構成で起動し、`http_request` 経由で以下を検証する:
//!   - GET / で 200 / `Content-Type: text/html` / `Welcome to nginx` を含むボディ
//!   - GET /missing で 404
//!   - HEAD / でステータス 200 / ボディ無し (`BodyKind::None` 経路)
//!   - 任意レスポンスに `Server: nginx/...` ヘッダー
//!   - ステータスラインのバージョンが `HTTP/1.1`

mod helpers;

use http11_client::{http_request, parse_url};
use shiguredo_http11::{HttpHead, Request};

/// nginx に対して 1 リクエスト送って Response を返す共通ヘルパー
///
/// 各テスト関数の重複を減らすため、ヘッダー組み立て + encode + spawn_blocking までを一括する。
async fn fetch(
    nginx: &helpers::NginxHandle,
    method: &str,
    path: &str,
) -> shiguredo_http11::Response {
    let url = nginx.http_url(path);
    let (_scheme, host, port, request_path) = parse_url(&url).expect("URL のパースに失敗");
    let request = Request::new(method, &request_path)
        .expect("Request::new に失敗")
        .header("Host", &host)
        .expect("Host ヘッダーの設定に失敗")
        .header("User-Agent", "http11_client-test")
        .expect("User-Agent ヘッダーの設定に失敗")
        .header("Connection", "close")
        .expect("Connection ヘッダーの設定に失敗");
    let request_method = request.method().to_string();
    let request_bytes = request.encode().expect("encode に失敗");

    tokio::task::spawn_blocking(move || http_request(&host, port, &request_method, &request_bytes))
        .await
        .expect("spawn_blocking タスクが失敗")
        .expect("http_request が失敗")
}

#[tokio::test]
async fn get_root_returns_200_html() {
    helpers::ensure_docker();
    let nginx = helpers::spawn_nginx_default().await;

    let response = fetch(&nginx, "GET", "/").await;

    assert_eq!(response.status_code(), 200);
    let content_type = response.get_header("Content-Type").unwrap_or("");
    assert!(
        content_type.starts_with("text/html"),
        "unexpected Content-Type: {content_type}"
    );
    let body = response.body_bytes().unwrap_or(&[]);
    let body_str = std::str::from_utf8(body).expect("body is UTF-8");
    assert!(
        body_str.contains("Welcome to nginx"),
        "body did not contain expected marker: {body_str}"
    );
}

#[tokio::test]
async fn get_unknown_returns_404() {
    helpers::ensure_docker();
    let nginx = helpers::spawn_nginx_default().await;

    let response = fetch(&nginx, "GET", "/this-path-does-not-exist").await;

    assert_eq!(response.status_code(), 404);
    let body = response.body_bytes().unwrap_or(&[]);
    let body_str = std::str::from_utf8(body).expect("body is UTF-8");
    assert!(
        body_str.contains("404 Not Found"),
        "body did not contain expected marker: {body_str}"
    );
}

#[tokio::test]
async fn head_root_returns_no_body() {
    helpers::ensure_docker();
    let nginx = helpers::spawn_nginx_default().await;

    let response = fetch(&nginx, "HEAD", "/").await;

    assert_eq!(response.status_code(), 200);
    // HEAD は BodyKind::None として扱われるため、body は None
    assert!(
        response.body_bytes().is_none(),
        "HEAD response unexpectedly carried a body"
    );
}

#[tokio::test]
async fn includes_server_header() {
    helpers::ensure_docker();
    let nginx = helpers::spawn_nginx_default().await;

    let response = fetch(&nginx, "GET", "/").await;

    let server = response.get_header("Server").unwrap_or("");
    assert!(
        server.starts_with("nginx"),
        "unexpected Server header: {server}"
    );
}

#[tokio::test]
async fn http_version_is_1_1() {
    helpers::ensure_docker();
    let nginx = helpers::spawn_nginx_default().await;

    let response = fetch(&nginx, "GET", "/").await;

    assert_eq!(HttpHead::version(&response), "HTTP/1.1");
}
