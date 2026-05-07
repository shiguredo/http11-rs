# 0027: examples/http11_client に testcontainers ベースの integration test を追加する

Created: 2026-05-07
Completed: 2026-05-07
Model: Opus 4.7

## 概要

`examples/http11_client` に testcontainers-rs を実 HTTP サーバー (nginx) のホスティング機構として使う integration test を `examples/http11_client/tests/` 配下に追加する。GET 200 / 404 / HEAD のような基本動作に加え、`Transfer-Encoding: chunked` レスポンスの decode、大きなボディの完全受信、`Connection: close` 終端時の挙動 (`BodyKind::CloseDelimited`) を end-to-end で検証する。

テストハーネスから `http_request` 等の内部関数を直接呼べるよう、`examples/http11_client/src/main.rs` を library + binary 構成に分割する:

- `src/lib.rs` を新設し、`parse_url` / `http_request` / `https_request` / `decompressor` を pub にする
- `src/main.rs` は CLI フロントエンド (noargs パース + lib 関数呼び出し) に絞る

実 nginx は testcontainers-rs の `GenericImage` で `nginx:1.27-alpine` を起動する。`testcontainers-modules` には nginx モジュールが存在しないため `GenericImage` 一本で組む。

ブランチ名は CLAUDE.md「git ブランチの命名規則」に従い `feature/add-http11-client-testcontainers-tests` を使用する。

## 根拠

### 問題 1: integration test がなくリグレッション検出機構が無い

CLAUDE.md は「サンプルは **お手本** なので性能と堅牢性を両立させること」と規定しているが、現在 `examples/http11_client` には自動テストが存在しない。`ResponseDecoder` を使ったストリーミング decode、`BodyKind` ごとの分岐 (`Length` / `Chunked` / `CloseDelimited` / `None`)、Content-Encoding に基づく圧縮展開は、すべて手動実行と目視確認に依存している。

これにより以下のリスクがある:

- ライブラリ本体 (`shiguredo_http11`) の `ResponseDecoder` API 変更に追従する際、サンプル側のリグレッションを CI で検出できない
- chunked decode や close-delimited body のループ条件 (issue 0014 / 0015 で整理した `BodyProgress`) の挙動変化に気付かない
- decompressor の入出力エラー処理を継続的に検証する仕組みが無い

### 問題 2: 0026 で server 側だけテストが整い client 側が手薄なまま

issue 0026 で server 側に curl ベースの integration test (23 ケース) を追加したが、client 側は手付かずのままで、サンプル間でテスト網羅性が大きく非対称になっている。

### 問題 3: 実 HTTP サーバー相手のテストが存在しない

ライブラリ本体の単体テスト・PBT は in-memory バイト列で完結している。実 TCP 接続 + 実 HTTP サーバー相手の振る舞い (nginx の chunked encoding, gzip on, keepalive_timeout 0 などの実装挙動) を確認する経路が、サンプル含めてどこにも無い。これは「Sans I/O ライブラリだから不要」ではなく「Sans I/O ライブラリを実 I/O から呼んだサンプルが正しく動くか」を保証するために必要。

### 問題 4: testcontainers を選ぶ理由

ローカルに nginx を立てると環境依存が強く、ポート競合・前回プロセス残骸・OS 差で flaky になりやすい。testcontainers は:

- コンテナのライフサイクルを Drop で確実に終了させる (`ContainerAsync` が drop されると stop)
- Docker daemon が無ければ `start()` が `Result::Err` で即時失敗する → CLAUDE.md「`#[ignore]` を使わない」方針に整合
- `with_exposed_port(80.tcp())` でポートをランダム割り当て → 並列実行に強い

## 対応方針

### src/lib.rs の新設 (library + binary 構成)

`examples/http11_client/src/main.rs` の関数を以下のように再配置する:

- `src/lib.rs` (新規): `pub mod decompressor;` と公開関数の re-export だけ持つエントリポイント
- `src/url.rs` (新規): `parse_url` を移動 (現 `main.rs` line 97-123)
- `src/transport.rs` (新規): `http_request` / `https_request` を移動 (現 `main.rs` line 125-354)
- `src/main.rs`: `noargs` パース + `print_response` のみ残し、`http_request` / `https_request` / `parse_url` は `use http11_client::*;` で取り込む

