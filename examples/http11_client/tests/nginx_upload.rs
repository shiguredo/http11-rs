//! 実 nginx 相手の大容量アップロード integration test
//!
//! `nginx:1.27-alpine` を WebDAV (PUT) 受付構成で起動し、10 MiB バイナリのラウンドトリップを検証する:
//!   - PUT /upload/10mb.bin で 10 MiB のバイナリをアップロード → 201 Created
//!   - GET /upload/10mb.bin で同一バイトを取得できることを検証 (ラウンドトリップ完全性)
//!   - POST 10 MiB を `return 200` エンドポイントに送信し、nginx が全ボディを受理することを検証

mod helpers;

use http11_client::{http_request, parse_url};
use shiguredo_http11::Request;

/// WebDAV PUT + 静的ファイル配信を有効にした nginx 設定
///
/// - `client_max_body_size 16m`: 10 MiB のアップロードを許可
/// - `dav_methods PUT`: PUT によるファイル書き込みを許可
/// - `create_full_put_path on`: 親ディレクトリが無い場合に自動作成
/// - `/tmp/upload/` を使用 (nginx ワーカーが書き込み可能)
/// - GET は `alias` 経由で同一ディレクトリから取得可能
const CONF_WEBDAV: &str = r#"
server {
    listen 80;
    server_name _;
    client_max_body_size 16m;

    location /upload/ {
        alias /tmp/upload/;
        dav_methods PUT;
        create_full_put_path on;
        dav_access user:rw group:rw all:r;
    }
}
"#;

/// POST を受理して 200 を返すだけの nginx 設定
///
/// ボディの内容は破棄されるが、nginx はレスポンスを返す前に全ボディを読み切る。
/// `client_max_body_size 16m` で 10 MiB の受理を保証する。
const CONF_POST_SINK: &str = r#"
server {
    listen 80;
    server_name _;
    client_max_body_size 16m;

    location /sink {
        return 200 "accepted\n";
    }
}
"#;

/// 10 MiB の決定論的バイナリデータを生成する
///
/// 各バイトはオフセットを 251 (素数) で剰余した値。パターン検証が容易で、
/// 全バイト値 (0x00-0xFA) が出現するため圧縮耐性テストとしても機能する。
fn generate_10mb_binary() -> Vec<u8> {
    const SIZE: usize = 10 * 1024 * 1024;
    (0..SIZE).map(|i| (i % 251) as u8).collect()
}

/// WebDAV PUT で 10 MiB のバイナリをアップロードし、GET で取得して完全一致を検証する
#[tokio::test]
async fn put_10mb_binary_roundtrip() {
    helpers::ensure_docker();
    let nginx = helpers::spawn_nginx_with_files(CONF_WEBDAV, &[]).await;
    let body = generate_10mb_binary();

    // PUT アップロード (location /upload/ に対応する URI)
    let url = nginx.http_url("/upload/10mb.bin");
    let (_scheme, host, port, path) = parse_url(&url).expect("URL のパースに失敗");

    let put_request = Request::new("PUT", &path)
        .expect("Request::new に失敗")
        .header("Host", &host)
        .expect("Host ヘッダーの設定に失敗")
        .header("Content-Type", "application/octet-stream")
        .expect("Content-Type ヘッダーの設定に失敗")
        .header("Connection", "close")
        .expect("Connection ヘッダーの設定に失敗")
        .body(body.clone());

    let put_method = put_request.method().to_string();
    let put_bytes = put_request.encode().expect("encode に失敗");

    let host_put = host.clone();
    let put_response =
        tokio::task::spawn_blocking(move || http_request(&host_put, port, &put_method, &put_bytes))
            .await
            .expect("spawn_blocking タスクが失敗")
            .expect("PUT リクエストが失敗");

    assert!(
        put_response.status_code() == 201 || put_response.status_code() == 204,
        "PUT レスポンスが 201/204 であるべき: 実際は {}",
        put_response.status_code()
    );

    // GET で取得して検証
    let get_request = Request::new("GET", &path)
        .expect("Request::new に失敗")
        .header("Host", &host)
        .expect("Host ヘッダーの設定に失敗")
        .header("Connection", "close")
        .expect("Connection ヘッダーの設定に失敗");

    let get_method = get_request.method().to_string();
    let get_bytes = get_request.encode().expect("encode に失敗");

    let host_get = host.clone();
    let get_response =
        tokio::task::spawn_blocking(move || http_request(&host_get, port, &get_method, &get_bytes))
            .await
            .expect("spawn_blocking タスクが失敗")
            .expect("GET リクエストが失敗");

    assert_eq!(get_response.status_code(), 200);
    let received = get_response
        .body_bytes()
        .expect("ボディバイトを取得できるべき");
    assert_eq!(
        received.len(),
        body.len(),
        "受信ボディ長の不一致: 取得 {} 期待 {}",
        received.len(),
        body.len()
    );
    assert!(
        received == body.as_slice(),
        "ラウンドトリップでバイト列が一致しない"
    );
}

