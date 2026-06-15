# リバースプロキシが下流接続を常に 1 リクエストで閉じる

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-reverse-proxy-downstream-connection-per-request
- Polished: 2026-06-16

## 目的

`examples/http11_reverse_proxy` が `Connection: keep-alive` を送信する下流クライアントに対しても、1 リクエスト処理後に接続を閉じてしまう問題を修正し、下流接続の再利用を実現する。本 issue は同 reverse_proxy サンプルの一連の修正 (0114 「`Connection: close` 伝達」、0115 「リクエストボディストリーミング」) の **最上流** に位置する。これらの後続 issue は本 issue で導入する「同一 socket で複数リクエストを逐次処理する loop 構造」を前提に記述されている。

## 優先度根拠

High とする。README では「接続プール」「Keep-Alive 対応」と謳われているが、実際には下流接続は 1 リクエストごとに閉じられており、接続プールの効果が全く得られていない。

## 現状

`examples/http11_reverse_proxy/src/main.rs:497-512` の accept ループは 1 接続ごとに `handle_client` を 1 回だけ `spawn` する。`handle_client`（`542-701`）は 1 リクエストを処理して `Ok(())` を返すだけで、同じ `socket` で次のリクエストを読むループが存在しない。関数から戻ると `TcpStream` が drop され、下流 TCP はクローズされる。

## 設計方針

1. `handle_client` 内で `loop { ... }` を回し、同一 `socket` で複数リクエストを逐次処理する。

2. 各イテレーションで `RequestDecoder::decode_headers()` を呼んでヘッダを読み出し、続いて `peek_body()` / `consume_body()` を使ってボディを読み進めながら `progress()` の戻り値 (`BodyProgress::Complete { trailers }`) でボディ完了を判定する。`Complete` 到達後に同一 decoder 上で再度 `decode_headers()` を呼び、内部バッファに先読み済みの次リクエストバイトを保持したまま次のサイクルへ遷移する。これは HTTP/1.1 のパイプライン経路 (RFC 9112 Section 9.3.2) に対応するため必要で、先読みバイト = pipelined 後続リクエストとなり得る。

3. 下流接続のループ継続/終了は以下のルールで判定する。

   - 下流の HTTP-version が `HTTP/1.0` の場合は、RFC 9112 Section 9.3 (`A proxy server MUST NOT maintain a persistent connection with an HTTP/1.0 client`) に厳格準拠し、`Connection: keep-alive` 有無にかかわらず 1 リクエストで閉じる。判定は `RequestHead::is_keep_alive()` ではなくバージョン文字列 (`HTTP/1.0`) を直接見る (`is_keep_alive()` は HTTP/1.0+keep-alive で true を返すため、本ケースでは利用しない)。
   - 下流が HTTP/1.1 で `RequestHead::is_keep_alive()` が false (= `Connection: close` 受信) の場合は break する。
   - 下流が先に切断 (read == 0) した場合、またはシャットダウンシグナルを受信した場合に break する。
   - 上流から `Connection: close` や close-delimited body が返却された場合でも、本 issue では **下流側の持続接続は維持する** (下流ループに影響しない)。これは 0114 (`reverse-proxy-connection-close-propagation`) で「上流の close 意思を下流にも伝達して `Connection: close` を付与する」方針に再修正される予定であり、本 issue は最小修正に留める旨を明示する。

4. 上流接続プールの返却判定は、上流レスポンスの `ResponseHead::is_keep_alive()`、`BodyKind::CloseDelimited`、およびエラー発生の有無に基づく。下流接続が閉じられる場合も、上流接続が再利用可能なら必ずプールに返却する。`stream_upstream_response_pooled` / `stream_response_on_connection` の戻り値は現状の `Result<bool, _>` (上流再利用可否) を維持し、下流側継続可否は呼び出し側 (`handle_client` ループ) で別途判定する (戻り値拡張は行わない、本 issue の最小破壊範囲のため)。

5. シャットダウン用の `CancellationToken` を `handle_client` のシグネチャに追加し、main の `tasks.spawn(...)` で `cancel_token.clone()` を渡す。

   - **リクエスト読み取り中の cancel**: `tokio::select! { _ = cancel.cancelled() => break, n = socket.read(buf) => ... }` でラップし、シャットダウン中にリクエスト読み取りで block しないようにする。
   - **1 リクエスト完了後の cancel チェック**: ループ末尾で `cancel.is_cancelled()` を確認し、true なら break する。
   - これにより graceful shutdown が有効な待機時間内に完了する。0090 (closed) で導入した graceful shutdown 機構と整合する。

6. **下流ライターのフラッシュ**: 各レスポンス送信後に下流ライター (`stream_response_on_connection` 内の `BufWriter<&mut TcpStream>`) をフラッシュし、次のリクエスト読み取りが正しい境界から始まるようにする。`BufWriter` の所有が関数スコープ内に閉じている現状実装では、関数 return 前に `flush().await?` を呼べば足りる。

7. **アイドルタイムアウト**: 下流接続のアイドル状態 (リクエスト読み取り待ち) の最大保持時間を 60 秒とし、`tokio::time::timeout` でラップする。Slowloris 対策および接続リーク防止のため。タイムアウト到達時はループを break する (RFC 9112 Section 9.5 で「persistent connections SHOULD be closed after a reasonable idle timeout」の方針に従う)。

   なお既存の上流側 `PoolConfig::idle_timeout_secs` (60 秒) と数値は同じだが、目的が異なる (上流側は「プール中の idle 接続を切る」、下流側は「Slowloris 対策」) ため定数は共有しない。下流アイドルタイムアウト用に専用の `const DOWNSTREAM_IDLE_TIMEOUT_SECS: u64 = 60;` を定義する。

