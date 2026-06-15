# リバースプロキシが Connection: close を下流/上流に伝達しない

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-reverse-proxy-connection-close-propagation
- Polished: 2026-06-16

## 目的

`examples/http11_reverse_proxy` が下流からの `Connection: close` を上流に、上流からの `Connection: close` を下流に正しく伝達するようにする。

issue 0111 で下流接続を複数リクエストに再利用できるよう修正した後も、接続終了の意思が hop-by-hop ヘッダーとして除去されたままだと、プロキシは誤って終了すべき上流接続を再利用し続ける。issue 0111 では下流接続の再利用を優先するため上流の close 意思を下流に伝達しない暫定設計としていた。本 issue では RFC 9112 Section 9.6 に従い close 意思を双方向に伝達し、上流接続の誤再利用を防ぐ。

本 issue は 0111 (下流接続再利用) → **0114 (close 伝達、本 issue)** → 0115 (リクエストボディストリーミング) の依存順の中央に位置する。0111 完了後にマージし、0115 はこれを前提として動かす。

### スコープ

本 issue が扱う対象は **`Connection` ヘッダの `close` トークンのみ**。下記は **射程外** として明示する。

- TE / Trailer / Upgrade など他の hop-by-hop ヘッダの上流付与・下流付与方針は本 issue では変更しない (現状の `is_hop_by_hop_header` 除去経路を維持)
- `Connection: close, upgrade` のような複数 connection option を持つ入力は、`close` トークンの有無のみを判定する (`is_keep_alive()` の現状動作と一致)。`upgrade` 等の他トークンの転送・処理は別 issue で扱う
- WebSocket / HTTP/2 のような Upgrade 経路は本サンプルでは未対応 (CONNECT は 0111 で 405 拒否済み)

## 優先度根拠

High とする。RFC 9112 Section 9.6 に従い、接続を終了する側は `Connection: close` を送信すべき (SHOULD) である。RFC 9110 Section 7.6.1 では仲介者は `Connection` ヘッダーを解析し、列挙された hop-by-hop ヘッダーを除去してから自身の接続制御ヘッダーを送信しなければならない (MUST)。現状は `Connection` を除去するのみで close の意思を新たな `Connection: close` として伝達しておらず、RFC 違反かつ誤った接続再利用を引き起こす。

## 現状

- `examples/http11_reverse_proxy/src/main.rs:640-676`: 下流リクエストから `Connection` を除外し、無条件で `Connection: keep-alive` を上流に付与する。下流が `Connection: close` を送信しても上流には伝わらない。
- `examples/http11_reverse_proxy/src/main.rs:826-890`: 上流レスポンスから `Connection` を除外し、`is_close_delimited` の場合のみ下流に `Connection: close` を付与する。上流が明示的に `Connection: close` を返した場合や HTTP/1.0 デフォルトで非持続の場合には伝達しない。
- `examples/http11_reverse_proxy/src/main.rs:750-761`: 上流接続のプール返却は `stream_response_on_connection` の戻り値 `can_reuse` に依存するが、下流の close 意思はこの値に反映されない。

## 設計方針

1. `Connection` ヘッダーは hop-by-hop であるため、下流/上流間で元の値をそのまま転送することはできない (RFC 9110 Section 7.6.1)。ただし、接続終了の意思は新たな `Connection: close` として伝達する。
2. 下流リクエストが keep-alive でない場合 (`is_keep_alive() == false`、つまり `Connection: close` を含むか HTTP/1.0 で `Connection: keep-alive` がない)、上流リクエストに `Connection: close` を付与する。同時に当該上流接続はプールに戻さない。
3. 上流レスポンスが keep-alive でない場合 (`is_keep_alive() == false`、つまり `Connection: close` を含むか HTTP/1.0 で `Connection: keep-alive` がない)、下流レスポンスに `Connection: close` を付与し、下流の `handle_client` ループも終了する。
4. close-delimited body の場合も引き続き `Connection: close` を下流に付与し、当該上流接続はプールに戻さない。`is_close_delimited` (`BodyKind::CloseDelimited`) は実用上 HTTP/1.0 経路で発生するが、HTTP/1.1 でも Content-Length / Transfer-Encoding が共に不在のケースで仕様上発生しうるため、HTTP/1.1 経路でも本判定を有効にする。
5. 下流が `Connection: close` を送信した場合、issue 0111 の `handle_client` ループが終了する。本 issue ではその終了意思を上流にも伝達する。
6. pipelined リクエストで `Connection: close` を含むリクエストの後に後続リクエストが存在する場合、RFC 9112 Section 9.6 「MUST NOT send further requests on that connection」は client 側義務だが、proxy 受信側ではこれを尊重し、`Connection: close` を観測した時点で後続の pipelined リクエストの読み取りを停止し、現在のリクエストの応答送信後にループを break する (0111 の persistent connection ループと整合)。

