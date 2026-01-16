//! HTTP/HTTPS サーバーの例 (tokio + tokio-rustls)
//!
//! 使い方:
//!   # HTTP サーバー (ポート 8080)
//!   cargo run -p http11_server
//!
//!   # HTTPS サーバー (ポート 8443)
//!   cargo run -p http11_server -- --tls --cert cert.pem --key key.pem
//!
//! テスト用の自己署名証明書の作成:
//!   openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes \
//!     -subj "/CN=localhost"

use std::sync::Arc;

use rustls::ServerConfig;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use shiguredo_http11::{RequestDecoder, Response};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

struct ServerOptions {
    port: u16,
    tls: bool,
    cert_path: Option<String>,
    key_path: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_args()?;

    let addr = format!("0.0.0.0:{}", options.port);
    let listener = TcpListener::bind(&addr).await?;

    if options.tls {
        let cert_path = options
            .cert_path
            .as_ref()
            .ok_or("--cert is required for TLS")?;
        let key_path = options
            .key_path
            .as_ref()
            .ok_or("--key is required for TLS")?;

        let config = load_tls_config(cert_path, key_path)?;
        let acceptor = TlsAcceptor::from(Arc::new(config));

        println!("HTTPS server listening on https://{}", addr);

        loop {
            let (stream, peer_addr) = listener.accept().await?;
            let acceptor = acceptor.clone();

            tokio::spawn(async move {
                match acceptor.accept(stream).await {
                    Ok(tls_stream) => {
                        if let Err(e) = handle_tls_client(tls_stream, peer_addr).await {
                            eprintln!("TLS client error: {}", e);
                        }
                    }
                    Err(e) => eprintln!("TLS handshake error from {}: {}", peer_addr, e),
                }
            });
        }
    } else {
        println!("HTTP server listening on http://{}", addr);

        loop {
            let (stream, peer_addr) = listener.accept().await?;

            tokio::spawn(async move {
                if let Err(e) = handle_client(stream, peer_addr).await {
                    eprintln!("Client error: {}", e);
                }
            });
        }
    }
}

fn parse_args() -> Result<ServerOptions, Box<dyn std::error::Error>> {
    let mut args = noargs::raw_args();
    args.metadata_mut().app_name = "http11_server";

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

    // --tls フラグ
    let tls: bool = noargs::flag("tls")
        .doc("Enable HTTPS")
        .take(&mut args)
        .is_present();

    // --port オプション (デフォルトは TLS の有無で変わる)
    let default_port = if tls { "8443" } else { "8080" };
    let port: u16 = noargs::opt("port")
        .short('p')
        .doc("Port to listen on (default: 8080, or 8443 with --tls)")
        .default(default_port)
        .take(&mut args)
        .then(|o| o.value().parse())
        .map_err(|e| format!("{:?}", e))?;

    // --cert オプション
    let cert_path: Option<String> = noargs::opt("cert")
        .doc("Path to certificate file (PEM)")
        .take(&mut args)
        .present_and_then(|o| Ok::<_, &str>(o.value().to_string()))
        .map_err(|e| format!("{:?}", e))?;

    // --key オプション
    let key_path: Option<String> = noargs::opt("key")
        .doc("Path to private key file (PEM)")
        .take(&mut args)
        .present_and_then(|o| Ok::<_, &str>(o.value().to_string()))
        .map_err(|e| format!("{:?}", e))?;

    // 未知の引数があればエラー、ヘルプが返されたら表示
    if let Some(help) = args.finish().map_err(|e| format!("{:?}", e))? {
        print!("{}", help);
        std::process::exit(0);
    }

    Ok(ServerOptions {
        port,
        tls,
        cert_path,
        key_path,
    })
}

