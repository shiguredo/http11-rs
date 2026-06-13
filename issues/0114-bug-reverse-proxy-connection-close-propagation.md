# リバースプロキシが Connection: close を下流/上流に伝達しない

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-reverse-proxy-connection-close-propagation
- Polished: {YYYY-MM-DD}

## 目的

`examples/http11_reverse_proxy` が下流からの `Connection: close` を上流に、上流からの `Connection: close` を下流に正しく伝達するようにする。

## 優先度根拠

High とする。RFC 9112 Section 9.6 に従い、接続を終了する側は `Connection: close` を送信すべきである。現状は `Connection` を hop-by-hop ヘッダーとして単純に除去しており、接続終了の意思が伝わらず、誤った接続再利用を引き起こす。

## 現状

- `examples/http11_reverse_proxy/src/main.rs:640-676`: 下流リクエストから `Connection` を除外し、無条件で `Connection: keep-alive` を上流に付与する。
- `examples/http11_reverse_proxy/src/main.rs:826-890`: 上流レスポンスから `Connection` を除外し、`is_close_delimited` の場合のみ下流に `Connection: close` を付与する。

## 設計方針

1. 下流リクエストの `Connection` トークンを解析し、`close` が含まれる場合は上流にも `Connection: close` を送信し、当該上流接続はプールに戻さない。
2. 上流レスポンスが `Connection: close` または `is_keep_alive() == false` の場合、下流レスポンスにも `Connection: close` を付与する。
3. Hop-by-hop ヘッダーの除去と `Connection` 意思の伝達を両立させる。

## 完了条件

- 下流から `Connection: close` が来た場合、上流にも `Connection: close` が送信されること。
- 上流から `Connection: close` が来た場合、下流にも `Connection: close` が送信されること。
- 接続プールが誤って終了すべき接続を再利用しないこと。

## 解決方法

- `src/main.rs` の上流リクエスト構築処理で、下流の `is_keep_alive()` と `Connection` ヘッダーを確認する。
- 下流レスポンス構築処理で、`can_reuse == false` の場合に `Connection: close` を付与する。
- テストで該当ケースを検証する。
