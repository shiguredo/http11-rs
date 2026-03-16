# 変更履歴

- UPDATE
  - 後方互換がある変更
- ADD
  - 後方互換がある追加
- CHANGE
  - 後方互換のない変更
- FIX
  - バグ修正

## develop

- [CHANGE] `MultipartParser::feed()` の戻り値を `Result<(), MultipartError>` に変更する
  - バッファ上限超過時に `MultipartError::BufferOverflow` を返す
  - @voluntas
- [ADD] `MultipartParser` にバッファ上限を追加する
  - `max_buffer_size` フィールドを追加し、デフォルト 10MB の上限を設ける
  - `with_max_buffer_size()` ビルダーメソッドを追加する
  - @voluntas

### misc

- [ADD] `feed_unchecked()` と `DecoderLimits::unlimited()` に未信頼入力での OOM リスクを警告するドキュメントを追加する

## 2026.1.1

**リリース日**: 2026-03-16

- [FIX] CONNECT リクエストの Content-Length / Transfer-Encoding の扱いを RFC 9110 Section 9.3.6 に準拠させる
  - ヘッダーが存在するだけで reject しないようにする (RFC は MUST NOT としていない)
  - ヘッダーが存在しても body として読まず BodyKind::None として扱うようにする ("does not have content")
  - @voluntas

## 2026.1.0

**リリース日**: 2026-02-25

**公開**
