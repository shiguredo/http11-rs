//! shiguredo_container ベース integration test の共通ヘルパー
//!
//! - `nginx:1.27-alpine` をコンテナ IP 直結で起動する (published port 経由は使わない。理由は「接続方式について」参照)
//! - カスタム `nginx.conf` を `/etc/nginx/conf.d/default.conf` に bind mount した構成も組める
//! - コンテナは `ContainerAsync` の Drop で自動停止する
//!
//! # 接続方式について (published port を使わずコンテナ IP 直結にする理由)
//!
//! shiguredo_container の macOS バックエンド (Apple Container) では、published port の
//! 転送をホスト側のフォワーダが担う。このフォワーダは、クライアントの読み込みが遅いと
//! 大容量レスポンスのボディを途中で切り捨て、正常な EOF を返す (途中切断) ことがある
//! (curl 等の高速な読み込みでは再現しない)。Linux (Docker) では再現しない。この制約は
//! container-rs 側でも macOS の既知の制約として文書化されている。
//!
//! 本テストは http11_client の HTTP/1.1 処理の検証が目的であり、接続先が published port
//! かコンテナ IP 直結かは本質に関係しない。そこでフォワーダを迂回するため、テストは
//! 常にコンテナ IP に直接接続する。コンテナ IP 直結は macOS (Apple Container) でも
//! Linux (Docker のブリッジ) でもホストから到達可能で、大容量レスポンスの途中切断が
//! 起きない。
//!
//! `with_exposed_port` は従来どおり宣言する (コンテナ設定としてポートを公開しておく
//! 標準的な形を保つ)。テストはこの公開ポートには接続せず、コンテナ IP 宛てに接続する。
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

use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use shiguredo_container::core::{IntoContainerPort, Mount, WaitFor};
use shiguredo_container::runners::AsyncRunner;
use shiguredo_container::{ContainerAsync, GenericImage, ImageExt};

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

/// 一時ファイル名をプロセス内で一意にするための連番
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// プロセス ID と連番から衝突しないファイル名の接尾辞を作る
fn unique_suffix() -> String {
    let count = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}_{}", std::process::id(), count)
}

/// ホスト側の一時設定ファイル。Drop 時に削除する。
///
/// `nginx.conf` を bind mount (macOS は virtiofs / Linux は Docker) でコンテナ起動前に
/// 見せるため、ホスト側の実ファイルがコンテナ稼働中も存在し続ける必要がある。
/// `NginxHandle` の `_container` より後に drop される (フィールド宣言順) ことで、
/// コンテナ停止後に確実に削除される。
struct TempConfigFile {
    path: PathBuf,
}

impl TempConfigFile {
    /// 内容を一時ディレクトリへ書き出し、ガードを返す
    fn write(contents: &[u8]) -> std::io::Result<Self> {
        let path =
            std::env::temp_dir().join(format!("http11_client_nginx_conf_{}.conf", unique_suffix()));
        fs::write(&path, contents)?;
        Ok(Self { path })
    }

