//! 実 nginx 相手のストリーミング系 integration test
//!
//! `BodyKind::Chunked` / `BodyKind::Length` (大きなボディ) / close 終端の各経路を網羅する:
//!   - `Transfer-Encoding: chunked` 受信 + gzip 展開 (chunked decoder + decompressor の合流経路)
//!   - 1 MiB のボディを Content-Length 経由で完全受信する (`BodyKind::Length`)
//!   - `keepalive_timeout 0` で nginx が `Connection: close` を返す (close 終端の挙動確認)
//!   - 1 MiB クラスの gzip 圧縮ボディを `AnyDecompressor` 経路で 8 KiB 出力
//!     バッファでストリーミング展開できることを検証 (transport.rs の堅牢性確認)
//!   - `ResponseDecoder::peek_body_decompressed` 経路を使って 8 KiB バッファで
//!     1 MiB クラスの gzip 圧縮ボディを段階的に展開できることを検証

mod helpers;

use std::io::{Read, Write};
use std::net::TcpStream;

use http11_client::decompressor::GzipDecompressor;
use http11_client::{http_request, parse_url};
use shiguredo_http11::compression::CompressionStatus;
use shiguredo_http11::{BodyProgress, Request, ResponseDecoder};

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
        "Content-Encoding must be gzip on the wire"
    );
    assert_eq!(
        response.get_header("Transfer-Encoding"),
        Some("chunked"),
        "Transfer-Encoding must be chunked (gzip filter discards Content-Length)"
    );

    // body_bytes() は transport.rs 内で AnyDecompressor によりストリーミング展開済み
    let received = response.body_bytes().expect("body bytes present");
    let recovered = std::str::from_utf8(received).expect("decompressed body is UTF-8");
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

/// 1 MiB クラスの gzip 圧縮ボディを `transport.rs` (= `AnyDecompressor` 経路) で
/// 受信し、ストリーミング展開が完全 / 正確に行われることを検証する
///
/// transport.rs は内部で 8 KiB の出力バッファを使うため、複数 OutputFull を経由して
/// 1 MiB 級のレスポンスを受信できることを確かめるストレステスト。
#[tokio::test]
async fn streams_large_gzip_body() {
    helpers::ensure_docker();
    // 1 MiB 程度の繰り返しテキスト (gzip filter が確実に走り、複数チャンクに分かれる量)
    let body_text = "lorem ipsum dolor sit amet ".repeat(40_000);
    let nginx = helpers::spawn_nginx_with_files(
        CONF_GZIP,
        &[(
            "/usr/share/nginx/html/big.txt",
            body_text.as_bytes().to_vec(),
        )],
    )
    .await;

    let response = fetch_with_headers(
        &nginx,
        "GET",
        "/big.txt",
        &[("Accept-Encoding", "gzip"), ("Connection", "close")],
    )
    .await;

    assert_eq!(response.status_code(), 200);
    assert_eq!(
        response.get_header("Content-Encoding"),
        Some("gzip"),
        "Content-Encoding must be gzip on the wire"
    );
    assert_eq!(
        response.get_header("Transfer-Encoding"),
        Some("chunked"),
        "Transfer-Encoding must be chunked"
    );

    let received = response.body_bytes().expect("body bytes present");
    assert_eq!(
        received.len(),
        body_text.len(),
        "decompressed length must match: got {} expected {}",
        received.len(),
        body_text.len()
    );
    assert!(
        received == body_text.as_bytes(),
        "decompressed body content mismatch"
    );
}