戻り値の `Box<dyn std::error::Error>` は `Box<dyn std::error::Error + Send + Sync>` に揃え、`tokio::task::spawn_blocking` 内で `?` で扱えるようにする (`http_request` / `https_request` は同期 API のままで、テスト側は `spawn_blocking` で囲む)。

`mod` 構造を変更するだけで API シグネチャは変えない。`main.rs` の挙動は等価。

### dev-dependencies の追加

`examples/http11_client/Cargo.toml` 末尾に `[dev-dependencies]` セクションを新設する:

```toml
[dev-dependencies]
# Docker コンテナ起動 (実 nginx 相手の integration test)
testcontainers = { version = "0.27", default-features = false, features = ["aws-lc-rs"] }
# 非同期ランタイム + spawn_blocking (同期 client API を async test 内で呼ぶ)
tokio = { version = "1.52", features = ["macros", "rt-multi-thread", "time"] }
```

CLAUDE.md ルール:

- 暗号バックエンドは aws-lc-rs (`testcontainers` の `aws-lc-rs` feature、`default-features = false` で `ring` を切る)
- バージョンはマイナーまで指定
- 各依存に用途コメント

`testcontainers-modules` は nginx モジュールが存在しないため使わない (調査結果)。

### tests ディレクトリ構造

```
examples/http11_client/tests/
├── helpers/
│   └── mod.rs              共通ヘルパー (Docker 検出、nginx 起動、カスタム conf)
├── nginx_basic.rs          GET 200 / 404 / HEAD / Server ヘッダーの基本動作
└── nginx_streaming.rs      chunked / 大きなボディ / Connection: close の検証
```

サブディレクトリ方式 (`tests/helpers/mod.rs`) は 0026 と同じ慣例を採用する。各 `tests/<name>.rs` の先頭で `mod helpers;` で取り込む。

### tests/helpers/mod.rs の API 設計

```rust
/// Docker daemon が応答するか確認する。無ければ panic で fail-fast。
///
/// CLAUDE.md「`#[ignore]` を使わない」に従い、環境差での skip ではなく明示的に失敗させる。
pub async fn ensure_docker();

/// 起動済み nginx コンテナへのハンドル
pub struct NginxHandle {
    _container: ContainerAsync<GenericImage>,
    pub port: u16,
}

impl NginxHandle {
    /// HTTP URL を組み立てる
    pub fn http_url(&self, path: &str) -> String;  // "http://127.0.0.1:PORT/path"
}

/// `nginx:1.27-alpine` をデフォルト構成で起動する
pub async fn spawn_nginx_default() -> NginxHandle;

/// カスタム nginx.conf を `/etc/nginx/conf.d/default.conf` にマウントして起動する
pub async fn spawn_nginx_with_conf(conf: &str) -> NginxHandle;
```

#### 起動シーケンス

1. `ensure_docker()` で `docker version` または testcontainers の bollard ping を実行 (Docker 不在で即 panic)
2. `GenericImage::new("nginx", "1.27-alpine").with_exposed_port(80.tcp())` で起動
3. `start().await.expect("nginx container failed to start")` でコンテナ取得
4. `get_host_port_ipv4(80).await` で host 側ポートを取得
5. `wait_for_log` で `start worker processes` を待機 (起動レース対策)

カスタム conf は `with_copy_to("/etc/nginx/conf.d/default.conf", conf.into_bytes())` で渡す (testcontainers 0.27 の API)。

### 各テストファイルのテストケース

#### tests/nginx_basic.rs (デフォルト nginx)

- `get_root_returns_200_html`: `GET /` → status 200 / `Content-Type: text/html` / body に "Welcome to nginx" を含む
- `get_unknown_returns_404`: `GET /missing` → status 404 / body に "404 Not Found" を含む
- `head_root_returns_no_body`: `HEAD /` → status 200 / body == 空 (`BodyKind::None` の経路)
- `includes_server_header`: 任意レスポンスに `Server: nginx/...` ヘッダー
- `http_version_is_1_1`: ステータスラインの version が `HTTP/1.1`

#### tests/nginx_streaming.rs (カスタム conf)

- `chunked_response_decoded_properly`:
  - conf: `gzip on; gzip_types text/plain;` で gzip 圧縮を強制 → nginx は `Content-Length` を付けず `Transfer-Encoding: chunked` で返す
  - `Accept-Encoding: gzip` を送って `Content-Encoding: gzip` レスポンスを取得
  - decompressor で展開した body が期待値と一致することを検証 (`BodyKind::Chunked` の経路)
- `large_body_received_completely`:
  - 1 MiB の固定バイト列を返すエンドポイントを `add_header X-Test-Size 1048576;` 等のダミーで配置 (実体は echo モジュールではなく、`with_copy_to` で `/usr/share/nginx/html/large.bin` を配置)
  - `GET /large.bin` で正確に 1 MiB 受信されること、内容が一致すること (`BodyKind::Length` の経路)
- `connection_close_terminates_request`:
  - conf: `keepalive_timeout 0;` で全レスポンスに `Connection: close` を付与
  - レスポンスヘッダーに `Connection: close` が含まれること、サーバー側 close で完全受信できること (`BodyKind::Length` でも close 終端でも完了する)

### CHANGES.md

`## develop` の `### misc` セクション (既存) に以下を追加する:

