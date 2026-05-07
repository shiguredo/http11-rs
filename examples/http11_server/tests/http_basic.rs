//! HTTP の基本動作 (GET / HEAD / POST / 404) を curl で検証する

mod helpers;

use helpers::{ensure_curl, find_header, run_curl, spawn_http_server, split_headers_body};

#[tokio::test(flavor = "current_thread")]
async fn get_root_returns_200_html() {
    ensure_curl();
    let server = spawn_http_server().await;

    let out = run_curl(["-sS", "-i", &server.http_url("/")]).await;
    assert_eq!(out.status, 0, "curl failed: stderr={}", out.stderr);

    let (headers, body) = split_headers_body(&out.stdout);
    let body = String::from_utf8(body).expect("response body is not utf-8");

    assert!(headers.starts_with("HTTP/1.1 200"), "headers={headers}");
    assert_eq!(
        find_header(&headers, "Content-Type"),
        Some("text/html; charset=utf-8")
    );
    assert!(
        body.contains("<title>shiguredo_http11 Server</title>"),
        "unexpected body: {body}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn get_info_returns_200_json() {
    ensure_curl();
    let server = spawn_http_server().await;

    let out = run_curl(["-sS", "-i", &server.http_url("/info")]).await;
    assert_eq!(out.status, 0, "curl failed: stderr={}", out.stderr);

    let (headers, body) = split_headers_body(&out.stdout);
    let body = String::from_utf8(body).expect("response body is not utf-8");

    assert!(headers.starts_with("HTTP/1.1 200"), "headers={headers}");
    assert_eq!(
        find_header(&headers, "Content-Type"),
        Some("application/json")
    );
    assert!(
        body.contains("\"server\":\"shiguredo_http11\""),
        "unexpected body: {body}"
    );
    assert!(body.contains("\"timestamp\":"), "unexpected body: {body}");
}

#[tokio::test(flavor = "current_thread")]
async fn get_echo_includes_request_headers() {
    ensure_curl();
    let server = spawn_http_server().await;

    let out = run_curl([
        "-sS",
        "-H",
        "X-Test-Header: hello-from-curl",
        &server.http_url("/echo"),
    ])
    .await;
    assert_eq!(out.status, 0, "curl failed: stderr={}", out.stderr);

    let body = out.stdout_string();
    assert!(body.contains("Method: GET"), "unexpected body: {body}");
    assert!(body.contains("URI: /echo"), "unexpected body: {body}");
    assert!(
        body.contains("Version: HTTP/1.1"),
        "unexpected body: {body}"
    );
    assert!(
        body.contains("X-Test-Header: hello-from-curl"),
        "unexpected body: {body}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn head_root_returns_no_body() {
    ensure_curl();
    let server = spawn_http_server().await;

    let out = run_curl([
        "-sS",
        "-I",
        "-w",
        "%{http_code}\n%{size_download}",
        "-o",
        "/dev/null",
        &server.http_url("/"),
    ])
    .await;
    assert_eq!(out.status, 0, "curl failed: stderr={}", out.stderr);

    let stdout = out.stdout_string();
    let mut lines = stdout.lines();
    let code = lines.next().unwrap_or("");
    let size = lines.next().unwrap_or("");
    assert_eq!(code, "200", "status code mismatch: stdout={stdout}");
    assert_eq!(
        size, "0",
        "size_download must be 0 for HEAD: stdout={stdout}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn post_echo_returns_request_body() {
    ensure_curl();
    let server = spawn_http_server().await;

    let out = run_curl([
        "-sS",
        "-X",
        "POST",
        "-d",
        "hello-body",
        &server.http_url("/echo"),
    ])
    .await;
    assert_eq!(out.status, 0, "curl failed: stderr={}", out.stderr);

    let body = out.stdout_string();
    assert!(body.contains("Method: POST"), "unexpected body: {body}");
    assert!(body.contains("Body (10 bytes):"), "unexpected body: {body}");
    assert!(body.contains("hello-body"), "unexpected body: {body}");
}

#[tokio::test(flavor = "current_thread")]
async fn get_unknown_returns_404() {
    ensure_curl();
    let server = spawn_http_server().await;

    let out = run_curl(["-sS", "-i", &server.http_url("/missing-path")]).await;
    assert_eq!(out.status, 0, "curl failed: stderr={}", out.stderr);

    let (headers, body) = split_headers_body(&out.stdout);
    let body = String::from_utf8(body).expect("response body is not utf-8");

    assert!(headers.starts_with("HTTP/1.1 404"), "headers={headers}");
    assert!(body.contains("404 Not Found"), "unexpected body: {body}");
}

#[tokio::test(flavor = "current_thread")]
async fn server_emits_date_and_server_headers() {
    ensure_curl();
    let server = spawn_http_server().await;

    let out = run_curl(["-sS", "-i", &server.http_url("/")]).await;
    assert_eq!(out.status, 0, "curl failed: stderr={}", out.stderr);

    let (headers, _body) = split_headers_body(&out.stdout);
    assert!(
        find_header(&headers, "Date").is_some(),
        "Date header missing: {headers}"
    );
    assert_eq!(
        find_header(&headers, "Server"),
        Some("shiguredo_http11/0.1.0")
    );
}