/// `ResponseDecoder::with_decompressor(GzipDecompressor::new())` 経路で
/// `peek_body_decompressed` を 8 KiB バッファに対し繰り返し呼び、
/// 1 MiB クラスの gzip 圧縮ボディが段階的に展開できることを検証する
///
/// `noflate::gzip::Decoder` は feed したバイトを内部 buffer に蓄積するため、
/// ボディ末尾の chunk を feed した後でも内部に未 drain のバイトが残ることがある。
/// 本テストは `peek_body_decompressed` がボディ枯渇後も空 input で展開器を駆動し
/// 内部 buffer を完全に drain できることを確認する (ライブラリ側の挙動検証)。
#[tokio::test]
async fn peek_body_decompressed_streams_gzip() {
    helpers::ensure_docker();
    // 1 MiB 程度の繰り返しテキスト (gzip filter が確実に走り、複数チャンクに分かれる量)
    let body_text = "lorem ipsum dolor sit amet ".repeat(40_000);
    let nginx = helpers::spawn_nginx_with_files(
        CONF_GZIP,
        &[(
            "/usr/share/nginx/html/big.txt",
            body_text.as_bytes().to_vec(),
        )],
    )
    .await;

    let url = nginx.http_url("/big.txt");
    let (_scheme, host, port, path) = parse_url(&url).expect("parse_url");
    let host_owned = host.clone();
    let path_owned = path.clone();
    let expected_len = body_text.len();

    let (decompressed, output_calls, max_output_chunk) = tokio::task::spawn_blocking(move || {
        let request_bytes = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: peek-test\r\nAccept-Encoding: gzip\r\nConnection: close\r\n\r\n",
            path_owned, host_owned
        );
        let mut stream =
            TcpStream::connect((host_owned.as_str(), port)).expect("TcpStream::connect");
        stream
            .write_all(request_bytes.as_bytes())
            .expect("write request");

        let mut decoder = ResponseDecoder::with_decompressor(GzipDecompressor::new());
        decoder.set_request_method("GET");

        const READ_CHUNK: usize = 8192;
        const OUTPUT_CAP: usize = 8192;
        let mut output = vec![0u8; OUTPUT_CAP];
        let mut decompressed = Vec::with_capacity(expected_len);
        let mut head_done = false;
        let mut output_calls = 0usize;
        let mut max_output_chunk = 0usize;
        let mut body_complete = false;

        'outer: loop {
            // ボディ完了済みなら I/O はもう不要。残った内部 buffer の drain だけ進める。
            if !body_complete {
                let want = decoder.available_buf().min(READ_CHUNK);
                assert!(want > 0, "decoder buffer full");
                let buf = decoder.mut_buf(want).expect("mut_buf");
                let n = stream.read(buf).expect("read");
                if n == 0 {
                    decoder.advance_buf(0);
                    decoder.mark_eof();
                } else {
                    decoder.advance_buf(n);
                }

                if !head_done {
                    if decoder.decode_headers().expect("decode_headers").is_some() {
                        head_done = true;
                    } else if n == 0 {
                        panic!("connection closed before headers");
                    } else {
                        continue;
                    }
                }
            }

            // peek_body_decompressed を回し、output が埋まる / 内部 buffer が
            // 一時的に空になるまで drain する
            loop {
                match decoder
                    .peek_body_decompressed(&mut output)
                    .expect("peek_body_decompressed")
                {
                    Some(status) => {
                        let produced = status.produced();
                        if produced > 0 {
                            decompressed.extend_from_slice(&output[..produced]);
                            output_calls += 1;
                            max_output_chunk = max_output_chunk.max(produced);
                        }
                        let consumed = status.consumed();
                        if consumed > 0 {
                            match decoder.consume_body(consumed).expect("consume_body") {
                                BodyProgress::Complete { .. } => {
                                    body_complete = true;
                                }
                                BodyProgress::Advanced | BodyProgress::NeedData => {}
                            }
                        }
                        if matches!(status, CompressionStatus::Complete { .. }) {
                            break 'outer;
                        }
                        if matches!(status, CompressionStatus::OutputFull { .. }) {
                            // 内部 buffer に leftover が残っているのでもう一度回す
                            continue;
                        }
                        // Continue: 内側ループを抜けて progress() / 外側 I/O に流す
                        break;
                    }
                    None => {
                        // body data 枯渇 + 内部 buffer 空。progress() を回して
                        // chunked terminator 等を進めるか、I/O に戻る
                        match decoder.progress().expect("progress") {
                            BodyProgress::Complete { .. } => break 'outer,
                            BodyProgress::Advanced => continue,
                            BodyProgress::NeedData => break,
                        }
                    }
                }
            }
        }

        (decompressed, output_calls, max_output_chunk)
    })
    .await
    .expect("blocking task");

    let recovered = String::from_utf8(decompressed).expect("decompressed body is UTF-8");
    assert_eq!(recovered, body_text);
    assert!(
        max_output_chunk <= 8192,
        "output chunk must not exceed buffer size: {}",
        max_output_chunk
    );
    assert!(
        output_calls > 1,
        "streaming decompression should yield multiple output chunks (got {})",
        output_calls
    );
}