```
- [ADD] examples/http11_client に testcontainers ベースの integration test を追加する
  - tests/nginx_basic.rs (GET 200 / 404 / HEAD / Server ヘッダーの 5 ケース) を追加する
  - tests/nginx_streaming.rs (chunked / 1 MiB ボディ / Connection: close の 3 ケース) を追加する
  - tests/helpers/mod.rs に Docker 検出・nginx 起動・カスタム conf 注入の共通ヘルパーを新設する
  - src を library + binary 構成に分割し、parse_url / http_request / https_request を lib 経由で再利用可能にする
  - dev-dependencies に testcontainers (aws-lc-rs feature) と tokio を追加する
  - @voluntas
```

機能影響なし (example 内部の library 化は破壊的変更ではない / テスト追加は本体動作に影響なし) のため `### misc` に分離する。

## 検証方針

### 検証コマンド

```bash
# Docker daemon が動作していることを前提に integration test を実行
cargo test -p http11_client

# 全体検証
make fmt && make clippy && make check && make test
```

### Docker 不在時の挙動確認

`docker stop` 等で daemon を止めた状態で `cargo test -p http11_client` を実行し、`ensure_docker()` が即時 panic することを目視確認する。

## 受け入れ基準

- ブランチ名が `feature/add-http11-client-testcontainers-tests` であること
- `make fmt && make clippy && make check && make test` がすべて成功する (Docker daemon が動作する環境で)
- `examples/http11_client/src/lib.rs` が新設され、`parse_url` / `http_request` / `https_request` / `decompressor` が pub で公開されている
- `examples/http11_client/src/main.rs` が `lib.rs` の関数を呼ぶ薄い CLI フロントエンドになっている (`http_request` / `https_request` / `parse_url` の実装本体は `main.rs` から消えている)
- `examples/http11_client/Cargo.toml` に `[dev-dependencies]` セクションが追加され、testcontainers (aws-lc-rs feature) と tokio が含まれている
- `examples/http11_client/tests/helpers/mod.rs` に `ensure_docker` / `NginxHandle` (Drop でコンテナ停止) / `spawn_nginx_default` / `spawn_nginx_with_conf` が実装されている
- `examples/http11_client/tests/nginx_basic.rs` の 5 ケースがすべて通る
- `examples/http11_client/tests/nginx_streaming.rs` の 3 ケースがすべて通る
- Docker daemon が起動していない環境で `ensure_docker()` が即座に panic で停止すること
- `#[ignore]` が一切使われていないこと
- 各 dev-dependency に用途コメントが書かれていること
- バージョン番号がマイナーまでで指定されていること
- コメントが日本語、ログメッセージが英語、エラーメッセージ (panic) が英語であること
- `CHANGES.md` の `## develop` の `### misc` に `[ADD]` エントリが追加されていること

## 解決方法

