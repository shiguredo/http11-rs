# chunked チャンクサイズと body_consumed を u64 に統一する

- Priority: Medium
- Created: 2026-05-25
- Model: Opus 4.7
- Branch: feature/change-chunked-body-consumed-u64

## 目的

`BodyChunkedData { remaining: usize }` と `body_consumed: usize` が `BodyContentLength { remaining: u64 }` と型不整合を起こしている。32-bit 環境では `usize::from_str_radix` が `u32::MAX` を超えるチャンクサイズで `Err` を返し、RFC 9112 Section 7.1 の MUST 要件 ("recipients MUST anticipate potentially large hexadecimal numerals and prevent parsing errors due to integer conversion overflows or precision loss") に抵触する。

また `max_body_size: usize` (`DecoderLimits`) も `usize` であり、Content-Length 側の `u64` と型レベルで不整合がある。

## 優先度根拠

32-bit 環境 (組み込み、WASM32 等) で RFC 9112 Section 7.1 MUST 違反となる。本ライブラリは `no_std` 対応で 32-bit ターゲットを想定している。64-bit 環境では実害はないが、`BodyContentLength { remaining: u64 }` との型不整合はコードの一貫性を損なう。

## 現状

| フィールド | 型 | ファイル:行 |
|-----------|-----|-----------|
| `BodyContentLength { remaining }` | `u64` | `src/decoder/phase.rs:11` |
| `BodyChunkedData { remaining }` | `usize` | `src/decoder/phase.rs:15` |
| `body_consumed` | `usize` | `src/decoder/body.rs:75` |
| `max_body_size` | `usize` | `src/limits.rs:11` |
| `DecoderLimits::unlimited()` の `max_body_size` | `usize::MAX` | `src/limits.rs:45` |
| `BodyTooLarge { size, limit }` | `usize, usize` | `src/error.rs:19` |
| `chunk_size` (パース結果) | `usize` | `src/decoder/body.rs:437` |
| `checked_add` オーバーフロー時の size 値 | `usize::MAX` 相当 | `src/decoder/body.rs:191,250,334,457` |

## 設計方針

- `BodyChunkedData { remaining }` を `u64` に変更する
- `body_consumed` を `u64` に変更する
- `chunk_size` のパースを `u64::from_str_radix` に変更する
- `max_body_size` を `u64` に変更する (破壊的変更)
- `BodyTooLarge { size, limit }` のフィールドを `u64` に変更する (破壊的変更)
- `DecoderLimits::unlimited()` の `max_body_size` を `u64::MAX` に変更する
- `checked_add` オーバーフロー時に `BodyTooLarge` に渡す size 値を `u64` に合わせる
- バッファ操作時 (`peek_body` の返却スライス長、`consume_body` の引数等) にのみ `usize` にキャストする
- `DecoderLimits` のデフォルト値は変更しない (数値は同じ、型のみ変更)

### キャスト方針

バッファ長 (`buf.len()`: `usize`) と `remaining` (`u64`) の比較・演算は以下のパターンに統一する:

```rust
// peek_body: 返却スライス長の計算
let available = (buf.len() as u64).min(*remaining) as usize;
// consume_body: 消費量の減算
*remaining -= len as u64;
// body_consumed: 加算
self.body_consumed = self.body_consumed.checked_add(len as u64).ok_or_else(|| ...)?;
```

Content-Length パス (body.rs:129, 186) が既にこのパターン (`len as u64` / `*remaining as usize`) を使用しており、chunked / close-delimited パスも同じ方針に揃える。

### 影響パス一覧

本変更は以下の全パスに影響する:
- **Content-Length**: `peek_body` (body.rs:129) / `consume_body` (body.rs:186-192) — 既存の u64 パスだが `BodyTooLarge` のフィールド型が変わる
- **Chunked**: `peek_body` (body.rs:142) / `consume_body` (body.rs:246-251) / `process_chunked_size` (body.rs:437-467) — `remaining` と `chunk_size` の u64 化
- **CloseDelimited**: `consume_body` (body.rs:328-344) — `body_consumed` の u64 化に伴い `checked_add` の型が変わる

### 破壊的変更の影響

- `DecoderLimits` の `max_body_size` フィールドが `usize` → `u64` に変更
- `Error::BodyTooLarge` のフィールドが `usize` → `u64` に変更
- 利用側で `max_body_size` を設定している箇所はキャスト不要 (`usize` リテラルは `u64` に暗黙変換される)
- `BodyTooLarge` をパターンマッチしている箇所は型の変更に伴い修正が必要

## テスト戦略

### PBT

既存の `pbt/tests/prop_decoder.rs` の Strategy が u32::MAX を超えるチャンクサイズを生成できることを確認する。必要に応じて chunk-size の Strategy 範囲を拡張する。

### 単体テスト

`tests/test_decoder/` に以下を追加:
- u32::MAX を超えるチャンクサイズ (`FFFFFFFF1` 等) が正しくパースされること (32-bit 環境でのリグレッション確認)
- `max_body_size` を `u64` で設定した場合に `BodyTooLarge` エラーのフィールドが `u64` であること
- `DecoderLimits::unlimited()` で大きなチャンクサイズが通過すること

### Fuzzing

既存 fuzz target がパニック安全性を担保。型変更後もコンパイルが通り既存テストが通ればリグレッションなし。

## 完了条件

- `src/decoder/phase.rs` の `BodyChunkedData { remaining }` が `u64` であること
- `body_consumed` が `u64` であること
- `chunk_size` のパースが `u64::from_str_radix` であること
- `max_body_size` が `u64` であること
- `DecoderLimits::unlimited()` の `max_body_size` が `u64::MAX` であること
- `BodyTooLarge { size, limit }` のフィールドが `u64` であること
- `checked_add` オーバーフロー時に `BodyTooLarge` に渡す値が `u64` であること
- Content-Length / Chunked / CloseDelimited の全パスでコンパイルが通り正しく動作すること
- 既存テスト (PBT / 単体テスト / fuzz) が全て通ること
- `CHANGES.md` に `[CHANGE]` として破壊的変更を記載すること
