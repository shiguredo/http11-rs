# Keep-Alive 接続で RequestDecoder / ResponseDecoder の Decompressor をリセットし状態漏れを防ぐ

- Priority: High
- Created: 2026-05-15
- Model: deepseek-v4-pro

## 目的

`RequestDecoder` と `ResponseDecoder` が `decode()` 完了時および `decode_headers()` の Complete→StartLine 遷移時に `self.decompressor` をリセットしていない。`NoCompression` に対しては無害だが、カスタム `Decompressor` (gzip 等) で前メッセージの内部展開状態が後続メッセージに持ち越される。

## 優先度根拠

- `Decompressor` トレイトはユーザーが任意の実装を差し込める公開 API
- 状態漏れはデータ破損またはセキュリティ境界違反に繋がり得る
- `NoCompression` 以外の実装が使われ始めた場合に顕在化する予防的修正

## 現状

**`decode()` 完了時のリセット** (4 箇所):

`src/decoder/request.rs:757-761`:
```rust
self.phase = DecodePhase::StartLine;
self.decoded_body_kind = None;
self.decoded_body.clear();
self.body_decoder.reset();
// self.decompressor がリセットされない
```

`src/decoder/response.rs:854-863`:
```rust
self.phase = DecodePhase::StartLine;
self.decoded_body_kind = None;
self.decoded_body.clear();
self.body_decoder.reset();
self.status_code = 0;
self.request_method = None;
// self.decompressor がリセットされない
```

**`decode_headers()` の Complete→StartLine 遷移** (2 箇所):

`src/decoder/request.rs:547-553` と `src/decoder/response.rs:603-615` で同様に `self.decompressor.reset()` が欠落。

## 設計方針

1. 上記 4 箇所すべてに `self.decompressor.reset()` を追加する
2. 追加後も `reset()` が呼ばれるタイミングで他にクリアすべき状態漏れがないか確認する

## 完了条件

- `decode()` 完了後、`decode_headers()` の Complete→StartLine 遷移後ともに `self.decompressor` がリセットされていること
- `NoCompression` とカスタム `Decompressor` 実装の両方で Keep-Alive 接続時に状態漏れが発生しないこと
- `cargo test` で全テストが通過すること