/// POST で 10 MiB のバイナリを送信し、nginx が正常に受理することを検証する
///
/// nginx はレスポンスを返す前にリクエストボディを全て読み切る (RFC 9112 Section 6 準拠)。
/// 200 が返れば全ボディが転送完了したことの確認となる。
#[tokio::test]
async fn post_10mb_binary_accepted() {
    helpers::ensure_docker();
    let nginx = helpers::spawn_nginx_with_files(CONF_POST_SINK, &[]).await;
    let body = generate_10mb_binary();

    let url = nginx.http_url("/sink");
    let (_scheme, host, port, path) = parse_url(&url).expect("URL のパースに失敗");

    let request = Request::new("POST", &path)
        .expect("Request::new に失敗")
        .header("Host", &host)
        .expect("Host ヘッダーの設定に失敗")
        .header("Content-Type", "application/octet-stream")
        .expect("Content-Type ヘッダーの設定に失敗")
        .header("Connection", "close")
        .expect("Connection ヘッダーの設定に失敗")
        .body(body);

    let method = request.method().to_string();
    let request_bytes = request.encode().expect("encode に失敗");

    // Content-Length が正しく自動付与されていることを確認 (10 MiB = 10485760)
    let header_end = request_bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("ヘッダー終端 (CRLFCRLF) が存在するべき");
    let header_str =
        std::str::from_utf8(&request_bytes[..header_end]).expect("ヘッダー部分は ASCII");
    assert!(
        header_str.contains("Content-Length: 10485760"),
        "Content-Length が 10485760 であるべき: ヘッダー = {header_str:?}"
    );

    let response =
        tokio::task::spawn_blocking(move || http_request(&host, port, &method, &request_bytes))
            .await
            .expect("spawn_blocking タスクが失敗")
            .expect("POST リクエストが失敗");

    assert_eq!(
        response.status_code(),
        200,
        "nginx が 10 MiB POST を受理して 200 を返すべき (実際: {})",
        response.status_code()
    );
    let resp_body = response
        .body_bytes()
        .expect("レスポンスボディを取得できるべき");
    assert_eq!(resp_body, b"accepted\n");
}

/// Content-Length を超える body_size が 413 で拒否されることを検証する
///
/// `client_max_body_size 1m` に設定した nginx に対して 10 MiB を送ると
/// 413 Request Entity Too Large が返る。エラーパスの確認。
#[tokio::test]
async fn post_10mb_rejected_by_size_limit() {
    helpers::ensure_docker();
    let conf = r#"
server {
    listen 80;
    server_name _;
    client_max_body_size 1m;

    location /sink {
        return 200 "accepted\n";
    }
}
"#;
    let nginx = helpers::spawn_nginx_with_files(conf, &[]).await;
    let body = generate_10mb_binary();

    let url = nginx.http_url("/sink");
    let (_scheme, host, port, path) = parse_url(&url).expect("URL のパースに失敗");

    let request = Request::new("POST", &path)
        .expect("Request::new に失敗")
        .header("Host", &host)
        .expect("Host ヘッダーの設定に失敗")
        .header("Content-Type", "application/octet-stream")
        .expect("Content-Type ヘッダーの設定に失敗")
        .header("Connection", "close")
        .expect("Connection ヘッダーの設定に失敗")
        .body(body);

    let method = request.method().to_string();
    let request_bytes = request.encode().expect("encode に失敗");

    let response =
        tokio::task::spawn_blocking(move || http_request(&host, port, &method, &request_bytes))
            .await
            .expect("spawn_blocking タスクが失敗")
            .expect("POST リクエスト自体は送信できるべき");

    assert_eq!(
        response.status_code(),
        413,
        "client_max_body_size 超過で 413 が返るべき (実際: {})",
        response.status_code()
    );
}
