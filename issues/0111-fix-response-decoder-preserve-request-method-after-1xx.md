# ResponseDecoder が 1xx 完了時に request_method を消して HEAD/CONNECT の最終応答 framing を壊す

- Priority: High
- Created: 2026-07-08
- Completed: {YYYY-MM-DD}
- Model: Grok 4.5
- Branch: feature/fix-response-decoder-preserve-request-method-after-1xx
- Polished: {YYYY-MM-DD}

## 目的

`ResponseDecoder` が informational response (1xx) を処理した直後に `request_method` をクリアするため、同一リクエストの最終応答で HEAD / CONNECT 向けの message body length 規則 (RFC 9112 Section 6.3) が適用されなくなる不具合を修正する。

## 優先度根拠

High とする。

- HEAD への `100 Continue` 後の最終応答で Content-Length / Transfer-Encoding を body ありとして解釈すると、クライアントが存在しない body を待ち続ける、または次メッセージ境界を誤る
- CONNECT への 1xx 後の 2xx で tunnel 判定を失うと、トンネル先頭データを HTTP として誤 parse しうる
- keep-alive 用の method クリア自体は必要だが、同一リクエスト内の informational → final と混同している

## 現状

### クリア箇所

`src/decoder/response.rs` の `decode_headers` 内 `DecodePhase::Complete` 分岐:

```rust
// request_method は元のリクエストごとに設定し直す前提で
// ここでクリアする。クリアしないと Keep-Alive 接続で前回の
// CONNECT などが残り、次のレスポンスを誤ってトンネル判定
// してしまう状態漏れバグになる。
self.request_method = None;
```

`decode()` 完了時にも同様に `self.request_method = None` する。

### framing 判定

`determine_body_kind` は `request_method` を見て:

- `"HEAD"` → `BodyKind::None` (RFC 9112 Section 6.3 item 1)
- `"CONNECT"` かつ 2xx → `BodyKind::Tunnel` (item 2)

1xx は `status_has_body` が false のため `BodyKind::None` → phase が `Complete` になる。

### 再現手順 (HEAD + 100 Continue)

1. `let mut decoder = ResponseDecoder::new();`
2. `decoder.set_request_method("HEAD");`
3. `decoder.feed(b"HTTP/1.1 100 Continue\r\n\r\n");`
4. `decode_headers()` → `BodyKind::None`、phase は Complete 相当
5. 続けて `decoder.feed(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nxxxxx");`
6. 次の `decode_headers()` の Complete 遷移で `request_method` が `None` になり、最終応答が `BodyKind::ContentLength(5)` になる (HEAD なら `None` であるべき)

CONNECT でも同様に、1xx 後の 2xx で Tunnel にならない。

### 既存テストの範囲

`pbt/tests/prop_decoder/response/streaming.rs` の
`prop_head_request_method_cleared_on_decode_headers_complete` 等は、
**次の HTTP レスポンス (別メッセージ)** で method がクリアされることを検証している。
keep-alive の次リクエスト用途としては正しいが、**同一リクエスト内の 1xx → final** はカバーしていない。

## 設計方針

- 1xx (informational) のメッセージ完了では `request_method` を保持する
- `request_method` をクリアしてよいのは次のいずれか:
  - 最終応答 (非 1xx) の処理完了時
  - `reset()` 時
  - 利用者が改めて `set_request_method` したとき (上書き)
- keep-alive で次のリクエストの応答を読む前には、利用者が再度 `set_request_method` する既存契約を維持する (クリア方針自体は残す)
- `decode_headers` の Complete 遷移と `decode()` 完了の両方で同じ規則にする

実装の切り分け例:

- Complete 遷移時に「直前に完了したのが 1xx か」を見て clear をスキップする
- または final response 完了時のみ clear し、1xx 完了では phase だけ StartLine に戻す

## 完了条件

- `set_request_method("HEAD")` のまま `100 Continue` の後に `200` + `Content-Length` を読んでも、最終応答は `BodyKind::None` のままである
- `set_request_method("CONNECT")` のまま 1xx の後に 2xx を読んでも、最終応答は `BodyKind::Tunnel` である
- keep-alive で最終応答完了後に method を再設定せず次レスポンスを読むと、HEAD/CONNECT 文脈が残らない (既存 PBT の意図を維持)
- `reset()` 後は `request_method` がクリアされる
- 単体または PBT で上記 1xx → final 経路を固定する
- 既存の keep-alive クリア系 PBT が通る (必要なら「最終応答後」であることをコメントで明確化)
- `cargo test --all` が通る
- `CHANGES.md` の `## develop` に `[FIX]` を追記する

## 解決方法

1. `src/decoder/response.rs` の Complete 遷移 / `decode()` 完了時の `request_method` クリア条件を「非 1xx 完了時のみ」に変更する
2. 1xx 完了時は phase / バッファ関連のみリセットし method を残す
3. `tests/test_decoder/` または `pbt/tests/prop_decoder/response/` に HEAD+100→200、CONNECT+1xx→2xx のテストを追加する
4. 既存の method クリア PBT のコメントを「最終応答完了後」の意図に合わせて直す
5. `CHANGES.md` を更新する

## 影響範囲

- `src/decoder/response.rs`
- `tests/test_decoder/**` / `pbt/tests/prop_decoder/response/**`
- `examples/http11_client` / `examples/http11_reverse_proxy` は 1xx を最終扱いする別問題があるが、本 issue のスコープ外 (別 issue)
