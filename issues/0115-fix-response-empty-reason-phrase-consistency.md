# Response と ResponseHead で空 reason-phrase の受理方針が食い違い proxy が落ちる

- Priority: High
- Created: 2026-07-08
- Completed: {YYYY-MM-DD}
- Model: Grok 4.5
- Branch: feature/fix-response-empty-reason-phrase-consistency
- Polished: {YYYY-MM-DD}

## 目的

RFC 9112 Section 4 で reason-phrase は absent (空) を許容するのに対し、`Response::new` / `with_version` だけが空を拒否している非対称を解消する。
`examples/http11_reverse_proxy` が `ResponseHead::into_parts` の後に `Response::new` で再構築するため、上流の空 reason-phrase で正当な応答を転送できない。

## 優先度根拠

High とする。

- CODEBASE.md は Request / Response / RequestHead / ResponseHead の API 一貫性を要求する
- reverse_proxy はお手本実装であり、空 reason の上流応答で `EncodeError` になる
- decoder / `from_raw_parts` は空を許容済みで、送信 API だけが厳しい

## 現状

### ResponseHead (`src/decoder/head.rs`)

```rust
// reason-phrase 空文字列は absent 扱いで許容、非空ならバリデート
if !reason_phrase.is_empty() && !is_valid_reason_phrase(reason_phrase) {
    return Err(...);
}
```

### Response (`src/response.rs`)

```rust
if !is_valid_reason_phrase(&reason_phrase) {
    return Err(EncodeError::InvalidReasonPhrase { phrase: reason_phrase });
}
```

`is_valid_reason_phrase` (`src/validate.rs`) は `!phrase.is_empty()` を要求する。
コメントは「空の扱いは呼び出し側の責務」と書いており、Head と Message で方針が割れている。

### from_raw_parts

`reason_phrase.is_empty() || is_valid_reason_phrase(...)` で空を許容。decoder 一括経路は通る。

### proxy (`examples/http11_reverse_proxy/src/main.rs`)

```rust
let (_version, status_code, reason_phrase, headers) = resp_head.into_parts();
let mut response_for_headers = Response::new(status_code, reason_phrase)?;
```

上流が `HTTP/1.1 200\r\n` (reason absent) を返すとここで失敗する。

## 設計方針

- **送信側 `Response::new` / `with_version` でも空 reason-phrase を absent として許容する** (ResponseHead / from_raw_parts / RFC 9112 に揃える)
- 非空のときだけ `is_valid_reason_phrase` の文字種検証を行う
- encoder は空 reason を `HTTP/1.1 200 \r\n` 形式 (status-code 後の SP は残し reason 空) で出力できることを確認する
- status-line 受信側が code 直後 SP を必須とする厳密化は本 issue のスコープ外 (別 issue 可)

破壊的変更にはならない (受理集合が広がるだけ)。ただし「空を Err にしていた」呼び出し側のエラーハンドリング前提は変わる。

## 完了条件

- `Response::new(200, "")` が `Ok` になる
- 非空で CTL を含む reason は従来どおり `Err`
- `ResponseHead` と `Response` の空 reason 方針が一致する
- `encode_response` / `encode_response_headers` が空 reason を正しく出力する
- reverse_proxy の `Response::new(status_code, reason_phrase)` が空 reason で落ちない
- 単体テストで固定する
- `cargo test --all` が通る
- `CHANGES.md` の `## develop` に `[FIX]` を追記する

## 解決方法

1. `Response::new` / `with_version` の reason 検証を Head と同じ分岐にする
2. encoder の status-line 出力が空 reason で破綻しないことを確認・必要なら修正する
3. `tests/test_response.rs` / `tests/test_encoder/` に空 reason の構築・encode テストを追加する
4. `CHANGES.md` を更新する

## 影響範囲

- `src/response.rs`
- `src/encoder.rs` (出力確認)
- `tests/test_response.rs` / `tests/test_encoder/**`
- `examples/http11_reverse_proxy` (挙動改善。コード変更は必須ではない)
