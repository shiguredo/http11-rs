# リバースプロキシが Connection: close を下流/上流に伝達しない

- Priority: High
- Created: 2026-06-13
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-reverse-proxy-connection-close-propagation
- Polished: 2026-06-13

## 目的

`examples/http11_reverse_proxy` が下流からの `Connection: close` を上流に、上流からの `Connection: close` を下流に正しく伝達するようにする。

issue 0111 で下流接続を複数リクエストに再利用できるよう修正した後も、接続終了の意思が hop-by-hop ヘッダーとして除去されたままだと、プロキシは誤って終了すべき上流接続を再利用し続ける。issue 0111 では下流接続の再利用を優先するため上流の close 意思を下流に伝達しない暫定設計としていた。本 issue では RFC 9112 Section 9.6 に従い close 意思を双方向に伝達し、上流接続の誤再利用を防ぐ。

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
4. close-delimited body の場合も引き続き `Connection: close` を下流に付与し、当該上流接続はプールに戻さない。
5. 下流が `Connection: close` を送信した場合、issue 0111 の `handle_client` ループが終了する。本 issue ではその終了意思を上流にも伝達する。

## 完了条件

- 下流から `Connection: close` が来た場合、上流リクエストにも `Connection: close` が付与されること。
- 下流から `Connection: close` が来た場合、当該上流接続はプールに戻らないこと。
- 上流から `Connection: close` が来た場合、下流レスポンスにも `Connection: close` が付与され、下流接続を終了すること。
- 上流から `Connection: close` が来た場合、当該上流接続はプールに戻らないこと。
- close-delimited body の場合も下流に `Connection: close` が付与され、当該上流接続はプールに戻らないこと。
- `examples/http11_reverse_proxy` にテストを追加し、上記ケースを検証すること。モックやスタブは使用しない。
- `CHANGES.md` に FIX エントリを追加すること。

## 解決方法

- `examples/http11_reverse_proxy/src/main.rs:640-676` の上流リクエスト構築処理で、下流リクエストの `is_keep_alive()` を確認する。`false` の場合は `Connection: keep-alive` の付与をやめ、`Connection: close` を付与する。
- 下流の close 状態を `stream_upstream_response_pooled` の引数に渡し、下流 close 時は `stream_response_on_connection` の戻り値 `can_reuse` を `false` にする。下流 close 時に上流も close を返した場合は自然に `can_reuse == false` となるが、保証されないため下流 close 状態を明示的に反映する。
- `examples/http11_reverse_proxy/src/main.rs:826-890` の下流レスポンス構築処理で、`is_close_delimited` または `can_reuse == false` の場合に `Connection: close` を付与する。
- 必要に応じて上流リクエスト/レスポンスのヘッダー構築ロジックをテスト可能な形に切り出し、`examples/http11_reverse_proxy/tests/` にテストを追加する。
- `CHANGES.md` の develop セクションに `[FIX] examples/http11_reverse_proxy が Connection: close を下流/上流に正しく伝達するよう修正する` を追加する。
