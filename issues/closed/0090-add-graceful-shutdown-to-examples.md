# examples/http11_reverse_proxy と http11_server の graceful shutdown を実装する

- Priority: Medium
- Created: 2026-05-15
- Completed: 2026-05-24
- Model: deepseek v4-pro
- Branch: feature/add-graceful-shutdown

## 目的

RFC 9112 Section 9.5 は「close を意図する client または server は接続に対して graceful close を発行すべき（SHOULD）」と定めている。

> A client or server that wishes to time out SHOULD issue a graceful close
> on the connection. Implementations SHOULD constantly monitor open
> connections for a received closure signal and respond to it as
> appropriate, since prompt closure of both sides of a connection enables
> allocated system resources to be reclaimed.

現在の examples はこの要件を満たしておらず、`tokio::spawn` のタスクが join されず、プロセス停止時に実行中リクエストが強制ドロップされる。お手本サーバとして、CTRL+C 受信後に accept ループを抜けて全タスクを待機し、実行中リクエストを完了させてから正常終了する flow を示すべき。

## 対象範囲

- `examples/http11_server` (tokio ベース)
- `examples/http11_reverse_proxy` (tokio ベース)

以下の example は対象外とする:
- `examples/http11_client`: ワンショットの CLI ツールであり、`tokio::spawn` によるタスク分散がなく graceful shutdown の恩恵がないため
- `examples/http11_server_io_uring`: 非 tokio の io_uring イベントループモデルであり、本 issue で扱う `JoinSet` + tokio シグナルハンドリングの方式が適用不可能なため（別 issue で対応）
- `examples/repro_cookie`: 単一目的の再現ツールのため

## 現状

### examples/http11_server/src/main.rs

- 116-143 行: TLS 用 (116-130) と HTTP 用 (134-142) の 2 つの accept loop が存在し、それぞれ `tokio::spawn` で `JoinHandle` が破棄されている

### examples/http11_reverse_proxy/src/main.rs

- 450-455 行: accept loop 内で `tokio::spawn` の `JoinHandle` が破棄されている（クリーンアップタスクも含め 2 系統の spawn がある）
- 422-432 行: クリーンアップタスクが `loop { interval.tick().await; ... }` で停止手段がない
- `ConnectionPool` (249-318 行) は `idle_connections: HashMap<UpstreamKey, Vec<PooledConnection>>` を持ち、shutdown 時にプール内接続の明示的クローズとメモリ解放が必要だが未実装

## 設計方針

### 1. JoinSet を使用して全接続タスクを管理する

`tokio::spawn` の代わりに `tokio::task::JoinSet` を使用し、accept 停止後に全タスクの完了を `join_next()` で待機する。

- `JoinSet` は tokio 1.21 以降で利用可能であり、追加依存・追加 feature 不要（`rt-multi-thread` が暗黙的に `rt` を有効化するため `JoinSet` は即利用可能）
- `Vec<JoinHandle<T>>` + `futures::future::join_all` 方式より `JoinSet` の方が `AbortHandle` 管理が組込で単純であり、かつ `JoinHandle` の poll 順序が公平（タスク追加順を保証しない）な点が多数接続のシナリオに適する

### 2. tokio::signal でシグナルを受信し accept ループを抜ける

`tokio::signal::ctrl_c()` で CTRL+C を受信する。Unix 環境では `tokio::signal::unix::signal(SignalKind::terminate())` で SIGTERM も受信する。

- `tokio::select!` で accept とシグナル受信を競合させる
- `http11_server` の 2 つの accept loop は、`if options.tls { ... } else { ... }` の分岐構造を残したまま、各分岐の `loop { ... }` を `loop { tokio::select! { ... } }` に変更する。TLS accept 時は `acceptor.accept(stream).await` の完了後、stream を enum でラップせず各分岐内で spawn する（既存の分岐コードを最大限再利用する）
- Windows 環境では `#[cfg(unix)]` で SIGTERM を conditionally にハンドリングする（`ctrl_c()` のみ常時有効）
- shutdown 状態は `Arc<AtomicBool>` で表現し、`handle_client` / `handle_tls_client` に渡す。`serve_request` にも伝播させ、shutdown 中のレスポンスに `Connection: close` を付与する（設計方針 3）

