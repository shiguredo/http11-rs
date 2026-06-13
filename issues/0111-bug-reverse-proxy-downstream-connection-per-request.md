# リバースプロキシが下流接続を常に 1 リクエストで閉じる

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-reverse-proxy-downstream-connection-per-request
- Polished: {YYYY-MM-DD}

## 目的

`examples/http11_reverse_proxy` が `Connection: keep-alive` を送信する下流クライアントに対しても、1 リクエスト処理後に接続を閉じてしまう問題を修正し、下流接続の再利用を実現する。

## 優先度根拠

High とする。README では「接続プール」「Keep-Alive 対応」と謳われているが、実際には下流接続は 1 リクエストごとに閉じられており、接続プールの効果が全く得られていない。

## 現状

`examples/http11_reverse_proxy/src/main.rs:497-512` の accept ループは 1 接続ごとに `handle_client` を 1 回だけ `spawn` する。`handle_client`（`542-700`）は 1 リクエストを処理して `Ok(())` を返すだけで、同じ `socket` で次のリクエストを読むループが存在しない。関数から戻ると `TcpStream` が drop され、下流 TCP はクローズされる。

## 設計方針

1. `handle_client` 内で `loop { ... }` を回し、同一 `socket` で複数リクエストを処理する。
2. 各イテレーションで `RequestDecoder::decode_headers()` を使い、ヘッダーとボディを読み出す。
3. 下流の `is_keep_alive()` が false、または上流から `Connection: close`、またはシャットダウン時に `break` する。
4. 上流接続プールの返却判定も、下流の接続意愿と上流の `Connection` ヘッダーを正しく反映する。

## 完了条件

- 下流が `Connection: keep-alive` を送信した場合、同一 TCP 接続で複数リクエストを処理できること。
- 下流が `Connection: close` を送信した場合、1 リクエストで接続を終了すること。
- 上流接続プールが引き続き正しく機能すること。

## 解決方法

- `handle_client` をリファクタリングし、リクエスト受信 → 上流転送 → レスポンス返却 のループを追加する。
- ボディの完全消費を保証し、次の `decode_headers()` が新しいリクエストを正しく読めるようにする。
- `RequestDecoder::reset()` を適切に呼び出し、Keep-Alive 境界での状態をリセットする。
- テストで同一接続からの複数リクエストを検証する。
