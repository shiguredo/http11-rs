//! Accept-Encoding に基づく自動圧縮 (gzip / br / zstd) の振る舞いを curl で検証する
//!
//! 圧縮対象は `/` の HTML レスポンス (圧縮で確実に元より小さくなるサイズ)。
//!
//! Windows のシステム curl は brotli / zstd 自動展開非対応かつ Unix 慣習の
//! 引数が動かないため、テスト全体を `#[cfg(not(windows))]` で除外する。

#![cfg(not(windows))]

mod helpers;

use helpers::{ensure_curl, find_header, run_curl, spawn_http_server, split_headers_body};

async fn fetch_with_accept_encoding(
    server_url: &str,
    accept_encoding: Option<&str>,
) -> (String, Vec<u8>) {
    let mut args: Vec<String> = vec!["-sS".into(), "-i".into()];
    if let Some(ae) = accept_encoding {
        args.push("-H".into());
        args.push(format!("Accept-Encoding: {ae}"));
    }
    args.push(server_url.to_string());
    let out = run_curl(&args).await;
    assert_eq!(out.status, 0, "curl failed: stderr={}", out.stderr);
    split_headers_body(&out.stdout)
}

#[tokio::test(flavor = "current_thread")]
async fn gzip_only_returns_gzip() {
    ensure_curl();
    let server = spawn_http_server().await;

    let (headers, _) = fetch_with_accept_encoding(&server.http_url("/"), Some("gzip")).await;
    assert_eq!(find_header(&headers, "Content-Encoding"), Some("gzip"));
}

#[tokio::test(flavor = "current_thread")]
async fn br_only_returns_br() {
    ensure_curl();
    let server = spawn_http_server().await;

    let (headers, _) = fetch_with_accept_encoding(&server.http_url("/"), Some("br")).await;
    assert_eq!(find_header(&headers, "Content-Encoding"), Some("br"));
}

#[tokio::test(flavor = "current_thread")]
async fn zstd_only_returns_zstd() {
    ensure_curl();
    let server = spawn_http_server().await;

    let (headers, _) = fetch_with_accept_encoding(&server.http_url("/"), Some("zstd")).await;
    assert_eq!(find_header(&headers, "Content-Encoding"), Some("zstd"));
}

#[tokio::test(flavor = "current_thread")]
async fn prefers_zstd_over_br_over_gzip() {
    ensure_curl();
    let server = spawn_http_server().await;

    let (headers, _) =
        fetch_with_accept_encoding(&server.http_url("/"), Some("gzip, br, zstd")).await;
    assert_eq!(find_header(&headers, "Content-Encoding"), Some("zstd"));
}

#[tokio::test(flavor = "current_thread")]
async fn quality_zero_excludes_encoding() {
    ensure_curl();
    let server = spawn_http_server().await;

    // gzip;q=0 で gzip を除外、br のみ候補に残る
    let (headers, _) =
        fetch_with_accept_encoding(&server.http_url("/"), Some("gzip;q=0, br")).await;
    assert_eq!(find_header(&headers, "Content-Encoding"), Some("br"));
}

#[tokio::test(flavor = "current_thread")]
async fn no_accept_encoding_no_content_encoding() {
    ensure_curl();
    let server = spawn_http_server().await;

    let (headers, _) = fetch_with_accept_encoding(&server.http_url("/"), None).await;
    assert_eq!(find_header(&headers, "Content-Encoding"), None);
}

#[tokio::test(flavor = "current_thread")]
async fn compressed_gzip_decoded_body_matches_uncompressed() {
    ensure_curl();
    let server = spawn_http_server().await;

    // 圧縮なし
    let plain_out = run_curl(["-sS", &server.http_url("/")]).await;
    assert_eq!(plain_out.status, 0);

    // gzip 経由 + curl が自動展開 (gzip は libcurl が標準でサポートする)
    let gz_out = run_curl([
        "-sS",
        "--compressed",
        "-H",
        "Accept-Encoding: gzip",
        &server.http_url("/"),
    ])
    .await;
    assert_eq!(gz_out.status, 0);
    assert_eq!(
        gz_out.stdout, plain_out.stdout,
        "gzip-decoded body must equal plain body"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn compressed_body_is_smaller_than_uncompressed() {
    ensure_curl();
    let server = spawn_http_server().await;

    // 圧縮なし時の Content-Length を取得
    let (plain_headers, _) = fetch_with_accept_encoding(&server.http_url("/"), None).await;
    let plain_len: usize = find_header(&plain_headers, "Content-Length")
        .expect("plain response must have Content-Length")
        .parse()
        .expect("plain Content-Length must be numeric");

    // gzip / br / zstd それぞれで Content-Length が plain より小さいことを確認
    for encoding in ["gzip", "br", "zstd"] {
        let (headers, _) = fetch_with_accept_encoding(&server.http_url("/"), Some(encoding)).await;
        let compressed_len: usize = find_header(&headers, "Content-Length")
            .unwrap_or_else(|| panic!("{encoding} response must have Content-Length"))
            .parse()
            .unwrap_or_else(|_| panic!("{encoding} Content-Length must be numeric"));
        assert!(
            compressed_len < plain_len,
            "{encoding} body ({compressed_len}) must be smaller than plain ({plain_len})"
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn vary_accept_encoding_present() {
    ensure_curl();
    let server = spawn_http_server().await;

    // Accept-Encoding 有無によらず Vary が付くこと
    let (h_with, _) = fetch_with_accept_encoding(&server.http_url("/"), Some("gzip")).await;
    assert_eq!(find_header(&h_with, "Vary"), Some("Accept-Encoding"));

    let (h_without, _) = fetch_with_accept_encoding(&server.http_url("/"), None).await;
    assert_eq!(find_header(&h_without, "Vary"), Some("Accept-Encoding"));
}