**伝播経路**: main（`Arc<AtomicBool>` 生成）→ handle_client / handle_tls_client → serve_request（bool パラメータとして受取）→ add_connection_headers（`should_keep_alive` と `shutting_down` の OR で `Connection: close` を付与）

### 3. shutdown 中のレスポンスに Connection: close を付与する

RFC 9112 Section 9.6 に従い、graceful shutdown 中に処理されるレスポンスには `Connection: close` を付与する。

> A server that sends a "close" connection option MUST initiate closure
> of the connection after it sends the response containing the "close"
> connection option. The server MUST NOT process any further requests
> received on that connection.

`Connection: close` 付与後、同一接続で後続リクエストが到達した場合は、即座に接続を閉じる（レスポンスを返さず接続を drop する）。

### 4. graceful shutdown タイムアウトを設ける

実行中リクエストが永遠に完了しない場合（slow client 等）に備え、全体の shutdown タイムアウトを設ける。

- タイムアウト値: 30 秒（Keep-Alive タイムアウト 60 秒より短く、かつ実用的なリクエスト完了に十分な時間。設定可能にする予定はなく、example としての単純さを優先する）
- タイムアウト後は `JoinSet::abort_all()` で残存タスクを破棄し、後続の `join_next()` が返す `JoinError` は ignore する（ログ出力不要、abort 後のエラーは期待動作のため）
- タイムアウトは `tokio::time::timeout` で `join_next()` ループ全体を wrap する

### 5. クリーンアップタスクに CancellationToken を導入する

`http11_reverse_proxy` のクリーンアップタスク (`422-432 行`) に `tokio_util::sync::CancellationToken` を導入し、shutdown 時に停止可能にする。

- `tokio_util` は `http11_reverse_proxy` の既存依存にはないため、`Cargo.toml` に `tokio-util = "0.7"` を追加する（version 0.7 が tokio 1.x 互換、`CancellationToken` は feature flag 不要）
- `loop { interval.tick().await; ... }` を `tokio::select! { _ = cancel_token.cancelled() => break, _ = interval.tick() => { ... } }` に変更する
- クリーンアップタスクの `JoinHandle` も `JoinSet` で管理し、shutdown 時に `cancel_token.cancel()` した後 `join_next()` で停止を待機する。wait が完了する前にタイムアウトした場合は他のタスクと同様に abort される
- `handle_client` の各接続タスクには `CancellationToken` を渡さない（shutdown は accept 停止 + `JoinSet::join_next()` で全完了を待つ方式で十分であり、`CancellationToken` の伝播範囲を最小限に留める）

### 6. 接続プール内の idle 接続を shutdown 時にクローズする

shutdown 時に `ConnectionPool` 内の idle 接続を破棄するため、`ConnectionPool` に `drain()` メソッド（引数なし、戻り値 `usize` で drain した接続数）を追加する。プール内接続は単に drop することで OS レベルでクローズされる（TLS close_notify は tokio_rustls の Drop 実装に委ねる）。

### 7. TLS 接続の half-close と closure alert

RFC 9112 Section 9.6 は TCP reset 問題を回避するため server-initiated half-close（write side の shutdown）を推奨している。

> To avoid the TCP reset problem, servers typically close a connection in
> stages. First, the server performs a half-close by closing only the
> write side of the read/write connection.

RFC 9112 Section 9.8 は TLS closure alert の交換を MUST としている。

> Servers MUST attempt to initiate an exchange of closure alerts with the
> client before closing the connection.

`http11_server` の `handle_client` は `stream.into_split()` で reader/writer を分離している。half-close 実装には `writer.into_inner().shutdown()` を呼ぶ必要があるが、`into_split()` で取得した `OwnedWriteHalf` からは `into_inner()` が使えない。この制約を踏まえ、本 issue では half-close は実装せず、`Connection: close` の付与のみを行う。TCP reset リスクのコードコメントを付記し、制約を明示する（お手本としての透明性を優先する）。TLS closure alert は tokio_rustls の Drop 実装に委ね、明示的な呼び出しは行わない。

## Cargo.toml の変更

### examples/http11_server/Cargo.toml

tokio features に `signal` を追加する（`ctrl_c()` および `unix::signal()` 用）。`JoinSet` は `rt` feature で利用可能であり、追加の feature は不要。

