# Keep-Alive 接続で RequestDecoder / ResponseDecoder の Decompressor をリセットし状態漏れを防ぐ

- Priority: High
- Created: 2026-05-15
- Model: deepseek v4-pro
- Branch: feature/fix-decompressor-keep-alive-reset

## 目的

`RequestDecoder` と `ResponseDecoder` が `decode()` 完了時および `decode_headers()` の Complete→StartLine 遷移時に `self.decompressor` をリセットしていない。`NoCompression` に対しては無害だが、カスタム `Decompressor` (gzip 等) で前メッセージの内部展開状態が後続メッセージに持ち越される。

## 優先度根拠

- `Decompressor` トレイトはユーザーが任意の実装を差し込める公開 API
- 状態漏れはデータ破損またはセキュリティ境界違反に繋がり得る
- `NoCompression` 以外の実装が使われ始めた場合に顕在化する予防的修正

## 現状

修正対象は 4 箇所。いずれも既存のリセットブロックに `self.decompressor.reset()` が欠落している:

1. `src/decoder/request.rs:757-761` — `decode()` 完了時リセット
2. `src/decoder/response.rs:854-863` — `decode()` 完了時リセット
3. `src/decoder/request.rs:547-553` — `decode_headers()` Complete→StartLine 遷移
4. `src/decoder/response.rs:603-615` — `decode_headers()` Complete→StartLine 遷移

## 設計方針

上記 4 箇所すべてに `self.decompressor.reset()` を追加する。既存の `self.body_decoder.reset()` の直後に追加することでリセット順序を統一する。

`Decompressor::reset()` は戻り値を持たない infallible なメソッドであるため、追加によるエラー経路の変化はない。

## 完了条件

- 上記 4 箇所すべてに `self.decompressor.reset()` が追加されていること
- `NoCompression` とカスタム `Decompressor` 実装の両方で Keep-Alive 接続時に状態漏れが発生しないこと
- `cargo test` で全テストが通過すること
- `CHANGES.md` の `## develop` に `[FIX]` エントリが追加されていること
