# 0026: examples/http11_server に curl ベースの integration test を追加する

Created: 2026-05-07
Model: Opus 4.7

## 概要

`examples/http11_server` に curl を実クライアントとして使用する integration test を `examples/http11_server/tests/` 配下に追加する。HTTP の基本動作、Accept-Encoding に基づく自動圧縮 (gzip / br / zstd)、Keep-Alive / Connection 制御、HTTPS / TLS の各機能を end-to-end で検証する。

テスト時のポート競合を回避するため、`examples/http11_server/src/main.rs` を改造する:

- tracing の出力先を stdout から stderr に変更する
- bind 後に `listener.local_addr()` から実ポートを取得し、`LISTENING_PORT=<port>` 形式で stdout に出力する

これにより `--port 0` を指定して OS にランダム割当させた場合でも、テストハーネスが stdout から実ポートを読み取って curl で叩けるようになる。

ブランチ名は CLAUDE.md「git ブランチの命名規則」に従い `feature/add-http11-server-curl-integration-tests` を使用する。

## 根拠

### 問題 1: integration test がなくリグレッション検出機構が無い

CLAUDE.md は「サンプルは **お手本** なので性能と堅牢性を両立させること」と規定しているが、現在 `examples/http11_server` には自動テストが存在しない。エンドポイント (`/`, `/info`, `/echo`)、Accept-Encoding に基づく圧縮選択ロジック (`compressor.rs`)、Keep-Alive 接続管理、HTTPS の振る舞いはすべて手動の curl 実行と目視確認に依存している。

これにより以下のリスクがある:

- ライブラリ本体 (`shiguredo_http11`) の API 変更に追従する際、サンプル側のリグレッションを CI で検出できない
- 圧縮優先順位 (zstd > br > gzip) の挙動変更を見逃す
- HTTPS 経路で TLS ハンドシェイク後の振る舞いが壊れていても気付かない

`examples/http11_client` や `examples/http11_reverse_proxy` よりも、サーバー側はリクエストパース・レスポンス組み立て・ボディハンドリングを多く実装しているため、自動テストの優先度が高い。

### 問題 2: 手動 curl 実行はカバレッジが安定しない

開発者が手動で curl を打つ場合、対象エンドポイントの網羅性が個人依存になる。Accept-Encoding `q=0` で除外する挙動、`Accept-Encoding` ヘッダー無しでの `Content-Encoding` 不在、`Vary: Accept-Encoding` の付与、HEAD でのボディ抑制など、サンプル側で意図的に実装されている振る舞いを継続的に検証する仕組みが無い。

### 問題 3: `--port 0` で OS 任せ起動ができない

現在の `main.rs` は `--port` を u16 で受けており 0 を渡すこと自体は可能だが、bind 後の実ポートを外部に通知する仕組みが無い。テスト並列実行時のポート競合を避けるには OS 任せのランダムポートが望ましいが、現状ではテストハーネスから実ポートを知る手段が存在しない。

## 対応方針

### main.rs の改造 (最小スコープ)

`examples/http11_server/src/main.rs` を 2 点改造する。改造範囲は HTTP / HTTPS の bind 共通の前段に集約する。

#### 改造 1: tracing の出力先を stderr に固定する

```rust
// 変更前 (line 88)
tracing_subscriber::fmt::init();

// 変更後
tracing_subscriber::fmt().with_writer(std::io::stderr).init();
```

理由: stdout を `LISTENING_PORT=` 出力専用にして、テストハーネスの行ベース parse を確実にするため。

#### 改造 2: bind 後の実ポートを stdout に出力する

```rust
// 変更前 (line 92-93)
let addr = format!("0.0.0.0:{}", options.port);
let listener = TcpListener::bind(&addr).await?;

// 変更後
let bind_addr = format!("0.0.0.0:{}", options.port);
let listener = TcpListener::bind(&bind_addr).await?;
let local_addr = listener.local_addr()?;
// テストハーネスが parse する machine-readable な行を stdout に出す
println!("LISTENING_PORT={}", local_addr.port());
// 子プロセス pipe 経由で確実に届けるため flush する
use std::io::Write;
std::io::stdout().flush().ok();
let addr = local_addr.to_string();
```