```diff
 tokio = { version = "1.52", features = [
   "io-util",
   "macros",
   "net",
   "rt-multi-thread",
   "time",
+  "signal",
 ] }
```

### examples/http11_reverse_proxy/Cargo.toml

tokio features に `signal` を追加し、`tokio-util` 依存を追加する。

```diff
 tokio = { version = "1.52", features = [
   "io-util",
   "macros",
   "net",
   "rt-multi-thread",
   "sync",
   "time",
+  "signal",
 ] }
+
+# CancellationToken
+tokio-util = "0.7"
```

## テスト戦略

### 既存テストの互換性

`examples/http11_server/tests/` の既存テスト (`http_basic`, `http_keep_alive`, `http_compression`, `https_tls`, `http_upload`) は `ServerHandle::Drop` で `child.start_kill()` を使用してプロセスを強制終了（SIGKILL）するため、graceful shutdown のコードパスは通過しない。したがって graceful shutdown 導入後も既存テストへの影響はない。

### graceful shutdown の検証

graceful shutdown のコードパス検証は以下の方針とする:

1. **手動確認**: `cargo run -p http11_server` で起動し、別端末から長時間リクエスト（例: `curl -v -d @/dev/zero http://localhost:8080/echo` 等で大きいボディを POST）を送った状態で CTRL+C を押下し、レスポンスが完全に返ってからプロセスが `exit code 0` で正常終了することを確認する
2. **自動テストは将来検討**: graceful shutdown の自動テストには SIGTERM 送信と exit code 監視が必要で、現状の curl ベーステストハーネスでは実現できない。integration test 用の基盤が整った段階で別 issue として追加する

## 完了条件

- `http11_server` で CTRL+C (Unix: SIGINT / SIGTERM) 受信後に実行中リクエストを完了させてから `exit code 0` で正常終了すること
- `http11_reverse_proxy` で CTRL+C 受信後に実行中リクエストを完了させ、クリーンアップタスクが正常に停止し、接続プールが drain されてから `exit code 0` で正常終了すること
- shutdown タイムアウト（30 秒）経過後は残存タスクを abort して強制終了できること
- shutdown 中のレスポンスに `Connection: close` が付与されること
- 既存の `examples/http11_server/tests/` の全テストが引き続き通過すること
- `CHANGES.md` の `## develop` の `### misc` に `[UPDATE]` エントリが追加されていること
  - エントリ例: `- [UPDATE] examples/http11_reverse_proxy と http11_server に graceful shutdown を実装する`
  - 担当者: `- @voluntas`

## 解決方法

### 変更ファイル

- `examples/http11_server/Cargo.toml`: tokio features に `signal` を追加
- `examples/http11_server/src/main.rs`: graceful shutdown を実装
- `examples/http11_reverse_proxy/Cargo.toml`: tokio features に `signal` を追加、`tokio-util` 依存を追加
- `examples/http11_reverse_proxy/src/main.rs`: graceful shutdown を実装

### http11_server の変更内容

- `tokio::spawn` を `JoinSet::spawn` に置き換え、全接続タスクを管理
- `shutdown_signal()` ヘルパーで CTRL+C / SIGTERM を待ち受け
- `tokio::select!` で accept とシグナル受信を競合させる
- シグナル受信後 `Arc<AtomicBool>` で shutdown 状態を伝播
- `serve_request` → `build_response` → `add_connection_headers` の経路で `Connection: close` を付与
- 30 秒タイムアウト後に `JoinSet::abort_all()` で残存タスクを破棄
- TLS half-close の制約をコードコメントで明記

### http11_reverse_proxy の変更内容

- `tokio::spawn` を `JoinSet::spawn` に置き換え
- クリーンアップタスクに `CancellationToken` を導入し停止可能にした
- `ConnectionPool::drain()` メソッドを追加し shutdown 時に idle 接続を破棄
- `shutdown_signal()` ヘルパーで CTRL+C / SIGTERM を待ち受け
- シグナル受信後: cancel_token.cancel() → pool.drain() → JoinSet 待機 (30 秒タイムアウト)

### テスト

- 既存の `examples/http11_server/tests/` の全テスト (26 件) が通過することを確認
- graceful shutdown 自体の自動テストは issue の設計方針どおり将来検討とする
