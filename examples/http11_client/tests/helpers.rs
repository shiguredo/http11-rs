//! testcontainers ベース integration test の共通ヘルパー
//!
//! - Docker daemon が起動していなければ `ensure_docker()` で即 panic (CLAUDE.md「`#[ignore]` を使わない」)
//! - `nginx:1.27-alpine` を `--port 0` 相当 (testcontainers のランダム host port) で起動する
//! - カスタム `nginx.conf` を `/etc/nginx/conf.d/default.conf` にコピーした構成も組める
//! - コンテナは `ContainerAsync` の Drop で自動停止する
//!
//! # 可視性について
//!
//! 各テストバイナリは独立した crate としてビルドされ、`mod helpers;` で本ファイル全体を
//! 取り込む。`spawn_nginx_default` は nginx_basic でのみ、`spawn_nginx_with_files` は
//! nginx_streaming / nginx_upload でのみ使用されるため、どのバイナリでも必ずどちらか
//! 一方が未使用と判定される。
//!
//! `pub fn` の未使用は `#[expect(dead_code)]` で満たせない (pub は外部公開の可能性が
//! あるとみなされるため) ことから、テストバイナリ内で閉じることを明示する
//! `pub(crate)` にする。`#[expect]` は「このファイル内で dead_code が発生する」ことを
//! 宣言するもので、将来すべての関数が全バイナリから使われるようになった場合は
//! `unfulfilled_lint_expectations` で気づける。

#![expect(dead_code)]

use std::process::Stdio;

use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

/// 起動完了とみなす nginx のログメッセージ
///
/// nginx 1.27 alpine は master プロセスが `notice: start worker processes` を出した時点で
/// listen socket を bind 済み。stdout / stderr どちらに出るかは構成依存のため両方を待つ。
const NGINX_READY_LOG: &str = "start worker processes";

/// nginx コンテナイメージ (タグはバージョンを固定して再現性を担保する)
const NGINX_IMAGE_NAME: &str = "nginx";
const NGINX_IMAGE_TAG: &str = "1.27-alpine";

/// nginx コンテナがリッスンする内部ポート (HTTP)
const NGINX_INTERNAL_PORT: u16 = 80;

/// Docker daemon が応答するか確認する。無ければ panic で fail-fast する。
///
/// CLAUDE.md「`#[ignore]` を使わない」に従い、環境差での skip ではなく
/// 明示的に失敗させて原因を分かりやすくする。
pub(crate) fn ensure_docker() {
    let status = std::process::Command::new("docker")
        .arg("version")
        .arg("--format")
        .arg("{{.Server.Version}}")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match status {
        Ok(s) if s.success() => {}
        _ => panic!("これらの結合テストは Docker デーモンが必要"),
    }
}

/// 起動済み nginx コンテナへのハンドル
///
/// `ContainerAsync` を保持することで Drop 時に testcontainers が自動的に
/// コンテナを停止 / 削除する。`port` は host 側に publish された TCP ポート。
pub(crate) struct NginxHandle {
    // Drop 時にコンテナを停止するためフィールドとして保持する (直接参照はしない)
    _container: ContainerAsync<GenericImage>,
    pub port: u16,
}

impl NginxHandle {
    /// `http://127.0.0.1:PORT/path` 形式の URL を組み立てる
    pub(crate) fn http_url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, path)
    }
}

/// `nginx:1.27-alpine` をデフォルト構成で起動する
pub(crate) async fn spawn_nginx_default() -> NginxHandle {
    let image = GenericImage::new(NGINX_IMAGE_NAME, NGINX_IMAGE_TAG)
        .with_exposed_port(NGINX_INTERNAL_PORT.tcp())
        .with_wait_for(WaitFor::message_on_either_std(NGINX_READY_LOG));
    spawn(image.into()).await
}

/// カスタム `nginx.conf` を `/etc/nginx/conf.d/default.conf` にコピーして起動する
///
/// nginx の Docker イメージは `/etc/nginx/conf.d/*.conf` を `http {}` ブロック内で `include` するため、
/// `default.conf` を上書きすればデフォルト server 定義を完全に置き換えられる。
/// `files` は `(コンテナ内パス, 内容)` の組のスライス。`/usr/share/nginx/html/` 配下に
/// 静的ファイルを置きたい場合や、テスト用 fixture を仕込みたい場合に使う。
pub(crate) async fn spawn_nginx_with_files(conf: &str, files: &[(&str, Vec<u8>)]) -> NginxHandle {
    let mut request = GenericImage::new(NGINX_IMAGE_NAME, NGINX_IMAGE_TAG)
        .with_exposed_port(NGINX_INTERNAL_PORT.tcp())
        .with_wait_for(WaitFor::message_on_either_std(NGINX_READY_LOG))
        .with_copy_to("/etc/nginx/conf.d/default.conf", conf.as_bytes().to_vec());
    for (path, content) in files {
        request = request.with_copy_to(path.to_string(), content.clone());
    }
    spawn(request).await
}

/// コンテナを起動し、host 側 port を取得して `NginxHandle` にまとめる
async fn spawn(request: testcontainers::ContainerRequest<GenericImage>) -> NginxHandle {
    let container = request.start().await.expect("nginx コンテナの起動に失敗");
    let port = container
        .get_host_port_ipv4(NGINX_INTERNAL_PORT)
        .await
        .expect("nginx ホストポートの取得に失敗");
    NginxHandle {
        _container: container,
        port,
    }
}
