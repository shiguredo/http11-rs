# リバースプロキシがリクエストボディを一括バッファリング

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-reverse-proxy-request-body-streaming
- Polished: {YYYY-MM-DD}

## 目的

`examples/http11_reverse_proxy` のリクエストボディ転送を一括バッファリングからストリーミングに変更し、大容量アップロード時のメモリ消費と TTFB 悪化を防ぐ。

## 優先度根拠

High とする。README では「ストリーミング転送」と講講われているが、リクエスト側は `Vec` に全ボディを貯めてから上流へ送信しており、実際にはストリーミングではない。大容量ボディやストリーミングクライアントに対して重大な問題となる。

## 現状

`examples/http11_reverse_proxy/src/main.rs:596-637` で `request_body = Vec::new()` に対し、`decoder.peek_body()` / `consume_body()` で全ボディを蓄積してから `upstream_request.body(request_body)` している。

## 設計方針

1. 上流接続確立後、下流ボディを読みながら上流に即座に書き出すパイプライン処理に変更する。
2. `BodyKind` に応じた終端判定を正しく行う。
3. エラー発生時は両方向の接続を適切にクローズする。

## 完了条件

- リクエストボディが `Vec` に全量貯められないこと。
- chunked / content-length / close-delimited 各経路でストリーミング転送が機能すること。
- 大容量アップロード時のメモリ使用量が大幅に削減されること。

## 解決方法

- `handle_client` のリクエストボディ読み出し部分を、上流への書き込みと連動させる。
- 上流接続を確立してからボディ転送を開始するようフローを変更する。
- 必要に応じて `tokio::io::copy_bidirectional` 的な処理を参考にしつつ、HTTP セマンティクスを保持する。
- テストで大容量ボディの転送を検証する。