## 完了条件

- 下流から `Connection: close` が来た場合、上流リクエストにも `Connection: close` が付与されること。
- 下流から `Connection: close` が来た場合、当該上流接続はプールに戻らないこと。
- 上流から `Connection: close` が来た場合、下流レスポンスにも `Connection: close` が付与され、下流接続を終了すること。
- 上流から `Connection: close` が来た場合、当該上流接続はプールに戻らないこと。
- close-delimited body の場合も下流に `Connection: close` が付与され、当該上流接続はプールに戻らないこと (HTTP/1.0 経路だけでなく HTTP/1.1 で CL / TE 双方不在のケースも対象)。
- pipelined リクエストで `Connection: close` 後に後続リクエストが続いた場合、後続を処理せず現在のリクエストの応答送信後に下流ループを break すること。
- 0111 で新設される `examples/http11_reverse_proxy/tests/` ディレクトリに、上記ケースを検証する結合テストを追加すること。モックやスタブは使用しない。本 issue は 0111 のディレクトリ新設を前提とし、新規ディレクトリ作成は行わない。
- `CHANGES.md` の `## develop` セクションの `### misc` 配下に `[FIX]` エントリを追加すること (本体ライブラリ修正ではなく `examples/` 配下の修正のため、`### misc` 配下)。例: `[FIX] examples/http11_reverse_proxy が Connection: close を下流/上流に正しく伝達するよう修正する`。

## 解決方法

設計方針に沿って次の順序で実装する。

1. `examples/http11_reverse_proxy/src/main.rs:640-676` の上流リクエスト構築処理で、下流リクエストの `is_keep_alive()` を確認する。`false` の場合は `Connection: keep-alive` の付与をやめ、`Connection: close` を付与する。
2. `stream_upstream_response_pooled` のシグネチャに `downstream_keep_alive: bool` を追加し、main から `req_head.is_keep_alive()` の結果を渡す。同様に `stream_response_on_connection` も `downstream_keep_alive: bool` を受け取れるよう拡張する (0111 では戻り値拡張は避ける方針だったが、本 issue では入力引数の追加に留め、戻り値の構造は維持する)。
3. `stream_response_on_connection` (`L826-890`) 内の `can_reuse` 計算で、`downstream_keep_alive == false` の場合は `can_reuse = false` とする。下流レスポンス構築 (`L886-889` 周辺) でも同条件で `Connection: close` を付与する。**`Connection: close` 付与は同関数内で完結する** (関数戻り後にレスポンスヘッダを後付けする経路は取らない、現状の関数構造で破綻するため)。
4. `is_hop_by_hop_header` の除去経路は変更せず、本 issue は `close` トークンの新規付与のみを扱う。`close, upgrade` のような複数トークン入力は `is_keep_alive()` の現状動作 (close トークン検出のみ判定) に従う。
5. 結合テストは 0111 が新設する `examples/http11_reverse_proxy/tests/` ディレクトリを前提に、本 issue では既存ディレクトリへの追加のみ行う。ヘッダー構築ロジックを「テスト可能な形に切り出す」必要が生じた場合でも、本 issue は close 伝達の機能変更を主目的とし、refactor は最小限に留める (関数 1 つの抽出までを許容、それ以上は別 issue で扱う)。
6. `CHANGES.md` の `## develop` セクションの `### misc` 配下に `[FIX] examples/http11_reverse_proxy が Connection: close を下流/上流に正しく伝達するよう修正する` を追加する。

## 参考: RFC 文面

- RFC 9110 Section 7.6.1 Connection
  > Intermediaries MUST parse a received Connection header field before a message is forwarded and, for each connection-option in this field, remove any header or trailer field(s) from the message with the same name as the connection-option, and then remove the Connection header field itself (or replace it with the intermediary's own connection options for the forwarded message).
- RFC 9112 Section 9.6 Tear-down
  > The "close" connection option is defined as a signal that the sender will close this connection after completion of the response. A sender SHOULD send a "close" connection option in its last request or response on that connection.
  > A client that sends a "close" connection option MUST NOT send further requests on that connection (after the one containing the "close" option) and MUST close the connection after reading the final response message corresponding to this request.
- RFC 9112 Section 9.3 Persistence
  > A proxy server MUST NOT maintain a persistent connection with an HTTP/1.0 client.
