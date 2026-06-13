# リバースプロキシが下流接続を常に 1 リクエストで閉じる

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-reverse-proxy-downstream-connection-per-request
- Polished: 2026-06-13

## 目的

`examples/http11_reverse_proxy` が `Connection: keep-alive` を送信する下流クライアントに対しても、1 リクエスト処理後に接続を閉じてしまう問題を修正し、下流接続の再利用を実現する。

## 優先度根拠

High とする。README では「接続プール」「Keep-Alive 対応」と謳われているが、実際には下流接続は 1 リクエストごとに閉じられており、接続プールの効果が全く得られていない。

## 現状

`examples/http11_reverse_proxy/src/main.rs:497-512` の accept ループは 1 接続ごとに `handle_client` を 1 回だけ `spawn` する。`handle_client`（`542-701`）は 1 リクエストを処理して `Ok(())` を返すだけで、同じ `socket` で次のリクエストを読むループが存在しない。関数から戻ると `TcpStream` が drop され、下流 TCP はクローズされる。

## 設計方針

1. `handle_client` 内で `loop { ... }` を回し、同一 `socket` で複数リクエストを逐次処理する。
2. 各イテレーションで `RequestDecoder::decode_headers()` を使いヘッダーを読み出し、`peek_body()` / `consume_body()` / `progress()` でボディを最後まで消費する。
3. 下流の `RequestHead::is_keep_alive()` が false、下流から `Connection: close` が送信された場合、下流が先に切断（read == 0）した場合、またはシャットダウンシグナルを受信した場合に `break` する。
   - 上流から `Connection: close` や close-delimited body が返却された場合でも、下流側の持続接続は維持する。上流接続の再利用可否は `stream_response_on_connection` の戻り値で判定し、下流ループには影響しない（RFC 9112 Section 9.3 / Section 9.6）。
   - 下流の HTTP-version が `HTTP/1.0` の場合は、リバースプロキシとして RFC 9112 Section 9.3（"A proxy server MUST NOT maintain a persistent connection with an HTTP/1.0 client"）に従い持続接続を維持しない（`Connection: keep-alive` 有無にかかわらず 1 リクエストで閉じる）。
4. 上流接続プールの返却判定は、上流レスポンスの `ResponseHead::is_keep_alive()`、`BodyKind::CloseDelimited`、およびエラー発生の有無に基づく。下流接続が閉じられる場合も、上流接続が再利用可能なら必ずプールに返却する。
5. シャットダウン用の `CancellationToken` を `handle_client` に渡し、1 リクエスト完了後にキャンセル済みならループを抜ける。これにより graceful shutdown が有効な待機時間内に完了する。
6. 各レスポンス送信後は `BufWriter` をフラッシュし、次のリクエスト読み取りが正しい境界から始まるようにする。

## 完了条件

- 下流が `Connection: keep-alive` を送信した場合、同一 TCP 接続で複数リクエストを処理できること。
- 下流が `Connection: close` を送信した場合、レスポンス送信後に接続を終了すること。
- 下流が HTTP/1.0 で `Connection: keep-alive` なしに接続した場合、1 リクエストで接続を終了すること（RFC 9112 Section 9.3）。
- 上流接続プールが引き続き正しく機能すること。
- 下流接続の切断やエラー時も、再利用可能な上流接続はプールに返却されること。
- Graceful shutdown 時に、活動中の下流接続が短時間で終了し、シャットダウンタイムアウトに引っかからないこと。
- `examples/http11_reverse_proxy/tests/` 以下に、同一 TCP 接続から複数リクエストを送信する結合テストを追加すること。テストはモックやスタブを使わず、実際の TCP 接続で検証すること。ディレクトリが存在しない場合は新規作成すること。
- `CHANGES.md` に `[FIX]` エントリーを追加すること。

## 解決方法

- `handle_client` をリファクタリングし、リクエスト受信 → 上流転送 → レスポンス返却 のループを追加する。
  - 関数シグネチャに `CancellationToken` を追加し、`main()` の `tasks.spawn(...)` で渡す。
- リクエストボディを最後まで消費し、`RequestDecoder` を次のメッセージの開始行解析に遷移させる。
  - `RequestDecoder::reset()` は内部バッファを破棄するため、パイプラインや先読み済みの次リクエストバイトを失う。ボディ完了後の `DecodePhase::Complete` から `decode_headers()` を呼び出すことで、残りバイトを保持したまま次のリクエストを解析する。
- 上流転送は既存の `stream_upstream_response_pooled` / `stream_response_on_connection` を再利用する。
- レスポンス送信後、下流の `is_keep_alive()` とシャットダウン状態を確認してループ継続/終了を決定する。
- 上流で `Connection: close` や close-delimited body が返却された場合は、当該上流接続をプールに返却せずクローズする。下流側には `Connection: close` を付与しない限り接続を維持する。
- CONNECT リクエストは引き続き 405 Method Not Allowed で拒否し、`Connection: close` を付与してループを抜ける。
