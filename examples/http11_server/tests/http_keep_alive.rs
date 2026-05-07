//! Keep-Alive と Connection: close の挙動を curl で検証する
//!
//! Windows のシステム curl は `-o /dev/null` 等の Unix 慣習が動かないため、
//! テスト全体を `#[cfg(not(windows))]` で除外する。

#![cfg(not(windows))]

mod helpers;

use helpers::{ensure_curl, find_header, run_curl, spawn_http_server, split_headers_body};

#[tokio::test(flavor = "current_thread")]
async fn keep_alive_two_requests_one_connection() {
    ensure_curl();
    let server = spawn_http_server().await;

    // `--next` で 1 つの curl 実行内で 2 リクエストを送る。
    // libcurl は HTTP/1.1 のデフォルト keep-alive で接続を再利用する。
    // -v の stderr に "Re-using existing connection" が出れば再利用成功。
    let url = server.http_url("/");
    let out = run_curl([
        "-sS",
        "-v",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}\n",
        &url,
        "--next",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}\n",
        &url,
    ])
    .await;
    assert_eq!(out.status, 0, "curl failed: stderr={}", out.stderr);

    let stdout = out.stdout_string();
    let codes: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        codes,
        ["200", "200"],
        "both requests must return 200: stdout={stdout}"
    );
    assert!(
        out.stderr.contains("Re-using existing connection"),
        "expected curl to re-use the connection: stderr={}",
        out.stderr
    );
}

#[tokio::test(flavor = "current_thread")]
async fn connection_close_header_terminates() {
    ensure_curl();
    let server = spawn_http_server().await;

    let out = run_curl([
        "-sS",
        "-i",
        "-H",
        "Connection: close",
        &server.http_url("/"),
    ])
    .await;
    assert_eq!(out.status, 0, "curl failed: stderr={}", out.stderr);

    let (headers, _body) = split_headers_body(&out.stdout);
    assert_eq!(
        find_header(&headers, "Connection"),
        Some("close"),
        "headers={headers}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn keep_alive_default_no_close_header() {
    ensure_curl();
    let server = spawn_http_server().await;

    // HTTP/1.1 のデフォルトは keep-alive。Connection: close ヘッダーは無い
    let out = run_curl(["-sS", "-i", &server.http_url("/")]).await;
    assert_eq!(out.status, 0, "curl failed: stderr={}", out.stderr);

    let (headers, _body) = split_headers_body(&out.stdout);
    assert_eq!(
        find_header(&headers, "Connection"),
        None,
        "Connection header must be absent on default keep-alive: headers={headers}"
    );
}