`addr` 変数は以降の `info!(addr = %addr, ...)` でそのまま使えるため、HTTP / HTTPS 両分岐の修正は不要。

`--port` を u16 で受ける既存パース (line 166-172) は変更不要。0 を渡せば OS が空きポートを割り当てる。

### dev-dependencies の追加

`examples/http11_server/Cargo.toml` 末尾に `[dev-dependencies]` セクションを新設する:

```toml
[dev-dependencies]
# 自己署名証明書を実行時生成 (fixture cert の期限切れを回避)
rcgen = { version = "0.13", default-features = false, features = ["aws_lc_rs", "pem"] }
# テスト用一時ディレクトリ (証明書ファイルの置き場)
tempfile = "3.13"
# 子プロセスの非同期管理 + stdout 行読み
tokio = { version = "1.52", features = ["process", "io-util", "macros", "rt", "time"] }
```

CLAUDE.md ルール:

- 暗号バックエンドは aws-lc-rs (`rcgen` の `aws_lc_rs` feature)
- バージョンはマイナーまで指定
- 各依存に用途コメント

### tests ディレクトリ構造

```
examples/http11_server/tests/
├── common/
│   └── mod.rs              共通ヘルパー (curl 検出、サーバー起動、cert 生成)
├── http_basic.rs           GET / HEAD / POST / 404 の基本動作
├── http_compression.rs     Accept-Encoding と Content-Encoding の検証
├── http_keep_alive.rs      Keep-Alive と Connection: close の挙動
└── https_tls.rs            HTTPS と自己署名証明書の検証
```

サブディレクトリ方式 (`tests/common/mod.rs`) は Cargo の慣例。各 `tests/<name>.rs` の先頭で `mod common;` で取り込む。`tests/common.rs` 単一ファイル方式は別バイナリとして扱われるため避ける。

### tests/common/mod.rs の API 設計

```rust
/// curl が PATH 上にあるか確認。無ければ panic。
pub fn ensure_curl();

/// 起動済みサーバーへのハンドル (Drop で kill)
pub struct ServerHandle {
    child: Option<tokio::process::Child>,
    pub port: u16,
}

impl ServerHandle {
    pub fn http_url(&self, path: &str) -> String;   // "http://127.0.0.1:PORT/path"
    pub fn https_url(&self, path: &str) -> String;  // "https://localhost:PORT/path"
}

impl Drop for ServerHandle { /* kill_on_drop に依存 */ }

/// HTTP サーバーを `--port 0` で起動。LISTENING_PORT を読むまで待機 (timeout 7s)
pub async fn spawn_http_server() -> ServerHandle;

/// HTTPS サーバーを起動 (cert/key の一時パスを渡す)
pub async fn spawn_https_server(cert_path: &Path, key_path: &Path) -> ServerHandle;

/// curl 実行結果
pub struct CurlOutput {
    pub stdout: Vec<u8>,
    pub stderr: String,
    pub status: i32,
}

/// curl をブロッキング実行 (tokio::task::spawn_blocking で囲む)
pub async fn run_curl<I, S>(args: I) -> CurlOutput;

/// 自己署名証明書 (CN=localhost, SAN: DNS:localhost / IP:127.0.0.1) を一時ディレクトリに生成
pub fn generate_self_signed() -> (tempfile::TempDir, PathBuf, PathBuf);
```

#### 起動シーケンス

