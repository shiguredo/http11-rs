# MultipartParser の InPart / AfterInnerDelimiter 状態で transport-padding に対応する

- Priority: High
- Created: 2026-05-15
- Completed: 2026-05-15
- Model: deepseek v4-pro
- Branch: feature/fix-multipart-transport-padding
## 目的

RFC 2046 Section 5.1.1 はすべての boundary delimiter (dash-boundary / delimiter / close-delimiter) に transport-padding (= *LWSP-char, SP/HTAB) を許容している。`MultipartParser` の `Initial` 状態では transport-padding が正しく処理されているが、`InPart` 状態と `AfterInnerDelimiter` 状態では未対応のままである。

## 優先度根拠

- RFC 2046 MUST accept の対象
- transport-padding 付き入力でパーサーが永続的に `Incomplete` を返し停止する DoS 経路
- Sans I/O では呼び出し側のループがブロックされ、タイムアウトなしで停止し得る

## 現状

**`InPart` 状態** (`src/multipart.rs:469-494`):

内部デリミタ `\r\n--boundary` 発見後に後続 2 バイト (`\r\n` / `--`) 判定を行うが、transport-padding のスキップがない。

```rust
let after_next = after_delim + 2;
// transport-padding スキップなしで直接判定
if self.buffer[after_delim] == b'\r' && self.buffer[after_delim + 1] == b'\n' {
```

**`AfterInnerDelimiter` 状態** (`src/multipart.rs:528-550`):

次回 `next_part()` でも同様に transport-padding スキップがない。

## 設計方針

1. `InPart` の内部デリミタ発見後に `Initial` と同様の transport-padding スキップループを追加する
   - パディングスキップ後に `\r\n` / `--` の 2 バイト判定を行う
   - パディング中に buffer が尽きた場合は `AfterInnerDelimiter` に遷移させる
2. `AfterInnerDelimiter` 状態でも transport-padding をスキップするロジックを追加する
   - パディング中に不足した場合は `AfterInnerDelimiter` に留まり `pos` のみ進行させる

## 完了条件

- `\r\n--boundary \t\r\n` (内部デリミタ + transport-padding + CRLF) が正しく処理されること
- `\r\n--boundary \t--\r\n` (内部デリミタ + transport-padding + close-delimiter) が正しく処理されること
- transport-padding の途中で feed が切れた場合も正常に継続できること
- 上記のテストケースが `tests/test_multipart.rs` に追加されていること
- `cargo test` と `cargo test -p pbt` で全テストが通過すること
- `CHANGES.md` の `## develop` に `[FIX]` エントリが追加されていること

## 解決方法

### `src/multipart.rs`

- `InPart` 状態の内部デリミタ発見後に transport-padding (SP/HTAB) をスキップするロジックを追加した
  - padding 途中で buffer が尽きた場合は `AfterInnerDelimiter` に遷移して `pos` を保持する
  - padding 後の 2 バイトが CRLF / `--` のいずれでもない場合は `InvalidPart` を返す
- `AfterInnerDelimiter` 状態でも transport-padding をスキップするロジックを追加した
  - padding 途中で buffer が尽きた場合は `AfterInnerDelimiter` に留まり `pos` を進行させる

### `tests/test_multipart.rs`

- 内部デリミタ + transport-padding + CRLF の正常系テスト 1 件を追加した
- 内部デリミタ + transport-padding + close-delimiter の正常系テスト 1 件を追加した
- transport-padding 途中で feed が切れた断片入力テスト 1 件を追加した
