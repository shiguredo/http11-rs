# 0008: encode / decode 用の request-target validation を分離する

Created: 2026-04-28
Model: Kimi 2.6 / GPT 5.5

## 概要

`is_valid_request_target()` は受信側の寛容さを含んで obs-text（`0x80..=0xFF`）を許容するが、`encoder.rs` では別途 `request.uri.bytes().any(|b| b > 0x7E)` で送信時に obs-text を拒否している。この混在を解消し、エンコード用とデコード用の validation を明示的に分離する。

## 根拠

現状の `src/validate.rs:180` の `is_valid_request_target()` は受信側の寛容さとして obs-text を許容している。しかし `src/encoder.rs:31` では:

```rust
// RFC 3986: URI は ASCII のみで構成される
// obs-text (0x80-0xFF) は受信側では許容するが、送信側では拒否する
if request.uri.bytes().any(|b| b > 0x7E) {
    return Err(EncodeError::InvalidRequestTarget { ... });
}
```

という別途のチェックがあり、obs-text を送信時に拒否している。この二重構造は以下の問題を生じさせる。

- `is_valid_request_target()` の責務が「受信側用なのか送信側用なのか」不明確。
- 将来 obs-text の扱いを変更する場合、2 箇所を把握して修正する必要がある。
- `encoder.rs` では `is_valid_request_target()`（L23）の呼び出し後に、別途 `request.uri.bytes().any(|b| b > 0x7E)`（L31）で obs-text を拒否している。obs-text を含む URI は `is_valid_request_target()` では通過するが、個別チェックで拒否される。受信側と送信側で obs-text の扱いが異なるにも関わらず、同じ関数名 `is_valid_request_target()` では意図が不明確である。

## 対象ファイルと変更点

### `src/validate.rs`

1. `is_valid_request_target()` を `is_valid_request_target_for_decode()` にリネームする。
   - 関数の doc comment に「受信側の寛容な検証。obs-text (0x80-0xFF) を許容する」と明記する。

2. `is_valid_request_target_for_encode()` を新設する。
   ```rust
   /// エンコード用 request-target 検証
   ///
   /// 送信側としては obs-text (0x80-0xFF) を拒否する。
   /// その他の制約は `is_valid_request_target_for_decode` と同じ。
   pub(crate) fn is_valid_request_target_for_encode(target: &str) -> bool {
       if !is_valid_request_target_for_decode(target) {
           return false;
       }
       // obs-text を拒否
       if target.bytes().any(|b| b > 0x7E) {
           return false;
       }
       true
   }
   ```

### `src/encoder.rs`

1. `use` 文で `is_valid_request_target` を `is_valid_request_target_for_encode` に変更する。

2. `validate_request_fields()` から個別の obs-text チェック（L31-L35）を削除する:
   ```rust
   // リクエストターゲットの検証（エンコード用）
   if !is_valid_request_target_for_encode(&request.uri) {
       return Err(EncodeError::InvalidRequestTarget {
           uri: request.uri.clone(),
       });
   }
   ```

### `src/decoder/request.rs`

`use` 文と呼び出しを `is_valid_request_target_for_decode` に変更する。

## 影響範囲

- `is_valid_request_target` のリネームは `pub(crate)` なので crate 外への影響はない。
- `encoder.rs` のエラーメッセージは変わらない（同じ `InvalidRequestTarget`）。

## 検証

- `make fmt && make clippy && make check && make test` を通す。
- obs-text を含む request-target のエンコードが拒否されることを確認する。
- obs-text を含む request-target のデコードについて:
  - 現状の `RequestDecoder` は request-line を `String::from_utf8` でパースしているため、UTF-8 として無効な obs-text バイト列は validation に到達する前に拒否される。
  - UTF-8 として有効な非 ASCII（例: マルチバイト文字）が request-target に含まれる場合、`is_valid_request_target_for_decode` が許容することを確認する。
- 既存の request-target validation テストがそのまま緑であることを確認する。
