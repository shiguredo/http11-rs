//! HTTP/HTTPS クライアントの例
//!
//! 使い方:
//!   cargo run -p http11_client -- https://example.com/
//!   cargo run -p http11_client -- http://httpbin.org/get
//!
//! 圧縮対応:
//!   Accept-Encoding ヘッダーで gzip, br, zstd を要求し、
//!   Content-Encoding ヘッダーに基づいてレスポンスボディを **ストリーミング展開** する。
//!   展開器の本体は `src/decompressor.rs` (Decompressor トレイト実装) で、
//!   `src/transport.rs` で peek_body() / consume_body() と組み合わせて駆動する。
//!
//! ストリーミング API:
//!   このサンプルは decode() 一括 API ではなく、
//!   decode_headers() + peek_body() / consume_body() / progress() を
//!   使用したストリーミング API の実装例 (`src/transport.rs` を参照)。

use http11_client::decompressor::supported_encodings;
use http11_client::{http_request, https_request, parse_url};
use shiguredo_http11::{HttpHead, Request, Response};
use tracing::info;

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::init();

    let mut args = noargs::raw_args();
    args.metadata_mut().app_name = "http11_client";

    // --help フラグ
    noargs::HELP_FLAG.take_help(&mut args);

    // --version フラグ
    let version_flag: bool = noargs::flag("version")
        .short('V')
        .doc("Show version")
        .take(&mut args)
        .is_present();
    if version_flag {
        println!("{}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    // 位置引数: URL
    let url: String = noargs::arg("<URL>")
        .doc("URL to fetch (e.g., https://example.com/)")
        .take(&mut args)
        .then(|a| Ok::<_, &str>(a.value().to_string()))
        .map_err(|e| format!("{:?}", e))?;

    // 未知の引数があればエラー、ヘルプが返されたら表示
    if let Some(help) = args.finish().map_err(|e| format!("{:?}", e))? {
        print!("{}", help);
        return Ok(());
    }

    let (scheme, host, port, path) = parse_url(&url)?;

    info!(host, port, "Connecting");

    let mut request = Request::new("GET", &path)?
        .header("Host", &host)?
        .header("User-Agent", "shiguredo_http11/0.1.0")?
        .header("Accept", "*/*")?
        .header("Connection", "close")?;

    // 有効な圧縮形式があれば Accept-Encoding を追加
    let encodings = supported_encodings();
    if !encodings.is_empty() {
        request = request.header("Accept-Encoding", encodings)?;
    }

    let request_method = request.method().to_string();
    let request_bytes = request.try_encode()?;

    if scheme == "https" {
        // HTTPS
        let response = https_request(&host, port, &request_method, &request_bytes)?;
        print_response(&response);
    } else {
        // HTTP
        let response = http_request(&host, port, &request_method, &request_bytes)?;
        print_response(&response);
    }

    Ok(())
}

fn print_response(response: &Response) {
    info!(
        version = HttpHead::version(response),
        status_code = response.status_code(),
        reason_phrase = response.reason_phrase(),
        "Response received"
    );

    for (name, value) in HttpHead::headers(response) {
        info!(name, value, "Header");
    }

    // ボディは transport.rs で既にストリーミング展開済み
    let body: &[u8] = response.body_bytes().unwrap_or(&[]);

    if let Ok(text) = std::str::from_utf8(body) {
        if text.len() > 1000 {
            info!(total_bytes = body.len(), "Body truncated");
            println!("{}...", &text[..1000]);
        } else {
            println!("{}", text);
        }
    } else {
        info!(bytes = body.len(), "Binary body");
    }
}