8. **CONNECT 拒否**: 既存仕様どおり CONNECT リクエストは 405 Method Not Allowed で拒否し、`Connection: close` を付与してループを抜ける。

## 完了条件

- 下流が HTTP/1.1 + `Connection: keep-alive` (または `Connection` 省略) を送信した場合、同一 TCP 接続で複数リクエストを処理できること。
- 下流が `Connection: close` を送信した場合、レスポンス送信後に接続を終了すること。
- 下流が HTTP/1.0 で接続した場合、`Connection: keep-alive` 有無にかかわらず 1 リクエストで接続を終了すること (RFC 9112 Section 9.3)。
- 下流接続のアイドル状態が 60 秒を超えた場合、接続を終了すること (Slowloris 対策)。
- 同一 TCP 接続で pipelined リクエストを受けた場合、`RequestDecoder` 内部に先読みされたバイトを保持したまま次のヘッダ解析へ遷移すること (`RequestDecoder::reset()` は呼ばない)。
- pipelined リクエストの受信順とレスポンス送信順が一致すること (RFC 9112 Section 9.3.2 「MUST send the corresponding responses in the same order」)。逐次 loop により自然に成立する点を結合テストで検証する。
- 上流接続プールが引き続き正しく機能すること。
- 下流接続の切断やエラー時も、再利用可能な上流接続はプールに返却されること。
- Graceful shutdown 時に、リクエスト読み取り中の `socket.read` でも `CancellationToken` で即座に break できること。アイドル待機中も同様。
- `examples/http11_reverse_proxy/tests/` 以下に、同一 TCP 接続から複数リクエストを送信する結合テストを追加すること。テストはモックやスタブを使わず、実際の TCP 接続で検証すること。ディレクトリが存在しない場合は新規作成すること。
- `examples/http11_reverse_proxy/Cargo.toml` の `[dev-dependencies]` に必要な依存を追加すること。`[dev-dependencies]` の `tokio` は本番 `[dependencies]` の `tokio` と features がマージされないため、テストで使う feature を全て再列挙する必要がある。最低限 `tokio = { version = "<本番と同バージョン>", features = ["macros", "rt-multi-thread", "net", "io-util", "time"] }` を指定し、テストが `tokio::time::pause()` 等の時間制御を使う場合は `test-util` も加える。
- `CHANGES.md` の `## develop` セクションの `### misc` 配下に `[FIX]` エントリを追加すること (本体ライブラリ修正ではなく `examples/` 配下の修正のため `### misc` 配下)。例: `[FIX] examples/http11_reverse_proxy で下流接続を 1 リクエストで閉じていた問題を修正し、HTTP/1.1 persistent connection を有効化する`。

## 解決方法

設計方針に沿って次の順序で実装する (詳細は設計方針節)。

1. `handle_client` (`L542`) のシグネチャに `cancel_token: CancellationToken` を追加し、`main()` の `tasks.spawn(...)` (`L504` 周辺) で `cancel_token.clone()` を渡す
2. `handle_client` 内に `loop { ... }` を追加し、各イテレーションで以下を実行する
   - リクエスト読み取り (`tokio::select!` で `cancel.cancelled()` と並列化、アイドル状態は `tokio::time::timeout(Duration::from_secs(60), ...)` でラップ)
   - `RequestDecoder::decode_headers()` でヘッダ取得
   - `peek_body()` / `consume_body()` / `progress()` でボディを `BodyProgress::Complete` まで消費
   - 上流転送は既存の `stream_upstream_response_pooled` (`L703`) / `stream_response_on_connection` (`L768`) を再利用 (戻り値の `Result<bool, _>` は維持)
   - 下流ライターの `BufWriter::flush().await?`
   - 継続/終了判定: HTTP/1.0 か / `is_keep_alive()` か / `read == 0` か / `cancel.is_cancelled()` か
3. `RequestDecoder::reset()` は呼ばない。`DecodePhase::Complete` から `decode_headers()` を再呼出することで先読み済みバイトを保持する
4. 上流で `Connection: close` や close-delimited body が返却された場合は当該上流接続をプールに返却せずクローズする。下流側には本 issue では `Connection: close` を付与しない (0114 で対応)
5. CONNECT リクエストは引き続き 405 Method Not Allowed で拒否し、`Connection: close` を付与してループを抜ける
6. `examples/http11_reverse_proxy/tests/` ディレクトリを新規作成し、結合テストを追加する。`Cargo.toml` `[dev-dependencies]` を更新する
7. `CHANGES.md` に `[FIX]` エントリを `### misc` 配下に追加する

## 参考: RFC 文面

- RFC 9112 Section 9.3 Persistence
  > A proxy server MUST NOT maintain a persistent connection with an HTTP/1.0 client (see Appendix C.2.2 of [RFC7230] for information and discussion of the problems with the Keep-Alive header field implemented by many HTTP/1.0 clients).
- RFC 9112 Section 9.3.2 Pipelining
  > A client that supports persistent connections MAY "pipeline" its requests (i.e., send multiple requests without waiting for each response). A server MAY process a sequence of pipelined requests in parallel if they all have safe methods (Section 9.2.1 of [HTTP]), but it MUST send the corresponding responses in the same order that the requests were received.
- RFC 9112 Section 9.5 Failures and Timeouts
  > Servers will usually have some timeout value beyond which they will no longer maintain an inactive connection. Proxy servers might make this a higher value since it is likely that the client will be making more connections through the same proxy server.
- RFC 9112 Section 9.6 Tear-down
  > The "close" connection option is defined as a signal that the sender will close this connection after completion of the response. A sender SHOULD send a "close" connection option in its last request or response on that connection.