- `examples/http11_client/src/main.rs` を 3 ファイルに分割した:
  - `src/lib.rs` を新設し、`pub mod decompressor;` と `parse_url` / `http_request` / `https_request` を pub re-export
  - `src/url.rs` に `parse_url` を移動 (戻り値の `Box<dyn Error>` を `Box<dyn Error + Send + Sync>` に統一)
  - `src/transport.rs` に `http_request` / `https_request` を移動 (戻り値も `Send + Sync` 化)
  - `src/main.rs` は noargs パース + `print_response` のみ残す薄い CLI フロントエンドに整理
- HEAD / CONNECT 経路で decoder が body を待ち続ける問題を解消するため、`http_request` / `https_request` のシグネチャに `request_method: &str` を追加し、内部で `ResponseDecoder::set_request_method` を呼ぶようにした (RFC 9110 §9.3.2 / §9.3.6 の body 抑止判定に必要)
- `examples/http11_client/Cargo.toml` に `[dev-dependencies]` を新設:
  - `testcontainers = { version = "0.27", default-features = false, features = ["aws-lc-rs"] }`
  - `tokio = { version = "1.52", features = ["macros", "rt-multi-thread", "time"] }`
  - `testcontainers-modules` には nginx モジュールが存在しないことを事前調査で確認したため、`GenericImage` 一本で組む方針に決定
  - 各依存に用途コメントを記載
- `examples/http11_client/tests/helpers/mod.rs` を新設:
  - `ensure_docker()`: `docker version --format '{{.Server.Version}}'` を実行し、失敗時は `panic!("Docker daemon is required for these integration tests")`
  - `NginxHandle`: `ContainerAsync<GenericImage>` を保持して Drop 時に testcontainers が自動停止する仕組みを利用 (`port: u16` を pub フィールドで保持)
  - `spawn_nginx_default()`: `nginx:1.27-alpine` を `with_exposed_port(80.tcp())` + `WaitFor::message_on_either_std("start worker processes")` で起動
  - `spawn_nginx_with_conf(conf)`: `with_copy_to("/etc/nginx/conf.d/default.conf", ...)` でカスタム設定を注入
  - `spawn_nginx_with_files(conf, files)`: 上記に加えて `/usr/share/nginx/html/` 等への追加ファイルコピーをループで適用
- `examples/http11_client/tests/nginx_basic.rs` を新設し 5 ケース実装:
  - `get_root_returns_200_html` / `get_unknown_returns_404` / `head_root_returns_no_body` / `includes_server_header` / `http_version_is_1_1`
  - `fetch` ヘルパーで `tokio::task::spawn_blocking` 経由で同期 client API を async test 内から呼び出す
- `examples/http11_client/tests/nginx_streaming.rs` を新設し 3 ケース実装:
  - `chunked_response_decoded_properly`: `gzip on; gzip_min_length 0;` で nginx の gzip filter 経由で `Transfer-Encoding: chunked` を強制し、decompressor で展開した本文が元と一致することを検証 (chunked decoder 経路)
  - `large_body_received_completely`: 1 MiB の決定論的バイト列 (`(0..1024*1024).map(|i| (i % 251) as u8)`) を `with_copy_to` で nginx html 配下に置き、Content-Length 経由で完全受信されることを検証 (BodyKind::Length 経路)
  - `connection_close_terminates_request`: `keepalive_timeout 0;` でサーバー側に close を強制し、リクエスト側が `Connection: keep-alive` を送ってもレスポンスに `Connection: close` が含まれることを検証
- `CHANGES.md` の `## develop` の `### misc` に `[ADD]` エントリを追加
- `make fmt && make clippy && make check && make test` がすべて成功することを確認 (新規 8 ケースが pass、既存テスト・http11_server 23 ケース・PBT などすべて回帰なし)
- 検証時の発見:
  - `with_copy_to` の第二引数は `Vec<u8>` を受け取れる (`From<Vec<u8>> for CopyDataSource` impl 済み)
  - `nginx:1.27-alpine` の起動完了ログ `start worker processes` は stderr / stdout どちらに出るか構成依存だったため `WaitFor::message_on_either_std` で OR 待ち
  - HEAD レスポンスでは nginx が Content-Length を返してくるため、decoder に request method を伝えないと length 分の body を待ってハングする (issue 内の方針に「シグネチャは変えない」と書いていたが、テスト要件として method 追加を実施)