1. `Command::new(env!("CARGO_BIN_EXE_http11_server"))` で Cargo が解決した bin パスを使う
2. `.stdout(Stdio::piped()).stderr(Stdio::inherit()).kill_on_drop(true)`
3. `BufReader::new(stdout).lines()` で `LISTENING_PORT=` 行を `tokio::time::timeout(Duration::from_secs(7), ...)` で待機
4. ハングや早期終了 (port 出力前に exit) は明示エラーで fail-fast

curl 不在時は `ensure_curl()` が panic する。CLAUDE.md「`#[ignore]` を使わない」に従い、環境差で skip ではなく明示的に失敗させる。

### 影響範囲一覧

| パス | 種別 | 内容 |
|---|---|---|
| `examples/http11_server/src/main.rs` | 修正 | tracing を stderr に固定 / bind 後の実ポートを stdout に出力 |
| `examples/http11_server/Cargo.toml` | 修正 | `[dev-dependencies]` セクション新設 (rcgen / tempfile / tokio) |
| `examples/http11_server/tests/common/mod.rs` | 新設 | テストハーネス共通モジュール |
| `examples/http11_server/tests/http_basic.rs` | 新設 | GET/HEAD/POST/404 の基本動作 (7 ケース) |
| `examples/http11_server/tests/http_compression.rs` | 新設 | gzip/br/zstd の Content-Encoding 検証 (8 ケース) |
| `examples/http11_server/tests/http_keep_alive.rs` | 新設 | Keep-Alive / Connection: close の挙動 (3 ケース) |
| `examples/http11_server/tests/https_tls.rs` | 新設 | rcgen で生成した自己署名証明書による HTTPS 検証 (4 ケース) |
| `CHANGES.md` | 修正 | `## develop` に `[ADD]` エントリと `### misc` を追加 |

## CHANGES.md

`## develop` セクションに以下を追加する (UPDATE → ADD → CHANGE → FIX の順):

```
- [ADD] examples/http11_server に `--port 0` 対応と `LISTENING_PORT=<port>` の stdout 出力を追加する
  - tracing の出力先を stdout から stderr に変更する (破壊的変更)
  - integration test 追加のためのテストハーネス前提
  - @voluntas

### misc

- examples/http11_server に curl ベースの integration test を追加する
  - tests/http_basic.rs / tests/http_compression.rs / tests/http_keep_alive.rs / tests/https_tls.rs
  - tests/common/mod.rs にテストハーネス共通モジュールを新設する
  - dev-dependencies に rcgen / tempfile / tokio (process feature) を追加する
  - @voluntas
```

`--port 0` 対応 + stdout 出力 + tracing の出力先変更は機能影響があるので `[ADD]`、テスト追加自体は機能影響なしのため `### misc` に分離する。

## 検証方針

### 各テストファイルのテストケース

#### tests/http_basic.rs

- `get_root_returns_200_html`: `GET /` → 200 + `Content-Type: text/html` + body 検証
- `get_info_returns_200_json`: `GET /info` → 200 + `Content-Type: application/json` + JSON 形式チェック
- `get_echo_includes_request_headers`: `GET /echo` → body にメソッド・URI・任意ヘッダーが echo されている
- `head_root_returns_no_body`: `HEAD /` → 200、`%{size_download}` で 0
- `post_echo_returns_request_body`: `curl -d 'hello'` → body に `hello`
- `get_unknown_returns_404`: `GET /missing` → 404
- `server_emits_date_and_server_headers`: 任意レスポンスに `Date` と `Server` ヘッダー

#### tests/http_compression.rs

- `gzip_only_returns_gzip`: `Accept-Encoding: gzip` → `Content-Encoding: gzip`
- `br_only_returns_br`: → `Content-Encoding: br`
- `zstd_only_returns_zstd`: → `Content-Encoding: zstd`
- `prefers_zstd_over_br_over_gzip`: 全エンコーディング送信時 → `zstd` 選択
- `quality_zero_excludes_encoding`: `gzip;q=0, br` → `br`
- `no_accept_encoding_no_content_encoding`: ヘッダー未送信 → `Content-Encoding` 無し
- `compressed_decoded_body_matches_uncompressed`: `curl --compressed` で展開後の body と plain の比較
- `vary_accept_encoding_present`: 全レスポンスに `Vary: Accept-Encoding`

