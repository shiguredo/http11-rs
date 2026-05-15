# examples/http11_reverse_proxy と http11_server の graceful shutdown を実装する

- Priority: Medium
- Created: 2026-05-15
- Model: deepseek-v4-pro

## 目的

全 example で `tokio::spawn` のタスクが join されず、プロセス停止時に実行中リクエストが強制ドロップされる。お手本サーバとして、CTRL+C 受信後に accept ループを抜けて全タスクを待機する flow を示すべき。

## 現状

- `examples/http11_server/src/main.rs:120,137`: `tokio::spawn(async move { ... })` で `JoinHandle` が破棄されている
- `examples/http11_reverse_proxy/src/main.rs:450`: 同様
- `examples/http11_reverse_proxy/src/main.rs:423-432`: クリーンアップタスクが `loop { interval.tick().await; ... }` で停止手段がない
- `examples/http11_server_io_uring/src/main.rs:153`: `ServerConnection::new(...).expect(...)` が 1 接続のエラーでサーバ全体を停止させる

## 設計方針

1. `tokio::spawn` の代わりに `JoinSet` を使用する
2. シグナルハンドリングで accept 停止 → `join_next` で全タスク終了待ち → 正常 `return` の流れを実装する
3. クリーンアップタスクに `CancellationToken` を導入する
4. `ServerConnection::new` の `expect` を `Result` ハンドリングに変更し、1 接続のエラーでプロセス全体が停止しないようにする

## 完了条件

- 全 example で graceful shutdown が実装されていること
- CTRL+C で実行中リクエストを完了させてからプロセスが終了すること
- クリーンアップタスクが確実に停止すること
