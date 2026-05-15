# Keep-Alive 接続で RequestDecoder / ResponseDecoder の Decompressor をリセットし状態漏れを防ぐ

- Priority: High
- Created: 2026-05-15
- Completed: 2026-05-15
- Model: deepseek v4-pro
- Branch: feature/fix-decompressor-reset

## 目的

`RequestDecoder` と `ResponseDecoder` が `decode()` 完了時および `decode_headers()` の Complete→StartLine 遷移時に `self.decompressor` をリセットしていない。RFC 9112 Section 9.6 の persistent connection では複数のリクエスト/レスポンスが同一接続上で連続するため、前メッセージの Decompressor 内部状態（gzip の辞書、zstd のコンテキスト等）が後続メッセージに持ち越されるとデータ破損が発生する。

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

`Decompressor::reset()` は戻り値を持たない infallible なメソッドであるため、追加によるエラー経路の変化はない。`NoCompression::reset()` は `self.finished = false` をセットするが、`NoCompression` の `decompress()` 実装はこのフィールドを参照しないため実害はない。

## テスト

### 単体テスト

`tests/test_decoder/` に以下を追加する:

- 内部状態（呼び出し回数カウンタ）を持つカスタム `Decompressor` stub 実装を使用し、2 メッセージ連続 decode で状態がメッセージ間でリセットされることを検証する
- 検証内容: 1 件目のメッセージ完了後に `decompressor.reset()` が呼ばれ、2 件目のデコード開始時に内部状態が初期値に戻っていること

### fuzz 拡充

`fuzz/fuzz_targets/fuzz_pipelined.rs` に `Decompressor` 注入経路を追加する。内部状態カウンタを持つ stub を注入し、連続 decode の panic 安全性と状態リセットを検証する。

## 完了条件

- 上記 4 箇所すべてに `self.decompressor.reset()` が追加されていること
- カスタム `Decompressor` stub を用いた単体テストでメッセージ間の状態リセットが検証されていること
- `cargo fuzz run fuzz_pipelined` が Decompressor 注入経路でも crash を報告しないこと
- `cargo test` で全テストが通過すること
- `CHANGES.md` の `## develop` に `[FIX]` エントリが追加されていること（文言例: `Keep-Alive 接続で RequestDecoder / ResponseDecoder の Decompressor をリセットし状態漏れを防ぐ`）

## 解決方法

### デコーダー本体

- `src/decoder/request.rs` の `decode()` 完了時と `decode_headers()` Complete→StartLine 遷移時の 2 箇所に `self.decompressor.reset()` を追加した
- `src/decoder/response.rs` の `decode()` 完了時と `decode_headers()` Complete→StartLine 遷移時の 2 箇所に `self.decompressor.reset()` を追加した

### 単体テスト

- `tests/test_decoder/streaming.rs` に `CountingDecompressor` (内部状態カウンタ付き Decompressor stub) を導入し、2 メッセージ連続 decode でメッセージ間の `reset()` が呼ばれることを検証するテスト 2 件を追加した

### fuzz

- `fuzz/fuzz_targets/fuzz_pipelined.rs` に `CountingFuzzDecompressor` を注入する Decompressor 経路を追加し、連続 decode の panic 安全性を検証する
