//! 大容量ファイルアップロードの integration test
//!
//! http11_server の `/echo` エンドポイントに対して curl で 10 MiB のバイナリファイルを
//! POST し、サーバーが全ボディを正しく受信できることを検証する。
//!
//! `/echo` は受信ボディがテキストならその内容を、バイナリなら `[binary data]` を含めて
//! レスポンスするため、本テストでは `Body (<N> bytes):` 行のバイト数と `[binary data]`
//! マーカーの存在で全ボディ受信を確認する。

#![cfg(not(windows))]

mod helpers;

use std::io::Write;

use helpers::{ensure_curl, run_curl, spawn_http_server};

/// 10 MiB の決定論的バイナリデータを生成する
///
/// 各バイトはオフセットを 251 (素数) で剰余した値。全バイト値 (0x00-0xFA) が
/// 出現するため、テキストとして解釈できず `/echo` は `[binary data]` を返す。
fn generate_10mb_binary() -> Vec<u8> {
    const SIZE: usize = 10 * 1024 * 1024;
    (0..SIZE).map(|i| (i % 251) as u8).collect()
}

/// 10 MiB バイナリを curl `--data-binary @file` で POST し、
/// サーバーが全ボディ (10485760 bytes) を受信できたことを検証する
#[tokio::test(flavor = "current_thread")]
async fn post_10mb_binary_via_curl() {
    ensure_curl();
    let server = spawn_http_server().await;

    let body = generate_10mb_binary();
    let mut tmp = tempfile::NamedTempFile::new().expect("一時ファイルの作成に失敗");
    tmp.write_all(&body)
        .expect("一時ファイルへの書き込みに失敗");
    tmp.flush().expect("一時ファイルのフラッシュに失敗");
    let file_path = tmp.path().to_string_lossy().to_string();

    let out = run_curl([
        "-sS",
        "-X",
        "POST",
        "--data-binary",
        &format!("@{}", file_path),
        &server.http_url("/echo"),
    ])
    .await;
    assert_eq!(out.status, 0, "curl 実行が失敗: stderr={}", out.stderr);

    let body_text = out.stdout_string();
    assert!(
        body_text.contains("Body (10485760 bytes):"),
        "/echo のレスポンスに正しいボディ長が含まれるべき: {body_text:?}"
    );
    assert!(
        body_text.contains("[binary data]"),
        "/echo のレスポンスにバイナリデータマーカーが含まれるべき: {body_text:?}"
    );
}

/// curl `--data-binary` で空ボディを POST し、`Body (0 bytes):` が
/// レスポンスに含まれないことを検証する (空ボディは echo で無視される)
#[tokio::test(flavor = "current_thread")]
async fn post_empty_body_includes_no_body_section() {
    ensure_curl();
    let server = spawn_http_server().await;

    let out = run_curl([
        "-sS",
        "-X",
        "POST",
        "--data-binary",
        "",
        &server.http_url("/echo"),
    ])
    .await;
    assert_eq!(out.status, 0, "curl 実行が失敗: stderr={}", out.stderr);

    let body_text = out.stdout_string();
    assert!(
        body_text.contains("Method: POST"),
        "/echo のレスポンスにメソッドが含まれるべき: {body_text:?}"
    );
    assert!(
        !body_text.contains("Body ("),
        "空ボディの場合、Body (...) 行は出力されるべきでない: {body_text:?}"
    );
}

/// curl `--data-binary` で 1 MiB のテキストデータを POST し、
/// テキストボディの完全なラウンドトリップを検証する
#[tokio::test(flavor = "current_thread")]
async fn post_1mb_text_roundtrip_via_curl() {
    ensure_curl();
    let server = spawn_http_server().await;

    // 1 MiB の ASCII テキスト (改行付き繰り返しパターン)
    // curl --data-binary で送信し、/echo がテキストとして echo するため
    // `[binary data]` にならず完全な内容比較が可能。
    let text = "abcdefghijklmnopqrstuvwxyz0123456789\n"
        .repeat(1024 * 1024 / 37)
        .into_bytes();
    let mut tmp = tempfile::NamedTempFile::new().expect("一時ファイルの作成に失敗");
    tmp.write_all(&text)
        .expect("一時ファイルへの書き込みに失敗");
    tmp.flush().expect("一時ファイルのフラッシュに失敗");
    let file_path = tmp.path().to_string_lossy().to_string();

    let out = run_curl([
        "-sS",
        "-X",
        "POST",
        "--data-binary",
        &format!("@{}", file_path),
        &server.http_url("/echo"),
    ])
    .await;
    assert_eq!(out.status, 0, "curl 実行が失敗: stderr={}", out.stderr);

    let body_text = out.stdout_string();
    assert!(
        body_text.contains(&format!("Body ({} bytes):", text.len())),
        "/echo のレスポンスに正しいボディ長が含まれるべき: {body_text:?}"
    );
    assert!(
        !body_text.contains("[binary data]"),
        "テキストボディでは [binary data] は含まれるべきでない"
    );
    // /echo は "Method: ...\nURI: ...\nVersion: ...\n\nHeaders:\n  ...\n\nBody (N bytes):\n" +
    // ボディ内容を出力する。本文の直前の改行以降が元のテキストと一致することを確認する。
    let marker = format!("Body ({} bytes):\n", text.len());
    let body_start = body_text.find(&marker).expect("Body 行が存在するべき") + marker.len();
    let echoed_body = &body_text[body_start..];
    assert_eq!(
        echoed_body.as_bytes(),
        text.as_slice(),
        "ラウンドトリップでテキストボディが一致しない"
    );
}