    /// ホスト側の絶対パスを返す
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempConfigFile {
    fn drop(&mut self) {
        // テスト終了時の後始末なので、削除に失敗しても黙る (残っても害は小さい)
        let _ = fs::remove_file(&self.path);
    }
}

/// 起動済み nginx コンテナへのハンドル
///
/// `ContainerAsync` を保持することで Drop 時に shiguredo_container が自動的に
/// コンテナを停止 / 削除する。`container_ip` はコンテナ自身の IP アドレスで、
/// published port を介さず直接接続するために使う (理由はモジュール冒頭の「接続方式について」参照)。
pub(crate) struct NginxHandle {
    // Drop 時にコンテナを停止するためフィールドとして保持する (直接参照はしない)
    _container: ContainerAsync<GenericImage>,
    // bind mount のソース (ホスト一時ファイル)。コンテナの後 (宣言順) に drop され、削除される
    _config_file: Option<TempConfigFile>,
    /// コンテナの IP アドレス (スパウン時に解決済み)
    container_ip: IpAddr,
}

impl NginxHandle {
    /// `http://{コンテナ IP}:80/path` 形式の URL を組み立てる
    ///
    /// published port (127.0.0.1:ランダム) ではなくコンテナ IP 直結にする。
    /// 理由はモジュール冒頭の「接続方式について」を参照。
    pub(crate) fn http_url(&self, path: &str) -> String {
        format!(
            "http://{}:{}{}",
            self.container_ip, NGINX_INTERNAL_PORT, path
        )
    }
}

/// `nginx:1.27-alpine` をデフォルト構成で起動する
///
/// `with_exposed_port` はコンテナ設定としてポートを公開しておくための宣言であり、
/// テストはこれには接続しない (コンテナ IP 直結。理由はモジュール冒頭の「接続方式について」参照)。
pub(crate) async fn spawn_nginx_default() -> NginxHandle {
    let image = GenericImage::new(NGINX_IMAGE_NAME, NGINX_IMAGE_TAG)
        .with_exposed_port(NGINX_INTERNAL_PORT.tcp())
        .with_wait_for(WaitFor::message_on_either_std(NGINX_READY_LOG));
    spawn(image.into(), None).await
}

/// カスタム `nginx.conf` を `/etc/nginx/conf.d/default.conf` に bind mount して起動する
///
/// nginx の Docker イメージは `/etc/nginx/conf.d/*.conf` を `http {}` ブロック内で `include` するため、
/// `default.conf` を上書きすればデフォルト server 定義を完全に置き換えられる。
///
/// 設定ファイルは nginx master が**起動時に**読むため、`with_copy_to` では使えない。
/// shiguredo_container の `with_copy_to` は macOS (Apple Container) では start 後に
/// 投入される (Linux は create 後・start 前) ため、macOS では設定が反映されない競合が
/// 起きる。ホスト一時ファイルに書き出して bind mount すれば、macOS (virtiofs) /
/// Linux (Docker) とも起動前に見える。
///
/// `files` は `(コンテナ内パス, 内容)` の組のスライス。`/usr/share/nginx/html/` 配下に
/// 静的ファイルを置きたい場合や、テスト用 fixture を仕込みたい場合に使う。静的ファイルは
/// リクエスト時に読まれるため、`with_copy_to` で十分である (start 内で投入完了後に
/// ready 待機へ進むため、テストのリクエスト時には必ず存在する)。
pub(crate) async fn spawn_nginx_with_files(conf: &str, files: &[(&str, Vec<u8>)]) -> NginxHandle {
    let config_file = TempConfigFile::write(conf.as_bytes()).expect("一時ファイルの書き込みに失敗");
    let mut request = GenericImage::new(NGINX_IMAGE_NAME, NGINX_IMAGE_TAG)
        .with_exposed_port(NGINX_INTERNAL_PORT.tcp())
        .with_wait_for(WaitFor::message_on_either_std(NGINX_READY_LOG))
        .with_mount(Mount::bind_mount(
            config_file.path().to_string_lossy().into_owned(),
            "/etc/nginx/conf.d/default.conf",
        ));
    for (path, content) in files {
        request = request.with_copy_to(path.to_string(), content.clone());
    }
    spawn(request, Some(config_file)).await
}

/// コンテナを起動し、コンテナ IP を解決して `NginxHandle` にまとめる
async fn spawn(
    request: shiguredo_container::ContainerRequest<GenericImage>,
    config_file: Option<TempConfigFile>,
) -> NginxHandle {
    let container = request.start().await.expect("nginx コンテナの起動に失敗");
    // テストは published port ではなくコンテナ IP 直結で接続する
    // (理由はモジュール冒頭の「接続方式について」参照)。macOS の published port
    // フォワーダは遅い消費者への大容量レスポンスを途中で切断するため、これを迂回する。
    let container_ip = container
        .get_bridge_ip_address()
        .await
        .expect("nginx コンテナ IP の取得に失敗");
    NginxHandle {
        _container: container,
        _config_file: config_file,
        container_ip,
    }
}
