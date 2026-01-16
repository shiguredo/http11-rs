//! HTTP/HTTPS クライアントの例
//!
//! 使い方:
//!   cargo run -p http11_client -- https://example.com/
//!   cargo run -p http11_client -- http://httpbin.org/get

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, StreamOwned};
use rustls_platform_verifier::ConfigVerifierExt;
use shiguredo_http11::{Request, ResponseDecoder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    println!("Connecting to {}:{} ...", host, port);

    let request = Request::new("GET", &path)
        .header("Host", &host)
        .header("User-Agent", "shiguredo_http11/0.1.0")
        .header("Accept", "*/*")
        .header("Connection", "close");

    let request_bytes = request.encode();

    if scheme == "https" {
        // HTTPS
        let response = https_request(&host, port, &request_bytes)?;
        print_response(&response);
    } else {
        // HTTP
        let response = http_request(&host, port, &request_bytes)?;
        print_response(&response);
    }

    Ok(())
}

fn parse_url(url: &str) -> Result<(String, String, u16, String), Box<dyn std::error::Error>> {
    let (scheme, rest) = if let Some(rest) = url.strip_prefix("https://") {
        ("https".to_string(), rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        ("http".to_string(), rest)
    } else {
        return Err("URL must start with http:// or https://".into());
    };

    let (host_port, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };

    let (host, port) = match host_port.find(':') {
        Some(i) => {
            let port: u16 = host_port[i + 1..].parse()?;
            (&host_port[..i], port)
        }
        None => {
            let port = if scheme == "https" { 443 } else { 80 };
            (host_port, port)
        }
    };

    Ok((scheme, host.to_string(), port, path.to_string()))
}

fn http_request(
    host: &str,
    port: u16,
    request_bytes: &[u8],
) -> Result<shiguredo_http11::Response, Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect((host, port))?;
    stream.write_all(request_bytes)?;

    let mut decoder = ResponseDecoder::new();
    let mut buf = [0u8; 4096];

    loop {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            return Err("Connection closed before response complete".into());
        }

        decoder.feed(&buf[..n])?;

        if let Some(response) = decoder.decode()? {
            return Ok(response);
        }
    }
}

fn https_request(
    host: &str,
    port: u16,
    request_bytes: &[u8],
) -> Result<shiguredo_http11::Response, Box<dyn std::error::Error>> {
    // TLS 設定 (システムのプラットフォーム証明書ストアを使用)
    let config = ClientConfig::with_platform_verifier()?;

    let server_name = ServerName::try_from(host.to_string())?;

    let conn = ClientConnection::new(Arc::new(config), server_name)?;
    let sock = TcpStream::connect((host, port))?;
    let mut tls = StreamOwned::new(conn, sock);

    // リクエスト送信
    tls.write_all(request_bytes)?;

    // レスポンス受信
    let mut decoder = ResponseDecoder::new();
    let mut buf = [0u8; 4096];

    loop {
        let n = match tls.read(&mut buf) {
            Ok(0) => return Err("Connection closed before response complete".into()),
            Ok(n) => n,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(e) => return Err(e.into()),
        };

        decoder.feed(&buf[..n])?;

        if let Some(response) = decoder.decode()? {
            return Ok(response);
        }
    }
}

fn print_response(response: &shiguredo_http11::Response) {
    println!("\n--- Response ---");
    println!(
        "{} {} {}",
        response.version, response.status_code, response.reason_phrase
    );

    for (name, value) in &response.headers {
        println!("{}: {}", name, value);
    }

    println!();

    // ボディを表示 (テキストの場合)
    if let Ok(body) = std::str::from_utf8(&response.body) {
        if body.len() > 1000 {
            println!("{}...", &body[..1000]);
            println!("\n[Body truncated, {} bytes total]", response.body.len());
        } else {
            println!("{}", body);
        }
    } else {
        println!("[Binary body, {} bytes]", response.body.len());
    }
}
