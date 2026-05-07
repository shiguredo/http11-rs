//! curl ベース integration test の共通ヘルパー
//!
//! - `--port 0` でサーバープロセスを起動し、stdout の `LISTENING_PORT=` 行から実ポートを取得
//! - `Drop` で確実にプロセスを kill する RAII ガード
//! - curl が PATH に無い場合は panic で即座に失敗
//! - `rcgen` で自己署名証明書を実行時生成 (fixture cert の期限切れを回避)

#![allow(dead_code)]

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use rcgen::{CertificateParams, DnType, KeyPair, SanType};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::timeout;

/// LISTENING_PORT 行を読む際のタイムアウト
const PORT_READ_TIMEOUT: Duration = Duration::from_secs(7);

/// curl が PATH 上にあるか確認する。無ければ panic で即座に失敗する。
///
/// CLAUDE.md「`#[ignore]` を使わない」に従い、環境差での skip ではなく
/// 明示的に失敗させて原因を分かりやすくする。
pub fn ensure_curl() {
    let status = std::process::Command::new("curl")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match status {
        Ok(s) if s.success() => {}
        _ => panic!("curl is required for these integration tests"),
    }
}

/// 起動済みサーバーへのハンドル
///
/// `Drop` で `kill_on_drop(true)` に従ってプロセスが終了する。
pub struct ServerHandle {
    child: Option<Child>,
    pub port: u16,
}

impl ServerHandle {
    /// HTTP URL を組み立てる
    pub fn http_url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, path)
    }

    /// HTTPS URL を組み立てる (host は localhost、curl --resolve と組み合わせる前提)
    pub fn https_url(&self, path: &str) -> String {
        format!("https://localhost:{}{}", self.port, path)
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // kill_on_drop(true) のため start_kill 呼び出しは保険
            let _ = child.start_kill();
        }
    }
}

/// HTTP サーバーを `--port 0` で起動し、LISTENING_PORT を読むまで待機する
pub async fn spawn_http_server() -> ServerHandle {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_http11_server"));
    cmd.arg("--port")
        .arg("0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    spawn_with_port_read(cmd).await
}

/// HTTPS サーバーを `--port 0 --tls --cert <p> --key <p>` で起動し、LISTENING_PORT を読むまで待機する
pub async fn spawn_https_server(cert_path: &Path, key_path: &Path) -> ServerHandle {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_http11_server"));
    cmd.arg("--port")
        .arg("0")
        .arg("--tls")
        .arg("--cert")
        .arg(cert_path)
        .arg("--key")
        .arg(key_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    spawn_with_port_read(cmd).await
}

/// 子プロセスを spawn して stdout から `LISTENING_PORT=N` を読む
async fn spawn_with_port_read(mut cmd: Command) -> ServerHandle {
    let mut child = cmd.spawn().expect("failed to spawn http11_server");
    let stdout = child
        .stdout
        .take()
        .expect("http11_server stdout was not captured");
    let mut reader = BufReader::new(stdout).lines();

    let port = match timeout(PORT_READ_TIMEOUT, reader.next_line()).await {
        Ok(Ok(Some(line))) => parse_listening_port(&line)
            .unwrap_or_else(|| panic!("unexpected first stdout line from server: {line:?}")),
        Ok(Ok(None)) => panic!("http11_server exited before printing LISTENING_PORT"),
        Ok(Err(e)) => panic!("failed to read http11_server stdout: {e}"),
        Err(_) => panic!("timed out waiting for LISTENING_PORT from http11_server"),
    };

    ServerHandle {
        child: Some(child),
        port,
    }
}

/// `LISTENING_PORT=<u16>` を parse する
fn parse_listening_port(line: &str) -> Option<u16> {
    let rest = line.strip_prefix("LISTENING_PORT=")?;
    rest.trim().parse().ok()
}

/// curl の実行結果
pub struct CurlOutput {
    pub stdout: Vec<u8>,
    pub stderr: String,
    pub status: i32,
}

impl CurlOutput {
    /// stdout を UTF-8 文字列として参照する (検証用)
    pub fn stdout_string(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
}

/// curl を引数列で同期実行する (内部で spawn_blocking)
pub async fn run_curl<I, S>(args: I) -> CurlOutput
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args: Vec<std::ffi::OsString> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();
    tokio::task::spawn_blocking(move || {
        let output = std::process::Command::new("curl")
            .args(&args)
            .output()
            .expect("failed to execute curl");
        CurlOutput {
            stdout: output.stdout,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            status: output.status.code().unwrap_or(-1),
        }
    })
    .await
    .expect("curl spawn_blocking task failed")
}

/// 自己署名証明書を一時ディレクトリに生成する
///
/// SAN に `DNS:localhost` と `IP:127.0.0.1` を入れて curl の hostname 検証を通過させる。
/// 戻り値: (一時ディレクトリのガード, cert.pem のパス, key.pem のパス)
pub fn generate_self_signed() -> (TempDir, PathBuf, PathBuf) {
    let mut params = CertificateParams::default();
    params
        .distinguished_name
        .push(DnType::CommonName, "localhost");
    params.subject_alt_names = vec![
        SanType::DnsName(
            "localhost"
                .try_into()
                .expect("static SAN dns name is valid"),
        ),
        SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
    ];
    let key_pair = KeyPair::generate().expect("failed to generate key pair");
    let cert = params
        .self_signed(&key_pair)
        .expect("failed to self-sign cert");

    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cert_path = dir.path().join("cert.pem");
    let key_path = dir.path().join("key.pem");
    std::fs::write(&cert_path, cert.pem()).expect("failed to write cert.pem");
    std::fs::write(&key_path, key_pair.serialize_pem()).expect("failed to write key.pem");

    (dir, cert_path, key_path)
}

/// レスポンスのヘッダー文字列から `name: value` の値を取得する (case-insensitive)
///
/// curl `-i` や `-D -` で取得した行ベースのヘッダーから対象を探す簡易ヘルパー。
pub fn find_header<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    for line in headers.lines() {
        if let Some((k, v)) = line.split_once(':')
            && k.trim().eq_ignore_ascii_case(name)
        {
            return Some(v.trim());
        }
    }
    None
}

/// curl `-i` で取得した「ヘッダー + 空行 + ボディ」の出力を分割する
///
/// HTTP/1.x のレスポンス形式に従い、最初に出現する `\r\n\r\n` または `\n\n` で分ける。
pub fn split_headers_body(raw: &[u8]) -> (String, Vec<u8>) {
    let crlf = b"\r\n\r\n";
    let lf = b"\n\n";
    if let Some(pos) = find_subsequence(raw, crlf) {
        let headers = String::from_utf8_lossy(&raw[..pos]).into_owned();
        let body = raw[pos + crlf.len()..].to_vec();
        (headers, body)
    } else if let Some(pos) = find_subsequence(raw, lf) {
        let headers = String::from_utf8_lossy(&raw[..pos]).into_owned();
        let body = raw[pos + lf.len()..].to_vec();
        (headers, body)
    } else {
        (String::from_utf8_lossy(raw).into_owned(), Vec::new())
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