#### tests/http_keep_alive.rs

- `keep_alive_two_requests_one_connection`: `curl -v --next` で 2 リクエスト → 両方 200、stderr に `Re-using existing connection` を grep
- `connection_close_header_terminates`: `curl -H 'Connection: close'` → レスポンスに `Connection: close`
- `keep_alive_default_no_close_header`: 単発 → `Connection: close` 無し (HTTP/1.1 デフォルト)

#### tests/https_tls.rs

- `https_get_root_with_self_signed_cert`: rcgen 生成 cert で起動 → `curl --cacert <ca.pem> --resolve localhost:PORT:127.0.0.1` で 200
- `https_compression_works`: HTTPS 上でも `Accept-Encoding: gzip` が効く
- `https_keep_alive_works`: `--next` で複数リクエスト OK
- `tls_handshake_with_invalid_ca_fails`: `--cacert` 無し → exit code 非 0、stderr に cert エラー

### 自己署名証明書の生成 (rcgen)

各 HTTPS テスト関数で都度生成する (rcgen は数 ms、並列実行時の競合を完全に避けられる)。SAN に `DNS:localhost` と `IP:127.0.0.1` を入れ、curl の hostname 検証を通過させる。

```rust
let mut params = CertificateParams::default();
params.distinguished_name.push(DnType::CommonName, "localhost");
params.subject_alt_names = vec![
    SanType::DnsName("localhost".try_into().unwrap()),
    SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
];
let key_pair = KeyPair::generate()?;  // aws-lc-rs バックエンド
let cert = params.self_signed(&key_pair)?;
```

### 検証コマンド

```bash
# 手動起動確認 (stdout に LISTENING_PORT が出るか)
cargo run -p http11_server -- --port 0

# integration test 実行
cargo test -p http11_server

# 全体検証
make fmt && make clippy && make check && make test
```

## 受け入れ基準

- ブランチ名が `feature/add-http11-server-curl-integration-tests` であること
- `make fmt && make clippy && make check && make test` がすべて成功する
- `examples/http11_server/src/main.rs` の `tracing_subscriber::fmt::init()` が `with_writer(std::io::stderr)` 付きに変更されている
- `examples/http11_server/src/main.rs` が bind 後に `LISTENING_PORT=<port>\n` を stdout に出力し、明示的に flush している
- `--port 0` で起動した場合に OS 任せのポートで bind し、stdout に実ポートが出力されることを手動で確認できる
- `examples/http11_server/Cargo.toml` に `[dev-dependencies]` セクションが追加され、rcgen / tempfile / tokio (process feature) が含まれている
- `examples/http11_server/tests/common/mod.rs` に `ensure_curl` / `ServerHandle` (Drop で kill) / `spawn_http_server` / `spawn_https_server` / `run_curl` / `generate_self_signed` が実装されている
- `examples/http11_server/tests/http_basic.rs` の 7 ケースがすべて通る
- `examples/http11_server/tests/http_compression.rs` の 8 ケースがすべて通る
- `examples/http11_server/tests/http_keep_alive.rs` の 3 ケースがすべて通る
- `examples/http11_server/tests/https_tls.rs` の 4 ケースがすべて通る
- curl が PATH 上に無い環境で `ensure_curl()` が即座に panic で停止すること
- `#[ignore]` が一切使われていないこと
- 各 dev-dependency に用途コメントが書かれていること
- バージョン番号がマイナーまでで指定されていること
- コメントが日本語、ログメッセージが英語、エラーメッセージ (panic) が英語であること
- `CHANGES.md` の `## develop` セクションに `[ADD]` エントリと `### misc` が追加されていること

## 解決方法

(完了時に追記)