fn load_tls_config(
    cert_path: &str,
    key_path: &str,
) -> Result<ServerConfig, Box<dyn std::error::Error>> {
    let certs: Vec<CertificateDer<'static>> =
        CertificateDer::pem_file_iter(cert_path)?.collect::<Result<Vec<_>, _>>()?;

    if certs.is_empty() {
        return Err("No certificates found in cert file".into());
    }

    let key = PrivateKeyDer::from_pem_file(key_path)?;

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    Ok(config)
}

async fn handle_client(
    mut stream: TcpStream,
    peer_addr: std::net::SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("Connection from {}", peer_addr);

    let mut decoder = RequestDecoder::new();
    let mut buf = [0u8; 4096];

    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            println!("Connection closed by {}", peer_addr);
            break;
        }

        decoder.feed(&buf[..n])?;

        while let Some(request) = decoder.decode()? {
            println!(
                "{} {} {} from {}",
                request.method, request.uri, request.version, peer_addr
            );

            let response = build_response(&request);
            let response_bytes = response.encode();
            stream.write_all(&response_bytes).await?;

            if !request.is_keep_alive() {
                println!("Connection close requested by {}", peer_addr);
                return Ok(());
            }
        }
    }

    Ok(())
}

async fn handle_tls_client(
    mut stream: tokio_rustls::server::TlsStream<TcpStream>,
    peer_addr: std::net::SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("TLS connection from {}", peer_addr);

    let mut decoder = RequestDecoder::new();
    let mut buf = [0u8; 4096];

    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            println!("TLS connection closed by {}", peer_addr);
            break;
        }

        decoder.feed(&buf[..n])?;

        while let Some(request) = decoder.decode()? {
            println!(
                "{} {} {} from {} (TLS)",
                request.method, request.uri, request.version, peer_addr
            );

            let response = build_response(&request);
            let response_bytes = response.encode();
            stream.write_all(&response_bytes).await?;

            if !request.is_keep_alive() {
                println!("Connection close requested by {}", peer_addr);
                return Ok(());
            }
        }
    }

    Ok(())
}

fn build_response(request: &shiguredo_http11::Request) -> Response {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    match request.uri.as_str() {
        "/" => {
            let body = r#"<!DOCTYPE html>
<html>
<head><title>shiguredo_http11 Server</title></head>
<body>
<h1>Welcome to shiguredo_http11 Server</h1>
<p>This server is powered by shiguredo_http11 library.</p>
<ul>
<li><a href="/info">/info</a> - Server information</li>
<li><a href="/echo">/echo</a> - Echo request details</li>
</ul>
</body>
</html>
"#;

            Response::new(200, "OK")
                .header("Content-Type", "text/html; charset=utf-8")
                .header("Server", "shiguredo_http11/0.1.0")
                .body(body.as_bytes().to_vec())
        }
        "/info" => {
            let body = format!(
                r#"{{"server":"shiguredo_http11","version":"0.1.0","timestamp":{}}}"#,
                now
            );

            Response::new(200, "OK")
                .header("Content-Type", "application/json")
                .header("Server", "shiguredo_http11/0.1.0")
                .body(body.into_bytes())
        }
        "/echo" => {
            let mut body = format!(
                "Method: {}\nURI: {}\nVersion: {}\n\nHeaders:\n",
                request.method, request.uri, request.version
            );

            for (name, value) in &request.headers {
                body.push_str(&format!("  {}: {}\n", name, value));
            }

            if !request.body.is_empty() {
                body.push_str(&format!("\nBody ({} bytes):\n", request.body.len()));
                if let Ok(text) = std::str::from_utf8(&request.body) {
                    body.push_str(text);
                } else {
                    body.push_str("[binary data]");
                }
            }

            Response::new(200, "OK")
                .header("Content-Type", "text/plain; charset=utf-8")
                .header("Server", "shiguredo_http11/0.1.0")
                .body(body.into_bytes())
        }
        _ => {
            let body = "404 Not Found\n";
            Response::new(404, "Not Found")
                .header("Content-Type", "text/plain")
                .header("Server", "shiguredo_http11/0.1.0")
                .body(body.as_bytes().to_vec())
        }
    }
}
