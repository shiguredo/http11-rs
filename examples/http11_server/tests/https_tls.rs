//! HTTPS / TLS の振る舞いを curl と自己署名証明書で検証する

mod helpers;

use helpers::{
    ensure_curl, find_header, generate_self_signed, run_curl, spawn_https_server,
    split_headers_body,
};

/// curl の `--resolve` 引数 `localhost:PORT:127.0.0.1` を組み立てる
fn resolve_arg(port: u16) -> String {
    format!("localhost:{port}:127.0.0.1")
}

#[tokio::test(flavor = "current_thread")]
async fn https_get_root_with_self_signed_cert() {
    ensure_curl();
    let (_dir, cert, key) = generate_self_signed();
    let server = spawn_https_server(&cert, &key).await;

    let out = run_curl([
        "-sS",
        "-i",
        "--cacert",
        cert.to_str().expect("cert path is utf-8"),
        "--resolve",
        &resolve_arg(server.port),
        &server.https_url("/"),
    ])
    .await;
    assert_eq!(out.status, 0, "curl failed: stderr={}", out.stderr);

    let (headers, body) = split_headers_body(&out.stdout);
    let body = String::from_utf8(body).expect("response body is not utf-8");

    assert!(headers.starts_with("HTTP/1.1 200"), "headers={headers}");
    assert!(
        body.contains("<title>shiguredo_http11 Server</title>"),
        "unexpected body: {body}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn https_compression_works() {
    ensure_curl();
    let (_dir, cert, key) = generate_self_signed();
    let server = spawn_https_server(&cert, &key).await;

    let out = run_curl([
        "-sS",
        "-i",
        "--cacert",
        cert.to_str().expect("cert path is utf-8"),
        "--resolve",
        &resolve_arg(server.port),
        "-H",
        "Accept-Encoding: gzip",
        &server.https_url("/"),
    ])
    .await;
    assert_eq!(out.status, 0, "curl failed: stderr={}", out.stderr);

    let (headers, _) = split_headers_body(&out.stdout);
    assert_eq!(find_header(&headers, "Content-Encoding"), Some("gzip"));
}

#[tokio::test(flavor = "current_thread")]
async fn https_keep_alive_works() {
    ensure_curl();
    let (_dir, cert, key) = generate_self_signed();
    let server = spawn_https_server(&cert, &key).await;

    let url = server.https_url("/");
    let cert_str = cert.to_str().expect("cert path is utf-8").to_string();
    let resolve = resolve_arg(server.port);
    let out = run_curl([
        "-sS",
        "-v",
        "--cacert",
        &cert_str,
        "--resolve",
        &resolve,
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}\n",
        &url,
        "--next",
        "--cacert",
        &cert_str,
        "--resolve",
        &resolve,
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
        "expected curl to re-use the TLS connection: stderr={}",
        out.stderr
    );
}

#[tokio::test(flavor = "current_thread")]
async fn tls_handshake_with_invalid_ca_fails() {
    ensure_curl();
    let (_dir, cert, key) = generate_self_signed();
    let server = spawn_https_server(&cert, &key).await;

    // --cacert を渡さないと自己署名証明書は信頼されず、curl は exit code 非 0 で失敗する。
    let out = run_curl([
        "-sS",
        "--resolve",
        &resolve_arg(server.port),
        &server.https_url("/"),
    ])
    .await;
    assert_ne!(
        out.status, 0,
        "curl must fail with self-signed cert without --cacert: stderr={}",
        out.stderr
    );
    assert!(
        out.stderr.to_lowercase().contains("certificate")
            || out.stderr.to_lowercase().contains("ssl"),
        "expected TLS-related error in stderr: {}",
        out.stderr
    );
}
